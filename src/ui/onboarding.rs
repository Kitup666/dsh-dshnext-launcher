//! 首次启动目录引导（first-run onboarding）。
//!
//! config.json 不存在时盖住整个界面，问两件事：
//! 1. **启动器数据目录**——config / 托管 runtime / 离线包住哪（pointer 机制见
//!    `core::store` 顶部注释）；2. **dsh-home**——harness 的 DSH_HOME，profiles 的根。
//!
//! 与 `modal.rs` 的差异：**不可关闭**（没有 ESC、点遮罩不退场）——目录是所有
//! 事情的第一个前置问题，必须给个答案（答案可以是「用默认」，两个输入框留空
//! 即是）。消息钉死为 `app::Message`（模态是通用件才走泛型 Actions，这里只有
//! 一组固定出口）。淡入复用 `anim "modal"` 补间（同一时间只会有一层浮层）。

use crate::app::{Message, Onboarding, PickField};
use crate::theme::{self, FS_LEAD, FS_SMALL, FS_TINY, Palette};
use crate::ui::anim::AnimState;
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::widgets;
use crate::ui::{txt, txt_bold};
use iced::widget::{center, column, container, opaque, row, space, stack};
use iced::{Alignment, Border, Element, Fill, Padding, Shadow, Theme};

/// 把目录表单叠在页面之上。`t` 是淡入补间值（0→1）。首启引导不可关闭；
/// 编辑模式（first_run=false）有取消按钮、点遮罩可退场，确认文案换成
/// 「保存并转移」。
#[allow(clippy::too_many_arguments)]
pub fn overlay<'a>(
    base: Element<'a, Message>,
    ob: &'a Onboarding,
    launcher_default: &'a str,
    home_default: &'a str,
    pal: &'static Palette,
    anim: &AnimState,
) -> Element<'a, Message> {
    let t = anim.value("modal").max(0.0).min(1.0);
    let edit_mode = !ob.first_run;
    let (title, desc, footnote, confirm_label) = if ob.first_run {
        (
            "欢迎使用 DshDesk",
            "开始之前，先决定两样东西放在哪。不确定就保持留空——默认路径适合绝大多数情况。",
            "两个都可以以后在系统里改；改启动器目录需要移动现有文件。",
            "开始使用",
        )
    } else {
        (
            "修改目录",
            "留空的项保持默认。确认后自动把现有文件转移到新位置（config、runtime、profiles）。",
            "运行中的实例要先停止；托管 runtime 可能几个 GB，跨盘转移需要一些时间。",
            "保存并转移",
        )
    };

    // 目录字段：标签 + 输入框（placeholder = 默认路径，留空即用默认）+ 浏览。
    let dir_field = |label: &'static str,
                     hint: &'a str,
                     draft: &'a str,
                     default: &'a str,
                     field: PickField,
                     key: &'static str,
                     busy: bool|
     -> Element<'a, Message> {
        widgets::field(
            label,
            row![
                container(widgets::input(
                    default,
                    draft,
                    move |v| Message::ObEdit(field, v),
                    false,
                    pal,
                ))
                .width(Fill),
                button::btn(
                    Spec::new(key, if busy { "…" } else { "浏览" }, Variant::Secondary)
                        .size(BtnSize::Small),
                    pal,
                    anim,
                    // 对话框开着时按钮无动作（防重复弹窗），仍可 hover。
                    (!busy).then_some(Message::ObBrowse(field)),
                    Some(Message::HoverEnter(key)),
                    Some(Message::HoverExit(key)),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            Some(hint),
            pal,
        )
        .into()
    };

    let body = column![
        txt_bold(title).size(FS_LEAD).color(pal.text),
        txt(desc).size(FS_SMALL).color(pal.text_3),
        space::vertical().height(8.0),
        dir_field(
            "启动器数据目录",
            "config.json、托管 runtime、离线安装包放这里",
            &ob.launcher_dir,
            launcher_default,
            PickField::LauncherDir,
            "ob.browse.launcher",
            ob.picking == Some(PickField::LauncherDir),
        ),
        dir_field(
            "dsh-home（DSH_HOME）",
            "harness 的数据根，profiles 存放于此；留空 = 数据目录下的 home",
            &ob.dsh_home,
            home_default,
            PickField::DshHome,
            "ob.browse.home",
            ob.picking == Some(PickField::DshHome),
        ),
        space::vertical().height(6.0),
        txt(footnote).size(FS_TINY).color(pal.text_3),
        row![
            space::horizontal(),
            button::btn(
                Spec::new("ob.defaults", "使用默认路径", Variant::Ghost).size(BtnSize::Small),
                pal,
                anim,
                Some(Message::ObDefaults),
                Some(Message::HoverEnter("ob.defaults")),
                Some(Message::HoverExit("ob.defaults")),
            ),
            // 编辑模式才有退路；首启引导必须被回答。
            edit_mode.then(|| {
                button::btn(
                    Spec::new("ob.cancel", "取消", Variant::Secondary).size(BtnSize::Small),
                    pal,
                    anim,
                    Some(Message::ObClose),
                    Some(Message::HoverEnter("ob.cancel")),
                    Some(Message::HoverExit("ob.cancel")),
                )
            }),
            button::btn(
                Spec::new("ob.confirm", confirm_label, Variant::Primary),
                pal,
                anim,
                Some(Message::ObConfirm),
                Some(Message::HoverEnter("ob.confirm")),
                Some(Message::HoverExit("ob.confirm")),
            ),
        ]
        .spacing(9)
        .align_y(Alignment::Center),
    ]
    .spacing(12);

    let panel = container(body)
        .width(560.0)
        .padding(Padding::from([26, 28]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text),
            background: Some(pal.surface_1.into()),
            border: Border {
                color: pal.card_border,
                width: 0.3,
                radius: 18.0.into(),
            },
            shadow: pal.shadow_pop,
            snap: true,
        });

    // 遮罩：bg_app 按 t 淡入。首启引导**点它不关闭**（不包 mouse_area）——
    // 引导必须被回答；编辑模式点遮罩等同取消。
    let mask_color = theme::with_alpha(pal.bg_app, 0.62 * t);
    let mask = container(center(panel))
        .width(Fill)
        .height(Fill)
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(mask_color.into()),
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        });
    let mask: Element<'a, Message> = if edit_mode {
        iced::widget::mouse_area(mask).on_press(Message::ObClose).into()
    } else {
        mask.into()
    };

    stack![base, opaque(mask)].width(Fill).height(Fill).into()
}
