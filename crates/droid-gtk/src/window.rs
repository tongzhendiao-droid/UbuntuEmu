//! 主窗口：标题栏、三个页面、拖放安装，以及会话状态轮询。

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use droid_core::waydroid::{self, SessionState};
use gtk::{gdk, gio, glib};

use crate::apps_page::AppsPage;
use crate::context::Ctx;
use crate::doctor_page::DoctorPage;
use crate::status_page::StatusPage;

/// 会话状态轮询间隔。查询走 D-Bus，一次只要几毫秒。
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 一次轮询的结果：会话状态，或者查不到的原因。
pub type SessionView = Result<SessionState, String>;

struct Pages {
    apps: Rc<AppsPage>,
    status: Rc<StatusPage>,
    doctor: Rc<DoctorPage>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Droid")
        .default_width(1000)
        .default_height(700)
        .build();
    let toasts = adw::ToastOverlay::new();
    let spinner = gtk::Spinner::builder().visible(false).tooltip_text("正在处理…").build();
    let ctx = Ctx::new(&window, &toasts, &spinner);
    let pages = Rc::new(Pages {
        apps: AppsPage::new(&ctx),
        status: StatusPage::new(&ctx),
        doctor: DoctorPage::new(&ctx),
    });

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(&pages.apps.root, Some("apps"), "应用", "view-app-grid-symbolic");
    stack.add_titled_with_icon(&pages.status.root, Some("status"), "运行状态", "phone-symbolic");
    stack.add_titled_with_icon(&pages.doctor.root, Some("doctor"), "环境诊断", "emblem-ok-symbolic");
    toasts.set_child(Some(&stack));

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    let install = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("安装 APK（Ctrl+O）")
        .action_name("win.install-apk")
        .build();
    header.pack_start(&install);
    let menu = gio::Menu::new();
    menu.append(Some("刷新"), Some("win.refresh"));
    menu.append(Some("关于 Droid"), Some("app.about"));
    menu.append(Some("退出"), Some("app.quit"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .primary(true)
        .tooltip_text("主菜单")
        .build();
    header.pack_end(&menu_button);
    header.pack_end(&spinner);

    let switcher_bar = adw::ViewSwitcherBar::builder().stack(&stack).build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_bottom_bar(&switcher_bar);
    toolbar.set_content(Some(&toasts));
    window.set_content(Some(&toolbar));

    // 窗口变窄时，把页面切换按钮挪到底部。
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        640.0,
        adw::LengthUnit::Sp,
    ));
    narrow.add_setter(&header, "title-widget", Some(&None::<gtk::Widget>.to_value()));
    narrow.add_setter(&switcher_bar, "reveal", Some(&true.to_value()));
    window.add_breakpoint(narrow);

    add_actions(&window, &pages);
    accept_dropped_apks(&toasts, &pages.apps);
    poll_session(&ctx, &pages);
    pages.status.refresh();
    pages.doctor.run();
    snapshot_if_requested(&window, &stack);
    window
}

fn add_actions(window: &adw::ApplicationWindow, pages: &Rc<Pages>) {
    let apps = pages.apps.clone();
    let install = gio::ActionEntry::builder("install-apk")
        .activate(move |_: &adw::ApplicationWindow, _, _| apps.choose_apk())
        .build();
    let pages = pages.clone();
    let refresh = gio::ActionEntry::builder("refresh")
        .activate(move |_: &adw::ApplicationWindow, _, _| {
            pages.apps.reload();
            pages.status.refresh();
            pages.doctor.run();
        })
        .build();
    window.add_action_entries([install, refresh]);
}

fn accept_dropped_apks(target: &impl IsA<gtk::Widget>, apps: &Rc<AppsPage>) {
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    let apps = apps.clone();
    drop.connect_drop(move |_, value, _, _| {
        let Ok(files) = value.get::<gdk::FileList>() else {
            return false;
        };
        let apks: Vec<PathBuf> = files.files().iter().filter_map(|file| file.path()).collect();
        if apks.is_empty() {
            return false;
        }
        apps.install(apks);
        true
    });
    target.add_controller(drop);
}

/// 定时查询会话状态，状态变化时通知各页面。
fn poll_session(ctx: &Ctx, pages: &Rc<Pages>) {
    let last: Rc<RefCell<Option<SessionView>>> = Rc::default();
    let in_flight = Rc::new(Cell::new(false));
    let ctx = ctx.clone();
    let pages = pages.clone();
    let poll = move || {
        if in_flight.replace(true) {
            return;
        }
        let (in_flight, last, pages) = (in_flight.clone(), last.clone(), pages.clone());
        ctx.run_quiet(
            || waydroid::session().map(|session| session.state).map_err(|err| err.message),
            move |view: SessionView| {
                in_flight.set(false);
                let previous = last.replace(Some(view.clone()));
                if previous.as_ref() == Some(&view) {
                    return;
                }
                pages.apps.set_session(&view);
                pages.status.set_session(&view);
                let was_active = matches!(previous, Some(Ok(state)) if state.is_active());
                let is_active = matches!(view, Ok(state) if state.is_active());
                if previous.is_some() && was_active != is_active {
                    // 会话起停之后，Waydroid 会更新应用列表和系统属性。
                    pages.status.refresh();
                    pages.apps.reload();
                }
            },
        );
    };
    poll();
    glib::timeout_add_local(POLL_INTERVAL, move || {
        poll();
        glib::ControlFlow::Continue
    });
}

/// 开发用：设置了 DROID_SNAPSHOT_DIR 时，依次把三个页面截成 PNG 存进这个目录，然后关闭窗口。
fn snapshot_if_requested(window: &adw::ApplicationWindow, stack: &adw::ViewStack) {
    let Some(dir) = std::env::var_os("DROID_SNAPSHOT_DIR").map(PathBuf::from) else {
        return;
    };
    let (window, stack) = (window.clone(), stack.clone());
    glib::spawn_future_local(async move {
        // 等第一轮数据加载完。
        glib::timeout_future(Duration::from_secs(4)).await;
        for page in ["apps", "status", "doctor"] {
            stack.set_visible_child_name(page);
            glib::timeout_future(Duration::from_millis(800)).await;
            let path = dir.join(format!("{page}.png"));
            if let Err(err) = save_png(window.upcast_ref(), &path) {
                eprintln!("截图失败 {}：{err}", path.display());
            }
        }
        window.close();
    });
}

fn save_png(widget: &gtk::Widget, path: &Path) -> Result<(), String> {
    let (width, height) = (widget.width(), widget.height());
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(widget)).snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node().ok_or("页面是空的")?;
    let renderer = widget.native().and_then(|native| native.renderer()).ok_or("拿不到渲染器")?;
    let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    renderer
        .render_texture(node, Some(&bounds))
        .save_to_png(path)
        .map_err(|err| err.to_string())
}
