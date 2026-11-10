use gpui_kit::{
    canvas, div, px, AnyElement, App, Context, ElementId, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::KraddieId;
use std::cell::Cell;
use std::rc::Rc;

use super::super::super::EnvironmentShell;
use super::super::layout::{page_layout, PageLayout};
use super::super::segmented::{sliding_track, Sliding};
use super::ProfileSection;
use crate::components::card;
use crate::components::Stretch;
use crate::theme::{palette, SPACE_5};

const PAGE_ID: &str = "kraddie-page";
const NAV_ID: &str = "kraddie-page-nav";
const NAV_MENU: &str = "kraddie-page-nav";
const WIDTH_TOLERANCE: f32 = 0.5;

pub(in crate::environment_shell) fn width_probe(seen: &Rc<Cell<Option<f32>>>) -> AnyElement {
    let seen = Rc::clone(seen);
    canvas(
        move |bounds, window, _: &mut App| {
            let width = bounds.size.width.as_f32();
            let changed = seen
                .get()
                .is_none_or(|known| (known - width).abs() > WIDTH_TOLERANCE);
            if changed {
                seen.set(Some(width));
                window.refresh();
            }
        },
        |_, (), _, _: &mut App| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

impl EnvironmentShell {
    pub(in crate::environment_shell::kraddies) fn kraddie_layout(
        &self,
        window: &Window,
    ) -> PageLayout {
        let measured = self
            .kraddies
            .profile
            .width
            .get()
            .unwrap_or_else(|| window.viewport_size().width.as_f32());
        page_layout(measured)
    }

    pub(in crate::environment_shell) fn kraddie_page(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.kraddies.profile.open.clone()?;
        let entry = self.kraddies.entry(&id)?;
        let pal = palette(cx);
        let layout = self.kraddie_layout(window);
        let section = self.kraddies.profile.section;
        let body = match section {
            ProfileSection::Glance => self.glance_grid(entry, &layout, cx),
            ProfileSection::Compass => self.compass_section(&layout, cx),
            ProfileSection::Keepsakes => self.keepsakes_section(&id, &layout, cx),
            ProfileSection::Memory => self.kraddie_memory_area(&id, layout.content_w, cx),
            ProfileSection::Tuning => self.tuning_grid(&id, &layout, cx),
        };
        let column = div()
            .flex()
            .flex_col()
            .w(px(layout.content_w))
            .mx_auto()
            .gap(px(SPACE_5))
            .child(self.kraddie_hero(entry, &layout, cx))
            .child(self.section_switch(&id, &layout, cx))
            .child(body);
        Some(
            card(pal, cx)
                .size_full()
                .relative()
                .child(width_probe(&self.kraddies.profile.width))
                .child(
                    div()
                        .id(ElementId::from(PAGE_ID))
                        .size_full()
                        .overflow_y_scroll()
                        .py(px(layout.padding))
                        .child(column),
                )
                .into_any_element(),
        )
    }

    fn section_switch(
        &self,
        id: &KraddieId,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let current = self.kraddies.profile.section;
        let labels = ProfileSection::ALL.map(ProfileSection::label);
        let short = ProfileSection::ALL.map(ProfileSection::short_label);
        let sliding = Sliding {
            id: NAV_ID,
            labels: &labels,
            short: &short,
            chosen: current.index(),
            container: layout.content_w,
            stretch: Stretch::Fill,
            menu_open: self.kraddies.profile.open_menu == Some(NAV_MENU),
        };
        let track = sliding_track(
            &sliding,
            pal,
            cx,
            |index, segment| {
                let target = id.clone();
                let section = ProfileSection::ALL[index];
                segment
                    .tooltip(move |_, cx| {
                        crate::components::tooltip_view(section.hint().into(), cx)
                    })
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.kraddies.profile.open_menu = None;
                        this.show_profile_section(&target, section, window, cx);
                    }))
            },
            |button| {
                button.on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.kraddies.profile.toggle_menu(NAV_MENU);
                    cx.notify();
                }))
            },
        );
        div().flex().flex_row().child(track).into_any_element()
    }

    fn show_profile_section(
        &mut self,
        id: &KraddieId,
        section: ProfileSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drafted = self
            .kraddies
            .profile
            .compass
            .as_ref()
            .is_some_and(|draft| draft.kraddie == *id);
        if matches!(section, ProfileSection::Compass | ProfileSection::Keepsakes) && !drafted {
            self.open_compass_draft(id, window, cx);
        }
        if section == ProfileSection::Tuning {
            self.open_session_model_draft(id, window, cx);
        }
        self.kraddies.profile.editing_line = None;
        self.kraddies.profile.section = section;
        cx.notify();
    }
}
