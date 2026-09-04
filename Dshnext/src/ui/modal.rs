//! 模态对话框（DESIGN.md §7.2）。iced 没有 modal，用 `stack!` + `opaque` 自拼。
//!
//! 关键点：
//! - **`opaque` 必须包在遮罩上**，否则鼠标事件会穿到背景控件（文档明确说它用来拦截）。
//! - 遮罩本身是 `mouse_area(...).on_press(关闭)`，点内容不关闭靠 `Stack::update`
//!   逆序派发 + 内容层先 capture。
//! - **ESC 关闭走 `keyboard::on_key_press` 订阅**，不是 widget 事件——模态是覆盖层，
//!   拿不到键盘焦点。
//! - 淡入动画复用 §7.1 的补间（key = `"modal"`）。

use crate::theme::{self, Palette};
use crate::ui::anim::AnimState;
use crate::ui::button::{self, Spec, Variant};
use crate::ui::widgets;
use crate::ui::{txt, txt_bold};
use iced::widget::{Column, center, container, mouse_area, opaque, row, space, stack};
use iced::{Alignment, Border, Element, Fill, Padding, Shadow, Theme};

/// 当前打开的对话框。放在 app State 里，`None` 表示没有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// 新建 profile。
    CreateProfile,
    /// 重命名（带原名）。
    RenameProfile(String),
    /// 复制（带源名）。
    CopyProfile(String),
    /// 删除确认（带名字）。
    DeleteProfile(String),
    /// 手动装插件。
    ManualPlugin,
    /// 卸载插件确认。
    RemovePlugin(String),
    /// 删除托管 Node 确认。
    RemoveNode,
    /// 卸载 dsh 确认。
    RemoveDsh,
}

impl Dialog {
    /// 是否需要输入框（决定 view 用 prompt 还是 confirm）。
    pub fn needs_input(&self) -> bool {
        matches!(
            self,
            Dialog::CreateProfile
                | Dialog::RenameProfile(_)
                | Dialog::CopyProfile(_)
                | Dialog::ManualPlugin
        )
    }

    /// 输入框的初始值。
    pub fn initial(&self) -> String {
        match self {
            Dialog::RenameProfile(n) => n.clone(),
            Dialog::CopyProfile(n) => format!("{n}-copy"),
            _ => String::new(),
        }
    }

    /// 标题 / 说明 / 确认按钮文案 / 是否危险操作。
    pub fn texts(&self) -> (String, &'static str, &'static str, bool) {
        match self {
            Dialog::CreateProfile => (
                "新建版本".into(),
                "将在 harness 数据目录下创建一个新的 profile（Web 应用模板）。",
                "创建",
                false,
            ),
            Dialog::RenameProfile(n) => (
                format!("重命名 {n}"),
                "目录会一起改名，package.json 里的 name 字段同步更新。",
                "重命名",
                false,
            ),
            Dialog::CopyProfile(n) => (
                format!("复制 {n}"),
                "复制配置与插件清单（不含已安装的依赖，首次启动会自动装回）。",
                "复制",
                false,
            ),
            Dialog::DeleteProfile(n) => (
                format!("删除版本 {n}？"),
                "该版本的配置、插件清单和已安装依赖将被永久删除，无法恢复。",
                "永久删除",
                true,
            ),
            Dialog::ManualPlugin => (
                "手动安装插件".into(),
                "支持 npm 包名（如 @scope/dsh-plugin-foo）或 GitHub 源（github:owner/repo）。安装转发给官方 dsh plugin add。",
                "安装插件",
                false,
            ),
            Dialog::RemovePlugin(n) => (
                format!("卸载 {n}？"),
                "将从该版本移除插件及其依赖；配置层（cordis.patch.yml）里的相关条目需自行清理。",
                "确认卸载",
                true,
            ),
            Dialog::RemoveNode => (
                "删除托管的 Node.js？".into(),
                "删除后需要重新下载才能启动 harness（不影响你系统里自己安装的 Node）。",
                "删除",
                true,
            ),
            Dialog::RemoveDsh => (
                "卸载 dsh？".into(),
                "仅删除启动器目录下的 dsh 及其依赖，你创建的版本和会话记录会保留。",
                "卸载",
                true,
            ),
        }
    }

    /// 输入框的标签与占位符。
    pub fn input_hints(&self) -> (&'static str, &'static str) {
        match self {
            Dialog::CreateProfile => ("版本名称（英文、数字、- 、_）", "例如 my-agent"),
            Dialog::RenameProfile(_) => ("新名称", ""),
            Dialog::CopyProfile(_) => ("新版本名称", ""),
            Dialog::ManualPlugin => ("插件源", "@scope/dsh-plugin-foo 或 github:owner/repo"),
            _ => ("", ""),
        }
    }
}

/// 模态需要的四个动作，由 app 层给出。
pub struct Actions<Message> {
    pub close: Message,
    pub confirm: Message,
    /// 输入框内容变化。
    pub on_input: Box<dyn Fn(String) -> Message>,
}

/// 把模态叠在页面之上。`t` 是淡入补间值（0→1）。
#[allow(clippy::too_many_arguments)]
pub fn overlay<'a, Message: Clone + 'a + 'static>(
    base: Element<'a, Message>,
    dialog: &'a Dialog,
    draft: &'a str,
    pal: &'static Palette,
    anim: &AnimState,
    actions: Actions<Message>,
    hover: impl Fn(crate::ui::anim::Key) -> Message + Copy + 'a,
    unhover: impl Fn(crate::ui::anim::Key) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    let t = anim.value("modal").max(0.0).min(1.0);
    let (title, desc, confirm_text, danger) = dialog.texts();
    let Actions {
        close,
        confirm,
        on_input,
    } = actions;

    let mut body = Column::new()
        .push(txt_bold(title).size(15).color(pal.text))
        .push(txt(desc).size(11.5).color(pal.text_3))
        .spacing(6);

    if dialog.needs_input() {
        let (label, placeholder) = dialog.input_hints();
        body = body.push(space::vertical().height(8.0)).push(widgets::field(
            label,
            widgets::input(placeholder, draft, move |v| on_input(v), false, pal),
            None,
            pal,
        ));
    }

    let can_confirm = !dialog.needs_input() || !draft.trim().is_empty();
    let foot = row![
        space::horizontal(),
        button::btn(
            Spec::new("modal.cancel", "取消", Variant::Secondary),
            pal,
            anim,
            Some(close.clone()),
            Some(hover("modal.cancel")),
            Some(unhover("modal.cancel")),
        ),
        button::btn(
            Spec::new(
                "modal.ok",
                confirm_text,
                if danger { Variant::Danger } else { Variant::Primary }
            )
            .disabled(!can_confirm),
            pal,
            anim,
            Some(confirm),
            Some(hover("modal.ok")),
            Some(unhover("modal.ok")),
        ),
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    let panel = container(body.push(space::vertical().height(14.0)).push(foot))
        .width(440.0)
        .padding(Padding::from([24, 26]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text),
            background: Some(pal.surface_1.into()),
            border: Border {
                color: pal.card_border,
                width: 1.0,
                radius: 18.0.into(),
            },
            shadow: pal.shadow_pop,
            snap: true,
        });

    // 遮罩：bg_app 55% alpha，按补间淡入。点它关闭。
    let mask_color = theme::with_alpha(pal.bg_app, 0.55 * t);
    let mask = mouse_area(
        container(center(panel))
            .width(Fill)
            .height(Fill)
            .style(move |_theme: &Theme| container::Style {
                text_color: None,
                background: Some(mask_color.into()),
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            }),
    )
    .on_press(close);

    // opaque 拦住穿透：不包的话点遮罩会点到下面的按钮上。
    stack![base, opaque(mask)].width(Fill).height(Fill).into()
}
