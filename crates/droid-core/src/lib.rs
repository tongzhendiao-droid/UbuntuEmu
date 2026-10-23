//! 核心库：图形界面、命令行（以及以后的守护进程和 MCP Server）共用的全部业务逻辑。
//! 界面层只负责展示，不直接调用 waydroid。

pub mod apps;
pub mod config;
pub mod doctor;
pub mod error;
pub mod launcher;
pub mod paths;
mod process;
pub mod waydroid;

pub use error::{Error, ErrorCode, Result};
