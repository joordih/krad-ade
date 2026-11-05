use gpui_kit::base::TextView;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, img, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_extensions::api::view::{
    ButtonVariant as ViewButton, Detail, Direction, Element, Enabled, ImageSource, Node, TextStyle,
    Tone, ViewEvent, Wrap, MAX_GRID_COLUMNS,
};
use std::path::{Component, Path, PathBuf};

use super::form::element_id;
use super::style::{
    color, flex_layout, icon_name, radius, sized_box, space, styled, text_size, tone_color, weight,
};
use super::ExtensionView;
use crate::components::{icon, working_dots, Button, ButtonVariant, IconName};
use crate::theme::{
    Palette, CHIP_H, ICON, ICON_XS, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_2XS,
    TEXT_XS,
};

const DETAIL_LABEL_W: f32 = 140.0;
const IMAGE_PLACEHOLDER: f32 = 48.0;
const TERMINAL_MIN_H: f32 = 160.0;
const DIVIDER_H: f32 = 1.0;
const SPINNER_KEY: u64 = 0;
const BUTTON_PREFIX: &str = "ext-button";
const SCROLL_PREFIX: &str = "ext-scroll";
const MARKDOWN_PREFIX: &str = "ext-markdown";
const TOGGLE_PREFIX: &str = "ext-toggle";
const TERMINAL_WAITING: &str = "Starting terminal…";

impl ExtensionView {
    pub(super) fn element(&self, node: &Node, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &node.element {
            Element::Flex { .. }
            | Element::Stack { .. }
            | Element::Grid { .. }
            | Element::Scroll { .. } => self.container(node, pal, cx),
            Element::Button { .. }
            | Element::Input { .. }
            | Element::Toggle { .. }
            | Element::Select { .. } => self.control(node, pal, cx),
            Element::List { count, selection } => Self::list(node, *count, *selection, cx),
            Element::Tree { .. } => self.tree(node, pal, cx),
            Element::Table {
                columns,
                count,
                sort,
            } => self.table(node, columns, *count, sort.as_ref(), pal, cx),
            Element::Form {
                submit,
                fields,
                submit_label,
            } => self.form_element(node, submit, fields, submit_label.as_deref(), pal, cx),
            Element::Details { entries } => details(entries, pal, cx),
            Element::Stream { lines, .. } => self.stream(node, lines, pal, cx),
            Element::Terminal { terminal } => self.terminals.get(terminal).map_or_else(
                || placeholder(TERMINAL_WAITING, pal),
                |view| {
                    div()
                        .flex_1()
                        .min_h(px(TERMINAL_MIN_H))
                        .child(view.clone())
                        .into_any_element()
                },
            ),
            Element::Text { .. }
            | Element::Markdown { .. }
            | Element::Icon { .. }
            | Element::Image { .. }
            | Element::Badge { .. }
            | Element::Spinner
            | Element::Divider => self.content(node, pal, cx),
        }
    }

    fn container(&self, node: &Node, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &node.element {
            Element::Flex {
                direction,
                align,
                justify,
                wrap,
                style,
                children,
            } => styled(
                flex_layout(div(), *direction, *align, *justify, *wrap),
                style,
                pal,
            )
            .min_w_0()
            .children(self.elements(children, pal, cx))
            .into_any_element(),
            Element::Stack { style, children } => styled(div().relative(), style, pal)
                .children(
                    self.elements(children, pal, cx)
                        .into_iter()
                        .map(|child| div().absolute().inset_0().child(child)),
                )
                .into_any_element(),
            Element::Grid {
                columns,
                style,
                children,
            } => styled(
                div()
                    .grid()
                    .grid_cols(u16::from((*columns).clamp(1, MAX_GRID_COLUMNS))),
                style,
                pal,
            )
            .children(self.elements(children, pal, cx))
            .into_any_element(),
            Element::Scroll {
                direction,
                style,
                children,
            } => {
                let scroll = styled(div(), style, pal)
                    .id(element_id(SCROLL_PREFIX, &node.key))
                    .flex()
                    .min_h_0();
                let scroll = match direction {
                    Direction::Column => scroll.flex_col().overflow_y_scroll(),
                    Direction::Row => scroll.flex_row().overflow_x_scroll(),
                };
                scroll
                    .children(self.elements(children, pal, cx))
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }

    fn content(&self, node: &Node, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &node.element {
            Element::Text { text, style } => text_element(text, *style, pal, cx),
            Element::Markdown { text } => div()
                .min_w_0()
                .child(
                    TextView::markdown(element_id(MARKDOWN_PREFIX, &node.key), text.clone())
                        .selectable(true)
                        .on_link_click(|_, _, _, _| {}),
                )
                .into_any_element(),
            Element::Icon {
                icon: glyph,
                color: tint,
            } => {
                let glyph = icon(icon_name(*glyph), ICON, pal.text_default);
                match tint {
                    Some(token) => glyph.text_color(color(*token, pal)).into_any_element(),
                    None => glyph.into_any_element(),
                }
            }
            Element::Image {
                source,
                width,
                height,
            } => self.image(source, *width, *height, pal),
            Element::Badge { text, tone } => badge(text, *tone, pal),
            Element::Spinner => working_dots(SPINNER_KEY, pal.text_muted).into_any_element(),
            Element::Divider => div()
                .h(px(DIVIDER_H))
                .w_full()
                .bg(gpui_kit::Hsla::from(pal.divider))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn control(&self, node: &Node, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &node.element {
            Element::Button {
                action,
                label,
                icon: glyph,
                variant,
                enabled,
                tooltip,
            } => {
                let mut button = Button::new(element_id(BUTTON_PREFIX, &node.key))
                    .variant(button_variant(*variant, *enabled));
                if let Some(label) = label {
                    button = button.label(SharedString::from(label.clone()));
                }
                if let Some(glyph) = glyph {
                    button = button.icon(icon_name(*glyph));
                }
                if let Some(tooltip) = tooltip {
                    button = button.tooltip(SharedString::from(tooltip.clone()));
                }
                if *enabled == Enabled::Yes {
                    let action = action.clone();
                    button = button.on_click(cx.listener(move |this, _, _, _| {
                        this.emit(ViewEvent::Click {
                            action: action.clone(),
                        });
                    }));
                }
                button.into_any_element()
            }
            Element::Input { .. } => self.input_element(&node.key),
            Element::Toggle {
                action,
                value,
                label,
            } => {
                let action = action.clone();
                let mut toggle = Switch::new(element_id(TOGGLE_PREFIX, &node.key))
                    .checked(*value)
                    .on_click(cx.listener(move |this, checked: &bool, _, _| {
                        this.emit(ViewEvent::Toggle {
                            action: action.clone(),
                            value: *checked,
                        });
                    }));
                if let Some(label) = label {
                    toggle = toggle.label(label.clone());
                }
                toggle.into_any_element()
            }
            Element::Select {
                action,
                options,
                value,
            } => {
                let action = action.clone();
                Self::choice_row(
                    &node.key,
                    options,
                    value.as_deref(),
                    pal,
                    cx,
                    move |this, picked, _| {
                        this.emit(ViewEvent::Select {
                            action: action.clone(),
                            value: picked,
                        });
                    },
                )
            }
            _ => div().into_any_element(),
        }
    }

    pub(super) fn elements(
        &self,
        children: &[Node],
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        children
            .iter()
            .map(|child| self.element(child, pal, cx))
            .collect()
    }

    fn image(
        &self,
        source: &ImageSource,
        width: krad_extensions::api::view::Size,
        height: krad_extensions::api::view::Size,
        pal: &Palette,
    ) -> AnyElement {
        let path = match source {
            ImageSource::Bundled(relative) => self
                .folder()
                .and_then(|folder| bundled_path(folder, relative)),
            ImageSource::Png(_) => None,
        };
        let Some(path) = path else {
            return div()
                .size(px(IMAGE_PLACEHOLDER))
                .rounded(px(RADIUS_XS))
                .bg(gpui_kit::Hsla::from(pal.bg_hover))
                .into_any_element();
        };
        sized_box(div(), width, height)
            .child(img(path).size_full())
            .into_any_element()
    }
}

pub(super) fn bundled_path(folder: &Path, relative: &Path) -> Option<PathBuf> {
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        .then(|| folder.join(relative))
}

const fn button_variant(variant: ViewButton, enabled: Enabled) -> ButtonVariant {
    if matches!(enabled, Enabled::No) {
        return ButtonVariant::Ghost;
    }
    match variant {
        ViewButton::Primary => ButtonVariant::Primary,
        ViewButton::Secondary => ButtonVariant::Secondary,
        ViewButton::Ghost => ButtonVariant::Ghost,
        ViewButton::Danger => ButtonVariant::Destructive,
    }
}

pub(super) fn text_element(
    text: &str,
    style: TextStyle,
    pal: &Palette,
    cx: &mut Context<ExtensionView>,
) -> AnyElement {
    let (size, line) = text_size(style.size);
    let mut label = div()
        .min_w_0()
        .text_size(px(size))
        .line_height(px(line))
        .font_weight(weight(style.weight))
        .text_color(
            style
                .color
                .map_or_else(|| rgb(pal.text_default).into(), |token| color(token, pal)),
        )
        .child(text.to_owned());
    if style.font == krad_extensions::api::view::Font::Mono {
        label = label.font_family(cx.theme().mono_font_family.clone());
    }
    if style.wrap == Wrap::NoWrap {
        label = label.whitespace_nowrap().text_ellipsis().overflow_hidden();
    }
    label.into_any_element()
}

pub(super) fn badge(text: &str, tone: Tone, pal: &Palette) -> AnyElement {
    div()
        .flex_none()
        .h(px(CHIP_H))
        .px(px(SPACE_1_5))
        .flex()
        .items_center()
        .rounded(px(radius(krad_extensions::api::view::Radius::Full)))
        .bg(rgb(pal.chip_bg))
        .text_size(px(TEXT_2XS))
        .text_color(rgb(tone_color(tone, pal)))
        .child(text.to_owned())
        .into_any_element()
}

fn details(entries: &[Detail], pal: &Palette, cx: &mut Context<ExtensionView>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .children(entries.iter().map(|entry| {
            div()
                .flex()
                .flex_row()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .flex_none()
                        .w(px(DETAIL_LABEL_W))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(entry.label.clone()),
                )
                .child(div().flex_1().min_w_0().child(text_element(
                    &entry.value,
                    entry.style,
                    pal,
                    cx,
                )))
        }))
        .into_any_element()
}

fn placeholder(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(SPACE_2))
        .p(px(space(krad_extensions::api::view::Space::Md)))
        .text_size(px(TEXT_XS))
        .text_color(rgb(pal.text_muted))
        .child(icon(IconName::Loading, ICON_XS, pal.text_muted))
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_images_never_leave_the_extension_folder() {
        let folder = Path::new("/ext/docker");
        assert_eq!(
            bundled_path(folder, Path::new("icons/whale.png")),
            Some(PathBuf::from("/ext/docker/icons/whale.png"))
        );
        assert_eq!(bundled_path(folder, Path::new("../secret.png")), None);
        assert_eq!(bundled_path(folder, Path::new("/etc/passwd")), None);
    }

    #[test]
    fn disabled_buttons_look_quiet_and_danger_looks_destructive() {
        assert_eq!(
            button_variant(ViewButton::Primary, Enabled::No),
            ButtonVariant::Ghost
        );
        assert_eq!(
            button_variant(ViewButton::Danger, Enabled::Yes),
            ButtonVariant::Destructive
        );
    }
}
