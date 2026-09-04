//! 首页（对应上一代 `pages/Home.tsx`）：hero 大标题 + meta 信息带 + 运行实例列表。

use crate::app::{Dshnext, Message};
use crate::pages::{Page, fmt_uptime};
use crate::theme::{HERO_NUM_SIZE, Palette};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono, txt, txt_bold};
use iced::widget::{Column, Row, column, container, row, space};
use iced::{Alignment, Border, Element, Fill, Length, Padding, Theme};

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();
    column![hero(app, pal), instances(app, pal)]
        .spacing(18)
        .into()
}

/// hero 卡（CSS `.hero`）：eyebrow + 大标题 + 描述 + 操作 + meta 带。
fn hero<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let selected = app.selected.as_str();
    let running = app.running(selected);
    let ready = app.env_ready();

    let title = if selected.is_empty() {
        "尚无版本".to_string()
    } else {
        selected.to_string()
    };
    let desc = if ready {
        "选择一个版本，一键启动 harness 的 Web 界面。"
    } else {
        "还没检测到 dsh，请先到「环境」页完成安装。"
    };

    let left = column![
        txt("DEEPSEEK HARNESS").size(9.5).color(pal.text_3),
        txt_bold(title).size(HERO_NUM_SIZE * 0.62).color(pal.text),
        txt(desc).size(12).color(pal.text_2),
    ]
    .spacing(6);

    // 版本下拉：选项是 profile 名（运行中的加后缀，与上一代一致）。
    let options: Vec<String> = app
        .profiles
        .iter()
        .map(|p| {
            if app.running(&p.name).is_some() {
                format!("{} · 运行中", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect();
    let current = if selected.is_empty() {
        None
    } else if running.is_some() {
        Some(format!("{selected} · 运行中"))
    } else {
        Some(selected.to_string())
    };

    let mut actions = Row::new().spacing(12).align_y(Alignment::Center);
    if !options.is_empty() {
        actions = actions.push(widgets::dropdown(
            // pick_list 需要 &'a [T]；这里的 Vec 活到 Element 构造完，
            // 用 leak 会漏内存，改为让 widgets::dropdown 接 owned Vec。
            options.clone(),
            current,
            |v: String| Message::Select(v.split(" · ").next().unwrap_or(&v).to_string()),
            190.0,
            15, // hero：与 48px 的启动按钮等高（上一代 .hero-actions .select）
            pal,
        ));
    }
    actions = match running {
        Some(_) => actions
            .push(hero_btn(
                app,
                "hero.stop",
                "停止运行",
                Variant::Teal,
                Message::Stop(selected.to_string()),
                pal,
            ))
            .push(hero_btn(
                app,
                "hero.open",
                "打开界面",
                Variant::Ghost,
                Message::OpenUi(selected.to_string()),
                pal,
            )),
        None => actions.push(hero_btn_maybe(
            app,
            "hero.start",
            "启动 harness",
            Variant::Primary,
            (ready && !selected.is_empty()).then(|| Message::Start(selected.to_string())),
            pal,
        )),
    };

    let meta = meta_strip(app, running, pal);

    container(
        column![
            row![left, space::horizontal(), actions]
                .width(Fill)
                .align_y(Alignment::End),
            widgets::divider(pal),
            meta,
        ]
        .spacing(22),
    )
    .width(Fill)
    .padding(Padding::from([30, 34]))
    .style(move |_theme: &Theme| container::Style {
        text_color: Some(pal.text),
        background: Some(pal.surface_1.into()),
        border: Border {
            color: pal.card_border,
            width: 1.0,
            radius: 20.0.into(),
        },
        shadow: pal.shadow_card,
        snap: true,
    })
    .into()
}

/// meta 信息带（CSS `.meta-strip`）：状态 / 时长 / 地址 / PID / 插件数。
fn meta_strip<'a>(
    app: &'a Dshnext,
    running: Option<&'a crate::core::procman::ProcStatus>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let plugin_count = app
        .profiles
        .iter()
        .find(|p| p.name == app.selected)
        .map(|p| p.dependencies.len())
        .unwrap_or(0);

    let status: Element<'_, Message> = match running {
        Some(_) => row![widgets::dot(pal.ok), txt_bold("运行中").size(13).color(pal.text)]
            .spacing(7)
            .align_y(Alignment::Center)
            .into(),
        None => txt_bold("空闲").size(13).color(pal.text).into(),
    };

    // 空闲时这只是「将要用的」端口，用弱色区分，否则整行读起来像已经在服务。
    let (addr, addr_color) = match running {
        Some(p) => (
            p.url
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .to_string(),
            pal.text,
        ),
        None => (format!("127.0.0.1:{} 待用", app.config.port), pal.text_3),
    };

    // 每格固定宽度：内容自适应会让列间距参差（第一版就是这样，看着像没对齐）。
    row![
        meta_cell("状态", status, 130.0, pal),
        meta_cell(
            "运行时长",
            txt_bold(
                running
                    .map(|p| fmt_uptime(p.uptime_secs))
                    .unwrap_or_else(|| DASH.into())
            )
            .size(13)
            .color(pal.text)
            .into(),
            150.0,
            pal
        ),
        meta_cell("WEB 地址", mono(addr).size(12).color(addr_color).into(), 190.0, pal),
        meta_cell(
            "进程 PID",
            // 有值走等宽（数字对齐），无值走正文——同一个破折号在两种字体下宽度不同，
            // 混用会让两个空位看起来是不同符号。
            match running {
                Some(p) => mono(p.pid.to_string()).size(12).color(pal.text).into(),
                None => txt_bold(DASH).size(13).color(pal.text).into(),
            },
            130.0,
            pal
        ),
        meta_cell(
            "插件",
            txt_bold(format!("{plugin_count} 个")).size(13).color(pal.text).into(),
            100.0,
            pal
        ),
    ]
    .align_y(Alignment::Start)
    .into()
}

/// 空值占位符。全局只有这一处定义，避免 em/en dash 混用。
const DASH: &str = "—";

fn meta_cell<'a>(
    label: &'static str,
    value: Element<'a, Message>,
    width: f32,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(column![txt(label).size(9).color(pal.text_3), value].spacing(3))
        .width(Length::Fixed(width))
        .into()
}

/// 运行中实例列表（CSS `.card.flush` + `.list`）。
fn instances<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let mut col = Column::new()
        .push(card::card_title("运行中的实例"))
        .push(card::card_sub(
            "每个版本独立进程，可同时运行多个（注意端口不要冲突）。",
            pal,
        ))
        .spacing(4);

    if app.procs.is_empty() {
        col = col
            .push(space::vertical().height(8.0))
            .push(widgets::empty("当前没有运行中的实例", pal));
    } else {
        col = col.push(space::vertical().height(8.0));
        for (i, p) in app.procs.iter().enumerate() {
            if i > 0 {
                col = col.push(widgets::divider(pal));
            }
            col = col.push(instance_row(app, p, pal));
        }
    }

    card::card(col, pal)
}

fn instance_row<'a>(
    app: &'a Dshnext,
    p: &'a crate::core::procman::ProcStatus,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let main = column![
        row![
            txt_bold(p.profile.clone()).size(13).color(pal.text),
            widgets::tag("运行中", Tone::Ok, pal),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        mono(format!(
            "{} · PID {} · {}",
            p.url,
            p.pid,
            fmt_uptime(p.uptime_secs)
        ))
        .size(10.5)
        .color(pal.text_3),
    ]
    .spacing(2);

    let actions = row![
        row_btn(app, "inst.open", "打开界面", Variant::Secondary, Message::OpenUi(p.profile.clone()), pal),
        row_btn(app, "inst.log", "看日志", Variant::Secondary, Message::Goto(Page::Console), pal),
        row_btn(app, "inst.stop", "停止", Variant::QuietDanger, Message::Stop(p.profile.clone()), pal),
    ]
    .spacing(7);

    widgets::list_row(
        Some(widgets::icon_badge(widgets::dot(pal.ok), pal)),
        main,
        actions,
        false,
        pal,
    )
}

// ---- 按钮小工具：hero 尺寸与行内小尺寸 ----

fn hero_btn<'a>(
    app: &'a Dshnext,
    key: &'static str,
    label: &'static str,
    variant: Variant,
    msg: Message,
    pal: &'static Palette,
) -> Element<'a, Message> {
    hero_btn_maybe(app, key, label, variant, Some(msg), pal)
}

fn hero_btn_maybe<'a>(
    app: &'a Dshnext,
    key: &'static str,
    label: &'static str,
    variant: Variant,
    msg: Option<Message>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let enabled = msg.is_some();
    button::btn(
        Spec::new(key, label, variant)
            .size(BtnSize::Hero)
            .disabled(!enabled),
        pal,
        &app.anim,
        msg,
        Some(Message::HoverEnter(key)),
        Some(Message::HoverExit(key)),
    )
}

/// 列表行里的小按钮。行数随数据变，hover key 会重复——所以不做过渡（传 None）。
pub fn row_btn<'a>(
    app: &'a Dshnext,
    key: &'static str,
    label: &'static str,
    variant: Variant,
    msg: Message,
    pal: &'static Palette,
) -> Element<'a, Message> {
    button::btn(
        Spec::new(key, label, variant).size(BtnSize::Small),
        pal,
        &app.anim,
        Some(msg),
        None,
        None,
    )
}
