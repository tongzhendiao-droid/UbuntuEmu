//! 环境诊断：逐项检查运行 Waydroid 需要的条件，给出结论和修复建议。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::error::ErrorCode;
use crate::process;
use crate::waydroid::{self, Status};

const NVIDIA_HINT: &str = "改用软件渲染：在 /var/lib/waydroid/waydroid_base.prop 里加上 ro.hardware.gralloc=default 和 ro.hardware.egl=swiftshader，然后重启会话";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// 稳定的检查项标识，脚本可以按它取结果。
    pub id: &'static str,
    pub title: &'static str,
    pub level: Level,
    pub summary: String,
    pub hint: Option<String>,
}

impl Check {
    fn new(id: &'static str, title: &'static str, level: Level, summary: impl Into<String>) -> Self {
        Self { id, title, level, summary: summary.into(), hint: None }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// 有 error 就是 error，否则有 warning 就是 warning，否则是 ok。
    pub overall: Level,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gpu {
    /// 渲染节点，例如 renderD128。
    pub node: String,
    /// 内核驱动，例如 amdgpu、i915、nvidia。
    pub driver: String,
    pub name: String,
}

pub fn run() -> Report {
    let status = waydroid::status();
    let checks = vec![
        desktop_session(),
        kernel_binder(),
        waydroid_installed(&status),
        android_images(&status),
        container_service(&status),
        graphics(),
        clipboard(),
        input_method(),
        multi_window(&status),
        keylayout(),
        adb(),
    ];
    let overall = if checks.iter().any(|check| check.level == Level::Error) {
        Level::Error
    } else if checks.iter().any(|check| check.level == Level::Warning) {
        Level::Warning
    } else {
        Level::Ok
    };
    Report { overall, checks }
}

fn desktop_session() -> Check {
    const ID: &str = "desktop_session";
    const TITLE: &str = "桌面会话";
    let kind = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktop = desktop.rsplit(':').next().filter(|name| !name.is_empty()).unwrap_or("未知桌面");
    match kind.as_str() {
        "wayland" => {
            let socket = wayland_socket();
            if socket.exists() {
                Check::new(ID, TITLE, Level::Ok, format!("Wayland（{desktop}）"))
            } else {
                Check::new(ID, TITLE, Level::Error, format!("找不到 Wayland 显示接口 {}", socket.display()))
                    .hint("请在图形桌面里的终端运行，不要通过 sudo 或 SSH")
            }
        }
        "x11" => Check::new(
            ID,
            TITLE,
            Level::Error,
            format!("当前是 X11 会话（{desktop}），Waydroid 只能在 Wayland 会话里运行"),
        )
        .hint("注销后在登录界面选择 Wayland 会话（Ubuntu 默认就是 Wayland）"),
        "" => Check::new(ID, TITLE, Level::Warning, "无法判断会话类型（没有设置 XDG_SESSION_TYPE）")
            .hint("请在图形桌面里的终端运行"),
        other => Check::new(ID, TITLE, Level::Warning, format!("会话类型是 {other}，Waydroid 需要 Wayland")),
    }
}

/// 与 Waydroid 一致：WAYLAND_DISPLAY 没设置时默认用 wayland-0。
fn wayland_socket() -> PathBuf {
    let display = env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|display| !display.is_empty())
        .unwrap_or_else(|| "wayland-0".into());
    if Path::new(&display).is_absolute() {
        return PathBuf::from(display);
    }
    env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_default().join(display)
}

fn kernel_binder() -> Check {
    const ID: &str = "kernel_binder";
    const TITLE: &str = "内核 binder";
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| release.trim().to_string())
        .unwrap_or_else(|_| "未知".into());
    let filesystems = fs::read_to_string("/proc/filesystems").unwrap_or_default();
    let enabled = filesystems.lines().any(|line| line.split_whitespace().last() == Some("binder"))
        || Path::new("/dev/binder").exists();
    if enabled {
        Check::new(ID, TITLE, Level::Ok, format!("内核 {kernel}，binder 已启用"))
    } else if kernel_has_module(&kernel, "binder_linux") {
        Check::new(ID, TITLE, Level::Ok, format!("内核 {kernel} 带 binder 模块，Waydroid 启动时会自动加载"))
    } else {
        Check::new(ID, TITLE, Level::Error, format!("内核 {kernel} 没有 binder 支持"))
            .hint("先试 sudo modprobe binder_linux；提示找不到模块时安装 linux-modules-extra-$(uname -r)")
    }
}

fn kernel_has_module(kernel: &str, module: &str) -> bool {
    let dir = Path::new("/lib/modules").join(kernel);
    let needle = format!("/{module}.ko");
    ["modules.dep", "modules.builtin"]
        .iter()
        .any(|file| fs::read_to_string(dir.join(file)).is_ok_and(|text| text.contains(&needle)))
}

fn waydroid_installed(status: &Status) -> Check {
    const ID: &str = "waydroid";
    const TITLE: &str = "Waydroid";
    if status.installed {
        let version = status.version.as_deref().unwrap_or("（版本未知）");
        Check::new(ID, TITLE, Level::Ok, format!("已安装 {version}"))
    } else {
        Check::new(ID, TITLE, Level::Error, "没有安装 Waydroid").hint(
            "按 https://docs.waydro.id/usage/install-on-desktops 添加官方软件源，然后 sudo apt install waydroid",
        )
    }
}

fn android_images(status: &Status) -> Check {
    const ID: &str = "android_images";
    const TITLE: &str = "安卓镜像";
    if !status.installed {
        return Check::new(ID, TITLE, Level::Info, "需要先安装 Waydroid");
    }
    if !status.initialized {
        return Check::new(ID, TITLE, Level::Error, "还没有下载安卓镜像")
            .hint("sudo waydroid init；需要 Google 服务时用 sudo waydroid init -s GAPPS");
    }
    let vendor = status.vendor_type.as_deref().unwrap_or("未知类型");
    let size = images_size_gb().map(|gb| format!("，共 {gb:.1} GB")).unwrap_or_default();
    Check::new(ID, TITLE, Level::Ok, format!("已下载（{vendor}{size}）"))
}

fn images_size_gb() -> Option<f64> {
    let dir = waydroid::images_dir(&waydroid::config()?);
    let bytes: u64 = ["system.img", "vendor.img"]
        .iter()
        .filter_map(|image| fs::metadata(dir.join(image)).ok())
        .map(|meta| meta.len())
        .sum();
    (bytes > 0).then(|| bytes as f64 / 1e9)
}

fn container_service(status: &Status) -> Check {
    const ID: &str = "container_service";
    const TITLE: &str = "容器服务";
    if !status.installed {
        return Check::new(ID, TITLE, Level::Info, "需要先安装 Waydroid");
    }
    match &status.error {
        // Waydroid 1.6.3 的容器服务每次启停会话都会泄漏文件句柄，默认上限 1024，
        // 启停十来次后就会报 Too many open files，再也启动不了会话。
        None if container_fd_limit().is_some_and(|limit| limit <= 4096) => Check::new(
            ID,
            TITLE,
            Level::Warning,
            "可以连接，但文件句柄上限偏低：Waydroid 每次启停会话都会泄漏句柄，多次之后会话会启动失败",
        )
        .hint("sudo systemctl edit waydroid-container，加上 [Service] 和 LimitNOFILE=65536 两行后重启服务；临时办法是 sudo systemctl restart waydroid-container"),
        None => Check::new(ID, TITLE, Level::Ok, "可以连接（需要时由系统自动启动）"),
        Some(err) if err.code == ErrorCode::NotInitialized => {
            Check::new(ID, TITLE, Level::Info, "可以连接，但要先下载安卓镜像")
        }
        Some(err) => Check::new(ID, TITLE, Level::Error, err.message.clone())
            .hint("sudo systemctl enable --now waydroid-container"),
    }
}

/// 容器服务的文件句柄软上限（systemd 的 LimitNOFILESoft）。
fn container_fd_limit() -> Option<u64> {
    let output = process::run(
        Command::new("systemctl").args(["show", "--property=LimitNOFILESoft", "--value", "waydroid-container"]),
        Duration::from_secs(5),
    )
    .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn graphics() -> Check {
    const ID: &str = "graphics";
    const TITLE: &str = "显卡";
    let gpus = gpus();
    if gpus.is_empty() {
        return Check::new(ID, TITLE, Level::Warning, "没有找到显卡渲染节点（/dev/dri/renderD*），只能软件渲染")
            .hint(NVIDIA_HINT);
    }
    let configured = waydroid::config().and_then(|config| config.get("drm_device").cloned());
    match pick_gpu(&gpus, configured.as_deref()) {
        Some(gpu) => {
            let mut summary = format!("Waydroid 会使用 {}（{}，{}）", gpu.name, gpu.driver, gpu.node);
            if let Some(nvidia) = gpus.iter().find(|gpu| gpu.driver == "nvidia") {
                summary.push_str(&format!("；{} 用的是 NVIDIA 专有驱动，Waydroid 不支持，已自动跳过", nvidia.name));
            }
            Check::new(ID, TITLE, Level::Ok, summary)
        }
        None => Check::new(ID, TITLE, Level::Warning, "只有 NVIDIA 专有驱动的显卡，Waydroid 用不了 GPU 加速")
            .hint(NVIDIA_HINT),
    }
}

/// 列出所有显卡渲染节点。
pub fn gpus() -> Vec<Gpu> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut nodes: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("renderD"))
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .map(|node| {
            let device = Path::new("/sys/class/drm").join(&node).join("device");
            let uevent = fs::read_to_string(device.join("uevent")).unwrap_or_default();
            let field = |key: &str| uevent.lines().find_map(|line| line.strip_prefix(key)).map(str::to_string);
            let vendor_id = fs::read_to_string(device.join("vendor")).unwrap_or_default();
            let vendor = vendor_name(vendor_id.trim());
            let model = field("PCI_SLOT_NAME=").and_then(|slot| lspci_device(&slot));
            let name = match model {
                Some(model) if !vendor.is_empty() => format!("{vendor} {model}"),
                Some(model) => model,
                None if !vendor.is_empty() => format!("{vendor} 显卡"),
                None => "未知显卡".to_string(),
            };
            Gpu { node, driver: field("DRIVER=").unwrap_or_else(|| "未知".into()), name }
        })
        .collect()
}

/// 与 Waydroid 的选择逻辑一致：优先用配置里的 drm_device，否则用第一个不是 NVIDIA 专有驱动的节点。
fn pick_gpu<'a>(gpus: &'a [Gpu], configured: Option<&str>) -> Option<&'a Gpu> {
    match configured.filter(|device| !device.is_empty()) {
        Some(device) => {
            let node = Path::new(device).file_name()?.to_str()?;
            gpus.iter().find(|gpu| gpu.node == node && gpu.driver != "nvidia")
        }
        None => gpus.iter().find(|gpu| gpu.driver != "nvidia"),
    }
}

fn vendor_name(pci_vendor_id: &str) -> &'static str {
    match pci_vendor_id {
        "0x10de" => "NVIDIA",
        "0x1002" => "AMD",
        "0x8086" => "Intel",
        _ => "",
    }
}

fn lspci_device(slot: &str) -> Option<String> {
    let output = process::run(Command::new("lspci").args(["-mm", "-s", slot]), Duration::from_secs(5)).ok()?;
    parse_lspci_device(&String::from_utf8_lossy(&output.stdout))
}

/// `lspci -mm` 一行里带引号的字段依次是类别、厂商、设备……设备名的方括号里通常是市场名。
fn parse_lspci_device(line: &str) -> Option<String> {
    let device = line.split('"').skip(1).step_by(2).nth(2)?;
    let name = match (device.find('['), device.rfind(']')) {
        (Some(start), Some(end)) if start < end => &device[start + 1..end],
        _ => device,
    };
    Some(name.trim().to_string()).filter(|name| !name.is_empty())
}

fn clipboard() -> Check {
    const ID: &str = "clipboard";
    const TITLE: &str = "剪贴板共享";
    let pyclip = process::run(Command::new("python3").args(["-c", "import pyclip"]), Duration::from_secs(10))
        .is_ok_and(|output| output.status.success());
    let wl_clipboard = process::which("wl-copy").is_some();
    if pyclip && wl_clipboard {
        return Check::new(ID, TITLE, Level::Ok, "桌面和安卓之间可以互相复制粘贴");
    }
    let mut missing = Vec::new();
    let mut steps = Vec::new();
    if !wl_clipboard {
        missing.push("wl-clipboard");
        steps.push("sudo apt install wl-clipboard");
    }
    if !pyclip {
        missing.push("pyclip");
        steps.push("python3 -m pip install --user --break-system-packages pyclip（Ubuntu 源里没有 python3-pyclip）");
    }
    Check::new(ID, TITLE, Level::Warning, format!("缺少 {}，桌面和安卓之间不能复制粘贴", missing.join("、")))
        .hint(format!("{}；装好后重启会话", steps.join("；")))
}

fn input_method() -> Check {
    let framework = if process_running("fcitx5") {
        "fcitx5"
    } else if process_running("ibus-daemon") {
        "IBus"
    } else {
        "未检测到"
    };
    Check::new(
        "input_method",
        "中文输入",
        Level::Warning,
        format!("桌面输入法：{framework}。安卓应用里暂时还不能直接用桌面输入法打中文"),
    )
    .hint("在安卓里装一个中文输入法，比如 fcitx5-android 或 Trime（同文输入法）")
}

fn process_running(name: &str) -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .any(|entry| fs::read_to_string(entry.path().join("comm")).is_ok_and(|comm| comm.trim() == name))
}

fn multi_window(status: &Status) -> Check {
    const ID: &str = "multi_window";
    const TITLE: &str = "多窗口模式";
    if !status.session.is_active() {
        return Check::new(ID, TITLE, Level::Info, "会话没有运行，启动后才能读取");
    }
    match status.multi_window {
        Some(true) => Check::new(ID, TITLE, Level::Ok, "已开启：每个安卓应用是独立的桌面窗口"),
        Some(false) => Check::new(ID, TITLE, Level::Info, "未开启：所有应用显示在同一个窗口里")
            .hint("在「运行状态」页打开，或运行 droid multi-window on，然后重启会话"),
        None => Check::new(ID, TITLE, Level::Warning, "读取失败，Android 可能还没启动完成"),
    }
}

/// Waydroid 只转发 0–238 的键码，模拟器窗口的「多任务」按钮发的是 F24，
/// 要靠 overlay 里的这份按键布局映射成 APP_SWITCH。
fn keylayout() -> Check {
    const ID: &str = "keylayout";
    const TITLE: &str = "按键映射";
    let path = Path::new("/var/lib/waydroid/overlay/system/usr/keylayout/wayland_keyboard.kl");
    let installed = fs::read_to_string(path)
        .is_ok_and(|text| text.lines().any(|line| line.split_whitespace().eq(["key", "194", "APP_SWITCH"])));
    if installed {
        Check::new(ID, TITLE, Level::Ok, "已安装，模拟器窗口的「多任务」按钮可以用")
    } else {
        Check::new(ID, TITLE, Level::Info, "没有安装，模拟器窗口的「多任务」按钮不起作用")
            .hint("在 droid 代码目录里运行 sudo scripts/install-keylayout.sh，然后重启会话")
    }
}

fn adb() -> Check {
    if process::which("adb").is_some() {
        Check::new("adb", "ADB", Level::Ok, "已安装（以后的 UI 自动化会用到）")
    } else {
        Check::new("adb", "ADB", Level::Info, "没有安装（以后的 UI 自动化会用到）").hint("sudo apt install adb")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(node: &str, driver: &str) -> Gpu {
        Gpu { node: node.into(), driver: driver.into(), name: format!("{driver} gpu") }
    }

    #[test]
    fn parses_lspci_device_names() {
        let amd = r#"06:00.0 "VGA compatible controller" "Advanced Micro Devices, Inc. [AMD/ATI]" "Cezanne [Radeon Vega Series / Radeon Vega Mobile Series]" -rc5 -p00 "Acer Incorporated [ALI]" "Device 151e""#;
        assert_eq!(parse_lspci_device(amd).as_deref(), Some("Radeon Vega Series / Radeon Vega Mobile Series"));
        let plain = r#"00:02.0 "VGA compatible controller" "Intel Corporation" "UHD Graphics 620" -r07 "Lenovo" "Device 2258""#;
        assert_eq!(parse_lspci_device(plain).as_deref(), Some("UHD Graphics 620"));
        assert_eq!(parse_lspci_device(""), None);
    }

    #[test]
    fn picks_gpu_like_waydroid() {
        let hybrid = [gpu("renderD128", "nvidia"), gpu("renderD129", "amdgpu")];
        assert_eq!(pick_gpu(&hybrid, None).map(|g| g.node.as_str()), Some("renderD129"));
        assert_eq!(pick_gpu(&hybrid, Some("")).map(|g| g.node.as_str()), Some("renderD129"));
        // 配置指定了 NVIDIA 节点时 Waydroid 会退回软件渲染。
        assert_eq!(pick_gpu(&hybrid, Some("/dev/dri/renderD128")), None);
        assert_eq!(pick_gpu(&[gpu("renderD128", "nvidia")], None), None);
    }
}
