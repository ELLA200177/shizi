//! 手写 Win32 FFI。刻意不引入 `windows` crate —— 需要的就这几个函数，
//! 自己声明反而没有版本漂移风险，而且结构体布局有单元测试兜底。

#![allow(non_snake_case)]

use std::ffi::c_void;

pub type HWND = *mut c_void;

#[link(name = "user32")]
extern "system" {
    pub fn GetForegroundWindow() -> HWND;
    pub fn SetForegroundWindow(hwnd: HWND) -> i32;
    pub fn IsWindow(hwnd: HWND) -> i32;
    pub fn GetClipboardSequenceNumber() -> u32;
    pub fn SendInput(cinputs: u32, pinputs: *const INPUT, cbsize: i32) -> u32;
    pub fn GetWindowThreadProcessId(hwnd: HWND, lpdwprocessid: *mut u32) -> u32;
    pub fn AttachThreadInput(id_attach: u32, id_attach_to: u32, f_attach: i32) -> i32;
    pub fn IsIconic(hwnd: HWND) -> i32;
    pub fn ShowWindow(hwnd: HWND, ncmdshow: i32) -> i32;
    pub fn keybd_event(bvk: u8, bscan: u8, dwflags: u32, dwextrainfo: usize);
    pub fn AllowSetForegroundWindow(dwprocessid: u32) -> i32;
    pub fn PostMessageW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> i32;
    pub fn GetAsyncKeyState(vkey: i32) -> i16;
    pub fn GetClassNameW(hwnd: HWND, buf: *mut u16, max: i32) -> i32;
}

/// 某个键此刻在系统看来是否处于按下状态（真实物理状态，跨进程有效）。
pub fn key_down(vk: u8) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

/// 某个窗口是否属于当前进程。
///
/// 用途：拾字自己的面板窗口曾经因为启动时被激活而从隐藏状态变成「系统前台窗口」，
/// 导致此后所有注入都粘回自己。捕获目标前必须先排除这种自指。
pub fn is_own_window(hwnd: isize) -> bool {
    let h = hwnd as HWND;
    if h.is_null() {
        return false;
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        pid != 0 && pid == GetCurrentProcessId()
    }
}

/// 前台窗口的窗口类名 —— 用来判断焦点到底落在谁身上（Edit / Chrome_WidgetWin_1 ...）。
pub fn class_name(hwnd: isize) -> String {
    let h = hwnd as HWND;
    if h.is_null() {
        return "(空)".into();
    }
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return "(取不到)".into();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

/// 供诊断输出：Alt/Ctrl/Shift 的当前按下状态。
pub fn modifier_state() -> String {
    format!(
        "Alt={} Ctrl={} Shift={}",
        key_down(VK_MENU),
        key_down(VK_CONTROL as u8),
        key_down(VK_SHIFT)
    )
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentThreadId() -> u32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentProcess() -> *mut c_void;
    pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    pub fn CloseHandle(h: *mut c_void) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    fn GetTokenInformation(token: *mut c_void, class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
}

/// GUITHREADINFO —— 用来查「某个线程里真正持有键盘焦点的控件是谁」。
/// 跨进程 `GetFocus()` 无效（它只看调用线程），必须走这个 API。
#[repr(C)]
pub struct GUITHREADINFO {
    pub cb_size: u32,
    pub flags: u32,
    pub hwnd_active: HWND,
    pub hwnd_focus: HWND,
    pub hwnd_capture: HWND,
    pub hwnd_menu_owner: HWND,
    pub hwnd_move_size: HWND,
    pub hwnd_caret: HWND,
    pub rc_caret: [i32; 4],
}

#[link(name = "user32")]
extern "system" {
    pub fn GetGUIThreadInfo(idthread: u32, pgui: *mut GUITHREADINFO) -> i32;
}

/// 目标窗口所属线程真正持有焦点的那个子控件句柄（拿不到返回 0）。
pub fn focused_child_of(hwnd: isize) -> isize {
    unsafe {
        let h = hwnd as HWND;
        if h.is_null() || IsWindow(h) == 0 {
            return 0;
        }
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(h, &mut pid);
        if tid == 0 {
            return 0;
        }
        let mut info = GUITHREADINFO {
            cb_size: std::mem::size_of::<GUITHREADINFO>() as u32,
            flags: 0,
            hwnd_active: std::ptr::null_mut(),
            hwnd_focus: std::ptr::null_mut(),
            hwnd_capture: std::ptr::null_mut(),
            hwnd_menu_owner: std::ptr::null_mut(),
            hwnd_move_size: std::ptr::null_mut(),
            hwnd_caret: std::ptr::null_mut(),
            rc_caret: [0; 4],
        };
        if GetGUIThreadInfo(tid, &mut info) == 0 {
            return 0;
        }
        info.hwnd_focus as isize
    }
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, pv: *const c_void, cb: u32) -> i32;
}

// ---- 结构体：布局必须与 Win32 一致，见 tests ----

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MOUSEINPUT {
    pub dx: i32,
    pub dy: i32,
    pub mouse_data: u32,
    pub dw_flags: u32,
    pub time: u32,
    pub dw_extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KEYBDINPUT {
    pub w_vk: u16,
    pub w_scan: u16,
    pub dw_flags: u32,
    pub time: u32,
    pub dw_extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HARDWAREINPUT {
    pub u_msg: u32,
    pub w_param_l: u16,
    pub w_param_h: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union INPUT_UNION {
    pub mi: MOUSEINPUT,
    pub ki: KEYBDINPUT,
    pub hi: HARDWAREINPUT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct INPUT {
    pub input_type: u32,
    pub u: INPUT_UNION,
}

pub const INPUT_KEYBOARD: u32 = 1;
pub const KEYEVENTF_KEYUP: u32 = 0x0002;
pub const KEYEVENTF_UNICODE: u32 = 0x0004;

pub const VK_CONTROL: u16 = 0x11;
pub const VK_V: u16 = 0x56;
pub const VK_BACK: u16 = 0x08;
pub const VK_MENU: u8 = 0x12; // Alt
pub const VK_SHIFT_STATE: u8 = 0x10;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_INTEGRITY_LEVEL: u32 = 25;
pub const VK_SHIFT: u8 = 0x10;
pub const SW_RESTORE: i32 = 9;
/// `AllowSetForegroundWindow(ASFW_ANY)` 用的通配进程号
pub const ASFW_ANY: u32 = 0xFFFF_FFFF;
pub const WM_CLOSE: u32 = 0x0010;

/// 给窗口发一条关闭消息 —— 等价于用户点了标题栏右上角的 X。
/// 只用于自测：验证「关掉之后还能不能再打开」。
pub fn post_close(hwnd: isize) {
    unsafe {
        PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
    }
}

/// 放开「设置前台窗口」的许可。
///
/// 必须在**自己的窗口还是前台时**调用 —— Windows 只允许前台进程发放这个许可。
/// 拿到许可后再抢前台就不需要 Alt 那种会污染修饰键状态的脏招了。
pub fn allow_set_foreground_any() -> bool {
    unsafe { AllowSetForegroundWindow(ASFW_ANY) != 0 }
}

pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
pub const DWMWCP_ROUND: u32 = 2;

fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        input_type: INPUT_KEYBOARD,
        u: INPUT_UNION {
            ki: KEYBDINPUT {
                w_vk: vk,
                w_scan: scan,
                dw_flags: flags,
                time: 0,
                dw_extra_info: 0,
            },
        },
    }
}

unsafe fn send(inputs: &[INPUT]) -> u32 {
    SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    )
}

/// 记录当前前台窗口。必须在面板获得焦点**之前**调用。
pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

/// 强制把目标窗口置为前台，并返回「是否成功」与「走了哪条路」。
///
/// 为什么要这么啰嗦：Windows 有「前台锁」——非前台进程调用 `SetForegroundWindow`
/// 大概率被静默拒绝（返回 0）。面板自己就是前台窗口，隐藏它之后要把焦点还给
/// 之前那个程序，正好撞上这条限制。三级退让：
///   1. 直接 `SetForegroundWindow`（最干净）
///   2. `AttachThreadInput` 挂到目标线程上再抢（经典解法）
///   3. 先发一次 Alt 按键解锁前台锁，再抢（另一条经典解法）
pub fn focus_window_force(hwnd: isize) -> (bool, String) {
    let h = hwnd as HWND;
    if h.is_null() {
        return (false, "目标句柄为空".into());
    }
    unsafe {
        if IsWindow(h) == 0 {
            return (false, "目标窗口已不存在".into());
        }
        let mut steps = String::new();
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
            steps.push_str("还原最小化; ");
        }
        if SetForegroundWindow(h) != 0 && GetForegroundWindow() == h {
            return (true, steps + "直接 SetForegroundWindow");
        }
        steps.push_str("直接失败; ");

        let cur = GetCurrentThreadId();
        let mut tmp = 0u32;
        let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), &mut tmp);
        let t_tid = GetWindowThreadProcessId(h, &mut tmp);
        if fg_tid != 0 && t_tid != 0 {
            AttachThreadInput(cur, t_tid, 1);
            AttachThreadInput(cur, fg_tid, 1);
            let ok = SetForegroundWindow(h) != 0 && GetForegroundWindow() == h;
            AttachThreadInput(cur, fg_tid, 0);
            AttachThreadInput(cur, t_tid, 0);
            if ok {
                return (true, steps + "AttachThreadInput");
            }
            steps.push_str("AttachThreadInput 失败; ");
        }

        keybd_event(VK_MENU, 0, 0, 0);
        keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0);
        // 必须清扫修饰键状态：keybd_event 注入的 Alt 会把逻辑状态卡住，
        // 之后的 Ctrl+V 会变成 Ctrl+Alt+V —— 表现就是「焦点对了但粘贴不动」。
        keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_SHIFT, 0, KEYEVENTF_KEYUP, 0);
        if SetForegroundWindow(h) != 0 && GetForegroundWindow() == h {
            return (true, steps + "Alt 解锁(已清扫修饰键)");
        }
        steps.push_str("Alt 解锁失败");
        (false, steps)
    }
}

pub fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// 模拟一次退格。
pub fn send_backspace() {
    let down = [key_input(VK_BACK, 0, 0)];
    let up = [key_input(VK_BACK, 0, KEYEVENTF_KEYUP)];
    unsafe {
        send(&down);
        send(&up);
    }
}

/// 取某个进程令牌里的完整性级别 RID（最后一级子授权值）。
fn integrity_rid(process: *mut c_void) -> Option<u32> {
    unsafe {
        let mut token: *mut c_void = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut len: u32 = 0;
        GetTokenInformation(token, TOKEN_INTEGRITY_LEVEL, std::ptr::null_mut(), 0, &mut len);
        if len == 0 {
            CloseHandle(token);
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let ok = GetTokenInformation(
            token,
            TOKEN_INTEGRITY_LEVEL,
            buf.as_mut_ptr() as *mut c_void,
            len,
            &mut len,
        );
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        // TOKEN_MANDATORY_LABEL { SID_AND_ATTRIBUTES { PSID Sid; DWORD Attributes } }
        let sid = *(buf.as_ptr() as *const *const u8);
        if sid.is_null() {
            return None;
        }
        // SID 布局：Revision(1) SubAuthorityCount(1) IdentifierAuthority(6) SubAuthority[]
        let count = *sid.add(1) as usize;
        if count == 0 {
            return None;
        }
        let base = sid.add(8) as *const u32;
        Some(*base.add(count - 1))
    }
}

/// 目标窗口的权限是否高于本进程。
///
/// 为什么必须有这个检查：打字展开是「先删掉触发码、再插入替换文本」。
/// 若目标进程权限更高，`SendInput` 会被 UIPI 拦掉 —— 于是**删掉了却插不进去**，
/// 用户的输入就被我们弄丢了。宁可什么都不做。
pub fn is_higher_integrity(hwnd: isize) -> bool {
    unsafe {
        let target = hwnd as HWND;
        if target.is_null() || IsWindow(target) == 0 {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(target, &mut pid);
        if pid == 0 {
            return false;
        }
        let mine = integrity_rid(GetCurrentProcess());
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // 打不开对方的进程，通常是权限更高
            return true;
        }
        let theirs = integrity_rid(h);
        CloseHandle(h);
        match (mine, theirs) {
            (Some(a), Some(b)) => b > a,
            _ => false,
        }
    }
}

/// 模拟一次 Ctrl+V。
pub fn send_ctrl_v() -> u32 {
    let down = [
        key_input(VK_CONTROL, 0, 0),
        key_input(VK_V, 0, 0),
    ];
    let up = [
        key_input(VK_V, 0, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ];
    let a = unsafe { send(&down) };
    std::thread::sleep(std::time::Duration::from_millis(18));
    let b = unsafe { send(&up) };
    a.min(b)
}

/// 用 KEYEVENTF_UNICODE 逐「UTF-16 码元」输入。
///
/// 注意：必须是码元（u16）而不是码点（char）。生僻字如 𠮷（U+20BB7）在 UTF-16 里
/// 占两个码元（代理对），按码点发送会变成乱码。
pub fn type_unicode(text: &str) -> u32 {
    let mut sent = 0;
    for unit in text.encode_utf16() {
        let down = [key_input(0, unit, KEYEVENTF_UNICODE)];
        let up = [key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)];
        unsafe {
            send(&down);
            send(&up);
        }
        sent += 2;
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    sent
}

/// 请求 Windows 11 给窗口四角做圆角。
pub fn set_rounded_corners(hwnd: isize) {
    let pref: u32 = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这几个断言是手写 FFI 的安全网：布局一旦错位，
    /// SendInput 会静默失败（返回 0），很难排查。
    #[test]
    fn input_layout_matches_win32() {
        assert_eq!(std::mem::size_of::<KEYBDINPUT>(), 24);
        assert_eq!(std::mem::size_of::<MOUSEINPUT>(), 32);
        assert_eq!(std::mem::size_of::<INPUT_UNION>(), 32);
        // 64 位下 INPUT = 4(type) + 4(padding) + 32(union)
        assert_eq!(std::mem::size_of::<INPUT>(), 40);
    }

    /// 生僻字必须拆成两个 UTF-16 码元，这是 A 类词的关键路径。
    #[test]
    fn astral_char_is_two_code_units() {
        let units: Vec<u16> = "\u{20BB7}".encode_utf16().collect();
        assert_eq!(units, vec![0xD842, 0xDFB7]);
    }

    /// 运行时验证：操作系统是否接受我们构造的 INPUT。
    /// 布局错了 SendInput 会返回 0 —— 但这条会真的往当前焦点窗口发一次 Ctrl+V，
    /// 所以默认 #[ignore]。需要时跑：
    ///     cargo test -- --ignored send_input
    #[test]
    #[ignore]
    fn send_input_accepted_by_os() {
        assert!(send_ctrl_v() > 0, "SendInput 返回 0，结构体布局可能不对");
    }

    #[test]
    fn cjk_and_invisible_are_single_units() {
        assert_eq!("\u{9F8B}".encode_utf16().count(), 1); // 龋
        assert_eq!("\u{00A0}".encode_utf16().count(), 1); // NBSP
        assert_eq!("\u{03B4}\u{00B9}\u{00B3}C".encode_utf16().count(), 4); // δ¹³C
    }
}
