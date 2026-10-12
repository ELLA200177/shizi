//! 打字自动展开（expand-on-type）
//!
//! 三条设计要点，都是踩过坑才定的：
//!
//! 1. **匹配用物理按键，绝不调 `ToUnicodeEx`。**
//!    中文输入法激活时，`;` 会变成全角 `；`、`m`/`u` 会进输入法的组字缓冲；
//!    但物理键码始终是 `VK_OEM_1` 和 `'M'`/`'U'`，照样能匹配上。
//!    反过来，调 `ToUnicodeEx` 会把按键**喂给输入法转换器**、污染它的状态。
//!    删除也不必担心：输入法把退格解释为「删掉组字里最后一个字符」，所以退格同样生效。
//! 2. **忽略一切注入按键（`LLKHF_INJECTED`）。** 这一条同时做到两件事：
//!    我们自己注入的按键不会再被匹配（天然防递归），别的工具注入的也不会被误吃。
//! 3. **钩子回调里绝不做重活。** 回调阻塞超过约 1 秒，Windows 会直接把钩子摘掉，
//!    整个功能静默失效。所以命中后只丢一个线程去做删除与粘贴。
//!
//! 副作用：因为第 2 条，**本功能无法用程序注入按键来自测**。
//! 纯逻辑有单元测试（键映射、尾匹配），真实打字行为只能人工验。

use crate::inject;
use crate::log::diag;
use crate::win32;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

type HookProc = extern "system" fn(i32, usize, isize) -> isize;

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        idhook: i32,
        lpfn: HookProc,
        hmod: *mut c_void,
        dwthreadid: u32,
    ) -> *mut c_void;
    fn UnhookWindowsHookEx(hhk: *mut c_void) -> i32;
    fn CallNextHookEx(hhk: *mut c_void, ncode: i32, wparam: usize, lparam: isize) -> isize;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

const WH_KEYBOARD_LL: i32 = 13;
const WM_KEYDOWN: usize = 0x0100;
const WM_SYSKEYDOWN: usize = 0x0104;
/// 该按键是程序注入的（不是用户物理按下）
const LLKHF_INJECTED: u32 = 0x0000_0010;

#[repr(C)]
struct KbdLlHookStruct {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

/// 缓冲区上限。取「最长触发码 + 余量」，避免无限增长。
const BUF_LIMIT: usize = 32;

struct Engine {
    buf: String,
    /// 触发码 → 替换文本
    map: HashMap<String, String>,
    enabled: bool,
}

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
/// 当前钩子句柄，0 表示未安装
static HOOK: AtomicUsize = AtomicUsize::new(0);

fn engine() -> Option<&'static Mutex<Engine>> {
    ENGINE.get()
}

// ---------------- 纯逻辑（可单测） ----------------

/// 物理键 + Shift 状态 → 字符。
///
/// 只覆盖触发码用得到的 ASCII 集合：`;`、数字、字母。
/// 触发码一律小写，所以字母不区分大小写地归一为小写。
fn vk_to_char(vk: u32, shift: bool) -> Option<char> {
    match vk {
        0x30..=0x39 => Some((b'0' + (vk - 0x30) as u8) as char),
        0x41..=0x5A => Some((b'a' + (vk - 0x41) as u8) as char),
        0xBA => Some(if shift { ':' } else { ';' }),
        0xBB => Some('='),
        0xBC => Some(','),
        0xBD => Some('-'),
        0xBE => Some('.'),
        0xBF => Some('/'),
        _ => None,
    }
}

/// 缓冲区末尾是否命中某个触发码。返回 (触发码, 替换文本)。
///
/// 从最长可能开始往前试，保证 `;x` 和 `;xila` 这类前缀关系里优先命中更长的那个。
fn match_tail(buf: &str, map: &HashMap<String, String>) -> Option<(String, String)> {
    let max = buf.len().min(BUF_LIMIT);
    for len in (2..=max).rev() {
        let start = buf.len() - len;
        if !buf.is_char_boundary(start) {
            continue;
        }
        let tail = &buf[start..];
        if let Some(text) = map.get(tail) {
            return Some((tail.to_string(), text.clone()));
        }
    }
    None
}

// ---------------- 钩子 ----------------

extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    // 回调里任何 panic 跨 FFI 边界都是灾难，所以只做最小的事
    if code >= 0 && (wparam == WM_KEYDOWN || wparam == WM_SYSKEYDOWN) && lparam != 0 {
        unsafe {
            let kb = &*(lparam as *const KbdLlHookStruct);
            if kb.flags & LLKHF_INJECTED == 0 {
                on_key(kb.vk_code);
            }
        }
    }
    // LL 钩子允许传 NULL 句柄
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn on_key(vk: u32) {
    let Some(eng) = engine() else { return };
    let Ok(mut e) = eng.lock() else { return };
    if !e.enabled {
        return;
    }

    let shift = win32::key_down(win32::VK_SHIFT_STATE);
    match vk_to_char(vk, shift) {
        Some(c) => {
            e.buf.push(c);
            if e.buf.len() > BUF_LIMIT {
                let cut = e.buf.len() - BUF_LIMIT;
                e.buf.drain(..cut);
            }
            if let Some((trigger, text)) = match_tail(&e.buf, &e.map) {
                e.buf.clear();
                drop(e); // 先解锁再开线程
                std::thread::spawn(move || run_expand(trigger, text));
            }
        }
        // 非字符键（退格、方向键、回车…）会打断触发码，直接清空
        None => e.buf.clear(),
    }
}

fn run_expand(trigger: String, text: String) {
    let target = win32::foreground_window();
    if win32::is_own_window(target) {
        diag("[展开] 前台是拾字自己，跳过");
        return;
    }
    // 目标权限更高时**一个字符都不能删** —— 否则删掉触发码却插不进去，等于弄丢用户输入
    if win32::is_higher_integrity(target) {
        diag("[展开] 目标窗口权限更高，跳过（不做任何删除）");
        return;
    }

    // 等最后一击真正落到目标程序里，否则退格会少删一个字符
    std::thread::sleep(std::time::Duration::from_millis(40));

    let n = trigger.chars().count();
    match inject::expand(n, &text) {
        Ok(()) => diag(&format!(
            "[展开] {trigger} → {} 字符",
            text.chars().count()
        )),
        Err(e) => diag(&format!("[展开] {trigger} 失败: {e}")),
    }
}

// ---------------- 对外接口 ----------------

pub fn install(map: HashMap<String, String>) -> Result<(), String> {
    let _ = ENGINE.set(Mutex::new(Engine {
        buf: String::with_capacity(BUF_LIMIT),
        map,
        enabled: true,
    }));
    install_hook()
}

fn install_hook() -> Result<(), String> {
    if HOOK.load(Ordering::Relaxed) != 0 {
        return Ok(()); // 已安装
    }
    let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, hook_proc, hmod, 0) };
    if hook.is_null() {
        return Err(format!(
            "SetWindowsHookExW 失败: {}",
            std::io::Error::last_os_error()
        ));
    }
    HOOK.store(hook as usize, Ordering::Relaxed);
    Ok(())
}

fn remove_hook() {
    let h = HOOK.swap(0, Ordering::Relaxed);
    if h != 0 {
        unsafe {
            UnhookWindowsHookEx(h as *mut c_void);
        }
    }
}

/// 开关展开。
///
/// 关掉时**把钩子整个摘掉**，而不是留在那儿靠回调里判断 ——
/// 这样「关闭」才是真的不再监听键盘（见公开版的隐私说明）。
/// 词条改动后热更新触发码表 —— 不需要重启。
pub fn set_triggers(map: HashMap<String, String>) {
    if let Some(eng) = engine() {
        if let Ok(mut e) = eng.lock() {
            e.map = map;
            e.buf.clear();
        }
    }
    diag(&format!("打字自动展开：触发码表已更新为 {} 个", trigger_count()));
}

pub fn set_enabled(on: bool) {
    if let Some(eng) = engine() {
        if let Ok(mut e) = eng.lock() {
            e.enabled = on;
            e.buf.clear();
        }
    }
    if on {
        match install_hook() {
            Ok(()) => diag("打字自动展开：已开启（键盘钩子已安装）"),
            Err(e) => diag(&format!("打字自动展开：开启失败 —— {e}")),
        }
    } else {
        remove_hook();
        diag("打字自动展开：已关闭（键盘钩子已摘除，不再监听键盘）");
    }
}

/// 程序退出时收拾干净。
pub fn shutdown() {
    remove_hook();
}

pub fn is_enabled() -> bool {
    engine()
        .and_then(|e| e.lock().ok().map(|g| g.enabled))
        .unwrap_or(false)
}

pub fn trigger_count() -> usize {
    engine()
        .and_then(|e| e.lock().ok().map(|g| g.map.len()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn vk_mapping_covers_ascii_triggers() {
        // 中文输入法下 `;` 会显示成全角，但物理键码不变 —— 这是能服务中文场景的关键
        assert_eq!(vk_to_char(0xBA, false), Some(';'));
        assert_eq!(vk_to_char(0xBA, true), Some(':'));
        assert_eq!(vk_to_char(0x4D, false), Some('m'));
        assert_eq!(vk_to_char(0x4D, true), Some('m'), "字母统一归一为小写");
        assert_eq!(vk_to_char(0x31, false), Some('1'));
        assert_eq!(vk_to_char(0x0D, false), None, "回车不是字符键");
        assert_eq!(vk_to_char(0x08, false), None, "退格应当打断触发码");
    }

    #[test]
    fn match_tail_matches_at_end_only() {
        let map = m(&[(";mu", "μ"), (";kx", "(A×B)×B")]);
        assert_eq!(match_tail(";mu", &map), Some((";mu".into(), "μ".into())));
        assert_eq!(match_tail("abc;mu", &map), Some((";mu".into(), "μ".into())));
        assert_eq!(match_tail(";m", &map), None);
        assert_eq!(match_tail(";mux", &map), None, "后面还有字符就不算命中");
        assert_eq!(match_tail("", &map), None);
    }

    #[test]
    fn longer_trigger_wins_over_prefix() {
        // ;x 与 ;xila 同时存在时，必须优先命中更长的那个
        let map = m(&[(";x", "×"), (";xila", "α β γ")]);
        assert_eq!(match_tail(";x", &map), Some((";x".into(), "×".into())));
        assert_eq!(
            match_tail(";xila", &map),
            Some((";xila".into(), "α β γ".into()))
        );
    }
}
