//! 用户偏好，存在 `$XDG_CONFIG_HOME/droid/config.json`。文件不存在或读不懂时用默认值。

use std::fs;

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode, Result};
use crate::launcher;
use crate::paths;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 安卓应用是否出现在 Ubuntu 的应用启动器里。
    pub launcher: launcher::Mode,
}

pub fn load() -> Config {
    fs::read_to_string(paths::config_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(config: &Config) -> Result<()> {
    let path = paths::config_file();
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(config).expect("配置序列化不会失败");
        fs::write(&path, text + "\n")
    };
    write().map_err(|err| Error::new(ErrorCode::Internal, format!("无法保存 {}：{err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_hidden_launcher_entries() {
        assert_eq!(Config::default().launcher, launcher::Mode::Hidden);
        let parsed: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.launcher, launcher::Mode::Hidden);
        let parsed: Config = serde_json::from_str(r#"{"launcher":"shown"}"#).unwrap();
        assert_eq!(parsed.launcher, launcher::Mode::Shown);
    }
}
