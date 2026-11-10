use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{
    BodyStyle, KraddieAnimation, KraddieColor, PaintTarget, PaintTool, PaletteIndex, MAX_LAYERS,
    MAX_PALETTE,
};

use super::super::super::dialog_parts::{
    field_label, segment, sheet_overlay, sheet_panel, sheet_title, SegmentSize,
};
use super::super::super::EnvironmentShell;
use super::canvas::CANVAS_PX;
use super::tools::{face_targets, target_label, SWATCHES};
use super::KraddieStudio;
use crate::components::{Button, IconName};
use crate::theme::{palette, Palette, SPACE_1, SPACE_2, SPACE_3, SPACE_6};

const SHEET_W: f32 = 1040.0;
const SWATCH: f32 = 20.0;
const SWATCH_RADIUS: f32 = 4.0;
const TITLE: &str = "Kraddie studio";
const SUBTITLE: &str = "Paint your Kraddie on layers; every mood gets its own face and motion";
const BTN_CLOSE: &str = "studio-close";
const BTN_ADD_LAYER: &str = "studio-add-layer";
const BTN_REMOVE_LAYER: &str = "studio-remove-layer";
const BTN_ADD_COLOR: &str = "studio-add-color";
const ADD_LAYER_LABEL: &str = "Add layer";
const REMOVE_LAYER_LABEL: &str = "Remove layer";
const ADD_COLOR_TOOLTIP: &str = "Add a color to the palette";
const EYE_TOOLTIP: &str = "Show or hide this layer";
const HIDDEN_MARK: &str = " (hidden)";
const TOOLS_LABEL: &str = "Tool";
const PALETTE_LABEL: &str = "Palette";
const MIX_LABEL: &str = "Recolor the selected slot";
const PAINT_ON_LABEL: &str = "Paint on";
const ANIMATION_LABEL: &str = "Animation of the shown mood";
const STYLE_LABEL: &str = "Start again from a Kraddie style";
const PREVIEW_LABEL: &str = "Moods";
const TOOL_PREFIX: &str = "studio-tool";
const SLOT_PREFIX: &str = "studio-slot";
const SWATCH_PREFIX: &str = "studio-swatch";
const SWATCH_PRESSED: f32 = 0.6;
const LAYER_PREFIX: &str = "studio-layer";
const EYE_PREFIX: &str = "studio-eye";
const FACE_PREFIX: &str = "studio-face";
const ANIMATION_PREFIX: &str = "studio-animation";
const STYLE_PREFIX: &str = "studio-style";

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_kraddie_studio(cx);
}

fn row() -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(SPACE_1))
}

fn swatch(color: KraddieColor, selected: bool, pal: &Palette) -> Div {
    let ring = pal.border;
    div()
        .flex_none()
        .size(px(SWATCH))
        .rounded(px(SWATCH_RADIUS))
        .cursor_pointer()
        .bg(rgb(color.hex()))
        .border_2()
        .border_color(pal.divider)
        .when(selected, |swatch| swatch.border_color(rgb(pal.accent)))
        .when(!selected, |swatch| {
            swatch.hover(move |style| style.border_color(ring))
        })
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_studio_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let studio = self.kraddies.studio.as_ref()?;
        let pal = palette(cx);
        let tools = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_2))
            .child(field_label(TOOLS_LABEL, pal))
            .child(row().children(Self::studio_tools(studio, pal, cx)))
            .child(field_label(PALETTE_LABEL, pal))
            .child(row().children(Self::studio_slots(studio, pal, cx)))
            .child(field_label(MIX_LABEL, pal))
            .child(row().children(Self::studio_swatches(pal, cx)))
            .child(field_label(PAINT_ON_LABEL, pal))
            .child(row().children(Self::studio_targets(studio, pal, cx)))
            .child(field_label(ANIMATION_LABEL, pal))
            .child(row().children(Self::studio_animations(studio, pal, cx)))
            .child(field_label(STYLE_LABEL, pal))
            .child(row().children(Self::studio_styles(studio, pal, cx)))
            .child(field_label(PREVIEW_LABEL, pal))
            .child(
                row()
                    .gap(px(SPACE_3))
                    .children(self.studio_previews(studio, pal, cx)),
            );
        let body = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_6))
            .px(px(SPACE_6))
            .pb(px(SPACE_6))
            .min_h(px(CANVAS_PX))
            .child(Self::studio_canvas(studio, pal, cx))
            .child(tools);
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(
                TITLE,
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                close,
                cx,
            ))
            .child(body)
            .child(Self::studio_footer(studio, pal, cx));
        Some(sheet_overlay(sheet, close, cx))
    }

    fn studio_tools(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        PaintTool::ALL
            .iter()
            .enumerate()
            .map(|(index, &tool)| {
                segment(
                    (TOOL_PREFIX, index),
                    tool.label(),
                    None,
                    studio.tool == tool,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, |studio| studio.tool = tool);
                }))
                .into_any_element()
            })
            .collect()
    }

    fn studio_slots(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut slots: Vec<AnyElement> = studio
            .design
            .palette
            .iter()
            .enumerate()
            .filter_map(|(index, &color)| {
                let slot = PaletteIndex::try_from(index).ok()?;
                Some(
                    swatch(color, studio.color == slot, pal)
                        .id((SLOT_PREFIX, index))
                        .active(|style| style.opacity(SWATCH_PRESSED))
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.edit_studio(cx, |studio| studio.color = slot);
                        }))
                        .into_any_element(),
                )
            })
            .collect();
        if studio.design.palette.len() < MAX_PALETTE {
            slots.push(
                Button::new(BTN_ADD_COLOR)
                    .icon(IconName::Add)
                    .small()
                    .tooltip(ADD_COLOR_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, KraddieStudio::add_color);
                    }))
                    .into_any_element(),
            );
        }
        slots
    }

    fn studio_swatches(pal: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        SWATCHES
            .iter()
            .enumerate()
            .map(|(index, &color)| {
                swatch(color, false, pal)
                    .id((SWATCH_PREFIX, index))
                    .active(|style| style.opacity(SWATCH_PRESSED))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, |studio| studio.mix(color));
                    }))
                    .into_any_element()
            })
            .collect()
    }

    fn studio_targets(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let layers = studio
            .design
            .layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let target = PaintTarget::Layer(index);
                let mut label = target_label(target, Some(&layer.name));
                if !layer.shown.is_on() {
                    label.push_str(HIDDEN_MARK);
                }
                row()
                    .child(
                        segment(
                            (LAYER_PREFIX, index),
                            label,
                            None,
                            studio.target == target,
                            SegmentSize::Compact,
                            pal,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this: &mut Self, _, _, cx| {
                                this.edit_studio(cx, |studio| {
                                    studio.target = target;
                                    studio.redraw();
                                });
                            },
                        )),
                    )
                    .child(
                        Button::new((EYE_PREFIX, index))
                            .icon(IconName::Expand)
                            .small()
                            .tooltip(EYE_TOOLTIP)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.edit_studio(cx, |studio| {
                                    studio.change(|design| design.toggle_layer(index));
                                });
                            })),
                    )
                    .into_any_element()
            });
        let faces = face_targets()
            .into_iter()
            .enumerate()
            .map(|(index, target)| {
                segment(
                    (FACE_PREFIX, index),
                    target_label(target, None),
                    None,
                    studio.target == target,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, |studio| {
                        studio.target = target;
                        studio.redraw();
                    });
                }))
                .into_any_element()
            });
        let mut targets: Vec<AnyElement> = layers.chain(faces).collect();
        if studio.design.layers.len() < MAX_LAYERS {
            targets.push(
                Button::new(BTN_ADD_LAYER)
                    .label(ADD_LAYER_LABEL)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, |studio| {
                            let mut added = None;
                            studio.change(|design| {
                                added = design.add_layer();
                                added.is_some()
                            });
                            if let Some(index) = added {
                                studio.target = PaintTarget::Layer(index);
                            }
                        });
                    }))
                    .into_any_element(),
            );
        }
        if let PaintTarget::Layer(index) = studio.target {
            targets.push(
                Button::new(BTN_REMOVE_LAYER)
                    .label(REMOVE_LAYER_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.edit_studio(cx, |studio| {
                            studio.change(|design| design.remove_layer(index));
                        });
                    }))
                    .into_any_element(),
            );
        }
        targets
    }

    fn studio_animations(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mood = studio.shown_mood();
        let current = studio.design.moods.get(mood).animation;
        KraddieAnimation::ALL
            .iter()
            .enumerate()
            .map(|(index, &animation)| {
                segment(
                    (ANIMATION_PREFIX, index),
                    animation.label(),
                    None,
                    current == animation,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, |studio| studio.animate(mood, animation));
                }))
                .into_any_element()
            })
            .collect()
    }

    fn studio_styles(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        BodyStyle::ALL
            .iter()
            .enumerate()
            .map(|(index, &style)| {
                segment(
                    (STYLE_PREFIX, index),
                    style.label(),
                    None,
                    studio.design.style == style,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, |studio| studio.restyle(style));
                }))
                .into_any_element()
            })
            .collect()
    }
}
