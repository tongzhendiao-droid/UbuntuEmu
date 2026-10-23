//! 「应用」页：安卓应用网格、搜索、打开应用和安装 APK。

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use droid_core::apps::{self, AndroidApp};
use droid_core::paths;
use droid_core::waydroid::{self, SessionState};
use gtk::{gio, glib, pango};

use crate::context::Ctx;
use crate::window::SessionView;

const STOPPED_HINT: &str = "Android 会话没有运行。点开任意应用会自动启动，第一次大约需要十几秒。";
/// 包括启动会话的时间。
const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

pub struct AppsPage {
    ctx: Ctx,
    pub root: gtk::Box,
    banner: adw::Banner,
    search: gtk::SearchEntry,
    content: gtk::Stack,
    grid: gtk::FlowBox,
    empty: adw::StatusPage,
    apps: RefCell<Vec<AndroidApp>>,
    session_active: Cell<bool>,
    reload_queued: Cell<bool>,
    monitor: RefCell<Option<gio::FileMonitor>>,
}

impl AppsPage {
    pub fn new(ctx: &Ctx) -> Rc<Self> {
        let banner = adw::Banner::builder().title(STOPPED_HINT).use_markup(false).build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text("搜索应用名称或包名")
            .width_request(360)
            .halign(gtk::Align::Center)
            .build();
        let drop_hint = gtk::Label::builder()
            .label("把 .apk 文件拖进窗口即可安装")
            .css_classes(["dim-label", "caption"])
            .build();
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(18)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        header.append(&search);
        header.append(&drop_hint);

        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .valign(gtk::Align::Start)
            .column_spacing(6)
            .row_spacing(6)
            .min_children_per_line(3)
            .max_children_per_line(10)
            .margin_top(6)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&adw::Clamp::builder().maximum_size(1100).child(&grid).build())
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("view-app-grid-symbolic")
            .vexpand(true)
            .build();
        let content = gtk::Stack::new();
        content.add_named(&scroller, Some("grid"));
        content.add_named(&empty, Some("empty"));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&banner);
        root.append(&header);
        root.append(&content);

        let page = Rc::new(Self {
            ctx: ctx.clone(),
            root,
            banner,
            search,
            content,
            grid,
            empty,
            apps: RefCell::default(),
            session_active: Cell::new(false),
            reload_queued: Cell::new(false),
            monitor: RefCell::default(),
        });

        let ctx = ctx.clone();
        page.banner.connect_button_clicked(move |_| ctx.start_session());
        let weak = Rc::downgrade(&page);
        page.search.connect_search_changed(move |_| {
            if let Some(page) = weak.upgrade() {
                page.rebuild();
            }
        });
        page.watch_desktop_files();
        page.reload();
        page
    }

    pub fn set_session(&self, view: &SessionView) {
        let active = matches!(view, Ok(state) if state.is_active());
        self.session_active.set(active);
        self.banner.set_revealed(!active);
        match view {
            Ok(SessionState::Starting) => {
                self.banner.set_title("Android 正在启动…");
                self.banner.set_button_label(None);
            }
            Ok(_) => {
                self.banner.set_title(STOPPED_HINT);
                self.banner.set_button_label(Some("启动会话"));
            }
            Err(message) => {
                self.banner.set_title(message);
                self.banner.set_button_label(None);
            }
        }
    }

    /// 重新读取应用列表。
    pub fn reload(self: &Rc<Self>) {
        let page = self.clone();
        self.ctx.run_quiet(apps::installed, move |apps| {
            page.apps.replace(apps);
            page.rebuild();
        });
    }

    fn rebuild(self: &Rc<Self>) {
        self.grid.remove_all();
        let query = self.search.text().trim().to_lowercase();
        let apps = self.apps.borrow();
        let matches = apps.iter().filter(|app| {
            query.is_empty()
                || app.name.to_lowercase().contains(&query)
                || app.package.to_lowercase().contains(&query)
        });
        let mut shown = 0;
        for app in matches {
            let tile = self.tile(app);
            self.grid.append(&tile);
            // 焦点留给里面的按钮，免得每个应用要按两次 Tab。
            if let Some(child) = tile.parent() {
                child.set_focusable(false);
            }
            shown += 1;
        }
        if shown > 0 {
            self.content.set_visible_child_name("grid");
            return;
        }
        if apps.is_empty() {
            self.empty.set_title("还没有安卓应用");
            self.empty.set_description(Some(
                "启动一次 Android 会话后，这里会列出已安装的应用。也可以把 APK 文件拖进窗口安装。",
            ));
        } else {
            self.empty.set_title("没有匹配的应用");
            self.empty.set_description(Some("换个关键词试试"));
        }
        self.content.set_visible_child_name("empty");
    }

    fn tile(self: &Rc<Self>, app: &AndroidApp) -> gtk::Button {
        let icon = match &app.icon {
            Some(path) => gtk::Image::from_gicon(&gio::FileIcon::new(&gio::File::for_path(path))),
            None => gtk::Image::from_icon_name("application-x-executable"),
        };
        icon.set_pixel_size(64);
        let label = gtk::Label::builder()
            .label(&app.name)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .justify(gtk::Justification::Center)
            .lines(2)
            .ellipsize(pango::EllipsizeMode::End)
            .max_width_chars(12)
            .build();
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        body.append(&icon);
        body.append(&label);
        let button = gtk::Button::builder()
            .child(&body)
            .css_classes(["flat"])
            .tooltip_text(&app.package)
            .build();
        let page = Rc::downgrade(self);
        let (package, name) = (app.package.clone(), app.name.clone());
        button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                page.launch(&package, &name);
            }
        });
        button
    }

    fn launch(&self, package: &str, name: &str) {
        let message = if self.session_active.get() {
            format!("正在打开 {name}…")
        } else {
            format!("正在启动 Android 并打开 {name}，第一次大约需要十几秒…")
        };
        self.ctx.toast(&message);
        let ctx = self.ctx.clone();
        let package = package.to_string();
        self.ctx.run(move || waydroid::launch_app(&package), move |result| {
            if let Err(err) = result {
                ctx.toast(&err.message);
            }
        });
    }

    /// 弹出文件选择框，安装选中的 APK。
    pub fn choose_apk(self: &Rc<Self>) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("安卓安装包（*.apk）"));
        filter.add_suffix("apk");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title("选择要安装的 APK")
            .modal(true)
            .filters(&filters)
            .default_filter(&filter)
            .build();
        let page = self.clone();
        dialog.open_multiple(Some(&self.ctx.window), gio::Cancellable::NONE, move |result| {
            // 用户取消时也会走到错误分支，不用提示。
            let Ok(files) = result else {
                return;
            };
            let apks: Vec<PathBuf> = (0..files.n_items())
                .filter_map(|index| files.item(index).and_downcast::<gio::File>())
                .filter_map(|file| file.path())
                .collect();
            if !apks.is_empty() {
                page.install(apks);
            }
        });
    }

    /// 依次安装 APK。会话没运行时会先启动。
    pub fn install(self: &Rc<Self>, apks: Vec<PathBuf>) {
        let names: Vec<String> = apks
            .iter()
            .map(|apk| apk.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned()))
            .collect();
        let starting = if self.session_active.get() { "" } else { "（会先启动 Android 会话）" };
        self.ctx.toast(&format!("正在安装 {}…{starting}", names.join("、")));
        let ctx = self.ctx.clone();
        self.ctx.run(
            move || {
                apks.iter()
                    .map(|apk| waydroid::install_apk(apk, INSTALL_TIMEOUT))
                    .collect::<Vec<_>>()
            },
            move |results| {
                for (name, result) in names.iter().zip(results) {
                    let message = match result {
                        Ok(report) if report.new_apps.is_empty() => {
                            format!("{name} 已执行安装，但没发现新应用（可能是更新，或 APK 只带 ARM 库）")
                        }
                        Ok(report) => {
                            let installed: Vec<&str> = report.new_apps.iter().map(|app| app.name.as_str()).collect();
                            format!("已安装 {}", installed.join("、"))
                        }
                        Err(err) => format!("{name} 安装失败：{}", err.message),
                    };
                    ctx.toast(&message);
                }
            },
        );
    }

    /// 应用的 .desktop 文件有变化时（安装、卸载、会话启动时 Waydroid 重写）刷新列表。
    fn watch_desktop_files(self: &Rc<Self>) {
        let dir = gio::File::for_path(paths::applications_dir());
        let Ok(monitor) = dir.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) else {
            return;
        };
        let page = Rc::downgrade(self);
        monitor.connect_changed(move |_, file, _, _| {
            let is_android_app = file
                .basename()
                .is_some_and(|name| name.to_string_lossy().starts_with("waydroid."));
            let Some(page) = page.upgrade().filter(|_| is_android_app) else {
                return;
            };
            // Waydroid 会一次改写所有应用的文件，合并成一次刷新。
            if page.reload_queued.replace(true) {
                return;
            }
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                page.reload_queued.set(false);
                page.reload();
            });
        });
        self.monitor.replace(Some(monitor));
    }
}
