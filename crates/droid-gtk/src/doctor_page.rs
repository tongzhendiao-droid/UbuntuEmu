//! 「环境诊断」页：展示 droid_core::doctor 的检查结果。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use droid_core::doctor::{self, Level, Report};

use crate::context::Ctx;

pub struct DoctorPage {
    ctx: Ctx,
    pub root: adw::PreferencesPage,
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
    report: RefCell<Option<Report>>,
    running: Cell<bool>,
}

impl DoctorPage {
    pub fn new(ctx: &Ctx) -> Rc<Self> {
        let copy = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("复制 JSON 报告")
            .css_classes(["flat"])
            .build();
        let rerun = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("重新检测")
            .css_classes(["flat"])
            .build();
        let actions = gtk::Box::builder().spacing(6).build();
        actions.append(&copy);
        actions.append(&rerun);
        let group = adw::PreferencesGroup::builder()
            .title("环境诊断")
            .description("检查运行 Waydroid 需要的系统条件")
            .header_suffix(&actions)
            .build();
        let root = adw::PreferencesPage::new();
        root.add(&group);

        let page = Rc::new(Self {
            ctx: ctx.clone(),
            root,
            group,
            rows: RefCell::default(),
            report: RefCell::default(),
            running: Cell::new(false),
        });
        let weak = Rc::downgrade(&page);
        rerun.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.run();
            }
        });
        let weak = Rc::downgrade(&page);
        copy.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.copy_report();
            }
        });
        page
    }

    pub fn run(self: &Rc<Self>) {
        if self.running.replace(true) {
            return;
        }
        self.group.set_description(Some("正在检测…"));
        let page = self.clone();
        self.ctx.run(doctor::run, move |report| {
            page.running.set(false);
            page.show(&report);
            page.report.replace(Some(report));
        });
    }

    fn show(&self, report: &Report) {
        for row in self.rows.borrow_mut().drain(..) {
            self.group.remove(&row);
        }
        let count = |level| report.checks.iter().filter(|check| check.level == level).count();
        let summary = match (count(Level::Error), count(Level::Warning)) {
            (0, 0) => "一切正常".to_string(),
            (0, warnings) => format!("可以使用，有 {warnings} 项需要注意"),
            (errors, 0) => format!("有 {errors} 项问题需要处理"),
            (errors, warnings) => format!("有 {errors} 项问题需要处理，另有 {warnings} 项需要注意"),
        };
        self.group.set_description(Some(&summary));

        let mut rows = self.rows.borrow_mut();
        for check in &report.checks {
            let subtitle = match &check.hint {
                Some(hint) => format!("{}\n建议：{hint}", check.summary),
                None => check.summary.clone(),
            };
            let row = adw::ActionRow::builder()
                .title(check.title)
                .subtitle(subtitle)
                .use_markup(false)
                .subtitle_selectable(true)
                .build();
            let (icon, style) = match check.level {
                Level::Ok => ("emblem-ok-symbolic", "success"),
                Level::Info => ("dialog-information-symbolic", "accent"),
                Level::Warning => ("dialog-warning-symbolic", "warning"),
                Level::Error => ("dialog-error-symbolic", "error"),
            };
            let image = gtk::Image::from_icon_name(icon);
            image.add_css_class(style);
            row.add_prefix(&image);
            self.group.add(&row);
            rows.push(row);
        }
    }

    fn copy_report(&self) {
        let Some(report) = self.report.borrow().clone() else {
            self.ctx.toast("诊断还没有完成");
            return;
        };
        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
        self.root.clipboard().set_text(&json);
        self.ctx.toast("诊断报告已复制（JSON）");
    }
}
