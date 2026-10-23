//! 已安装的安卓应用。数据来自 Waydroid 给每个应用生成的 .desktop 文件，所以会话没运行时也能读到。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::launcher;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AndroidApp {
    pub package: String,
    pub name: String,
    pub icon: Option<PathBuf>,
    /// Waydroid 自带的系统应用。它们的 .desktop 文件带 NoDisplay=true，不会出现在系统的应用菜单里。
    pub system: bool,
}

/// 读取当前用户的安卓应用，用户安装的排在系统应用前面，再按名称排序。
pub fn installed() -> Vec<AndroidApp> {
    in_dir(&paths::applications_dir())
}

pub fn in_dir(dir: &Path) -> Vec<AndroidApp> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut apps: Vec<AndroidApp> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            let package = file_name.strip_prefix("waydroid.")?.strip_suffix(".desktop")?;
            let text = fs::read_to_string(entry.path()).ok()?;
            let mut app = parse_desktop_entry(package, &text);
            app.icon = app.icon.filter(|icon| icon.is_file());
            Some(app)
        })
        .collect();
    apps.sort_by(|a, b| {
        a.system
            .cmp(&b.system)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

fn parse_desktop_entry(package: &str, text: &str) -> AndroidApp {
    let mut in_entry = false;
    let mut name = None;
    let mut icon = None;
    let mut no_display = false;
    // 我们为了不让应用出现在 Ubuntu 启动器里而隐藏的入口，不算系统应用。
    let mut hidden_by_us = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => name = Some(value.to_string()),
            "Icon" => icon = Some(PathBuf::from(value)),
            "NoDisplay" => no_display = value == "true",
            launcher::HIDDEN_MARK => hidden_by_us = value == "true",
            _ => {}
        }
    }
    AndroidApp {
        package: package.to_string(),
        name: name.unwrap_or_else(|| package.to_string()),
        icon,
        system: no_display && !hidden_by_us,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waydroid_desktop_entry() {
        let text = "[Desktop Entry]\nType=Application\nName=Settings\nExec=waydroid app launch com.android.settings\nIcon=/home/u/.local/share/waydroid/data/icons/com.android.settings.png\nNoDisplay=true\n\n[Desktop Action app-settings]\nName=App Settings\n";
        let app = parse_desktop_entry("com.android.settings", text);
        assert_eq!(app.name, "Settings");
        assert!(app.system);
        assert_eq!(
            app.icon.as_deref(),
            Some(Path::new("/home/u/.local/share/waydroid/data/icons/com.android.settings.png"))
        );
    }

    #[test]
    fn apps_hidden_from_launcher_are_not_system_apps() {
        let text = "[Desktop Entry]\nNoDisplay=true\nX-Droid-Hidden=true\nName=F-Droid\n";
        assert!(!parse_desktop_entry("org.fdroid.fdroid", text).system);
    }

    #[test]
    fn falls_back_to_package_name() {
        let app = parse_desktop_entry("com.example.app", "[Desktop Entry]\nType=Application\n");
        assert_eq!(app.name, "com.example.app");
        assert!(!app.system);
        assert_eq!(app.icon, None);
    }
}
