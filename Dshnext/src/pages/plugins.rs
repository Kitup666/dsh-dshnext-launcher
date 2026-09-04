//! 插件管理页（对应上一代 `pages/Plugins.tsx`）：已装列表 / 插件市场两个分页。

use crate::app::{Dshnext, Message, PluginTab};
use crate::pages::home::row_btn;
use crate::theme::Palette;
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::icon;
use crate::ui::modal::Dialog;
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono, txt, txt_bold};
use iced::widget::{Column, column, row, space};
use iced::{Alignment, Element, Fill};

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let head = widgets::page_head(
        "插件管理",
        "harness 里一切能力都是插件。每个版本有独立的插件集，互不影响。",
        None,
        pal,
    );

    column![head, toolbar_card(app, pal), list_card(app, pal)]
        .spacing(18)
        .into()
}

/// 工具条卡：版本选择 + 搜索 + 分页 + 手动安装。
fn toolbar_card<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let profile_names: Vec<String> = app.profiles.iter().map(|p| p.name.clone()).collect();
    let current = (!app.selected.is_empty()).then(|| app.selected.clone());

    let mut r = row![txt("版本").size(11).color(pal.text_2)]
        .spacing(10)
        .align_y(Alignment::Center);

    if profile_names.is_empty() {
        r = r.push(txt("（无可用版本）").size(11.5).color(pal.text_3));
    } else {
        r = r.push(widgets::dropdown(
            profile_names,
            current,
            Message::Select,
            180.0,
            pal,
        ));
    }

    r = r
        .push(space::horizontal().width(6.0))
        .push(widgets::input(
            "搜索插件名称或描述…",
            &app.query,
            Message::Query,
            false,
            pal,
        ))
        .push(space::horizontal().width(4.0))
        .push(widgets::segmented(
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
        ))
        // 与分段控件拉开，别看着像第三个 tab
        .push(space::horizontal().width(10.0))
        .push(button::btn(
            Spec::new("plg.manual", "手动安装", Variant::Secondary).size(BtnSize::Small),
            pal,
            &app.anim,
            Some(Message::OpenDialog(Dialog::ManualPlugin)),
            Some(Message::HoverEnter("plg.manual")),
            Some(Message::HoverExit("plg.manual")),
        ));

    let mut col = Column::new().push(r.width(Fill)).spacing(12);
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

    if shown.is_empty() {
        let msg = if app.plugins.is_empty() {
            "该版本还没安装任何插件。去「插件市场」挑一个，或用「手动安装」填 npm 包名 / github:owner/repo。"
        } else {
            "没有匹配的插件"
        };
        return col.push(widgets::empty(msg, pal));
    }

    for (i, p) in shown.iter().enumerate() {
        if i > 0 {
            col = col.push(widgets::divider(pal));
        }
        col = col.push(widgets::list_row(
            Some(widgets::icon_badge(
                icon::icon::<Message>(icon::PLUGINS, 18.0, pal.text_2),
                pal,
            )),
            column![
                txt_bold(p.name.clone()).size(13).color(pal.text),
                mono(p.version.clone()).size(10.5).color(pal.text_3),
            ]
            .spacing(2),
            row![row_btn(
                app,
                "plg.remove",
                "卸载",
                Variant::QuietDanger,
                Message::OpenDialog(Dialog::RemovePlugin(p.name.clone())),
                pal
            )],
            false,
            pal,
        ));
    }
    col
}

fn market_list<'a>(app: &'a Dshnext, q: &str, pal: &'static Palette) -> Column<'a, Message> {
    let shown: Vec<&crate::core::plugins::MarketItem> = app
        .market
        .iter()
        .filter(|m| {
            q.is_empty()
                || m.name.to_lowercase().contains(q)
                || m.description.to_lowercase().contains(q)
        })
        .collect();

    let mut col = Column::new()
        .push(card::card_title(format!(
            "插件市场 · {} 个",
            app.market.len()
        )))
        .push(card::card_sub(
            "来自 npm 的社区插件。可在「设置」里追加第三方插件商店地址。",
            pal,
        ))
        .push(space::vertical().height(8.0))
        .spacing(4);

    if shown.is_empty() {
        let msg = if app.market_loaded {
            "没有匹配的插件。"
        } else {
            "点上方「插件市场」加载列表。"
        };
        return col.push(widgets::empty(msg, pal));
    }

    let installed: Vec<&str> = app.plugins.iter().map(|p| p.name.as_str()).collect();
    // 只渲染前 60 条：上百条全构造会拖慢每帧 layout，搜索框是更好的收窄手段。
    let cap = 60;
    for (i, m) in shown.iter().take(cap).enumerate() {
        if i > 0 {
            col = col.push(widgets::divider(pal));
        }
        let already = installed.contains(&m.name.as_str());
        let mut name_row = row![txt_bold(m.name.clone()).size(13).color(pal.text)]
            .spacing(8)
            .align_y(Alignment::Center);
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

        col = col.push(widgets::list_row(
            Some(widgets::icon_badge(
                icon::icon::<Message>(icon::VERSIONS, 18.0, pal.text_2),
                pal,
            )),
            column![
                name_row,
                txt(widgets::ellipsize(&desc, 96))
                    .size(10.5)
                    .color(pal.text_3),
            ]
            .spacing(2),
            row![install_btn(app, already, &m.source, pal)],
            false,
            pal,
        ));
    }
    if shown.len() > cap {
        col = col.push(widgets::divider(pal)).push(
            txt(format!(
                "还有 {} 条未显示，用搜索框收窄范围。",
                shown.len() - cap
            ))
            .size(10.5)
            .color(pal.text_3),
        );
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
            Variant::Primary,
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
