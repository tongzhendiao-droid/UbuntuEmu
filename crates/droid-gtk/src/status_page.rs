//! 「运行状态」页：会话控制、多窗口模式和 Waydroid 信息。

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use droid_core::waydroid::{self, SessionState, Status};

use crate::context::Ctx;
use crate::window::SessionView;

pub struct StatusPage {
    ctx: Ctx,
    pub root: adw::PreferencesPage,
    session_row: adw::ActionRow,
    session_button: gtk::Button,
    ip_row: adw::ActionRow,
    display_row: adw::ActionRow,
    multi_window_row: adw::SwitchRow,
    version_row: adw::ActionRow,
    image_row: adw::ActionRow,
    container_row: adw::ActionRow,
    session_active: Cell<bool>,
    /// 程序在更新开关状态，这时的变化不是用户操作。
    syncing: Cell<bool>,
}

fn info_row(title: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle("—")
        .use_markup(false)
        .subtitle_selectable(true)
        .css_classes(["property"])
        .build()
}

fn link_row(title: &str, subtitle: &str, icon: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .activatable(true)
        .build();
    row.add_suffix(&gtk::Image::from_icon_name(icon));
    row
}

impl StatusPage {
    pub fn new(ctx: &Ctx) -> Rc<Self> {
        let session_button = gtk::Button::builder()
            .label("启动")
            .valign(gtk::Align::Center)
            .sensitive(false)
            .build();
        let session_row = adw::ActionRow::builder()
            .title("会话")
            .subtitle("正在查询…")
            .use_markup(false)
            .build();
        session_row.add_suffix(&session_button);
        let ip_row = info_row("安卓 IP 地址");
        let display_row = info_row("Wayland 显示接口");
        let full_ui_row = link_row("打开完整安卓界面", "在一个窗口里显示整个安卓桌面", "go-next-symbolic");
        let restart_row = link_row("重启会话", "切换多窗口模式等设置后需要重启", "view-refresh-symbolic");
        let session_group = adw::PreferencesGroup::builder().title("Android 会话").build();
        session_group.add(&session_row);
        session_group.add(&ip_row);
        session_group.add(&display_row);
        session_group.add(&full_ui_row);
        session_group.add(&restart_row);

        let multi_window_row = adw::SwitchRow::builder()
            .title("多窗口模式")
            .subtitle("每个安卓应用显示成独立的桌面窗口，可以固定到 Dock。会话运行时才能切换，切换后需要重启会话。")
            .sensitive(false)
            .build();
        let window_group = adw::PreferencesGroup::builder().title("窗口").build();
        window_group.add(&multi_window_row);

        let version_row = info_row("Waydroid 版本");
        let image_row = info_row("安卓镜像");
        let container_row = info_row("容器服务");
        let waydroid_group = adw::PreferencesGroup::builder().title("Waydroid").build();
        waydroid_group.add(&version_row);
        waydroid_group.add(&image_row);
        waydroid_group.add(&container_row);

        let root = adw::PreferencesPage::new();
        root.add(&session_group);
        root.add(&window_group);
        root.add(&waydroid_group);

        let page = Rc::new(Self {
            ctx: ctx.clone(),
            root,
            session_row,
            session_button,
            ip_row,
            display_row,
            multi_window_row,
            version_row,
            image_row,
            container_row,
            session_active: Cell::new(false),
            syncing: Cell::new(false),
        });

        let weak = Rc::downgrade(&page);
        page.session_button.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                if page.session_active.get() {
                    page.ctx.stop_session();
                } else {
                    page.ctx.start_session();
                }
            }
        });
        let ctx_for_full_ui = ctx.clone();
        full_ui_row.connect_activated(move |_| ctx_for_full_ui.show_full_ui());
        let ctx_for_restart = ctx.clone();
        restart_row.connect_activated(move |_| ctx_for_restart.restart_session());
        let weak = Rc::downgrade(&page);
        page.multi_window_row.connect_active_notify(move |row| {
            if let Some(page) = weak.upgrade() {
                page.toggle_multi_window(row.is_active());
            }
        });
        page
    }

    /// 轮询到的会话状态（只更新状态相关的部分）。
    pub fn set_session(&self, view: &SessionView) {
        let active = matches!(view, Ok(state) if state.is_active());
        self.session_active.set(active);
        match view {
            Ok(state) => self.session_row.set_subtitle(state.label()),
            Err(message) => self.session_row.set_subtitle(message),
        }
        self.session_button.set_label(if active { "停止" } else { "启动" });
        if active {
            self.session_button.remove_css_class("suggested-action");
        } else {
            self.session_button.add_css_class("suggested-action");
        }
        let actionable = matches!(
            view,
            Ok(SessionState::Stopped | SessionState::Running | SessionState::Frozen)
        );
        self.session_button.set_sensitive(actionable);
        if !active {
            self.multi_window_row.set_sensitive(false);
        }
    }

    /// 重新读取完整状态（会调用几次 waydroid，比轮询慢）。
    pub fn refresh(self: &Rc<Self>) {
        let page = self.clone();
        self.ctx.run_quiet(waydroid::status, move |status| page.show(&status));
    }

    fn show(&self, status: &Status) {
        let view = match &status.error {
            Some(err) => Err(err.message.clone()),
            None => Ok(status.session),
        };
        self.set_session(&view);
        self.ip_row.set_subtitle(status.ip_address.as_deref().unwrap_or("—"));
        self.display_row.set_subtitle(status.wayland_display.as_deref().unwrap_or("—"));
        self.version_row.set_subtitle(status.version.as_deref().unwrap_or("未安装"));
        let images = match (status.initialized, status.vendor_type.as_deref()) {
            (true, Some(vendor)) => format!("已下载（{vendor}）"),
            (true, None) => "已下载".to_string(),
            (false, _) => "未下载，需要运行 sudo waydroid init".to_string(),
        };
        self.image_row.set_subtitle(&images);
        self.container_row.set_subtitle(if status.container_available { "正常" } else { "连接不上" });

        self.syncing.set(true);
        self.multi_window_row.set_active(status.multi_window.unwrap_or(false));
        self.syncing.set(false);
        self.multi_window_row.set_sensitive(status.multi_window.is_some());
    }

    fn toggle_multi_window(self: &Rc<Self>, enabled: bool) {
        if self.syncing.get() {
            return;
        }
        self.multi_window_row.set_sensitive(false);
        let page = self.clone();
        self.ctx.run(move || waydroid::set_multi_window(enabled), move |result| {
            page.multi_window_row.set_sensitive(true);
            match result {
                Ok(()) => {
                    let title = if enabled {
                        "多窗口模式已开启，重启会话后生效"
                    } else {
                        "多窗口模式已关闭，重启会话后生效"
                    };
                    let toast = adw::Toast::builder().title(title).button_label("立即重启").timeout(10).build();
                    let ctx = page.ctx.clone();
                    toast.connect_button_clicked(move |_| ctx.restart_session());
                    page.ctx.add_toast(toast);
                }
                Err(err) => {
                    page.syncing.set(true);
                    page.multi_window_row.set_active(!enabled);
                    page.syncing.set(false);
                    page.ctx.toast(&err.message);
                }
            }
        });
    }
}
