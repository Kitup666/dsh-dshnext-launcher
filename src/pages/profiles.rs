//! 版本管理页（对应上一代 `pages/Profiles.tsx`）：profile 列表 + CRUD 模态。

use crate::app::{Dshnext, Message};
use crate::pages::home::row_btn;
use crate::theme::{FS_BODY, FS_TINY, Palette};
use crate::ui::anim::{self, PAGE_SHIFT};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::icon;
use crate::ui::modal::Dialog;
use crate::ui::reveal_at;
use crate::ui::widgets::{self, Tone};
use crate::ui::{card, mono, txt, txt_bold};
use iced::widget::{Column, column, container, mouse_area, row, space};
use iced::{Alignment, Border, Element, Fill, Length, Shadow, Theme};

pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let head = widgets::page_head(
        "版本管理",
        "每个版本对应一个 harness profile（独立插件集与配置层），互不影响。",
        Some(button::btn(
            Spec::new("prof.new", "+ 新建版本", Variant::Primary),
            pal,
            &app.anim,
            Some(Message::OpenDialog(Dialog::CreateProfile)),
            Some(Message::HoverEnter("prof.new")),
            Some(Message::HoverExit("prof.new")),
        )), app.narrow(),
        pal,
    );

    let mut list = Column::new().spacing(0);
    if app.profiles.is_empty() {
        list = list.push(widgets::empty_state(
            icon::VERSIONS,
            "还没有版本",
            "点右上角「新建版本」创建第一个（也可点下面的占位框）。",
            None,
            pal,
        ));
    } else {
        for (i, p) in app.profiles.iter().enumerate() {
            if i > 0 {
                list = list.push(widgets::divider(pal));
            }
            list = list.push(profile_row(app, p, pal));
        }
    }

    // 错峰：页头先落位，列表卡迟到 12%，ghost 占位行再迟一拍。
    // spacing 原来由 page_stack 补，现在卡各自包了 reveal（Element），
    // 得自己按 GAP_CARD 排。
    let t = app.anim.value(anim::PAGE);
    let mut body = Column::new()
        .spacing(crate::theme::GAP_CARD)
        .push(reveal_at(card::card(list, pal), t, PAGE_SHIFT, 0.12));
    if !app.profiles.is_empty() {
        body = body.push(reveal_at(ghost_new_row(pal), t, PAGE_SHIFT, 0.2));
    }

    widgets::page_stack(reveal_at(head, t, PAGE_SHIFT, 0.0), body).into()
}

/// 列表尾的 ghost 占位行（judge P6）：给列表一个「完整且可继续」的收尾，
/// 点击等同「新建版本」。iced 的 Border 不支持虚线，用 1px 实线细描边近似。
fn ghost_new_row<'a>(pal: &'static Palette) -> Element<'a, Message> {
    let body = container(
        row![txt("+ 新建版本").size(FS_BODY).color(pal.text_3)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .width(Fill)
    .height(Length::Fixed(72.0))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| container::Style {
        text_color: Some(pal.text_3),
        background: None,
        border: Border {
            color: pal.border_mid,
            width: 1.0,
            radius: 14.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    });
    mouse_area(body)
        .on_press(Message::OpenDialog(Dialog::CreateProfile))
        .into()
}

fn profile_row<'a>(
    app: &'a Dshnext,
    p: &'a crate::core::profiles::ProfileInfo,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let is_running = app.running(&p.name).is_some();
    let is_selected = app.selected == p.name;
    let plugin_count = p.dependencies.len();

    let mut name_row = row![txt_bold(p.name.clone()).size(FS_BODY).color(pal.text)]
        .spacing(8)
        .align_y(Alignment::Center);
    if is_running {
        name_row = name_row.push(widgets::tag("运行中", Tone::Ok, pal));
    }
    if is_selected {
        name_row = name_row.push(widgets::tag("当前", Tone::Accent, pal));
    }
    name_row = name_row.push(widgets::tag(
        format!("{plugin_count} 个插件"),
        Tone::Neutral,
        pal,
    ));

    let bundles = if p.bundles.is_empty() {
        "未声明 bundle".to_string()
    } else {
        p.bundles.join(" + ")
    };

    let main = column![
        name_row,
        mono(widgets::ellipsize(&bundles, 88))
            .size(FS_TINY)
            .color(pal.text_3),
    ]
    .spacing(2);

    // 启动/停止 + 插件 + 复制 + 重命名 + 打开目录 + 删除（与上一代同一组）
    // 「启动」用 Accent 而非 Primary：这一屏的主 CTA 是右上角的「+ 新建版本」，
    // 列表每行再来一个渐变按钮就没有主次了。
    let primary: Element<'_, Message> = if is_running {
        row_btn(
            app,
            "row.stop",
            "停止",
            Variant::Danger,
            Message::Stop(p.name.clone()),
            pal,
        )
    } else if app.env_ready() {
        row_btn(
            app,
            "row.start",
            "启动",
            Variant::Accent,
            Message::Start(p.name.clone()),
            pal,
        )
    } else {
        // 环境没就绪时按钮在位但禁用（对应上一代 disabled={!env?.dsh_version}）
        button::btn(
            Spec::new("row.start", "启动", Variant::Accent)
                .size(BtnSize::Small)
                .disabled(true),
            pal,
            &app.anim,
            None,
            None,
            None,
        )
    };

    let actions = row![
        primary,
        // 「插件」一次完成两件事：把这一行设为当前版本，再跳插件页。
        // 早先拆成两个按钮（第二个只显示一个 →），看上去像掉了标签的野字符。
        row_btn(
            app,
            "row.plugins",
            "插件",
            Variant::Secondary,
            Message::GotoPlugins(p.name.clone()),
            pal
        ),
        row_btn(
            app,
            "row.copy",
            "复制",
            Variant::Secondary,
            Message::OpenDialog(Dialog::CopyProfile(p.name.clone())),
            pal
        ),
        row_btn(
            app,
            "row.rename",
            "重命名",
            Variant::Secondary,
            Message::OpenDialog(Dialog::RenameProfile(p.name.clone())),
            pal
        ),
        row_btn(
            app,
            "row.dir",
            "打开目录",
            Variant::Secondary,
            Message::OpenPath(p.path.clone()),
            pal
        ),
        space::horizontal().width(4.0),
        row_btn(
            app,
            "row.del",
            "删除",
            Variant::QuietDanger,
            Message::OpenDialog(Dialog::DeleteProfile(p.name.clone())),
            pal
        ),
    ]
    .spacing(7);

    let content = widgets::list_row(
        Some(widgets::icon_badge(
            icon::icon::<Message>(icon::VERSIONS, 18.0, pal.text_2),
            pal,
        )),
        main,
        actions,
        is_selected, app.narrow(),
        pal,
    );

    // 整行可点选中（对应上一代 onClick={() => setSelected(x.name)}）。
    // 行内按钮先 capture，不会被这层吃掉。
    widgets::hoverable(
        content,
        None,
        &app.anim,
        pal,
        Some(Message::Select(p.name.clone())),
        None,
        None,
    )
}
