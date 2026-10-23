//! Ubuntu 应用启动器里的安卓应用入口。
//!
//! Waydroid 会给每个有桌面图标的安卓应用生成 `waydroid.<包名>.desktop`，让它们出现在 Ubuntu 的应用网格里。
//! 我们让安卓应用只在模拟器窗口里使用，所以默认把这些入口隐藏：加上 `NoDisplay=true`，再加上
//! `X-Droid-Hidden=true` 作为标记，恢复显示时只撤销我们自己加的（系统应用本来就被 Waydroid 隐藏）。
//! Waydroid 重写入口时会保留文件里已有的 NoDisplay，所以只需要处理新出现的入口。

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::{Error, ErrorCode, Result};
use crate::paths;

/// 我们隐藏入口时加的标记。
pub const HIDDEN_MARK: &str = "X-Droid-Hidden";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// 安卓应用不出现在 Ubuntu 的应用启动器里（默认）。
    #[default]
    Hidden,
    /// 保持 Waydroid 的默认行为。
    Shown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub mode: Mode,
    /// 这次改动了入口的应用包名。
    pub changed: Vec<String>,
}

/// 当前的偏好。
pub fn mode() -> Mode {
    config::load().launcher
}

/// 保存偏好并立刻应用到所有入口。
pub fn set_mode(mode: Mode) -> Result<Report> {
    let mut config = config::load();
    config.launcher = mode;
    config::save(&config)?;
    sync()
}

/// 按当前偏好处理所有安卓应用入口。可以反复调用，没有变化时不会写文件。
pub fn sync() -> Result<Report> {
    let mode = mode();
    let changed = apply(&paths::applications_dir(), mode)?;
    Ok(Report { mode, changed })
}

/// 刚装完应用时用：Waydroid 稍后才会生成新应用的入口，等它出现再同步。
/// 没有桌面图标的应用不会有入口，最多等 `timeout`。
pub fn sync_after_install(packages: &[String], timeout: Duration) -> Result<Report> {
    let dir = paths::applications_dir();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline
        && !packages.iter().all(|package| dir.join(format!("waydroid.{package}.desktop")).exists())
    {
        thread::sleep(Duration::from_millis(200));
    }
    sync()
}

fn apply(dir: &Path, mode: Mode) -> Result<Vec<String>> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut changed = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(package) = file_name.strip_prefix("waydroid.").and_then(|rest| rest.strip_suffix(".desktop"))
        else {
            continue;
        };
        let path = entry.path();
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let updated = match mode {
            Mode::Hidden => hide_entry(&text),
            Mode::Shown => show_entry(&text),
        };
        if let Some(updated) = updated {
            write_atomically(&path, &updated)?;
            changed.push(package.to_string());
        }
    }
    changed.sort();
    Ok(changed)
}

/// 先写临时文件再改名，免得桌面环境读到写了一半的入口。
fn write_atomically(path: &Path, text: &str) -> Result<()> {
    let temp = path.with_extension("desktop.droid-tmp");
    fs::write(&temp, text)
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|err| Error::new(ErrorCode::Internal, format!("无法修改 {}：{err}", path.display())))
}

/// 返回隐藏后的内容；已经隐藏的返回 None。
fn hide_entry(text: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (start, end) = entry_group(&lines)?;
    if lines[start..end].iter().any(|line| key_value(line) == Some(("NoDisplay", "true"))) {
        return None;
    }
    lines.retain_range(start, end, |line| key_value(line).map(|(key, _)| key) != Some("NoDisplay"));
    let insert_at = start + 1;
    lines.insert(insert_at, format!("{HIDDEN_MARK}=true"));
    lines.insert(insert_at, "NoDisplay=true".to_string());
    Some(join_lines(&lines))
}

/// 返回撤销隐藏后的内容；不是我们隐藏的返回 None。
fn show_entry(text: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (start, end) = entry_group(&lines)?;
    if !lines[start..end].iter().any(|line| key_value(line) == Some((HIDDEN_MARK, "true"))) {
        return None;
    }
    lines.retain_range(start, end, |line| {
        !matches!(key_value(line).map(|(key, _)| key), Some("NoDisplay") | Some(HIDDEN_MARK))
    });
    Some(join_lines(&lines))
}

/// `[Desktop Entry]` 组的范围：组标题所在行到下一个组标题（不含）。
fn entry_group(lines: &[String]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| line.trim() == "[Desktop Entry]")?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    Some((start, end))
}

fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn join_lines(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

trait RetainRange {
    /// 只在 [start, end) 范围内按条件删除行，范围外的行保持不动。
    fn retain_range(&mut self, start: usize, end: usize, keep: impl Fn(&str) -> bool);
}

impl RetainRange for Vec<String> {
    fn retain_range(&mut self, start: usize, end: usize, keep: impl Fn(&str) -> bool) {
        let mut index = 0;
        self.retain(|line| {
            let inside = index > start && index < end;
            index += 1;
            !inside || keep(line)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_APP: &str = "[Desktop Entry]\nType=Application\nName=F-Droid\nExec=waydroid app launch org.fdroid.fdroid\n\n[Desktop Action app-settings]\nName=App Settings\nNoDisplay=false\n";

    #[test]
    fn hides_and_restores_user_apps() {
        let hidden = hide_entry(USER_APP).unwrap();
        assert!(hidden.starts_with("[Desktop Entry]\nNoDisplay=true\nX-Droid-Hidden=true\nType=Application\n"));
        // 其他组里的同名键不受影响。
        assert!(hidden.contains("[Desktop Action app-settings]\nName=App Settings\nNoDisplay=false\n"));
        assert_eq!(hide_entry(&hidden), None);

        let shown = show_entry(&hidden).unwrap();
        assert_eq!(shown, USER_APP);
        assert_eq!(show_entry(&shown), None);
    }

    #[test]
    fn leaves_waydroid_hidden_system_apps_alone() {
        let system = "[Desktop Entry]\nName=Settings\nNoDisplay=true\n";
        assert_eq!(hide_entry(system), None);
        assert_eq!(show_entry(system), None);
    }

    #[test]
    fn replaces_explicit_no_display_false() {
        let visible = "[Desktop Entry]\nName=Settings\nNoDisplay=false\n";
        let hidden = hide_entry(visible).unwrap();
        assert_eq!(hidden, "[Desktop Entry]\nNoDisplay=true\nX-Droid-Hidden=true\nName=Settings\n");
    }

    #[test]
    fn applies_mode_to_directory() {
        let dir = std::env::temp_dir().join(format!("droid-launcher-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("waydroid.org.fdroid.fdroid.desktop"), USER_APP).unwrap();
        fs::write(dir.join("org.gnome.Nautilus.desktop"), USER_APP).unwrap();

        assert_eq!(apply(&dir, Mode::Hidden).unwrap(), ["org.fdroid.fdroid"]);
        assert!(apply(&dir, Mode::Hidden).unwrap().is_empty());
        // 不是 Waydroid 的入口不动。
        assert_eq!(fs::read_to_string(dir.join("org.gnome.Nautilus.desktop")).unwrap(), USER_APP);
        assert_eq!(apply(&dir, Mode::Shown).unwrap(), ["org.fdroid.fdroid"]);
        assert_eq!(fs::read_to_string(dir.join("waydroid.org.fdroid.fdroid.desktop")).unwrap(), USER_APP);

        fs::remove_dir_all(&dir).unwrap();
    }
}
