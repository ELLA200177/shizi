//! 拾字 · M1 内核
//!
//! 托盘常驻 + 全局热键唤起面板 + 把选中词条注入到唤起前的前台窗口。
//! 词条来自前端的 `ui/entries.js`（由 packs/*.json 生成），此阶段不引入数据库与包管理。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod expand;
mod inject;
mod log;
mod store;
mod win32;

use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// 唤起面板前的前台窗口句柄（存 isize 以便跨线程）。
#[derive(Default)]
struct AppState {
    target: Mutex<isize>,
    /// 实际注册成功的那个热键（用于界面提示）
    hotkey: Mutex<String>,
    /// 词条库。启动时打开，之后所有读写共用这一个连接。
    db: Mutex<Option<rusqlite::Connection>>,
}

/// 候选热键，按优先级排列。
///
/// 为什么不写死一个：本机实测 `Alt+Space` / `Win+Space` 已被占用（RegisterHotKey 返回 1409），
/// 而且占用情况会随用户装了别的软件而变化。写死一个键 = 用户装上就崩。
/// PRD 9.4 要求「注册失败必须当场提示并给出备选，不能只写日志」——这里就是落地。
fn hotkey_candidates() -> Vec<(Modifiers, Code, &'static str)> {
    vec![
        (Modifiers::CONTROL | Modifiers::ALT, Code::Space, "Ctrl+Alt+Space"),
        (Modifiers::CONTROL | Modifiers::SHIFT, Code::Space, "Ctrl+Shift+Space"),
        (Modifiers::CONTROL | Modifiers::ALT, Code::KeyQ, "Ctrl+Alt+Q"),
        (Modifiers::ALT, Code::KeyK, "Alt+K"),
    ]
}

fn toggle_panel(app: &AppHandle) {
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };
    let visible = win.is_visible().unwrap_or(false);

    if visible {
        let _ = win.hide();
        return;
    }

    // 关键顺序：必须在面板抢到焦点之前记录前台窗口
    let mut target = win32::foreground_window();
    if win32::is_own_window(target) {
        // 自指护栏：绝不允许把文本粘回拾字自己（曾经真的这么错过）
        log::diag("警告：捕获到的前台窗口属于拾字自己，丢弃该目标");
        target = 0;
    }
    log::diag(&format!("唤起面板，记录目标窗口 = 0x{:x}", target));
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut t) = state.target.lock() {
            *t = target;
        }
    }

    // 屏幕中央偏上，像 Spotlight
    if let Ok(Some(mon)) = win.current_monitor() {
        let scale = mon.scale_factor();
        let size = mon.size();
        let (w, _h) = (size.width as f64 / scale, size.height as f64 / scale);
        let win_w = 582.0;
        let x = ((w - win_w) / 2.0).max(0.0);
        let y = (size.height as f64 / scale * 0.16).max(0.0);
        let _ = win.set_position(PhysicalPosition::new(
            (x * scale) as i32,
            (y * scale) as i32,
        ));
    }

    // Windows 11 圆角（失败也无妨）
    if let Ok(h) = win.hwnd() {
        win32::set_rounded_corners(h.0 as isize);
    }

    let _ = win.show();
    let _ = win.set_focus();
}

/// 打开词库管理窗口。
fn open_manager(app: &AppHandle) {
    match app.get_webview_window("manager") {
        Some(win) => {
            let _ = win.show();
            let _ = win.set_focus();
        }
        None => log::diag("找不到管理窗口"),
    }
}

#[tauri::command]
fn commit_text(app: AppHandle, text: String) -> Result<inject::InjectResult, String> {
    let target = app
        .try_state::<AppState>()
        .and_then(|s| s.target.lock().ok().map(|t| *t))
        .unwrap_or(0);

    // 关键顺序（2026-10-10 实测）：
    //   1) 趁面板**还是前台窗口**时，先放开「设置前台窗口」的许可（只有前台进程能发放）
    //   2) 再隐藏面板 —— 由系统把前台和**内部焦点控件**一起还给目标（先抢前台会丢内部焦点）
    //   3) 隐藏之后再抢前台，此时许可已放开，不需要 Alt 那种会污染修饰键的脏招
    let allowed = win32::allow_set_foreground_any();
    log::diag(&format!("放开设置前台许可: {}", allowed));

    if let Some(win) = app.get_webview_window("panel") {
        let _ = win.hide();
    }

    // 等目标程序接稳焦点
    std::thread::sleep(std::time::Duration::from_millis(140));

    log::diag(&format!(
        "回车提交: target=0x{:x}, {} 字符",
        target,
        text.chars().count()
    ));
    let r = inject::inject(&text, target, false);
    match &r {
        Ok(v) => log::diag(&format!(
            "注入结果: method={} focused={} 路径={}",
            v.method, v.focused, v.focus_path
        )),
        Err(e) => log::diag(&format!("注入失败: {}", e)),
    }
    r
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        let _ = win.hide();
    }
}

/// 注册一个热键；失败返回错误（不 panic，交给调用方决定退让）。
fn try_register_hotkey(app: &AppHandle, mods: Modifiers, code: Code) -> Result<(), String> {
    let sc = Shortcut::new(Some(mods), code);
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(sc, move |_app, _sc, event| {
            if event.state == ShortcutState::Pressed {
                toggle_panel(&handle);
            }
        })
        .map_err(|e| e.to_string())
}

/// 按优先级试装热键。`prefer` 是用户上次选定的那个，优先尝试它。
fn install_hotkey(app: &AppHandle, prefer: Option<&str>) -> Option<&'static str> {
    let mut list = hotkey_candidates();
    if let Some(p) = prefer {
        if let Some(i) = list.iter().position(|(_, _, label)| *label == p) {
            let first = list.remove(i);
            list.insert(0, first);
        }
    }
    for (mods, code, label) in list {
        match try_register_hotkey(app, mods, code) {
            Ok(()) => return Some(label),
            Err(e) => eprintln!("[拾字] 热键 {label} 注册失败（{e}），尝试下一个候选"),
        }
    }
    None
}

fn norm_trigger(t: Option<String>) -> Option<String> {
    t.map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .map(|x| if x.starts_with(';') { x } else { format!(";{x}") })
}

/// 词条改动后，把新的触发码表推给展开引擎 —— 不用重启。
fn refresh_expand_triggers(state: &tauri::State<AppState>) {
    if let Ok(g) = state.db.lock() {
        if let Some(conn) = g.as_ref() {
            if let Ok(m) = store::trigger_map(conn) {
                expand::set_triggers(m);
            }
        }
    }
}

/// 从任意文本猜拼音：返回 (全拼, 首字母)。非汉字字符不参与。
fn pinyin_of(s: &str) -> (String, String) {
    use pinyin::ToPinyin;
    let mut full = String::new();
    let mut ini = String::new();
    for p in s.to_pinyin() {
        if let Some(py) = p {
            let plain = py.plain();
            full.push_str(plain);
            if let Some(c) = plain.chars().next() {
                ini.push(c);
            }
        }
    }
    (full, ini)
}

/// 给用户**建议**触发码。刻意不让用户自己"想" —— 那等于把「必须记触发码」
/// 这个缺点装回自己的产品。
/// 触发码建议的纯逻辑。抽出来是为了能单测 —— 它是「不让用户自己想触发码」
/// 这个设计的核心，光靠编译通过不算验证。
fn suggest_for(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        let s = s.trim().to_lowercase();
        if s.len() >= 2 && s.len() <= 24 && s.is_ascii() && !out.contains(&s) {
            out.push(s);
        }
    };

    // 1) 内容里的 ASCII 片段：A123 / h2so4 / 13c
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
        } else {
            if cur.len() >= 2 {
                push(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.len() >= 2 {
        push(cur);
    }

    // 2) 拼音首字母、全拼
    let (full, ini) = pinyin_of(&text);
    push(ini);
    push(full);

    // 3) 去掉符号后的整体
    push(text.chars().filter(|c| c.is_ascii_alphanumeric()).collect());

    out.truncate(6);
    out
}

#[tauri::command]
fn suggest_triggers(text: String) -> Vec<String> {
    suggest_for(&text)
}

#[derive(serde::Deserialize)]
struct EntryInput {
    id: Option<String>,
    text: String,
    trigger: Option<String>,
    group: Option<String>,
    note: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

#[tauri::command]
fn save_entry(state: tauri::State<AppState>, input: EntryInput) -> Result<String, String> {
    let text = input.text.trim().to_string();
    if text.is_empty() {
        return Err("要插入的内容不能为空".into());
    }
    let trigger = norm_trigger(input.trigger);

    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;

    // 官方包的条目不许改 —— 否则包升级时会和用户的修改互相覆盖
    if let Some(i) = input.id.as_deref() {
        match store::origin_of(conn, i).map_err(|e| e.to_string())? {
            Some(o) if o == "user" => {}
            Some(_) => return Err("这是官方词库包的条目，不能修改（可以停用它）".into()),
            None => return Err("词条不存在".into()),
        }
    }
    // 触发码冲突：报错时把占用者告诉用户，别让人猜
    if let Some(t) = trigger.as_deref() {
        if let Some(owner) =
            store::trigger_owner(conn, t, input.id.as_deref()).map_err(|e| e.to_string())?
        {
            return Err(format!("触发码 {t} 已被占用：{owner}"));
        }
    }

    let id = store::upsert_user_entry(
        conn,
        input.id.as_deref(),
        &text,
        trigger.as_deref(),
        input.group.as_deref(),
        input.note.as_deref(),
        &input.aliases,
    )
    .map_err(|e| e.to_string())?;
    drop(guard);
    refresh_expand_triggers(&state);
    log::diag(&format!("已保存用户词条 {id}（触发码 {:?}）", trigger));
    Ok(id)
}

#[tauri::command]
fn delete_entry(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    let n = store::delete_user_entry(conn, &id).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("只能删除自己添加的词条；官方包的条目请用「停用」".into());
    }
    drop(guard);
    refresh_expand_triggers(&state);
    log::diag(&format!("已删除用户词条 {id}"));
    Ok(())
}

#[tauri::command]
fn toggle_entry(state: tauri::State<AppState>, id: String, on: bool) -> Result<(), String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    store::set_entry_enabled(conn, &id, on).map_err(|e| e.to_string())?;
    drop(guard);
    refresh_expand_triggers(&state);
    log::diag(&format!("词条 {id} 已{}", if on { "启用" } else { "停用" }));
    Ok(())
}

/// 打字自动展开的开关状态。
#[tauri::command]
fn expand_enabled() -> bool {
    expand::is_enabled()
}

#[tauri::command]
fn set_expand_enabled(on: bool) {
    expand::set_enabled(on);
}

/// 面板加载词条。词条现在住在 SQLite 里，用户能自己增删改。
#[tauri::command]
fn load_entries(
    state: tauri::State<AppState>,
    include_disabled: Option<bool>,
) -> Result<Vec<store::EntryOut>, String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    store::load_all(conn, include_disabled.unwrap_or(false)).map_err(|e| e.to_string())
}

/// ---------------- 设置 ----------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsOut {
    hotkey: String,
    hotkey_candidates: Vec<String>,
    expand_enabled: bool,
    autostart: bool,
    first_run_done: bool,
    data_dir: String,
    db_path: String,
    version: String,
}

fn get_bool_setting(conn: &rusqlite::Connection, key: &str, default: bool) -> bool {
    store::get_setting(conn, key)
        .ok()
        .flatten()
        .map(|v| v == "1")
        .unwrap_or(default)
}

#[tauri::command]
fn get_settings(app: AppHandle, state: tauri::State<AppState>) -> Result<SettingsOut, String> {
    use tauri_plugin_autostart::ManagerExt;
    let dir = user_data_dir()?;
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    let hotkey = app
        .try_state::<AppState>()
        .and_then(|s| s.hotkey.lock().ok().map(|g| g.clone()))
        .unwrap_or_default();
    Ok(SettingsOut {
        hotkey,
        hotkey_candidates: hotkey_candidates()
            .into_iter()
            .map(|(_, _, l)| l.to_string())
            .collect(),
        expand_enabled: expand::is_enabled(),
        // 开机自启以系统实际状态为准，不信数据库
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        first_run_done: get_bool_setting(conn, "first_run_done", false),
        data_dir: dir.display().to_string(),
        db_path: store::db_path(&dir).display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
fn set_setting(
    app: AppHandle,
    state: tauri::State<AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    match key.as_str() {
        "expand_enabled" => {
            let on = value == "1";
            expand::set_enabled(on);
        }
        "autostart" => {
            let on = value == "1";
            let r = if on {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
            r.map_err(|e| format!("设置开机自启失败: {e}"))?;
        }
        "first_run_done" | "hotkey" => {}
        other => return Err(format!("未知的设置项: {other}")),
    }

    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    store::set_setting(conn, &key, &value).map_err(|e| e.to_string())?;
    log::diag(&format!("设置已更新: {key} = {value}"));
    Ok(())
}

/// 换全局热键。失败会把原来的装回去，不让用户陷入「没有热键」的状态。
#[tauri::command]
fn set_hotkey(app: AppHandle, state: tauri::State<AppState>, label: String) -> Result<(), String> {
    let Some((mods, code, _)) = hotkey_candidates()
        .into_iter()
        .find(|(_, _, l)| *l == label)
    else {
        return Err(format!("不支持的热键: {label}"));
    };

    let old = app
        .try_state::<AppState>()
        .and_then(|s| s.hotkey.lock().ok().map(|g| g.clone()))
        .unwrap_or_default();

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("注销旧热键失败: {e}"))?;
    if let Err(e) = try_register_hotkey(&app, mods, code) {
        // 装不回去就退回默认，务必留一个可用热键
        let back = install_hotkey(&app, None).unwrap_or("(无)");
        return Err(format!("{label} 注册失败（{e}），已退回 {back}"));
    }

    if let Some(st) = app.try_state::<AppState>() {
        if let Ok(mut g) = st.hotkey.lock() {
            *g = label.clone();
        }
    }
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    if let Some(conn) = guard.as_ref() {
        store::set_setting(conn, "hotkey", &label).ok();
    }
    log::diag(&format!("热键已从 {old} 切换为 {label}"));
    Ok(())
}

/// 数据目录：刻意放在用户**看得见、找得到**的地方，而不是 `%APPDATA%` 深处。
///
/// 两个理由：
///  1. 拾字的目标用户不写配置，词库就是他们的"资产" —— 必须能自己找到、复制、备份；
///  2. `%APPDATA%` 会被 MSIX 打包进程（例如从 Codex 里启动）重定向进沙箱，
///     数据放这里可以绕开那一整类坑（见下方 `open_data_dir` 的注释）。
///
/// 为什么不直接用「文档」：文档目录可能被 OneDrive 之类的云同步接管，
/// 而 SQLite 数据库放进同步目录有损坏风险 —— 确认不在云同步下才用它，否则退回用户主目录。
fn user_data_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "找不到用户主目录".to_string())?;
    if let Some(docs) = dirs::document_dir() {
        let synced = std::env::var("OneDrive")
            .map(|od| docs.starts_with(od))
            .unwrap_or(false);
        if !synced {
            return Ok(docs.join("拾字数据"));
        }
    }
    Ok(home.join("拾字数据"))
}

/// 在资源管理器里打开数据目录。
///
/// ⚠️ 这里记录一次**诊断错误及其更正**（2026-10-12），因为踩的坑本身很有价值。
///
/// 现象：用户点「打开数据目录」，资源管理器毫无反应。
/// 当时的结论是「`AppData` 带隐藏属性，资源管理器拒绝从命令行进入隐藏目录」，
/// 于是改用 `/select,<目录>`（只停在父目录 `Roaming` 并选中该文件夹），
/// 还加了「复制路径」当替代路线。
///
/// **那个结论是错的。** 用**非打包进程**（`Invoke-CimMethod Win32_Process Create` 派生，
/// 脱离 MSIX 上下文）查真实文件系统后才看清：数据目录当时**根本不在真实的 AppData 里** ——
/// 它只存在于 Codex 这个 MSIX 应用的虚拟存储
/// `%LOCALAPPDATA%\Packages\OpenAI.Codex_*\LocalCache\Roaming\com.shizi.app`。
/// 资源管理器跑在真实上下文，看不到这个路径，于是**什么都不打开**
/// （退出码仍然是 0，所以看起来像"被策略拒绝"）。
///
/// 把目录真正建到真实路径之后，`explorer <目录>` **可以直接打开它** ——
/// 即使 `AppData` 自身确实带隐藏属性（`attrib` 实测为 `H`）也不影响导航。
/// 所以这里就用最直接的一句。
///
/// 同一天还把数据目录从 `%APPDATA%` 挪到了用户可见的位置（见 `user_data_dir`），
/// 从根上避开这类沙箱问题。
#[tauri::command]
fn open_data_dir() -> Result<(), String> {
    let dir = user_data_dir()?;
    if !dir.exists() {
        return Err(format!("数据目录还不存在: {}", dir.display()));
    }
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("调用资源管理器失败: {e}"))?;
    log::diag(&format!("已请求资源管理器打开 {}", dir.display()));
    Ok(())
}

/// 复制一段文本到剪贴板（设置页「复制路径」用，方便备份或排障）。
#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板: {e}"))?;
    cb.set_text(text).map_err(|e| format!("写剪贴板失败: {e}"))
}

/// ---------------- 词库包导入导出 ----------------

#[tauri::command]
fn export_pack(
    state: tauri::State<AppState>,
    pack_id: Option<String>,
    path: String,
) -> Result<usize, String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    let (_, text) = store::export_pack(conn, pack_id.as_deref())?;
    std::fs::write(&path, text).map_err(|e| format!("写文件失败: {e}"))?;
    log::diag(&format!("已导出词库包到 {path}"));
    Ok(0)
}

#[tauri::command]
fn preview_import(
    state: tauri::State<AppState>,
    path: String,
) -> Result<store::ImportPreview, String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    store::preview_import(conn, &path)
}

#[tauri::command]
fn apply_import(
    state: tauri::State<AppState>,
    path: String,
) -> Result<store::ImportResult, String> {
    let mut guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_mut().ok_or_else(|| "数据库未初始化".to_string())?;
    let r = store::apply_import(conn, &path)?;
    drop(guard);
    refresh_expand_triggers(&state);
    log::diag(&format!(
        "已导入包 {}：{} 条，{} 条因触发码冲突降级为仅搜索",
        r.pack_id, r.imported, r.trigger_dropped
    ));
    Ok(r)
}

#[tauri::command]
fn remove_pack(state: tauri::State<AppState>, pack_id: String) -> Result<usize, String> {
    let guard = state.db.lock().map_err(|_| "数据库锁失效".to_string())?;
    let conn = guard.as_ref().ok_or_else(|| "数据库未初始化".to_string())?;
    let n = store::remove_pack(conn, &pack_id).map_err(|e| e.to_string())?;
    drop(guard);
    refresh_expand_triggers(&state);
    log::diag(&format!("已移除导入的包 {pack_id}（{n} 条）"));
    Ok(n)
}

/// 供面板底部的「词库管理」入口调用。
#[tauri::command]
fn show_manager(app: AppHandle) {
    open_manager(&app);
}

/// 前端往诊断日志写一行。发布版看不到 webview 的 console，这个口子用于排查。
#[tauri::command]
fn diag_log(msg: String) {
    log::diag(&format!("[ui] {msg}"));
}

#[tauri::command]
fn active_hotkey(app: AppHandle) -> String {
    app.try_state::<AppState>()
        .and_then(|s| s.hotkey.lock().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

#[tauri::command]
fn target_window(app: AppHandle) -> isize {
    app.try_state::<AppState>()
        .and_then(|s| s.target.lock().ok().map(|t| *t))
        .unwrap_or(0)
}

fn main() {
    // ---- 开发/回归测试入口：`shizi.exe --inject "<文本>"` ----
    // 不创建窗口和托盘，抓当前前台窗口并把文本注入进去，
    // 让注入模块能在没有键盘、没有面板的情况下被自动化测试。
    let args: Vec<String> = std::env::args().collect();
    // 支持两种：`--inject <文本>` 与 `--inject-file <路径>`。
    // 后者让自动化测试只传 ASCII 路径、把载荷写在 UTF-8 文件里，
    // 避开命令行参数编码这个与业务无关的坑。
    let payload = if let Some(i) = args.iter().position(|a| a == "--inject") {
        args.get(i + 1).cloned()
    } else if let Some(i) = args.iter().position(|a| a == "--inject-file") {
        args.get(i + 1).and_then(|f| std::fs::read_to_string(f).ok())
    } else {
        None
    };

    if let Some(text) = payload {
        let target = win32::foreground_window();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        match inject::inject(&text, target, false) {
            Ok(r) => println!(
                "OK method={} focused={} restored={}",
                r.method, r.focused, r.clipboard_restored
            ),
            Err(e) => {
                println!("ERR {e}");
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }

    tauri::Builder::default()
        // ⚠️ 单实例插件必须**第一个**注册：它要在其余初始化之前决定这个进程是走是留。
        //
        // 为什么必须有：没有它，用户双击两次托盘图标就会起两个拾字 ——
        // 两个托盘图标、两个键盘钩子、两份词库连接，两个都可能往目标插文本。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 第二个实例启动时的意图就是"我要打开它"，所以唤出已有窗口即可。
            log::diag("检测到第二个实例启动，唤出已有面板后自身退出");
            toggle_panel(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commit_text,
            hide_panel,
            target_window,
            active_hotkey,
            load_entries,
            show_manager,
            diag_log,
            expand_enabled,
            set_expand_enabled,
            suggest_triggers,
            save_entry,
            delete_entry,
            toggle_entry,
            get_settings,
            set_setting,
            set_hotkey,
            open_data_dir,
            copy_to_clipboard,
            export_pack,
            preview_import,
            apply_import,
            remove_pack
        ])
        .setup(|app| {
            // ---- 词条库（SQLite）----
            // 数据目录放在用户可见的位置（见 user_data_dir 的注释）
            let data_dir = user_data_dir()?;
            let mut conn = store::open(&data_dir).map_err(|e| format!("打开词条库失败: {e}"))?;
            store::sync_builtin_packs(&mut conn).map_err(|e| format!("导入内置包失败: {e}"))?;
            log::diag(&format!(
                "词条库就绪: {} —— {}",
                store::db_path(&data_dir).display(),
                store::summary(&conn)
            ));

            // 开发用：`shizi.exe --dump-entries` 打印库里加载到的内容后退出
            if std::env::args().any(|a| a == "--dump-entries") {
                if let Ok(list) = store::load_all(&conn, false) {
                    for e in list.iter().take(5) {
                        println!("[dump] {} | {} | trigger={}", e.id, e.text, e.trigger);
                    }
                    println!("[dump] 共 {} 条", list.len());
                }
                std::process::exit(0);
            }

            // 用户上次选定的热键要在 conn 被移进 state 之前读出来
            let prefer_hotkey = store::get_setting(&conn, "hotkey").ok().flatten();

            // 首启引导：第一次运行自动打开管理窗口，里面会显示欢迎卡片。
            // 这一次是**故意**抢焦点的 —— 用户需要看到它，否则等于没引导。
            if !get_bool_setting(&conn, "first_run_done", false) {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    open_manager(&h);
                    if let Some(w) = h.get_webview_window("manager") {
                        let _ = w.set_focus();
                    }
                    log::diag("首次运行，已打开欢迎界面");
                });
            }

            // ---- 打字自动展开 ----
            // 必须装在有消息循环的线程上 —— setup 就跑在主线程，正好。
            let triggers = store::trigger_map(&conn).unwrap_or_default();
            match expand::install(triggers) {
                Ok(()) => log::diag(&format!(
                    "打字自动展开已启用：{} 个触发码",
                    expand::trigger_count()
                )),
                Err(e) => log::diag(&format!("打字自动展开安装失败：{e}")),
            }

            app.manage(AppState::default());
            {
                let st = app.state::<AppState>();
                // 先把 Result 绑到变量再匹配：否则临时值会在块尾才析构，借用活不够久
                let locked = st.db.lock();
                if let Ok(mut g) = locked {
                    *g = Some(conn);
                }
            }

            // ---- 托盘 ----
            let open_i = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
            // 打字自动展开的开关。有它是为了「随时能关掉」——
            // 这类会监听键盘的功能，必须给用户一个立刻停下来的出口。
            let expand_i =
                CheckMenuItem::with_id(app, "expand", "打字自动展开", true, true, None::<&str>)?;
            let manage_i = MenuItem::with_id(app, "manage", "管理词库…", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &manage_i, &expand_i, &quit_i])?;

            // 图标：优先进程内已有的窗口图标，否则从编译期嵌入的图标资源取
            let icon = match app.default_window_icon().cloned() {
                Some(i) => i,
                None => tauri::image::Image::from_app_icon_resource(32)?,
            };

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("拾字 · 取词器")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => toggle_panel(app),
                    "manage" => open_manager(app),
                    "expand" => expand::set_enabled(!expand::is_enabled()),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        toggle_panel(tray.app_handle());
                    }
                })
                .build(app)?;

            // 启动自检：窗口创建后不能让「隐身面板」霸占前台，
            // 否则用户一启动拾字就丢焦点，且后续注入会粘回自己。
            {
                let fg = win32::foreground_window();
                log::diag(&format!(
                    "启动后前台窗口 = 0x{:x} class={} 属于自己={}",
                    fg,
                    win32::class_name(fg),
                    win32::is_own_window(fg)
                ));
            }

            // ---- 全局热键：优先用用户上次选定的，否则按候选顺序试 ----
            let chosen = install_hotkey(app.handle(), prefer_hotkey.as_deref());

            match chosen {
                Some(label) => {
                    println!("[拾字] 热键已就绪：{label}");
                    if let Ok(mut h) = app.state::<AppState>().hotkey.lock() {
                        *h = label.to_string();
                    }
                }
                None => {
                    eprintln!(
                        "[拾字] 所有候选热键都被占用。仍可从托盘图标打开面板，\
                         请换一个热键后重启。"
                    );
                }
            }

            // 回归自测：`shizi.exe --inject-test`
            // 完整复现「面板抢走前台 → 隐藏 → 把焦点还给原窗口 → Ctrl+V」这条链路，
            // 也就是最容易静默失败的一步（Windows 前台锁）。自动化测试靠它。
            if std::env::args().any(|a| a == "--inject-test") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    const PAYLOAD: &str = "\u{03B4}\u{00B9}\u{00B3}C\u{20BB7}\u{00B5}\u{2103}";
                    let target = win32::foreground_window();
                    log::diag(&format!("[selftest] 面板出现前的前台窗口 = 0x{:x}", target));
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    toggle_panel(&h);
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    let allowed = win32::allow_set_foreground_any();
                    log::diag(&format!("[selftest] 放开设置前台许可: {}", allowed));
                    if let Some(win) = h.get_webview_window("panel") {
                        let _ = win.hide();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(140));
                    let r = inject::inject(PAYLOAD, target, false);
                    log::diag(&format!("[selftest] 剪贴板注入结果 = {:?}", r));

                    // 决定性探针：剪贴板粘贴失败时，键盘输入本身到底能不能到达目标？
                    // 能到 => 问题在 Ctrl+V/剪贴板；不能到 => 问题在键盘输入通路。
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let fg2 = win32::foreground_window();
                    log::diag(&format!("[selftest] 追加打字前前台=0x{:x}", fg2));
                    let n = win32::type_unicode("ZZZ");
                    log::diag(&format!("[selftest] 追加打字已发码元数={}", n));
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    std::process::exit(0);
                });
            }

            // 自测：`shizi.exe --selftest-manager-close`
            // 复现「关掉管理窗口 → 再打开」这个用户实际踩到的场景。
            if std::env::args().any(|a| a == "--selftest-manager-close") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    let wait = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
                    wait(900);
                    open_manager(&h);
                    wait(1200);
                    // 模拟用户点右上角的 X
                    if let Some(w) = h.get_webview_window("manager") {
                        if let Ok(hwnd) = w.hwnd() {
                            win32::post_close(hwnd.0 as isize);
                        }
                    }
                    wait(1200);
                    let found = h.get_webview_window("manager").is_some();
                    let vis_after_close = h
                        .get_webview_window("manager")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    open_manager(&h);
                    wait(500);
                    let vis_after_reopen = h
                        .get_webview_window("manager")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    log::diag(&format!(
                        "[selftest] 点X之后: 窗口仍存在={found} 可见={vis_after_close} ; 再打开后可见={vis_after_reopen}"
                    ));
                    // 顺便验证我在风险里提过的那条：拦了 CloseRequested 之后
                    // 「退出」还能不能正常退掉。走正常退出流程，而不是强杀。
                    log::diag("[selftest] 调用 app.exit(0) 验证程序仍能正常退出");
                    h.exit(0);
                });
            }

            // 开发用：`shizi.exe --open-data-dir` 直接验证「定位数据目录」这个动作
            if std::env::args().any(|a| a == "--open-data-dir") {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(700));
                    match open_data_dir() {
                        Ok(()) => log::diag("[selftest] open_data_dir 调用成功"),
                        Err(e) => log::diag(&format!("[selftest] open_data_dir 失败: {e}")),
                    }
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    std::process::exit(0);
                });
            }

            // 开发用：`shizi.exe --show-manager` 启动后自动打开词库管理窗口
            if std::env::args().any(|a| a == "--show-manager") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    open_manager(&h);
                });
            }

            // 开发用：`shizi.exe --show-now` 启动后自动弹出面板，便于在没有键盘的环境下验证
            if std::env::args().any(|a| a == "--show-now") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    toggle_panel(&h);
                });
            }

            Ok(())
        })
        .on_window_event(|win, event| {
            // 失焦即隐藏：面板是"随取随用"的，不该赖在屏幕上
            if let tauri::WindowEvent::Focused(false) = event {
                if win.label() == "panel" {
                    let _ = win.hide();
                }
            }

            // 管理窗口点 X 只隐藏、不销毁。
            //
            // 为什么必须拦：Tauri 里关窗口默认**销毁**，而窗口只在启动时创建一次，
            // 销毁之后 `get_webview_window("manager")` 永远返回空 ——
            // 用户点托盘里的「管理词库…」就再也没反应了（只能重启程序）。
            // 注意只拦 manager：拦多了会导致连「退出」都被挡住。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if win.label() == "manager" {
                    api.prevent_close();
                    let _ = win.hide();
                    log::diag("管理窗口已隐藏（未销毁）");
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("拾字初始化失败")
        .run(|_app, event| {
            // 退出时摘掉键盘钩子，别留残留（卸载干净是公开版的要求）
            if let tauri::RunEvent::Exit = event {
                expand::shutdown();
                log::diag("拾字退出，键盘钩子已摘除");
            }
        });
}

#[cfg(test)]
mod suggest_tests {
    use super::suggest_for;

    #[test]
    fn ascii_content_yields_ascii_fragments() {
        let s = suggest_for("(A123×B456)×B456");
        assert!(s.contains(&"a123".to_string()), "缺 ASCII 片段: {s:?}");
        assert!(s.contains(&"b456".to_string()), "缺 ASCII 片段: {s:?}");
        assert!(s.len() <= 6, "建议条数应受限: {s:?}");
        assert!(s.iter().all(|x| x.is_ascii() && x.len() >= 2), "{s:?}");
    }

    #[test]
    fn chinese_content_yields_pinyin() {
        // 肝(g) 豆(d) 状(z) 核(h) 变(b) 性(x)
        // 注：我当初手工写进医学包的是 "gdzxbx" —— 漏了「核」的 h，是错的。
        // 机器从这里开始就是可信来源。
        let s = suggest_for("肝豆状核变性");
        assert!(s.contains(&"gdzhbx".to_string()), "缺拼音首字母: {s:?}");
        assert!(
            s.contains(&"gandouzhuanghebianxing".to_string()),
            "缺全拼: {s:?}"
        );
        let s2 = suggest_for("癥瘕");
        assert!(s2.contains(&"zhengjia".to_string()), "缺全拼: {s2:?}");
    }

    #[test]
    fn useless_content_yields_nothing() {
        assert!(suggest_for("——").is_empty());
        assert!(suggest_for("  ").is_empty());
    }
}
