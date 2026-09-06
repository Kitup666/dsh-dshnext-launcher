//! 设置页（对应上一代 `pages/Settings.tsx`）：外观 / 模型访问 / 启动行为 / 下载源 / 关于。
//!
//! 与上一代的差异：**「Web 界面打开方式」项已移除**（DESIGN.md §5，改为一律
//! 系统浏览器）。`config.json` 里的 `open_mode` 字段被忽略但不删——两代共用
//! 同一个配置文件，删了会让上一代读不到自己的设置。

use crate::app::{Dshnext, Message};
use crate::theme::{FS_TINY, Palette};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, txt};
use iced::widget::{Column, column, row, space};
use iced::{Alignment, Element, Fill, Length};

const NODE_MIRRORS: [(&str, &str); 2] = [
    ("npmmirror（国内推荐）", "https://npmmirror.com/mirrors/node"),
    ("nodejs.org 官方", ""),
];
const NPM_REGISTRIES: [(&str, &str); 2] = [
    ("npmjs.org 官方", ""),
    ("npmmirror（国内推荐）", "https://registry.npmmirror.com"),
];

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();
    let dirty = app.cfg_dirty();

    let mut right = row![].spacing(10).align_y(Alignment::Center);
    if dirty {
        right = right.push(widgets::tag("有未保存的修改", Tone::Warn, pal));
    }
    right = right.push(button::btn(
        Spec::new("set.save", "保存", Variant::Primary).disabled(!dirty),
        pal,
        &app.anim,
        dirty.then_some(Message::SaveConfig),
        dirty.then_some(Message::HoverEnter("set.save")),
        dirty.then_some(Message::HoverExit("set.save")),
    ));

    let head = widgets::page_head(
        "设置",
        "设置保存在启动器数据目录的 config.json 里；API Key 仅在启动时注入子进程环境变量。",
        Some(right.into()),
        pal,
    );

    widgets::page_stack(
        head,
        Column::new()
            .push(appearance(app, pal))
            .push(model_access(app, pal))
            .push(launch_behavior(app, pal))
            .push(sources(app, pal))
            .push(about(app, pal)),
    )
    .into()
}

fn appearance<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    // 主题是即时生效项，不进 dirty（与上一代一致）。选中态看落盘值而非解析后
    // 的 mode：「跟随系统」在深色系统上选中的也必须是它自己。
    let current = app.config.theme.as_str();
    let opt = |label: &'static str, value: &'static str| {
        let on = current == value;
        (
            label,
            on,
            if on { Message::Noop } else { Message::SetTheme(value) },
        )
    };
    card::card(
        column![
            card::card_title("外观"),
            card::card_sub("切换后立即生效并落盘。「跟随系统」在下次启动时也会跟着系统走。", pal),
            space::vertical().height(6.0),
            widgets::segmented(
                vec![opt("浅色", "light"), opt("深色", "dark"), opt("跟随系统", "system")],
                pal
            ),
        ]
        .spacing(4),
        pal,
    )
}

fn model_access<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let toggle_label = if app.show_key { "隐藏" } else { "显示" };
    card::card(
        column![
            card::card_title("模型访问"),
            card::card_sub(
                "harness 需要 DeepSeek API Key 才能调用模型。留空则由 harness 自身的配置或环境变量决定。",
                pal
            ),
            space::vertical().height(8.0),
            widgets::field(
                "DEEPSEEK_API_KEY",
                row![
                    iced::widget::container(widgets::secret_input(
                        "sk-…",
                        &app.cfg_draft.api_key,
                        Message::CfgApiKey,
                        !app.show_key,
                        pal
                    ))
                    .width(Length::Fixed(420.0)),
                    button::btn(
                        Spec::new("set.showkey", toggle_label, Variant::Secondary)
                            .size(BtnSize::Small),
                        pal,
                        &app.anim,
                        Some(Message::ToggleShowKey),
                        Some(Message::HoverEnter("set.showkey")),
                        Some(Message::HoverExit("set.showkey")),
                    ),
                ]
                .spacing(9)
                .align_y(Alignment::Center)
                .into(),
                Some("以明文保存在本机 config.json 中，仅作为环境变量传给 dsh 子进程，不会上传到任何地方。"),
                pal
            ),
        ]
        .spacing(4),
        pal,
    )
}

fn launch_behavior<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    card::card(
        column![
            card::card_title("启动行为"),
            card::card_sub("控制 harness 的监听端口与启动后的行为。", pal),
            space::vertical().height(8.0),
            widgets::field(
                "默认端口",
                iced::widget::container(widgets::input(
                    "3080",
                    &app.port_text,
                    Message::CfgPort,
                    true,
                    pal
                ))
                .width(Length::Fixed(130.0))
                .into(),
                Some("同时运行多个版本时端口会冲突，届时改这里再启动下一个。"),
                pal
            ),
            space::vertical().height(4.0),
            widgets::check(
                "启动成功后自动打开 Web 界面",
                app.cfg_draft.auto_open,
                Message::CfgAutoOpen,
                pal
            ),
            widgets::check(
                "开机自动启动 DshDesk",
                app.cfg_draft.autostart,
                Message::CfgAutoStart,
                pal
            ),
            // 说明文字与卡片内容列同左边缘（不缩进到复选框标签下）。早先按「对齐它
            // 解释的那个标签」缩进了 24px（方框 15 + 间距 9），量下来确实对齐了标签，
            // 但它是整张卡里唯一不在内容列上的一行——同页另两处 field 的说明都在
            // 内容列上，扫下来就这一行突出来。表单卡里共享一条左边缘比「对齐标签」重要。
            txt("Web 界面在系统默认浏览器中打开；自启写入当前用户的 Run 键，无需管理员权限。")
                .size(FS_TINY)
                .color(pal.text_3),
        ]
        .spacing(4),
        pal,
    )
}

fn sources<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    // 下拉给预设，输入框允许自定义——与上一代同一套双控件。
    let node_labels: Vec<String> = NODE_MIRRORS.iter().map(|(l, _)| l.to_string()).collect();
    let node_current = NODE_MIRRORS
        .iter()
        .find(|(_, v)| *v == app.cfg_draft.node_mirror)
        .map(|(l, _)| l.to_string());
    let npm_labels: Vec<String> = NPM_REGISTRIES.iter().map(|(l, _)| l.to_string()).collect();
    let npm_current = NPM_REGISTRIES
        .iter()
        .find(|(_, v)| *v == app.cfg_draft.npm_registry)
        .map(|(l, _)| l.to_string());

    card::card(
        column![
            card::card_title("下载源"),
            card::card_sub("国内网络建议使用镜像，能显著加快 Node 与插件的下载。", pal),
            space::vertical().height(8.0),
            widgets::field(
                "Node.js 下载镜像",
                column![
                    widgets::dropdown(node_labels, node_current, |label: String| {
                        let v = NODE_MIRRORS
                            .iter()
                            .find(|(l, _)| *l == label)
                            .map(|(_, v)| v.to_string())
                            .unwrap_or_default();
                        Message::CfgNodeMirror(v)
                    }, 260.0, 9, pal),
                    iced::widget::container(widgets::input(
                        "https://nodejs.org/dist",
                        &app.cfg_draft.node_mirror,
                        Message::CfgNodeMirror,
                        true,
                        pal
                    ))
                    .width(Length::Fixed(420.0)),
                ]
                .spacing(8)
                .into(),
                None,
                pal
            ),
            space::vertical().height(6.0),
            widgets::field(
                "npm registry",
                column![
                    widgets::dropdown(npm_labels, npm_current, |label: String| {
                        let v = NPM_REGISTRIES
                            .iter()
                            .find(|(l, _)| *l == label)
                            .map(|(_, v)| v.to_string())
                            .unwrap_or_default();
                        Message::CfgNpmRegistry(v)
                    }, 260.0, 9, pal),
                    iced::widget::container(widgets::input(
                        "https://registry.npmjs.org",
                        &app.cfg_draft.npm_registry,
                        Message::CfgNpmRegistry,
                        true,
                        pal
                    ))
                    .width(Length::Fixed(420.0)),
                ]
                .spacing(8)
                .into(),
                None,
                pal
            ),
            space::vertical().height(6.0),
            widgets::field(
                "插件商店目录（catalog.json）",
                iced::widget::container(widgets::input(
                    "留空则只用 npm 上的 dsh-plugin 关键词作为来源",
                    &app.cfg_draft.plugin_catalog_url,
                    Message::CfgCatalog,
                    true,
                    pal
                ))
                .width(Length::Fixed(420.0))
                .into(),
                Some("填入第三方插件目录的 JSON 地址后，插件市场会把它和 npm 结果合并展示。"),
                pal
            ),
        ]
        .spacing(4),
        pal,
    )
}

fn about<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let link = |key: &'static str, label: &'static str, url: &'static str| {
        button::btn(
            Spec::new(key, label, Variant::Secondary).size(BtnSize::Small),
            pal,
            &app.anim,
            Some(Message::OpenPath(url.to_string())),
            Some(Message::HoverEnter(key)),
            Some(Message::HoverExit(key)),
        )
    };

    card::card(
        Column::new()
            .push(card::card_title("关于"))
            .push(card::card_sub(
                "DshDesk 是 DeepSeek Harness 的第三方 Windows 启动器，负责运行时托管、版本隔离与插件管理。harness 本体是 DeepSeek 开源的 MIT 项目。",
                pal,
            ))
            .push(space::vertical().height(8.0))
            .push(
                row![
                    link("ab.site", "DeepSeek Harness 官网", "https://www.deepseek.com/harness/"),
                    link("ab.gh", "GitHub 仓库", "https://github.com/deepseek-ai/deepseek-harness"),
                    link("ab.plugins", "社区插件", "https://www.npmjs.com/search?q=keywords:dsh-plugin"),
                    button::btn(
                        Spec::new("ab.diag", "导出诊断", Variant::Secondary).size(BtnSize::Small),
                        pal,
                        &app.anim,
                        Some(Message::ExportDiag),
                        Some(Message::HoverEnter("ab.diag")),
                        Some(Message::HoverExit("ab.diag")),
                    ),
                ]
                .spacing(10),
            )
            .push(space::vertical().height(4.0))
            .push(
                txt("单文件绿色版，无需安装运行时。")
                    .size(FS_TINY)
                    .color(pal.text_3),
            )
            .spacing(4)
            .width(Fill),
        pal,
    )
}
