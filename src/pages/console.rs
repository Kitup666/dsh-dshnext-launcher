//! 控制台页（对应上一代 `pages/Console.tsx`）：来源过滤 + 自动滚动 + 复制/清空。
//!
//! 大文本性能（DESIGN.md §7.3）：2000 行全量构造 Element 会让每帧 layout 变慢。
//! 这里的做法是**只把尾部 `WINDOW` 行构造出来**——日志的使用场景就是看最新的，
//! 往上翻历史交给「复制」导出。比 `text_editor` 简单，也避开了它的编辑态包袱。

use crate::app::{Dshnext, Message};
use crate::core::event::LogStream;
use crate::pages::fmt_time;
use crate::theme::{FS_MICRO, FS_TINY, GAP_SECTION, Palette, R_CTL};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono};
use iced::widget::{Column, column, container, row, scrollable, space};
use iced::{Alignment, Border, Element, Fill, Padding, Shadow, Theme};

/// 「全部来源」的过滤值。
pub const ALL: &str = "全部来源";

/// 一次最多渲染多少行。40 行铺满一屏还有余量。
const WINDOW: usize = 200;

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let head = widgets::page_head(
        "控制台",
        "harness 进程、插件安装和环境安装的实时输出（最多保留 2000 行）。",
        None, app.narrow(),
        pal,
    );

    let shown: Vec<&crate::app::LogLine> = visible(app).collect();
    let total = shown.len();

    // 来源下拉：全部 + 日志里出现过的 + 已有 profile（与上一代一致）
    let mut sources: Vec<String> = vec![ALL.to_string()];
    let mut seen: Vec<String> = app.logs.iter().map(|l| l.profile.clone()).collect();
    seen.extend(app.profiles.iter().map(|p| p.name.clone()));
    seen.sort();
    seen.dedup();
    sources.extend(seen);

    let toolbar = row![
        widgets::dropdown(
            sources,
            Some(app.log_filter.clone()),
            Message::SetLogFilter,
            180.0,
            9,
            pal
        ),
        widgets::check("自动滚动", app.auto_scroll, Message::ToggleAutoScroll, pal),
        widgets::tag(format!("{total} 行"), Tone::Neutral, pal),
        space::horizontal(),
        tool_btn(
            app,
            "con.copy",
            "复制",
            (total > 0).then_some(Message::CopyLogs),
            pal
        ),
        tool_btn(
            app,
            "con.clear",
            "清空",
            (!app.logs.is_empty()).then_some(Message::ClearLogs),
            pal
        ),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let body: Element<'_, Message> = if total == 0 {
        // 空状态要在整块日志区里居中，而不是贴在顶部——控制台卡片是撑满高度的，
        // 顶部对齐会在下面留一大片没有边界的黑，看着像布局坏了。
        container(widgets::empty_state(
            crate::ui::icon::CONSOLE,
            "暂无输出",
            "启动 harness 或安装插件后，日志会实时出现在这里。",
            None,
            pal,
        ))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text_3),
            background: Some(pal.bg_log.into()),
            border: Border {
                color: pal.border,
                width: 0.3,
                radius: R_CTL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
    } else {
        let skipped = total.saturating_sub(WINDOW);
        let mut lines = Column::new().spacing(1);
        if skipped > 0 {
            lines = lines.push(
                mono(format!("…前 {skipped} 行已折叠（用「复制」导出完整日志）"))
                    .size(FS_MICRO)
                    .color(pal.text_3),
            );
        }
        for l in shown.iter().skip(skipped) {
            lines = lines.push(log_row(app, l, pal));
        }

        let sc = scrollable(container(lines).width(Fill).padding(Padding::from([12, 16])))
            .direction(scrollable::Direction::Vertical(widgets::slim_scrollbar()))
            .style(widgets::slim_scroll_style(pal))
            .width(Fill)
            .height(Fill);
        // 自动滚动：0.14 新增的 auto_scroll 正是「内容增长时跟随尾部」，
        // 比 anchor_bottom 好——后者会把视口钉死在底部，翻不上去。
        let sc = sc.auto_scroll(app.auto_scroll);
        container(sc)
            .width(Fill)
            .height(Fill)
            .style(move |_theme: &Theme| container::Style {
                text_color: Some(pal.text_2),
                background: Some(pal.bg_log.into()),
                border: Border {
                    color: pal.border,
                    width: 0.3,
                    radius: R_CTL.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .into()
    };

    // 控制台要占满剩余高度：卡片内部自己滚，页面外层不滚（见 pages::view）。
    // 这里不用 page_stack：它给的是 Shrink 的 Column，撑不满高度。
    // 错峰入场：页头先落位，日志卡迟到 12%。
    let t = app.anim.value(crate::ui::anim::PAGE);
    column![
        crate::ui::reveal_at(head, t, crate::ui::anim::PAGE_SHIFT, 0.0),
        // 磨砂大卡（与其他页的卡片同一条 shader 路径：场 + 修正 + 颗粒），
        // 占满剩余高度，卡片内部自己滚（见 pages::view）。
        crate::ui::reveal_at(
            card::card_fill(
                column![toolbar, body]
                    .spacing(14)
                    .width(Fill)
                    .height(Fill),
                pal,
            ),
            t,
            crate::ui::anim::PAGE_SHIFT,
            0.12,
        ),
    ]
    .spacing(GAP_SECTION)
    .height(Fill)
    .into()
}

/// 当前过滤下可见的日志。
fn visible(app: &Dshnext) -> impl Iterator<Item = &crate::app::LogLine> {
    let all = app.log_filter == ALL;
    let filter = app.log_filter.clone();
    app.logs
        .iter()
        .filter(move |l| all || l.profile == filter)
}

/// 供「复制」用的纯文本（格式与上一代一致）。
pub fn visible_text(app: &Dshnext) -> String {
    visible(app)
        .map(|l| format!("[{}] [{}] {}", fmt_time(l.ts), l.profile, l.line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn log_row<'a>(
    app: &'a Dshnext,
    l: &'a crate::app::LogLine,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let color = match l.stream {
        LogStream::Stdout => pal.text,
        LogStream::Stderr => pal.bad,
        LogStream::System => pal.accent,
        LogStream::Plugin => pal.teal,
    };
    let mut r = row![mono(fmt_time(l.ts)).size(FS_MICRO).color(pal.text_3)].spacing(9);
    // 「全部来源」时标出归属，与上一代一致
    if app.log_filter == ALL {
        r = r.push(
            mono(format!("[{}]", l.profile))
                .size(FS_MICRO)
                .color(pal.text_3),
        );
    }
    r.push(mono(l.line.as_str()).size(FS_TINY).color(color)).into()
}

fn tool_btn<'a>(
    app: &'a Dshnext,
    key: &'static str,
    label: &'static str,
    msg: Option<Message>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let enabled = msg.is_some();
    button::btn(
        Spec::new(key, label, Variant::Secondary)
            .size(BtnSize::Small)
            .disabled(!enabled),
        pal,
        &app.anim,
        msg,
        enabled.then_some(Message::HoverEnter(key)),
        enabled.then_some(Message::HoverExit(key)),
    )
}
