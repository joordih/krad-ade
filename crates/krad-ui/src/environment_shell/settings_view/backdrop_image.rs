use std::sync::Arc;

use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::{
    div, img, px, rgb, AnyElement, AppContext as _, Context, Entity, IntoElement, ObjectFit,
    ParentElement, StatefulInteractiveElement as _, Styled, StyledImage, Subscription,
};
use krad_domain::{BackdropFit, BackdropImage, BackdropPercent, BackdropPlace};

use super::super::dialog_parts::{segment, sheet_section, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, RADIUS_SM, SPACE_1, SPACE_2, SPACE_3, TEXT_XS};
use crate::window_frame::transparency_reduced;

const SECTION_GAP: f32 = SPACE_3;
const WINDOW_SECTION: &str = "Window backdrop";
const WINDOW_SECTION_HINT: &str =
    "A picture painted behind the whole window, under its panes, in both light and dark themes.";
const CHAT_SECTION: &str = "Chat backdrop";
const CHAT_SECTION_HINT: &str =
    "A picture painted behind the messages of chat sessions, separate from the window backdrop.";
const IMAGE_LABEL: &str = "Image";
const IMAGE_HINT: &str = "Copied into the app's settings folder, so moving the original keeps it.";
const REDUCED_HINT: &str = "Hidden while the system reduces transparency.";
const FIT_LABEL: &str = "Image fit";
const FIT_HINT: &str = "Fill crops to cover the window, Fit shows the whole picture, Tile repeats it and Center keeps its own size.";
const OPACITY_LABEL: &str = "Image opacity";
const OPACITY_HINT: &str = "How strongly the picture shows through.";
const BLUR_LABEL: &str = "Image blur";
const BLUR_HINT: &str = "Softens the picture so text in front of it stays easy to read.";
const DIM_LABEL: &str = "Image dim";
const DIM_HINT: &str = "Lays the theme's color over the picture to calm it down.";
const CHOOSE_LABEL: &str = "Choose…";
const REPLACE_LABEL: &str = "Replace…";
const REMOVE_LABEL: &str = "Remove";
const WINDOW_IDS: PlaceIds = PlaceIds {
    choose: "window-backdrop-choose",
    remove: "window-backdrop-remove",
    fit: "window-backdrop-fit",
};
const CHAT_IDS: PlaceIds = PlaceIds {
    choose: "chat-backdrop-choose",
    remove: "chat-backdrop-remove",
    fit: "chat-backdrop-fit",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlaceIds {
    choose: &'static str,
    remove: &'static str,
    fit: &'static str,
}
const PERCENT_SUFFIX: &str = "%";
const PERCENT_STEP: f32 = 1.0;
const SLIDER_W: f32 = 180.0;
const PERCENT_LABEL_W: f32 = 36.0;
const PREVIEW_W: f32 = 48.0;
const PREVIEW_H: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Amount {
    Opacity,
    Blur,
    Dim,
}

impl Amount {
    const ALL: [Self; 3] = [Self::Opacity, Self::Blur, Self::Dim];

    const fn of(self, image: &BackdropImage) -> BackdropPercent {
        match self {
            Self::Opacity => image.opacity,
            Self::Blur => image.blur,
            Self::Dim => image.dim,
        }
    }

    fn set(self, image: &mut BackdropImage, percent: BackdropPercent) {
        match self {
            Self::Opacity => image.opacity = percent,
            Self::Blur => image.blur = percent,
            Self::Dim => image.dim = percent,
        }
    }

    const fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Opacity => (OPACITY_LABEL, OPACITY_HINT),
            Self::Blur => (BLUR_LABEL, BLUR_HINT),
            Self::Dim => (DIM_LABEL, DIM_HINT),
        }
    }
}

const fn place_ids(place: BackdropPlace) -> PlaceIds {
    match place {
        BackdropPlace::Window => WINDOW_IDS,
        BackdropPlace::Chat => CHAT_IDS,
    }
}

const fn place_heading(place: BackdropPlace) -> (&'static str, &'static str) {
    match place {
        BackdropPlace::Window => (WINDOW_SECTION, WINDOW_SECTION_HINT),
        BackdropPlace::Chat => (CHAT_SECTION, CHAT_SECTION_HINT),
    }
}

fn percent_at(value: f32) -> BackdropPercent {
    let rounded = value.round();
    BackdropPercent::new(
        (0..=BackdropPercent::MAX)
            .find(|step| f32::from(*step) >= rounded)
            .unwrap_or(BackdropPercent::MAX),
    )
}

pub(super) struct BackdropUi {
    sliders: Vec<(BackdropPlace, Amount, Entity<SliderState>)>,
    _events: Vec<Subscription>,
}

fn percent_slider(
    slider: &Entity<SliderState>,
    percent: BackdropPercent,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1))
        .child(div().w(px(SLIDER_W)).child(Slider::new(slider)))
        .child(
            div()
                .w(px(PERCENT_LABEL_W))
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.text_muted))
                .child(format!("{}{PERCENT_SUFFIX}", percent.percent())),
        )
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn backdrop_ui(&self, cx: &mut Context<Self>) -> BackdropUi {
        let count = BackdropPlace::ALL.len() * Amount::ALL.len();
        let mut sliders = Vec::with_capacity(count);
        let mut events = Vec::with_capacity(count);
        for place in BackdropPlace::ALL {
            let start = self
                .settings
                .backdrop(place)
                .cloned()
                .unwrap_or_else(|| BackdropImage::new(""));
            for amount in Amount::ALL {
                let slider = cx.new(|_| {
                    SliderState::new()
                        .min(0.0)
                        .max(f32::from(BackdropPercent::MAX))
                        .step(PERCENT_STEP)
                        .default_value(f32::from(amount.of(&start).percent()))
                });
                events.push(
                    cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
                        let SliderEvent::Change(value) = event else {
                            return;
                        };
                        let percent = percent_at(value.start());
                        if this
                            .settings
                            .backdrop(place)
                            .is_some_and(|image| amount.of(image) == percent)
                        {
                            return;
                        }
                        this.edit_backdrop_image(place, |image| amount.set(image, percent), cx);
                    }),
                );
                sliders.push((place, amount, slider));
            }
        }
        BackdropUi {
            sliders,
            _events: events,
        }
    }

    pub(super) fn backdrop_sections(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let sections: Vec<AnyElement> = BackdropPlace::ALL
            .into_iter()
            .map(|place| {
                let (label, detail) = place_heading(place);
                let detail = if transparency_reduced() {
                    REDUCED_HINT
                } else {
                    detail
                };
                sheet_section(label, Some(detail), pal)
                    .child(panel_card(self.backdrop_rows(place, cx), pal))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .children(sections)
            .into_any_element()
    }

    fn backdrop_rows(&self, place: BackdropPlace, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        let chosen = self.settings.backdrop(place);
        let mut rows = vec![form_row(
            IMAGE_LABEL,
            Some(IMAGE_HINT),
            self.backdrop_picker(place, chosen.is_some(), pal, cx),
            pal,
        )];
        let Some(chosen) = chosen else {
            return rows;
        };
        rows.push(form_row(
            FIT_LABEL,
            Some(FIT_HINT),
            Self::backdrop_fits(place, chosen.fit, pal, cx),
            pal,
        ));
        let Some(ui) = &self.settings_ui else {
            return rows;
        };
        rows.extend(
            ui.backdrop
                .sliders
                .iter()
                .filter(|(owner, _, _)| *owner == place)
                .map(|(_, amount, slider)| {
                    let (label, detail) = amount.label();
                    form_row(
                        label,
                        Some(detail),
                        percent_slider(slider, amount.of(chosen), pal),
                        pal,
                    )
                }),
        );
        rows
    }

    fn backdrop_picker(
        &self,
        place: BackdropPlace,
        chosen: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ids = place_ids(place);
        let preview = self.backdrop_preview(place, pal);
        let choose = Button::new(ids.choose)
            .label(if chosen { REPLACE_LABEL } else { CHOOSE_LABEL })
            .icon(IconName::Folder)
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(move |_: &mut Self, _, _, cx| {
                Self::pick_backdrop_image(place, cx);
            }));
        let remove = chosen.then(|| {
            Button::new(ids.remove)
                .label(REMOVE_LABEL)
                .icon(IconName::Close)
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.clear_backdrop_image(place, cx);
                }))
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .children(preview)
            .child(choose)
            .children(remove)
            .into_any_element()
    }

    fn backdrop_preview(&self, place: BackdropPlace, pal: &Palette) -> Option<AnyElement> {
        let image = self.backdrop_picture(place)?;
        Some(
            div()
                .flex_none()
                .w(px(PREVIEW_W))
                .h(px(PREVIEW_H))
                .rounded(px(RADIUS_SM))
                .overflow_hidden()
                .border_1()
                .border_color(pal.border)
                .child(
                    img(Arc::clone(image))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .into_any_element(),
        )
    }

    fn backdrop_fits(
        place: BackdropPlace,
        current: BackdropFit,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let prefix = place_ids(place).fit;
        let segments = BackdropFit::ALL.into_iter().map(|fit| {
            segment(
                (prefix, fit as u64),
                fit.display_name(),
                None,
                current == fit,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_backdrop_image(place, |image| image.fit = fit, cx);
            }))
        });
        segmented_track(pal).children(segments).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slider_value_lands_on_a_whole_percent_inside_the_range() {
        assert_eq!(percent_at(0.0).percent(), 0);
        assert_eq!(percent_at(41.6).percent(), 42);
        assert_eq!(percent_at(180.0).percent(), BackdropPercent::MAX);
    }

    #[test]
    fn the_window_and_the_chat_get_their_own_section_and_ids() {
        assert_ne!(
            place_ids(BackdropPlace::Window),
            place_ids(BackdropPlace::Chat)
        );
        assert_ne!(
            place_heading(BackdropPlace::Window).0,
            place_heading(BackdropPlace::Chat).0
        );
    }

    #[test]
    fn each_amount_edits_only_its_own_field() {
        let mut image = BackdropImage::new("image.png");
        let before = image.clone();
        Amount::Blur.set(&mut image, BackdropPercent::new(64));
        assert_eq!(image.blur.percent(), 64);
        assert_eq!(image.opacity, before.opacity);
        assert_eq!(image.dim, before.dim);
    }
}
