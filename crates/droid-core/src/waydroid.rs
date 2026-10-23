//! 与 Waydroid 交互。会话状态通过 D-Bus 查询（容器服务的 GetSession），其余操作调用 waydroid 命令行。
//!
//! 注意：waydroid 出错时退出码通常仍是 0，只在 stderr 打一行日志，所以要按 stderr 的内容判断结果。

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode, Result};
use crate::paths;
use crate::process;

const MULTI_WINDOW_PROP: &str = "persist.waydroid.multi_windows";
/// 会话进程在 session bus 上注册的名字。
const SESSION_BUS_NAME: &str = "id.waydro.Session";
/// 不需要等安卓系统的命令。
const QUICK: Duration = Duration::from_secs(15);
/// 需要连接安卓系统服务的命令：Android 还在启动时，waydroid 会一直等到服务就绪。
const ANDROID: Duration = Duration::from_secs(90);

/// Waydroid 安装 APK 时总是复制到同一个临时文件，同时安装会互相覆盖，所以要排队。
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Stopped,
    Starting,
    Running,
    Frozen,
    Stopping,
    Unknown,
}

impl SessionState {
    fn from_lxc(state: &str) -> Self {
        match state {
            "RUNNING" => Self::Running,
            "FROZEN" | "FREEZING" => Self::Frozen,
            "STARTING" => Self::Starting,
            "STOPPING" | "ABORTING" => Self::Stopping,
            "STOPPED" => Self::Stopped,
            _ => Self::Unknown,
        }
    }

    /// 容器在运行。冻结的会话在执行命令时会被 waydroid 自动解冻，也算在运行。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Frozen)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stopped => "未运行",
            Self::Starting => "正在启动",
            Self::Running => "运行中",
            Self::Frozen => "空闲（已冻结）",
            Self::Stopping => "正在停止",
            Self::Unknown => "未知",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub state: SessionState,
    pub user: Option<String>,
    pub wayland_display: Option<String>,
}

/// `waydroid app list` 里的一个应用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveApp {
    pub name: String,
    pub package: String,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub apk: PathBuf,
    /// 安装后新出现的应用。为空时可能是更新了已有应用，也可能安装失败（waydroid 不报告安装结果）。
    pub new_apps: Vec<LiveApp>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub installed: bool,
    pub version: Option<String>,
    pub initialized: bool,
    pub vendor_type: Option<String>,
    pub container_available: bool,
    pub session: SessionState,
    pub ip_address: Option<String>,
    pub wayland_display: Option<String>,
    /// 只有会话在运行时才能读到。
    pub multi_window: Option<bool>,
    /// 查询会话失败的原因。
    pub error: Option<Error>,
}

pub fn is_installed() -> bool {
    process::which("waydroid").is_some()
}

pub fn version() -> Option<String> {
    let output = process::run(Command::new("waydroid").arg("--version"), QUICK).ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// 读取 /var/lib/waydroid/waydroid.cfg 的 [waydroid] 段。
pub fn config() -> Option<HashMap<String, String>> {
    fs::read_to_string(paths::WAYDROID_CFG).ok().map(|text| parse_config(&text))
}

pub fn images_dir(config: &HashMap<String, String>) -> PathBuf {
    config
        .get("images_path")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(paths::WAYDROID_DEFAULT_IMAGES))
}

fn images_present(config: &HashMap<String, String>) -> bool {
    let dir = images_dir(config);
    dir.join("system.img").is_file() && dir.join("vendor.img").is_file()
}

/// 已经执行过 `waydroid init`：配置文件和两个系统镜像都在。
pub fn is_initialized() -> bool {
    config().is_some_and(|config| images_present(&config))
}

/// 查询会话状态，只要几毫秒，适合轮询。
///
/// 会话运行时，容器服务的 GetSession 每调用一次会泄漏两个文件句柄（Waydroid 1.6.3 的问题），
/// 句柄用完后会话就再也启动不了。所以会话运行时只看 session bus 上的会话进程，显示接口从它的
/// 环境变量里读；会话没运行时 GetSession 不会泄漏，用它顺便确认容器服务可用、已经初始化。
/// 代价是区分不出「冻结」，运行中的会话一律报 Running。
pub fn session() -> Result<Session> {
    match session_pid()? {
        Some(pid) => Ok(Session {
            state: SessionState::Running,
            user: process_env(pid, "USER"),
            // 与 Waydroid 一致：没设置 WAYLAND_DISPLAY 时用 wayland-0。
            wayland_display: Some(process_env(pid, "WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into())),
        }),
        None => container_session(),
    }
}

/// 向容器服务查询会话（GetSession）。只在会话没运行时调用，原因见 [`session`]。
fn container_session() -> Result<Session> {
    let output = process::run(
        Command::new("busctl").args([
            "--system",
            "--json=short",
            "call",
            "id.waydro.Container",
            "/ContainerManager",
            "id.waydro.ContainerManager",
            "GetSession",
        ]),
        QUICK,
    )?;
    if !output.status.success() {
        return Err(classify_dbus_error(&String::from_utf8_lossy(&output.stderr)));
    }
    parse_session_reply(&String::from_utf8_lossy(&output.stdout))
}

/// 会话进程（在 session bus 上注册了 id.waydro.Session）的 PID，没有会话时返回 None。
fn session_pid() -> Result<Option<u32>> {
    let has_owner: bool = session_bus_call("NameHasOwner")?.unwrap_or(false);
    if !has_owner {
        return Ok(None);
    }
    // 会话可能恰好在两次调用之间退出，这时当作没有会话。
    Ok(session_bus_call("GetConnectionUnixProcessID").ok().flatten())
}

fn session_bus_call<T: DeserializeOwned>(method: &str) -> Result<Option<T>> {
    let output = process::run(
        Command::new("busctl").args([
            "--user",
            "--json=short",
            "call",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            "s",
            SESSION_BUS_NAME,
        ]),
        QUICK,
    )?;
    if !output.status.success() {
        return Err(Error::new(
            ErrorCode::Internal,
            format!("查询 session bus 失败：{}", String::from_utf8_lossy(&output.stderr).trim()),
        ));
    }
    parse_first_value(&String::from_utf8_lossy(&output.stdout))
}

fn process_env(pid: u32, key: &str) -> Option<String> {
    let environ = fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{key}=");
    environ
        .split(|&byte| byte == 0)
        .find_map(|entry| std::str::from_utf8(entry).ok()?.strip_prefix(prefix.as_str()).map(str::to_string))
        .filter(|value| !value.is_empty())
}

pub fn device_ip() -> Option<String> {
    fs::read_to_string(paths::DNSMASQ_LEASES).ok().and_then(|text| parse_lease_ip(&text))
}

/// 汇总状态。不会失败：查不到的字段留空，原因放在 `error` 里。
pub fn status() -> Status {
    let installed = is_installed();
    let config = config();
    let mut status = Status {
        installed,
        version: installed.then(version).flatten(),
        initialized: config.as_ref().is_some_and(images_present),
        vendor_type: config.as_ref().and_then(|config| config.get("vendor_type").cloned()),
        container_available: false,
        session: SessionState::Stopped,
        ip_address: None,
        wayland_display: None,
        multi_window: None,
        error: None,
    };
    if !installed {
        status.error = Some(not_installed());
        return status;
    }
    match session() {
        Ok(session) => {
            status.container_available = true;
            status.session = session.state;
            status.wayland_display = session.wayland_display;
            if session.state.is_active() {
                status.ip_address = device_ip();
                status.multi_window = multi_window().ok();
            }
        }
        Err(err) => {
            status.container_available = err.code == ErrorCode::NotInitialized;
            status.error = Some(err);
        }
    }
    status
}

/// 在后台启动会话。返回时会话多半还在启动，需要等就用 [`wait_for`]。
pub fn start_session() -> Result<SessionState> {
    let state = session()?.state;
    if state.is_active() || state == SessionState::Starting {
        return Ok(state);
    }
    detached(&["session", "start"])?;
    Ok(SessionState::Starting)
}

pub fn stop_session() -> Result<()> {
    run(&["session", "stop"], QUICK).map(drop)
}

pub fn restart_session() -> Result<()> {
    stop_session()?;
    wait_for(|state| state == SessionState::Stopped, Duration::from_secs(30))?;
    detached(&["session", "start"])
}

/// 等到会话状态满足条件。
pub fn wait_for(done: impl Fn(SessionState) -> bool, timeout: Duration) -> Result<SessionState> {
    let deadline = Instant::now() + timeout;
    loop {
        let state = session()?.state;
        if done(state) {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                ErrorCode::Timeout,
                format!("等待 Android 会话超时（当前状态：{}）", state.label()),
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// 确保会话在运行并且 Android 已经启动完成，没有就启动并等待。
pub fn ensure_session(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let state = session()?.state;
    if !state.is_active() && state != SessionState::Starting {
        detached(&["session", "start"])?;
    }
    wait_until_ready(remaining(deadline))
}

/// 等到 Android 可以使用：容器进入运行状态后，系统还要十几秒才能启动完成，
/// 这之前查到的应用列表和 IP 地址都不完整。
///
/// 每次 `waydroid prop get` 也会让容器服务泄漏两个句柄（见 [`session`]），所以两秒才问一次。
pub fn wait_until_ready(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    wait_for(SessionState::is_active, timeout)?;
    loop {
        match run(&["prop", "get", "sys.boot_completed"], remaining(deadline)) {
            Ok(value) if value.trim() == "1" => return Ok(()),
            // 系统服务还没起来时会报 CommandFailed，继续等。
            Ok(_) => {}
            Err(err) if err.code == ErrorCode::CommandFailed => {}
            Err(err) => return Err(err),
        }
        if Instant::now() >= deadline {
            return Err(Error::new(ErrorCode::Timeout, "等待 Android 启动完成超时"));
        }
        thread::sleep(Duration::from_secs(2));
    }
}

/// 启动应用。会话没运行时 waydroid 会先启动会话。
pub fn launch_app(package: &str) -> Result<()> {
    check_package(package)?;
    // 会话没运行时，这个 waydroid 进程会直接变成会话进程，所以只能放到后台运行。
    detached(&["app", "launch", package])
}

/// 在一个窗口里显示完整的安卓界面。
pub fn show_full_ui() -> Result<()> {
    detached(&["show-full-ui"])
}

/// 向运行中的 Android 查询已安装的应用。
pub fn live_apps() -> Result<Vec<LiveApp>> {
    Ok(parse_app_list(&run(&["app", "list"], ANDROID)?))
}

/// 安装 APK。会话没运行时先启动；`timeout` 包括启动会话的时间。
pub fn install_apk(apk: &Path, timeout: Duration) -> Result<InstallReport> {
    let apk = fs::canonicalize(apk).map_err(|_| {
        Error::new(ErrorCode::InvalidArgument, format!("找不到文件：{}", apk.display()))
    })?;
    let is_apk = apk.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("apk"));
    if !apk.is_file() || !is_apk {
        return Err(Error::new(
            ErrorCode::InvalidArgument,
            format!("不是 APK 文件：{}", apk.display()),
        ));
    }
    let apk_arg = apk
        .to_str()
        .ok_or_else(|| Error::new(ErrorCode::InvalidArgument, "文件路径里有无法识别的字符"))?
        .to_string();

    let _queue = INSTALL_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let deadline = Instant::now() + timeout;
    ensure_session(remaining(deadline))?;
    let before: BTreeSet<String> = live_apps()?.into_iter().map(|app| app.package).collect();
    run(&["app", "install", &apk_arg], remaining(deadline))?;

    // waydroid 不报告安装结果，只能对比安装前后的应用列表；新应用可能稍晚才出现。
    let mut new_apps = Vec::new();
    for _ in 0..5 {
        new_apps = live_apps()?
            .into_iter()
            .filter(|app| !before.contains(&app.package))
            .collect();
        if !new_apps.is_empty() || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    Ok(InstallReport { apk, new_apps })
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveReport {
    pub package: String,
    /// 没能直接卸载，已经在安卓里打开系统的卸载确认框，要用户点「确定」。
    pub needs_confirmation: bool,
}

/// 卸载应用。会话没运行时先启动。
pub fn remove_app(package: &str, timeout: Duration) -> Result<RemoveReport> {
    check_package(package)?;
    ensure_session(timeout)?;
    let output = process::run(Command::new("waydroid").args(["app", "remove", package]), ANDROID)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Waydroid 1.6.3 的静默卸载会被 Android 拒绝（DELETE_FAILED_USER_RESTRICTED，代码 -3，
    // 用 root 执行 pm uninstall 则可以），退而打开系统的卸载确认框。
    if stderr.contains("Failed with code: -3") {
        detached(&["app", "intent", "android.intent.action.DELETE", &format!("package:{package}")])?;
        return Ok(RemoveReport { package: package.to_string(), needs_confirmation: true });
    }
    if let Some(err) = classify_stderr(&stderr) {
        return Err(err);
    }
    Ok(RemoveReport { package: package.to_string(), needs_confirmation: false })
}

pub fn multi_window() -> Result<bool> {
    Ok(run(&["prop", "get", MULTI_WINDOW_PROP], ANDROID)?.trim() == "true")
}

/// 切换多窗口模式，重启会话后生效。
pub fn set_multi_window(enabled: bool) -> Result<()> {
    let value = if enabled { "true" } else { "false" };
    run(&["prop", "set", MULTI_WINDOW_PROP, value], ANDROID).map(drop)
}

fn run(args: &[&str], timeout: Duration) -> Result<String> {
    let output = process::run(Command::new("waydroid").args(args), timeout)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(err) = classify_stderr(&stderr) {
        return Err(err);
    }
    if !output.status.success() {
        let detail = stderr.lines().rev().map(str::trim).find(|line| !line.is_empty());
        return Err(Error::new(
            ErrorCode::CommandFailed,
            format!("waydroid {} 失败：{}", args.join(" "), detail.unwrap_or("没有输出")),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn detached(args: &[&str]) -> Result<()> {
    process::spawn_detached("waydroid", args, &paths::log_file())
}

/// 包名会作为参数传给 waydroid，只接受合法的包名，也防止被当成命令行选项。
fn check_package(package: &str) -> Result<()> {
    let valid = !package.is_empty()
        && !package.starts_with(['.', '-'])
        && package.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::new(ErrorCode::InvalidArgument, format!("包名不合法：{package}")))
    }
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now()).max(Duration::from_secs(1))
}

fn not_installed() -> Error {
    Error::new(ErrorCode::NotInstalled, "没有安装 Waydroid")
}

fn not_initialized() -> Error {
    Error::new(ErrorCode::NotInitialized, "Waydroid 还没有初始化，请先运行 sudo waydroid init")
}

/// 识别 waydroid 打在 stderr 里的已知错误。
fn classify_stderr(stderr: &str) -> Option<Error> {
    if stderr.contains("session is stopped") {
        Some(Error::new(ErrorCode::SessionNotRunning, "Android 会话没有运行"))
    } else if stderr.contains("not initialized") {
        Some(not_initialized())
    } else if stderr.contains("container is not listening") {
        Some(Error::new(ErrorCode::ContainerUnavailable, "Waydroid 容器服务没有响应"))
    } else if stderr.contains("Failed to uninstall package") {
        Some(Error::new(ErrorCode::CommandFailed, "卸载失败，请确认包名正确、应用已经安装"))
    } else if stderr.contains("Failed to access IPlatform service")
        || stderr.contains("Service Manager never appeared")
    {
        Some(Error::new(
            ErrorCode::CommandFailed,
            "连接不到安卓系统服务，Android 可能还没启动完成",
        ))
    } else {
        None
    }
}

fn classify_dbus_error(stderr: &str) -> Error {
    let detail = stderr.trim().trim_start_matches("Call failed: ");
    if detail.contains("not initialized") {
        not_initialized()
    } else {
        Error::new(ErrorCode::ContainerUnavailable, format!("连接不到 Waydroid 容器服务：{detail}"))
    }
}

/// 取 `busctl --json=short call` 输出里的第一个返回值。
fn parse_first_value<T: DeserializeOwned>(json: &str) -> Result<Option<T>> {
    #[derive(Deserialize)]
    struct Reply<T> {
        data: Vec<T>,
    }
    let reply: Reply<T> = serde_json::from_str(json)
        .map_err(|err| Error::new(ErrorCode::Internal, format!("无法解析 D-Bus 的返回：{err}")))?;
    Ok(reply.data.into_iter().next())
}

/// 解析 `busctl --json=short call … GetSession` 的输出。没有会话时返回的是空字典。
fn parse_session_reply(json: &str) -> Result<Session> {
    let fields: HashMap<String, String> = parse_first_value(json)?.unwrap_or_default();
    if fields.is_empty() {
        return Ok(Session { state: SessionState::Stopped, user: None, wayland_display: None });
    }
    Ok(Session {
        state: fields.get("state").map_or(SessionState::Unknown, |state| SessionState::from_lxc(state)),
        user: fields.get("user_name").cloned(),
        wayland_display: fields.get("wayland_display").cloned(),
    })
}

/// 解析 `waydroid app list`：每个应用依次是 `Name:`、`packageName:`、`categories:` 和以制表符开头的类别。
fn parse_app_list(text: &str) -> Vec<LiveApp> {
    let mut apps = Vec::new();
    let mut current: Option<LiveApp> = None;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("Name: ") {
            apps.extend(current.take());
            current = Some(LiveApp { name: name.to_string(), package: String::new(), categories: Vec::new() });
        } else if let Some(package) = line.strip_prefix("packageName: ") {
            if let Some(app) = current.as_mut() {
                app.package = package.trim().to_string();
            }
        } else if let Some(category) = line.strip_prefix('\t') {
            if let Some(app) = current.as_mut() {
                app.categories.push(category.trim().to_string());
            }
        }
    }
    apps.extend(current);
    apps.retain(|app| !app.package.is_empty());
    apps
}

fn parse_config(text: &str) -> HashMap<String, String> {
    let mut in_section = false;
    let mut values = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == "[waydroid]";
        } else if in_section {
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    values
}

/// 与 Waydroid 一致：取租约文件里第一个 IPv4 地址。
fn parse_lease_ip(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|token| token.parse::<Ipv4Addr>().is_ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_running_and_empty_sessions() {
        let running = r#"{"type":"a{ss}","data":[{"state":"RUNNING","user_name":"tz","wayland_display":"wayland-0"}]}"#;
        let session = parse_session_reply(running).unwrap();
        assert_eq!(session.state, SessionState::Running);
        assert_eq!(session.user.as_deref(), Some("tz"));
        assert_eq!(session.wayland_display.as_deref(), Some("wayland-0"));

        let stopped = parse_session_reply(r#"{"type":"a{ss}","data":[{}]}"#).unwrap();
        assert_eq!(stopped.state, SessionState::Stopped);
        assert!(parse_session_reply("not json").is_err());
    }

    #[test]
    fn parses_session_bus_replies() {
        assert_eq!(parse_first_value::<bool>(r#"{"type":"b","data":[true]}"#).unwrap(), Some(true));
        assert_eq!(parse_first_value::<u32>(r#"{"type":"u","data":[4242]}"#).unwrap(), Some(4242));
        assert_eq!(parse_first_value::<bool>(r#"{"type":"b","data":[]}"#).unwrap(), None);
    }

    #[test]
    fn parses_app_list() {
        let text = "Name: Settings\npackageName: com.android.settings\ncategories:\n\tandroid.intent.category.LAUNCHER\nName: Files\npackageName: com.android.documentsui\ncategories:\n";
        let apps = parse_app_list(text);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].package, "com.android.settings");
        assert_eq!(apps[0].categories, ["android.intent.category.LAUNCHER"]);
        assert_eq!(apps[1].name, "Files");
        assert!(apps[1].categories.is_empty());
    }

    #[test]
    fn reads_config_section_and_lease_ip() {
        let config = parse_config("[waydroid]\nvendor_type = MAINLINE\nimages_path = /x\n\n[properties]\nfoo = bar\n");
        assert_eq!(config.get("vendor_type").map(String::as_str), Some("MAINLINE"));
        assert_eq!(config.get("images_path").map(String::as_str), Some("/x"));
        assert!(!config.contains_key("foo"));

        let lease = "1792328515 00:16:3e:f9:d3:03 192.168.240.112 WayDroid-x86-64-Device 01:00:16:3e:f9:d3:03\n";
        assert_eq!(parse_lease_ip(lease).as_deref(), Some("192.168.240.112"));
        assert_eq!(parse_lease_ip(""), None);
    }

    #[test]
    fn classifies_waydroid_errors() {
        let stopped = classify_stderr("[20:32:47] WayDroid session is stopped\n").unwrap();
        assert_eq!(stopped.code, ErrorCode::SessionNotRunning);
        // 自动启动会话时 waydroid 也用 error 级别打日志，这不是错误。
        assert!(classify_stderr("[20:32:47] Starting waydroid session\n").is_none());
        let missing = classify_dbus_error("Call failed: The name id.waydro.Container was not provided by any .service files");
        assert_eq!(missing.code, ErrorCode::ContainerUnavailable);
    }

    #[test]
    fn validates_package_names() {
        assert!(check_package("com.android.settings").is_ok());
        assert!(check_package("org.lineageos.jelly_2").is_ok());
        assert!(check_package("--help").is_err());
        assert!(check_package("com.example; rm -rf /").is_err());
        assert!(check_package("").is_err());
    }
}
