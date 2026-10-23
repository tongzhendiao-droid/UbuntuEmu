//! 子进程工具：带超时的执行、在 PATH 里查找命令、在后台脱离运行。

use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::{Error, ErrorCode, Result};

/// 在 PATH 里查找可执行文件。
pub fn which(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// 执行命令并收集输出，超时就结束子进程。
pub fn run(command: &mut Command, timeout: Duration) -> Result<Output> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| spawn_error(&program, &err))?;
    // 在单独的线程里读管道，免得输出太多把管道写满、子进程卡住。
    let stdout = child.stdout.take().map(read_to_end);
    let stderr = child.stderr.take().map(read_to_end);

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| internal(&program, &err))? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::new(
                ErrorCode::Timeout,
                format!("{program} 在 {} 秒内没有完成", timeout.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    Ok(Output { status, stdout: join(stdout), stderr: join(stderr) })
}

/// 在新的进程会话里后台运行命令，输出追加到日志文件。
/// `setsid -f` 会立刻 fork 并返回：子进程不随本程序或终端退出，也不会留下僵尸进程。
pub fn spawn_detached(program: &str, args: &[&str], log_path: &Path) -> Result<()> {
    if which(program).is_none() {
        return Err(spawn_error(program, &io::Error::from(io::ErrorKind::NotFound)));
    }
    let log = open_log(log_path, program, args)?;
    let log_for_stderr = log.try_clone().map_err(|err| internal("日志文件", &err))?;
    let status = Command::new("setsid")
        .arg("-f")
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_for_stderr)
        .status()
        .map_err(|err| spawn_error("setsid", &err))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::CommandFailed,
            format!("无法在后台运行 {program} {}", args.join(" ")),
        ))
    }
}

fn open_log(path: &Path, program: &str, args: &[&str]) -> Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| internal("日志目录", &err))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| internal("日志文件", &err))?;
    let _ = writeln!(file, "=== {program} {} ===", args.join(" "));
    Ok(file)
}

fn read_to_end<R: Read + Send + 'static>(mut reader: R) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    })
}

fn join(handle: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|handle| handle.join().ok()).unwrap_or_default()
}

fn spawn_error(program: &str, err: &io::Error) -> Error {
    match err.kind() {
        io::ErrorKind::NotFound if program == "waydroid" => {
            Error::new(ErrorCode::NotInstalled, "找不到 waydroid 命令，请先安装 Waydroid")
        }
        io::ErrorKind::NotFound => Error::new(ErrorCode::Internal, format!("找不到命令 {program}")),
        _ => internal(program, err),
    }
}

fn internal(what: &str, err: &io::Error) -> Error {
    Error::new(ErrorCode::Internal, format!("{what}：{err}"))
}
