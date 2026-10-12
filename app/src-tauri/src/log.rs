//! 极简诊断日志。
//!
//! 为什么需要它：GUI 版没有可见的控制台，`println!` 等于扔掉。
//! 注入失败时最需要知道的恰恰是「焦点给到谁了、SendInput 返回了什么」，
//! 所以写到一个固定文件里，便于事后排查。
//! 可用环境变量 SHIZI_DIAG 覆盖路径。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn path() -> PathBuf {
    if let Ok(p) = std::env::var("SHIZI_DIAG") {
        return PathBuf::from(p);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("shizi-diag.log")))
        .unwrap_or_else(|| std::env::temp_dir().join("shizi-diag.log"))
}

pub fn diag(msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path()) {
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
    println!("[拾字] {}", msg);
}
