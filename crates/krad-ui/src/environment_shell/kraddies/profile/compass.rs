use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::{
    div, px, rgb, Animation, AnimationExt as _, AnyElement, AppContext as _, Context, IntoElement,
    ParentElement, SharedString, Styled, Window,
};
use krad_domain::{KraddieCompass, KraddieId, MAX_COMPASS_CHARS};

use super::super::super::EnvironmentShell;
use super::super::layout::PageLayout;
use super::super::motion::{ease_out_cubic, shaking, FADE, SAVED_HOLD};
use super::super::tile::{caption, grid, tile, Tile};
use super::CompassDraft;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, ICON_SM, LINE_SM, SPACE_1, SPACE_3, TEXT_SM};

const COMPASS_ROWS: usize = 16;
const COMPASS_TITLE: &str = "Compass";
const COMPASS_DETAIL: &str =
    "Markdown that rides along with every run. Saved as compass.md in your config folder.";
const COMPASS_LIMIT_PREFIX: &str = "Up to";
const COMPASS_LIMIT_SUFFIX: &str = "characters";
const COMPASS_SAVE: &str = "kraddie-compass-save";
const COMPASS_SAVE_LABEL: &str = "Save compass";
const COMPASS_TOO_LONG: &str = "That compass is longer than it can carry";
const SAVED_LABEL: &str = "Saved";
const SAVED_ID: &str = "kraddie-compass-saved";
const REFUSED_ID: &str = "kraddie-compass-refused";
const TILE_PREFIX: &str = "kraddie-mind";
const MEMORY_PLACEHOLDER: &str = "Something it should not forget";

impl EnvironmentShell {
    pub(super) fn open_compass_draft(
        &mut self,
        id: &KraddieId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.kraddie_mind(id).compass.as_str().to_owned();
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(COMPASS_ROWS)
                .default_value(text)
        });
        self.kraddies.profile.compass = Some(CompassDraft {
            kraddie: id.clone(),
            text: state,
        });
        self.kraddies.profile.new_line =
            Some(cx.new(|cx| InputState::new(window, cx).placeholder(MEMORY_PLACEHOLDER)));
    }

    fn save_compass(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.kraddies.profile.compass.as_ref() else {
            return;
        };
        let kraddie = draft.kraddie.clone();
        let text = draft.text.read(cx).value().to_string();
        let Ok(compass) = KraddieCompass::try_from(text) else {
            self.kraddies.profile.compass_refused += 1;
            return self.toast(COMPASS_TOO_LONG, cx);
        };
        self.kraddies.profile.compass_saves += 1;
        self.edit_mind(&kraddie, |mind| mind.compass = compass, cx);
    }

    pub(super) fn compass_section(
        &self,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let profile = &self.kraddies.profile;
        let editor = profile.compass.as_ref().map(|draft| {
            let save_count = profile.compass_saves;
            let saved = (save_count > 0).then(|| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.success_fg))
                    .child(icon(IconName::CheckCircle, ICON_SM, pal.success_fg))
                    .child(SAVED_LABEL)
                    .with_animations(
                        (SAVED_ID, save_count),
                        vec![
                            Animation::new(FADE),
                            Animation::new(SAVED_HOLD),
                            Animation::new(FADE),
                        ],
                        |badge, phase, t| {
                            let shown = match phase {
                                0 => ease_out_cubic(t),
                                1 => 1.0,
                                _ => 1.0 - ease_out_cubic(t),
                            };
                            badge.opacity(shown)
                        },
                    )
            });
            let footer = div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap(px(SPACE_3))
                .children(saved)
                .child(
                    Button::new(COMPASS_SAVE)
                        .label(COMPASS_SAVE_LABEL)
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| this.save_compass(cx))),
                );
            let footer = if profile.compass_refused == 0 {
                footer.into_any_element()
            } else {
                shaking(footer, (REFUSED_ID, profile.compass_refused)).into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_3))
                .child(Textarea::new(&draft.text))
                .child(footer)
                .into_any_element()
        });
        let key = profile
            .compass
            .as_ref()
            .map_or_default(|draft| draft.kraddie.to_string());
        let limit = format!("{COMPASS_LIMIT_PREFIX} {MAX_COMPASS_CHARS} {COMPASS_LIMIT_SUFFIX}");
        let body = editor.unwrap_or_else(|| div().into_any_element());
        grid(
            layout.gutter,
            vec![tile(
                Tile {
                    id: SharedString::from(format!("{TILE_PREFIX}-{key}-compass")),
                    index: 0,
                    width: layout.content_w,
                    title: COMPASS_TITLE,
                    note: Some(COMPASS_DETAIL.into()),
                    accessory: Some(caption(limit, pal).into_any_element()),
                    body,
                },
                pal,
            )],
        )
        .into_any_element()
    }
}
