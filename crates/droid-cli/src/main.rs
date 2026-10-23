//! droid 命令行。
//!
//! 所有命令都支持 `--json`：成功时输出 `{"ok":true,"data":…}`，失败时输出
//! `{"ok":false,"error":{"code":…,"message":…}}`，退出码见 `droid_core::ErrorCode`。

use std::fmt::Display;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use droid_core::doctor::{self, Level, Report};
use droid_core::waydroid::{self, SessionState, Status};
use droid_core::{Error, apps, launcher};
use serde::Serialize;
use serde_json::json;

#[derive(Parser)]
#[command(name = "droid", version, about = "在 Ubuntu 上使用安卓应用（基于 Waydroid）")]
struct Cli {
    /// 输出 JSON，方便脚本和 AI Agent 解析
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 查看 Waydroid 和 Android 会话的状态
    Status,
    /// 检查运行环境，给出修复建议
    Doctor,
    /// 管理安卓应用
    #[command(subcommand)]
    App(AppCommand),
    /// 启动或停止 Android 会话
    #[command(subcommand)]
    Session(SessionCommand),
    /// 查看或切换多窗口模式（每个应用一个桌面窗口），切换后重启会话生效
    MultiWindow {
        #[arg(value_enum)]
        state: Option<Toggle>,
    },
    /// 在一个窗口里打开完整的安卓界面
    FullUi,
    /// 查看或设置安卓应用图标是否出现在 Ubuntu 的应用启动器里（默认不出现，只在模拟器里用）
    Launcher {
        #[arg(value_enum)]
        action: Option<LauncherAction>,
    },
}

#[derive(Subcommand)]
enum AppCommand {
    /// 列出已安装的应用
    List {
        /// 直接向运行中的 Android 查询（包括没有桌面图标的应用）
        #[arg(long)]
        live: bool,
    },
    /// 打开应用，会话没运行时会自动启动
    Launch { package: String },
    /// 卸载应用，会话没运行时会自动启动
    Remove {
        package: String,
        /// 最长等待秒数，包括启动会话的时间
        #[arg(long, default_value_t = 180)]
        timeout: u64,
    },
    /// 安装 APK，会话没运行时会自动启动
    Install {
        apk: PathBuf,
        /// 最长等待秒数，包括启动会话的时间
        #[arg(long, default_value_t = 180)]
        timeout: u64,
    },
}

#[derive(Subcommand)]
enum SessionCommand {
    /// 在后台启动会话
    Start {
        /// 等 Android 启动完成再返回
        #[arg(long)]
        wait: bool,
        /// 配合 --wait 使用的最长等待秒数
        #[arg(long, default_value_t = 90)]
        timeout: u64,
    },
    /// 停止会话
    Stop,
    /// 只查会话状态、显示接口和 IP（走 D-Bus，适合频繁轮询）
    Status,
}

#[derive(Clone, Copy, ValueEnum)]
enum Toggle {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum LauncherAction {
    /// 不在 Ubuntu 启动器里显示（默认）
    Hide,
    /// 恢复 Waydroid 的默认行为，在启动器里显示
    Show,
    /// 按当前设置处理新出现的图标（模拟器运行时会自动调用）
    Sync,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command, cli.json) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if cli.json {
                print_json(&json!({ "ok": false, "error": err }));
            } else {
                eprintln!("错误：{err}");
            }
            ExitCode::from(err.code.exit_code())
        }
    }
}

fn run(command: Command, json: bool) -> Result<(), Error> {
    match command {
        Command::Status => emit(json, &waydroid::status(), print_status),
        Command::Doctor => emit(json, &doctor::run(), print_report),
        Command::App(AppCommand::List { live: false }) => emit(json, &apps::installed(), |apps| {
            let width = apps.iter().map(|app| app.package.len()).max().unwrap_or(0);
            for app in apps {
                let system = if app.system { "（系统）" } else { "" };
                println!("{:<width$}  {}{system}", app.package, app.name);
            }
            if apps.is_empty() {
                println!("没有找到安卓应用。启动一次会话后 Waydroid 才会生成应用列表。");
            }
        }),
        Command::App(AppCommand::List { live: true }) => emit(json, &waydroid::live_apps()?, |apps| {
            let width = apps.iter().map(|app| app.package.len()).max().unwrap_or(0);
            for app in apps {
                println!("{:<width$}  {}", app.package, app.name);
            }
        }),
        Command::App(AppCommand::Launch { package }) => {
            let was_active = waydroid::session().is_ok_and(|session| session.state.is_active());
            waydroid::launch_app(&package)?;
            emit(json, &json!({ "package": package }), |_| {
                if was_active {
                    println!("已打开 {package}");
                } else {
                    println!("正在启动 Android 并打开 {package}，第一次大约需要十几秒");
                }
            });
        }
        Command::App(AppCommand::Install { apk, timeout }) => {
            let report = waydroid::install_apk(&apk, Duration::from_secs(timeout))?;
            // 新应用的启动器图标由 Waydroid 稍后生成，按设置隐藏；失败不影响安装结果。
            let packages: Vec<String> = report.new_apps.iter().map(|app| app.package.clone()).collect();
            let _ = launcher::sync_after_install(&packages, Duration::from_secs(5));
            emit(json, &report, |report| {
                if report.new_apps.is_empty() {
                    println!("安装命令已执行，但没有发现新应用：可能是更新了已有应用，也可能安装失败（例如 APK 只带 ARM 库）");
                }
                for app in &report.new_apps {
                    println!("已安装：{}（{}）", app.name, app.package);
                }
            });
        }
        Command::App(AppCommand::Remove { package, timeout }) => {
            let report = waydroid::remove_app(&package, Duration::from_secs(timeout))?;
            emit(json, &report, |report| {
                if report.needs_confirmation {
                    println!("Waydroid 不能直接卸载 {package}，已在安卓里打开卸载确认框，请在模拟器里点「确定」");
                } else {
                    println!("已卸载 {package}");
                }
            });
        }
        Command::Launcher { action } => {
            let report = match action {
                None => launcher::Report { mode: launcher::mode(), changed: Vec::new() },
                Some(LauncherAction::Hide) => launcher::set_mode(launcher::Mode::Hidden)?,
                Some(LauncherAction::Show) => launcher::set_mode(launcher::Mode::Shown)?,
                Some(LauncherAction::Sync) => launcher::sync()?,
            };
            emit(json, &report, |report| {
                let place = match report.mode {
                    launcher::Mode::Hidden => "不显示在 Ubuntu 应用启动器里，只在模拟器里用",
                    launcher::Mode::Shown => "显示在 Ubuntu 应用启动器里",
                };
                println!("安卓应用图标：{place}");
                if !report.changed.is_empty() {
                    println!("这次处理了 {} 个：{}", report.changed.len(), report.changed.join("、"));
                }
            });
        }
        Command::Session(SessionCommand::Start { wait, timeout }) => {
            let mut state = waydroid::start_session()?;
            if wait {
                waydroid::wait_until_ready(Duration::from_secs(timeout))?;
                state = waydroid::session()?.state;
            }
            emit(json, &json!({ "state": state }), |_| {
                if state.is_active() {
                    println!("Android 会话正在运行");
                } else {
                    println!("正在后台启动 Android 会话，可以用 droid status 查看进度");
                }
            });
        }
        Command::Session(SessionCommand::Stop) => {
            waydroid::stop_session()?;
            emit(json, &json!({ "state": SessionState::Stopped }), |_| println!("Android 会话已停止"));
        }
        Command::Session(SessionCommand::Status) => {
            let session = waydroid::session()?;
            let ip = session.state.is_active().then(waydroid::device_ip).flatten();
            let data = json!({
                "state": session.state,
                "wayland_display": session.wayland_display,
                "ip_address": ip,
            });
            emit(json, &data, |_| {
                field("会话", session.state.label());
                if let Some(display) = &session.wayland_display {
                    field("显示接口", display);
                }
                if let Some(ip) = &ip {
                    field("IP 地址", ip);
                }
            });
        }
        Command::MultiWindow { state: None } => {
            let enabled = waydroid::multi_window()?;
            emit(json, &json!({ "enabled": enabled }), |_| {
                println!("多窗口模式：{}", if enabled { "已开启" } else { "未开启" });
            });
        }
        Command::MultiWindow { state: Some(toggle) } => {
            let enabled = matches!(toggle, Toggle::On);
            waydroid::set_multi_window(enabled)?;
            emit(json, &json!({ "enabled": enabled, "restart_required": true }), |_| {
                let verb = if enabled { "开启" } else { "关闭" };
                println!("已{verb}多窗口模式，重启会话后生效：droid session stop && droid session start");
            });
        }
        Command::FullUi => {
            waydroid::show_full_ui()?;
            emit(json, &serde_json::Value::Null, |_| println!("正在打开完整的安卓界面"));
        }
    }
    Ok(())
}

fn emit<T: Serialize>(json: bool, data: &T, human: impl FnOnce(&T)) {
    if json {
        print_json(&json!({ "ok": true, "data": data }));
    } else {
        human(data);
    }
}

/// 在终端里缩进输出方便阅读，被管道读取时输出单行。
fn print_json(value: &serde_json::Value) {
    let text = if std::io::stdout().is_terminal() {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    println!("{}", text.expect("JSON 序列化不会失败"));
}

fn print_status(status: &Status) {
    field("Waydroid", status.version.as_deref().unwrap_or("未安装"));
    let images = match (status.initialized, status.vendor_type.as_deref()) {
        (true, Some(vendor)) => format!("已下载（{vendor}）"),
        (true, None) => "已下载".to_string(),
        (false, _) => "未下载（sudo waydroid init）".to_string(),
    };
    field("安卓镜像", images);
    field("容器服务", if status.container_available { "正常" } else { "连接不上" });
    field("会话", status.session.label());
    if let Some(ip) = &status.ip_address {
        field("IP 地址", ip);
    }
    if let Some(display) = &status.wayland_display {
        field("显示接口", display);
    }
    if let Some(enabled) = status.multi_window {
        field("多窗口", if enabled { "已开启" } else { "未开启" });
    }
    if let Some(err) = &status.error {
        field("问题", &err.message);
    }
}

fn print_report(report: &Report) {
    let color = std::io::stdout().is_terminal();
    for check in &report.checks {
        let (mark, ansi) = match check.level {
            Level::Ok => ("✓", "32"),
            Level::Info => ("i", "34"),
            Level::Warning => ("!", "33"),
            Level::Error => ("✗", "31"),
        };
        let mark = if color { format!("\x1b[{ansi}m{mark}\x1b[0m") } else { mark.to_string() };
        println!("{mark} {}{}", pad(check.title, 12), check.summary);
        if let Some(hint) = &check.hint {
            println!("  {}建议：{hint}", pad("", 12));
        }
    }
    let verdict = match report.overall {
        Level::Error => "有问题需要处理",
        Level::Warning => "可以使用，但有需要注意的地方",
        Level::Ok | Level::Info => "一切正常",
    };
    println!("\n结论：{verdict}");
}

fn field(label: &str, value: impl Display) {
    println!("{}{value}", pad(label, 10));
}

/// 按终端显示宽度补齐空格（中文字符占两列）。
fn pad(text: &str, width: usize) -> String {
    let used: usize = text.chars().map(|c| if c as u32 >= 0x2E80 { 2 } else { 1 }).sum();
    format!("{text}{}", " ".repeat(width.saturating_sub(used).max(1)))
}
