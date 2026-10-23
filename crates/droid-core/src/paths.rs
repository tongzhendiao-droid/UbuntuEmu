//! 文件路径。用户目录按 XDG 规范取值，与 Waydroid 自己的取法一致。

use std::env;
use std::path::PathBuf;

pub const WAYDROID_CFG: &str = "/var/lib/waydroid/waydroid.cfg";
pub const WAYDROID_DEFAULT_IMAGES: &str = "/var/lib/waydroid/images";
/// Waydroid 从这个 DHCP 租约文件里读安卓的 IP 地址。
pub const DNSMASQ_LEASES: &str = "/var/lib/misc/dnsmasq.waydroid0.leases";

pub fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home_dir().join(fallback),
    }
}

pub fn data_home() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share")
}

pub fn cache_home() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache")
}

pub fn config_home() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// 用户偏好。
pub fn config_file() -> PathBuf {
    config_home().join("droid").join("config.json")
}

/// Waydroid 为每个安卓应用生成的 .desktop 文件放在这里。
pub fn applications_dir() -> PathBuf {
    data_home().join("applications")
}

/// 后台运行的 waydroid 进程把输出追加到这里，排查问题时看。
pub fn log_file() -> PathBuf {
    cache_home().join("droid").join("waydroid.log")
}
