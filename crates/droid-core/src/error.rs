//! 统一的错误类型。错误码（字符串和退出码）是对外的稳定接口，脚本和 Agent 依赖它：只能新增，不能改动已有的值。

use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// 未归类的内部错误。
    Internal,
    /// 系统里找不到 waydroid 命令。
    NotInstalled,
    /// 还没有执行过 `waydroid init`。
    NotInitialized,
    /// 操作需要正在运行的 Android 会话。
    SessionNotRunning,
    /// 连接不到 Waydroid 容器服务（system bus 上的 id.waydro.Container）。
    ContainerUnavailable,
    /// 参数不合法，例如 APK 文件不存在。
    InvalidArgument,
    /// 等待超时。
    Timeout,
    /// waydroid 命令执行失败。
    CommandFailed,
}

impl ErrorCode {
    /// 进程退出码。2 留给命令行参数错误（clap 的默认值）。
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Internal => 1,
            Self::NotInstalled => 3,
            Self::NotInitialized => 4,
            Self::SessionNotRunning => 5,
            Self::ContainerUnavailable => 6,
            Self::InvalidArgument => 7,
            Self::Timeout => 8,
            Self::CommandFailed => 9,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
