//! 各页面共用的上下文：提示条、后台任务、忙碌指示和会话操作。

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use droid_core::waydroid;
use gtk::{gio, glib};

#[derive(Clone)]
pub struct Ctx {
    pub window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    spinner: gtk::Spinner,
    busy: Rc<Cell<u32>>,
}

impl Ctx {
    pub fn new(window: &adw::ApplicationWindow, toasts: &adw::ToastOverlay, spinner: &gtk::Spinner) -> Self {
        Self {
            window: window.clone(),
            toasts: toasts.clone(),
            spinner: spinner.clone(),
            busy: Rc::default(),
        }
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(&glib::markup_escape_text(text)));
    }

    pub fn add_toast(&self, toast: adw::Toast) {
        self.toasts.add_toast(toast);
    }

    /// 在后台线程执行可能阻塞的操作，完成后回到主线程处理结果；期间标题栏显示忙碌指示。
    pub fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(T) + 'static,
    ) {
        self.spawn(work, done, true);
    }

    /// 和 `run` 一样，但不显示忙碌指示，用于定时刷新。
    pub fn run_quiet<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(T) + 'static,
    ) {
        self.spawn(work, done, false);
    }

    fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(T) + 'static,
        busy: bool,
    ) {
        if busy {
            self.set_busy(1);
        }
        let ctx = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(work).await;
            if busy {
                ctx.set_busy(-1);
            }
            match result {
                Ok(value) => done(value),
                Err(_) => ctx.toast("后台任务异常退出"),
            }
        });
    }

    fn set_busy(&self, delta: i32) {
        let count = self.busy.get().saturating_add_signed(delta);
        self.busy.set(count);
        self.spinner.set_spinning(count > 0);
        self.spinner.set_visible(count > 0);
    }

    pub fn start_session(&self) {
        self.toast("正在启动 Android 会话…");
        let ctx = self.clone();
        self.run(waydroid::start_session, move |result| {
            if let Err(err) = result {
                ctx.toast(&err.message);
            }
        });
    }

    pub fn stop_session(&self) {
        let ctx = self.clone();
        self.run(waydroid::stop_session, move |result| match result {
            Ok(()) => ctx.toast("Android 会话已停止"),
            Err(err) => ctx.toast(&err.message),
        });
    }

    pub fn restart_session(&self) {
        self.toast("正在重启 Android 会话…");
        let ctx = self.clone();
        self.run(waydroid::restart_session, move |result| {
            if let Err(err) = result {
                ctx.toast(&err.message);
            }
        });
    }

    pub fn show_full_ui(&self) {
        self.toast("正在打开完整的安卓界面…");
        let ctx = self.clone();
        self.run(waydroid::show_full_ui, move |result| {
            if let Err(err) = result {
                ctx.toast(&err.message);
            }
        });
    }
}
