//! 环境页（对应上一代 `pages/Env.tsx`）：三项检测 + 安装动作 + 数据目录。

use crate::app::{Dshnext, Message};
use crate::pages::home::row_btn;
use crate::theme::{FS_BODY, FS_TINY, Palette};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::icon;
use crate::ui::modal::Dialog;
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono, txt, txt_bold};
use iced::widget::{Column, Row, column, row, space};
use iced::{Alignment, Element, Fill};

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let head = widgets::page_head(
        "环境",
        "启动器在私有目录里托管运行时，与系统里的 Node / dsh 互不干扰。",
        Some(button::btn(
            Spec::new("env.refresh", "重新检测", Variant::Secondary)
                .disabled(app.busy.is_some()),
            pal,
            &app.anim,
            Some(Message::RefreshEnv),
            Some(Message::HoverEnter("env.refresh")),
            Some(Message::HoverExit("env.refresh")),
        )),
        pal,
    );

    let mut body = Column::new();

    if let Some(busy) = &app.busy {
        body = body.push(card::card(
            column![
                widgets::busy(busy.as_str(), pal),
                txt("安装过程的完整输出会实时写入「控制台」页。")
                    .size(FS_TINY)
                    .color(pal.text_3),
            ]
            .spacing(8),
            pal,
        ));
    }

    body = body
        .push(detect_card(app, pal))
        .push(data_dir_card(app, pal));
    widgets::page_stack(head, body).into()
}

/// 检测结果卡：Node / dsh / pnpm 三行 + 预发布开关。
fn detect_card<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let env = app.env.as_ref();
    let node_ok = env.is_some_and(|e| e.node_version.is_some());
    let dsh_ok = env.is_some_and(|e| e.dsh_version.is_some());
    let pnpm_ok = env.is_some_and(|e| e.pnpm_version.is_some());
    let busy = app.busy.is_some();

    let mut list = Column::new()
        .push(card::card_title("检测结果"))
        .push(card::card_sub("三项齐全才能启动 harness 并管理插件。", pal))
        .push(space::vertical().height(8.0))
        .spacing(4);

    // ---- Node.js ----
    let node_managed = env.is_some_and(|e| e.node_managed);
    let mut node_name = row![
        txt_bold("Node.js").size(FS_BODY).color(pal.text),
        widgets::tag(
            env.and_then(|e| e.node_version.clone())
                .unwrap_or_else(|| "未安装".into()),
            if node_ok { Tone::Ok } else { Tone::Bad },
            pal
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if node_managed {
        node_name = node_name.push(widgets::tag("启动器托管", Tone::Accent, pal));
    }

    let node_actions = {
        let mut r = Row::new().spacing(7).align_y(Alignment::Center);
        if !app.node_versions.is_empty() {
            r = r.push(widgets::dropdown(
                app.node_versions.clone(),
                Some(app.node_pick.clone()),
                Message::PickNode,
                150.0,
                9,
                pal,
            ));
        }
        if node_managed {
            r = r.push(row_btn(
                app,
                "env.node.del",
                "删除托管版",
                Variant::QuietDanger,
                Message::OpenDialog(Dialog::RemoveNode),
                pal,
            ));
        } else {
            // 系统已有 node 时说「下载托管版」——避免和 pnpm 行的「重新安装」
            // 说同一件事却用不同动词。
            r = r.push(action_btn(
                app,
                "env.node.install",
                if node_ok { "下载托管版" } else { "下载安装" },
                Variant::Accent,
                (!busy).then_some(Message::InstallNode),
                pal,
            ));
        }
        r
    };

    list = list.push(widgets::list_row(
        Some(widgets::icon_badge(
            icon::icon::<Message>(icon::NODE, 18.0, pal.text_2),
            pal,
        )),
        column![
            node_name,
            mono(widgets::ellipsize(
                env.and_then(|e| e.node_path.clone())
                    .unwrap_or_else(|| "未找到可用的 node".into())
                    .as_str(),
                72
            ))
            .size(FS_TINY)
            .color(pal.text_3),
        ]
        .spacing(2),
        node_actions,
        false,
        pal,
    ));

    // ---- dsh ----
    list = list.push(widgets::divider(pal));
    let mut dsh_name = row![
        txt_bold("DeepSeek Harness (dsh)").size(FS_BODY).color(pal.text),
        widgets::tag(
            env.and_then(|e| e.dsh_version.clone())
                .unwrap_or_else(|| "未安装".into()),
            if dsh_ok { Tone::Ok } else { Tone::Bad },
            pal
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    // 有新版可更新时提示（与上一代一致）
    if let (Some(latest), Some(cur)) = (
        app.dsh_latest.as_ref(),
        env.and_then(|e| e.dsh_version.as_ref()),
    ) {
        if latest != cur {
            dsh_name = dsh_name.push(widgets::tag(
                format!("可更新到 {latest}"),
                Tone::Warn,
                pal,
            ));
        }
    }

    // dsh 版本下拉：latest + 具体版本号
    let mut dsh_options = vec![match &app.dsh_latest {
        Some(l) => format!("latest ({l})"),
        None => "latest".to_string(),
    }];
    dsh_options.extend(app.dsh_versions.iter().cloned());
    let dsh_current = if app.dsh_pick == "latest" {
        Some(dsh_options[0].clone())
    } else {
        Some(app.dsh_pick.clone())
    };

    let dsh_actions = {
        let mut r = Row::new().spacing(7).align_y(Alignment::Center);
        r = r.push(widgets::dropdown(
            dsh_options,
            dsh_current,
            |v: String| {
                // "latest (0.1.1-rc.2)" → "latest"
                Message::PickDsh(if v.starts_with("latest") {
                    "latest".to_string()
                } else {
                    v
                })
            },
            170.0,
            9,
            pal,
        ));
        r = r.push(action_btn(
            app,
            "env.dsh.install",
            if dsh_ok { "安装 / 切换" } else { "安装" },
            Variant::Accent,
            (!busy && node_ok).then_some(Message::InstallDsh),
            pal,
        ));
        if dsh_ok {
            r = r.push(row_btn(
                app,
                "env.dsh.del",
                "卸载",
                Variant::QuietDanger,
                Message::OpenDialog(Dialog::RemoveDsh),
                pal,
            ));
        }
        r
    };

    list = list.push(widgets::list_row(
        Some(widgets::icon_badge(
            icon::icon::<Message>(icon::HARNESS, 18.0, pal.text_2),
            pal,
        )),
        column![
            dsh_name,
            mono(widgets::ellipsize(
                env.and_then(|e| e.dsh_path.clone())
                    .unwrap_or_else(|| "未安装到启动器目录".into())
                    .as_str(),
                72
            ))
            .size(FS_TINY)
            .color(pal.text_3),
        ]
        .spacing(2),
        dsh_actions,
        false,
        pal,
    ));

    // ---- pnpm ----
    list = list.push(widgets::divider(pal));
    list = list.push(widgets::list_row(
        Some(widgets::icon_badge(
            icon::icon::<Message>(icon::DOWNLOAD, 18.0, pal.text_2),
            pal,
        )),
        column![
            row![
                txt_bold("pnpm").size(FS_BODY).color(pal.text),
                widgets::tag(
                    env.and_then(|e| e.pnpm_version.clone())
                        .unwrap_or_else(|| "未安装".into()),
                    if pnpm_ok { Tone::Ok } else { Tone::Warn },
                    pal
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            txt("dsh 的插件命令依赖 pnpm，缺失时插件安装/卸载会失败。")
                .size(FS_TINY)
                .color(pal.text_3),
        ]
        .spacing(2),
        row![action_btn(
            app,
            "env.pnpm.install",
            if pnpm_ok { "重新安装" } else { "安装" },
            Variant::Accent,
            (!busy && node_ok).then_some(Message::InstallPnpm),
            pal,
        )],
        false,
        pal,
    ));

    // ---- 预发布开关条 ----
    list = list.push(space::vertical().height(6.0)).push(widgets::strip(
        row![
            widgets::check(
                "显示预发布版本（rc / beta）",
                app.include_rc,
                Message::ToggleRc,
                pal
            ),
            space::horizontal(),
            action_btn(
                app,
                "env.reload",
                "刷新版本列表",
                Variant::Secondary,
                (!busy).then_some(Message::LoadVersions),
                pal,
            ),
        ]
        .width(Fill),
        pal,
    ));

    card::card(list, pal)
}

fn data_dir_card<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let env = app.env.as_ref();
    let dir = env.map(|e| e.data_dir.clone()).unwrap_or_default();

    card::card(
        column![
            row![
                column![
                    card::card_title("数据目录"),
                    card::card_sub("配置、运行时和所有版本都存放在这里。", pal),
                ]
                .spacing(3),
                space::horizontal(),
                action_btn(
                    app,
                    "env.opendir",
                    "在资源管理器中打开",
                    Variant::Secondary,
                    (!dir.is_empty()).then(|| Message::OpenPath(dir.clone())),
                    pal,
                ),
            ]
            .width(Fill)
            .align_y(Alignment::Start),
            space::vertical().height(6.0),
            widgets::kv(
                "启动器目录",
                env.map(|e| widgets::ellipsize(&e.data_dir, 76))
                    .unwrap_or_else(|| "—".into()),
                pal
            ),
            widgets::divider(pal),
            widgets::kv(
                "DSH_HOME",
                env.map(|e| widgets::ellipsize(&e.home_dir, 76))
                    .unwrap_or_else(|| "—".into()),
                pal
            ),
        ]
        .spacing(8),
        pal,
    )
}

/// 小尺寸按钮，`msg` 为 None 时渲染禁用态。
fn action_btn<'a>(
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
            .size(BtnSize::Small)
            .disabled(!enabled),
        pal,
        &app.anim,
        msg,
        enabled.then_some(Message::HoverEnter(key)),
        enabled.then_some(Message::HoverExit(key)),
    )
}
