//! 插件管理页（对应上一代 `pages/Plugins.tsx`）：已装列表 / 插件市场两个分页。

use crate::app::{Dshnext, MarketSort, Message, PluginTab};
use crate::pages::home::row_btn;
use crate::theme::{FS_BODY, FS_SMALL, FS_TINY, Palette};
use crate::ui::anim::{self, PAGE_SHIFT};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::icon;
use crate::ui::modal::Dialog;
use crate::ui::reveal_at;
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono, txt, txt_bold};
use iced::widget::{container, Column, column, row, space};
use iced::{Alignment, Element, Fill};

/// 插件市场每页条数（全构造上百条会拖慢每帧 layout）。
const MARKET_PAGE_SIZE: usize = 60;

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let head = widgets::page_head(
        "插件管理",
        "harness 里一切能力都是插件。每个版本有独立的插件集，互不影响。",
        None, app.narrow(),
        pal,
    );

    // 错峰入场：页头 → 工具条 → 列表卡，依次落位。
    let t = app.anim.value(anim::PAGE);
    widgets::page_stack(
        reveal_at(head, t, PAGE_SHIFT, 0.0),
        Column::new()
            .push(reveal_at(toolbar_card(app, pal), t, PAGE_SHIFT, 0.12))
            .push(reveal_at(list_card(app, pal), t, PAGE_SHIFT, 0.24)),
    )
    .into()
}

/// 工具条卡：版本选择 + 搜索 + 分页 + 手动安装。
fn toolbar_card<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let profile_names: Vec<String> = app.profiles.iter().map(|p| p.name.clone()).collect();
    let current = (!app.selected.is_empty()).then(|| app.selected.clone());

    let mut r = row![txt("版本").size(FS_SMALL).color(pal.text_2)]
        .spacing(10)
        .align_y(Alignment::Center);

    if profile_names.is_empty() {
        r = r.push(txt("（无可用版本）").size(FS_SMALL).color(pal.text_3));
    } else {
        r = r.push(widgets::dropdown(
            profile_names,
            current,
            Message::Select,
            180.0,
            9,
            pal,
        ));
    }

    // 窄窗分两行：搜索框必须保住宽度（它是 Fill），否则被下拉+分段+按钮
    // 挤成一条竖缝（截图实测几乎消失）。
    let input = widgets::input(
        "搜索插件名称或描述…",
        &app.query,
        Message::Query,
        false,
        pal,
    );
    let tabs = widgets::segmented(
        vec![
            (
                "已安装",
                app.plugin_tab == PluginTab::Installed,
                Message::SetPluginTab(PluginTab::Installed),
            ),
            (
                "插件市场",
                app.plugin_tab == PluginTab::Market,
                Message::SetPluginTab(PluginTab::Market),
            ),
        ],
        pal,
    );
    let manual = button::btn(
        // 与分段控件拉开，别看着像第三个 tab
        Spec::new("plg.manual", "手动安装", Variant::Secondary).size(BtnSize::Small),
        pal,
        &app.anim,
        Some(Message::OpenDialog(Dialog::ManualPlugin)),
        Some(Message::HoverEnter("plg.manual")),
        Some(Message::HoverExit("plg.manual")),
    );
    // 崩溃恢复入口：离线扫描会导致 harness 启动失败的问题。
    let diagnose = button::btn(
        Spec::new("plg.diag", "诊断", Variant::Secondary).size(BtnSize::Small),
        pal,
        &app.anim,
        (!app.selected.is_empty()).then_some(Message::DiagnosePlugins),
        Some(Message::HoverEnter("plg.diag")),
        Some(Message::HoverExit("plg.diag")),
    );

    let mut col = Column::new().spacing(12);
    if app.narrow() {
        col = col
            .push(
                r.push(space::horizontal())
                    .push(container(input).width(Fill))
                    .width(Fill),
            )
            .push(row![tabs, space::horizontal(), diagnose, manual].spacing(8).width(Fill));
    } else {
        col = col.push(
            r.push(space::horizontal().width(6.0))
                .push(input)
                .push(space::horizontal().width(4.0))
                .push(tabs)
                .push(space::horizontal().width(10.0))
                .push(diagnose)
                .push(space::horizontal().width(6.0))
                .push(manual)
                .width(Fill),
        );
    }
    if let Some(busy) = &app.busy {
        col = col.push(widgets::busy(busy.as_str(), pal));
    }
    card::card(col, pal)
}

fn list_card<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let q = app.query.trim().to_lowercase();
    let body = match app.plugin_tab {
        PluginTab::Installed => installed_list(app, &q, pal),
        PluginTab::Market => market_list(app, &q, pal),
    };
    card::card(body, pal)
}

fn installed_list<'a>(
    app: &'a Dshnext,
    q: &str,
    pal: &'static Palette,
) -> Column<'a, Message> {
    let shown: Vec<&crate::core::plugins::PluginInfo> = app
        .plugins
        .iter()
        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(q))
        .collect();

    let mut col = Column::new()
        .push(card::card_title(format!("已安装 · {} 个", app.plugins.len())))
        .push(card::card_sub(
            "这个版本当前装了哪些插件。卸载后依赖会一起移除。",
            pal,
        ))
        .push(space::vertical().height(8.0))
        .spacing(4);

    // 诊断结果：崩溃恢复的主入口——直接列出会导致启动失败的插件并给卸载动作。
    if !app.problems.is_empty() {
        col = col.push(txt("诊断结果").size(FS_SMALL).color(pal.text_2));
        for p in &app.problems {
            let (label, tone) = if p.severity == "error" {
                ("启动失败", Tone::Bad)
            } else {
                ("警告", Tone::Warn)
            };
            col = col.push(widgets::list_row(
                Some(widgets::tag(label, tone, pal)),
                column![
                    txt_bold(p.package.clone()).size(FS_BODY).color(pal.text),
                    txt(p.message.clone()).size(FS_TINY).color(pal.text_3),
                ]
                .spacing(2),
                row![row_btn(
                    app,
                    "plg.remove",
                    "卸载",
                    Variant::QuietDanger,
                    Message::OpenDialog(Dialog::RemovePlugin(p.package.clone())),
                    pal
                )],
                false, app.narrow(),
                pal,
            ));
        }
        col = col.push(space::vertical().height(4.0))
            .push(widgets::divider(pal));
    }

    if shown.is_empty() {
        if app.plugins.is_empty() {
            // 完全空装：给图标圈 + 直达动作，一行灰字撑不起这张大卡。
            let action = button::btn(
                Spec::new("plg.tab.market", "浏览插件市场", Variant::Accent)
                    .size(BtnSize::Small),
                pal,
                &app.anim,
                Some(Message::SetPluginTab(PluginTab::Market)),
                Some(Message::HoverEnter("plg.tab.market")),
                Some(Message::HoverExit("plg.tab.market")),
            );
            return col.push(widgets::empty_state(
                icon::PLUGINS,
                "还没有安装插件",
                "去插件市场挑一个，或用「手动安装」填 npm 包名 / github:owner/repo。",
                Some(action),
                pal,
            ));
        }
        return col.push(widgets::empty("没有匹配的插件", pal));
    }

    for (i, p) in shown.iter().enumerate() {
        if i > 0 {
            col = col.push(widgets::divider(pal));
        }
        let mut name_row = row![txt_bold(p.name.clone()).size(FS_BODY).color(pal.text)]
            .spacing(8)
            .align_y(Alignment::Center);
        if p.disabled {
            name_row = name_row.push(widgets::tag("已禁用", Tone::Warn, pal));
        }
        col = col.push(widgets::list_row(
            Some(widgets::icon_badge(
                icon::icon::<Message>(icon::PLUGINS, 18.0, pal.text_2),
                pal,
            )),
            column![
                name_row,
                mono(p.version.clone()).size(FS_TINY).color(pal.text_3),
            ]
            .spacing(2),
            row![
                row_btn(
                    app,
                    "plg.toggle",
                    if p.disabled { "启用" } else { "禁用" },
                    Variant::Secondary,
                    Message::TogglePlugin(p.name.clone(), !p.disabled),
                    pal
                ),
                row_btn(
                    app,
                    "plg.remove",
                    "卸载",
                    Variant::QuietDanger,
                    Message::OpenDialog(Dialog::RemovePlugin(p.name.clone())),
                    pal
                ),
            ]
            .spacing(8),
            false, app.narrow(),
            pal,
        ));
    }
    col
}

fn market_list<'a>(app: &'a Dshnext, q: &str, pal: &'static Palette) -> Column<'a, Message> {
    let mut shown: Vec<&crate::core::plugins::MarketItem> = app
        .market
        .iter()
        .filter(|m| {
            q.is_empty()
                || m.name.to_lowercase().contains(q)
                || m.description.to_lowercase().contains(q)
        })
        .collect();

    // 排序：星星降序（次键名称），或名称升序。
    match app.market_sort {
        MarketSort::Stars => {
            shown.sort_by(|a, b| b.stars.cmp(&a.stars).then_with(|| a.name.cmp(&b.name)))
        }
        MarketSort::Id => shown.sort_by(|a, b| a.name.cmp(&b.name)),
    }

    let total = shown.len();
    let pages = total.div_ceil(MARKET_PAGE_SIZE).max(1);
    let page = app.market_page.min(pages - 1);
    let start = page * MARKET_PAGE_SIZE;
    let end = (start + MARKET_PAGE_SIZE).min(total);

    let mut col = Column::new()
        .push(card::card_title(format!(
            "插件市场 · {} 个",
            app.market.len()
        )))
        .push(card::card_sub(
            "默认来自 awesome-dsh-plugin 策展目录，并合并 npm 结果。",
            pal,
        ))
        .push(space::vertical().height(8.0))
        .push(
            row![
                txt("排序").size(FS_SMALL).color(pal.text_2),
                widgets::segmented(
                    vec![
                        (
                            "按星星",
                            app.market_sort == MarketSort::Stars,
                            Message::SetMarketSort(MarketSort::Stars),
                        ),
                        (
                            "按名称",
                            app.market_sort == MarketSort::Id,
                            Message::SetMarketSort(MarketSort::Id),
                        ),
                    ],
                    pal,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .spacing(4);

    // 分页器放在列表上方：一页 60 行，放底部要滚很久才够得到。
    if pages > 1 {
        let prev = button::btn(
            Spec::new("plg.prev", "上一页", Variant::Secondary)
                .size(BtnSize::Small)
                .disabled(page == 0),
            pal,
            &app.anim,
            (page > 0).then(|| Message::SetMarketPage(page - 1)),
            None,
            None,
        );
        let next = button::btn(
            Spec::new("plg.next", "下一页", Variant::Secondary)
                .size(BtnSize::Small)
                .disabled(page + 1 >= pages),
            pal,
            &app.anim,
            (page + 1 < pages).then(|| Message::SetMarketPage(page + 1)),
            None,
            None,
        );
        col = col
            .push(
                row![
                    prev,
                    txt(format!("第 {}/{} 页 · 共 {} 个", page + 1, pages, total))
                        .size(FS_TINY)
                        .color(pal.text_3),
                    next,
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .push(space::vertical().height(4.0))
            .push(widgets::divider(pal));
    }

    if shown.is_empty() {
        if app.market_loaded {
            return col.push(widgets::empty("没有匹配的插件。", pal));
        }
        // 市场还没加载：图标圈 + 讲清楚怎么加载。
        return col.push(widgets::empty_state(
            icon::MARKET,
            "市场列表还没加载",
            "点上方「插件市场」分页加载来自策展目录与 npm 的社区插件。",
            None,
            pal,
        ));
    }

    let installed: Vec<&str> = app.plugins.iter().map(|p| p.name.as_str()).collect();
    for (i, m) in shown[start..end].iter().enumerate() {
        if i > 0 {
            col = col.push(widgets::divider(pal));
        }
        let already = installed.contains(&m.name.as_str());
        let mut name_row = row![txt_bold(m.name.clone()).size(FS_BODY).color(pal.text)]
            .spacing(8)
            .align_y(Alignment::Center);
        name_row = name_row.push(widgets::tag(
            format!("★ {}", m.stars),
            Tone::Neutral,
            pal,
        ));
        if !m.version.is_empty() {
            name_row = name_row.push(widgets::tag(
                format!("v{}", m.version),
                Tone::Neutral,
                pal,
            ));
        }
        if already {
            name_row = name_row.push(widgets::tag("已安装", Tone::Ok, pal));
        }

        let desc = if m.description.is_empty() {
            m.source.clone()
        } else {
            m.description.clone()
        };

        // 来源徽章：第三方商店目录用购物袋，npm 关键词检索用下载箭头
        // （与上一代 `origin === "catalog" ? "market" : "download"` 一致）。
        let badge = if m.origin == "catalog" {
            icon::MARKET
        } else {
            icon::DOWNLOAD
        };
        col = col.push(widgets::list_row(
            Some(widgets::icon_badge(
                icon::icon::<Message>(badge, 18.0, pal.text_2),
                pal,
            )),
            column![
                name_row,
                txt(widgets::ellipsize(&desc, 96))
                    .size(FS_TINY)
                    .color(pal.text_3),
            ]
            .spacing(2),
            row![
                page_btn(app, &m.page, pal),
                install_btn(app, already, &m.source, pal)
            ]
            .spacing(8),
            false, app.narrow(),
            pal,
        ));
    }

    col
}

fn install_btn<'a>(
    app: &'a Dshnext,
    already: bool,
    source: &str,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let enabled = app.busy.is_none() && !app.selected.is_empty();
    button::btn(
        Spec::new(
            "plg.install",
            if already { "重新安装" } else { "安装" },
            Variant::Accent,
        )
        .size(BtnSize::Small)
        .disabled(!enabled),
        pal,
        &app.anim,
        enabled.then(|| Message::InstallPlugin(source.to_string())),
        None,
        None,
    )
}

/// 在浏览器里打开插件的介绍页（策展页 / npm / GitHub）。
fn page_btn<'a>(app: &'a Dshnext, page: &str, pal: &'static Palette) -> Element<'a, Message> {
    let enabled = !page.is_empty();
    button::btn(
        Spec::new("plg.page", "插件页", Variant::Secondary)
            .size(BtnSize::Small)
            .disabled(!enabled),
        pal,
        &app.anim,
        enabled.then(|| Message::OpenPath(page.to_string())),
        None,
        None,
    )
}
