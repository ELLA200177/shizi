//! 文本注入。
//!
//! 主路径「剪贴板法」：写剪贴板 → 强制抢回焦点 → 模拟 Ctrl+V → 按序列号决定是否还原剪贴板。
//! 之所以不用逐字符发送：剪贴板法对 Electron / 浏览器 / WPS 全部有效，且 Unicode 无限制。
//! 逐字符（`type_unicode`）作为剪贴板不可用或 SendInput 被拒时的兜底。
//!
//! 焦点这一步是整条链路最容易静默失败的地方（Windows 前台锁），
//! 所以全程写诊断日志，见 `log.rs`。

use crate::log::diag;
use crate::win32;
use std::time::Duration;

/// 抢到焦点后等目标程序稳定下来。
const FOCUS_SETTLE_MS: u64 = 90;
/// 粘贴后等目标程序处理完。
const PASTE_SETTLE_MS: u64 = 140;

#[derive(Debug, serde::Serialize)]
pub struct InjectResult {
    pub method: String,
    pub clipboard_restored: bool,
    pub focused: bool,
    /// 抢焦点走的是哪条路（直接 / AttachThreadInput / Alt 解锁）
    pub focus_path: String,
}

/// `pre_focused`：调用方是否已经在隐藏面板之前把前台让给了目标。
/// 传 true 就不再重复抢焦点 —— 那时我们的进程已经不是前台，
/// 再抢会失败并触发 Alt 解锁，反而污染修饰键状态。
pub fn inject(text: &str, target: isize, pre_focused: bool) -> Result<InjectResult, String> {
    diag(&format!(
        "inject 开始: {} 字符, target=0x{:x}",
        text.chars().count(),
        target
    ));

    let mut clip = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板: {e}"))?;
    let backup = clip.get_text().ok();

    if clip.set_text(text.to_string()).is_err() {
        let (focused, focus_path) = if pre_focused {
            (true, "调用方已让出前台".to_string())
        } else {
            win32::focus_window_force(target)
        };
        diag(&format!(
            "剪贴板写入失败 -> unicode 兜底, focused={} ({})",
            focused, focus_path
        ));
        std::thread::sleep(Duration::from_millis(FOCUS_SETTLE_MS));
        let n = win32::type_unicode(text);
        diag(&format!("type_unicode 已发码元数={}", n));
        return Ok(InjectResult {
            method: "unicode".into(),
            clipboard_restored: false,
            focused,
            focus_path,
        });
    }
    let seq_after_set = win32::clipboard_seq();

    let (focused, focus_path) = if pre_focused {
        (true, "调用方已让出前台".to_string())
    } else {
        let r = win32::focus_window_force(target);
        diag(&format!("抢焦点结果: {} ({})", r.0, r.1));
        r
    };
    std::thread::sleep(Duration::from_millis(FOCUS_SETTLE_MS));

    let fg = win32::foreground_window();
    // 关键探针：目标窗口内部到底是谁持有键盘焦点？
    // 强制抢窗口焦点不等于恢复它的子控件焦点 —— 这是第二层坑。
    let child = win32::focused_child_of(target);
    diag(&format!(
        "粘贴前: 前台=0x{:x} 目标=0x{:x} {} ; 目标内部焦点控件=0x{:x} {}",
        fg,
        target,
        if fg == target { "一致" } else { "不一致!" },
        child,
        if child == 0 { "(无! 粘贴会丢)" } else { "" }
    ));

    diag(&format!(
        "发 Ctrl+V 之前: {} ; 前台 class={}",
        win32::modifier_state(),
        win32::class_name(fg)
    ));
    let sent = win32::send_ctrl_v();
    diag(&format!("SendInput(Ctrl+V) 返回={}", sent));
    if sent == 0 {
        let n = win32::type_unicode(text);
        diag(&format!("SendInput 被拒 -> unicode 兜底, 码元数={}", n));
        std::thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));
        return Ok(InjectResult {
            method: "unicode-fallback".into(),
            clipboard_restored: false,
            focused,
            focus_path,
        });
    }
    std::thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));

    // 只有剪贴板仍是我们的内容时才还原；若已被外部改动，放弃还原（别覆盖用户的复制）
    let mut restored = false;
    if win32::clipboard_seq() == seq_after_set {
        if let Some(b) = backup {
            if clip.set_text(b).is_ok() {
                restored = true;
            }
        }
    }

    diag(&format!("完成: method=clipboard 剪贴板已还原={}", restored));
    Ok(InjectResult {
        method: "clipboard".into(),
        clipboard_restored: restored,
        focused,
        focus_path,
    })
}

/// 打字自动展开专用：目标窗口**已经**是前台，只需「删掉 N 个字符 + 粘贴替换文本」。
///
/// 与 `inject()` 的区别：这里**不动焦点**（焦点本来就是对的），
/// 也不做完整性级别检查 —— 那一步由调用方 `expand::run_expand` 提前做完。
pub fn expand(delete_chars: usize, text: &str) -> Result<(), String> {
    let mut clip = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板: {e}"))?;
    let backup = clip.get_text().ok();

    if clip.set_text(text.to_string()).is_err() {
        diag("expand: 剪贴板被占用，改用逐字符注入");
        for _ in 0..delete_chars {
            win32::send_backspace();
            std::thread::sleep(Duration::from_millis(8));
        }
        win32::type_unicode(text);
        return Ok(());
    }
    let seq_after_set = win32::clipboard_seq();

    // 先删触发码。每步留一点间隔：中文输入法的组字缓冲会先吃掉几个退格，
    // 一次性灌进去容易和目标程序的处理节奏错位。
    for _ in 0..delete_chars {
        win32::send_backspace();
        std::thread::sleep(Duration::from_millis(8));
    }
    std::thread::sleep(Duration::from_millis(20));

    let sent = win32::send_ctrl_v();
    if sent == 0 {
        diag("expand: SendInput(Ctrl+V) 返回 0，改用逐字符注入");
        win32::type_unicode(text);
    }
    std::thread::sleep(Duration::from_millis(130));

    let mut restored = false;
    if win32::clipboard_seq() == seq_after_set {
        if let Some(b) = backup {
            if clip.set_text(b).is_ok() {
                restored = true;
            }
        }
    }
    diag(&format!(
        "expand 完成: 删除 {delete_chars} 字符, 剪贴板还原={restored}"
    ));
    Ok(())
}
