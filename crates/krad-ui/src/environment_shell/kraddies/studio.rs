mod canvas;
mod footer;
mod tools;
mod view;

use gpui_kit::{Bounds, Context, Pixels};
use krad_domain::{
    BodyStyle, CanvasPoint, DesignHistory, KraddieAnimation, KraddieColor, KraddieDesign,
    KraddieId, KraddieMood, PaintTarget, PaintTool, PaletteIndex,
};
use std::cell::Cell;
use std::rc::Rc;

use super::super::EnvironmentShell;
use crate::kraddie_avatar::KraddieSprite;

struct Stroke {
    anchor: CanvasPoint,
    last: CanvasPoint,
    base: Option<KraddieDesign>,
}

pub(in crate::environment_shell) struct KraddieStudio {
    kraddie: KraddieId,
    design: KraddieDesign,
    history: DesignHistory,
    tool: PaintTool,
    color: PaletteIndex,
    target: PaintTarget,
    mood: KraddieMood,
    stroke: Option<Stroke>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    sprite: Rc<KraddieSprite>,
    previews: [Rc<KraddieSprite>; 5],
}

fn previews(design: &KraddieDesign) -> [Rc<KraddieSprite>; 5] {
    KraddieMood::ALL.map(|mood| Rc::new(KraddieSprite::new(design, mood)))
}

impl KraddieStudio {
    fn new(kraddie: KraddieId, design: KraddieDesign) -> Self {
        let mood = KraddieMood::Idle;
        Self {
            sprite: Rc::new(KraddieSprite::new(&design, mood)),
            previews: previews(&design),
            kraddie,
            history: DesignHistory::default(),
            tool: PaintTool::default(),
            color: PaletteIndex::FIRST,
            target: PaintTarget::Layer(0),
            mood,
            stroke: None,
            bounds: Rc::new(Cell::new(None)),
            design,
        }
    }

    const fn shown_mood(&self) -> KraddieMood {
        match self.target {
            PaintTarget::Expression(mood) => mood,
            PaintTarget::Layer(_) => self.mood,
        }
    }

    fn redraw(&mut self) {
        self.sprite = Rc::new(KraddieSprite::new(&self.design, self.shown_mood()));
    }

    fn refresh(&mut self) {
        self.redraw();
        self.previews = previews(&self.design);
        let last = self.design.layers.len().saturating_sub(1);
        if let PaintTarget::Layer(index) = self.target {
            if index > last {
                self.target = PaintTarget::Layer(last);
            }
        }
        if self.color.get() >= self.design.palette.len() {
            self.color = PaletteIndex::FIRST;
        }
    }

    fn change(&mut self, edit: impl FnOnce(&mut KraddieDesign) -> bool) {
        let before = self.design.clone();
        if edit(&mut self.design) {
            self.history.record(&before);
            self.refresh();
        }
    }

    fn press(&mut self, at: CanvasPoint) {
        self.history.record(&self.design);
        let base = self.tool.drags_a_shape().then(|| self.design.clone());
        self.design
            .paint(self.target, self.tool, at, at, self.color);
        self.stroke = Some(Stroke {
            anchor: at,
            last: at,
            base,
        });
        self.redraw();
    }

    fn drag(&mut self, at: CanvasPoint) {
        let Some(stroke) = &mut self.stroke else {
            return;
        };
        if stroke.last == at || self.tool == PaintTool::Fill {
            return;
        }
        let from = match &stroke.base {
            Some(base) => {
                self.design = base.clone();
                stroke.anchor
            }
            None => stroke.last,
        };
        stroke.last = at;
        self.design
            .paint(self.target, self.tool, from, at, self.color);
        self.redraw();
    }

    fn release(&mut self) {
        if self.stroke.take().is_some() {
            self.previews = previews(&self.design);
        }
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.design) {
            self.refresh();
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.design) {
            self.refresh();
        }
    }

    fn restyle(&mut self, style: BodyStyle) {
        self.change(|design| {
            *design = KraddieDesign::kraddie(style);
            true
        });
    }

    fn mix(&mut self, color: KraddieColor) {
        let slot = self.color;
        self.change(|design| design.set_color(slot, color));
    }

    fn add_color(&mut self) {
        let Some(color) = self.design.palette.get(self.color.get()).copied() else {
            return;
        };
        let mut added = None;
        self.change(|design| {
            added = design.add_color(color);
            added.is_some()
        });
        if let Some(added) = added {
            self.color = added;
        }
    }

    fn animate(&mut self, mood: KraddieMood, animation: KraddieAnimation) {
        self.change(|design| {
            let look = design.moods.get_mut(mood);
            let changed = look.animation != animation;
            look.animation = animation;
            changed
        });
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_kraddie_studio(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        let Some(design) = self
            .kraddies
            .entry(id)
            .map(|entry| entry.kraddie.design.clone())
        else {
            return;
        };
        self.kraddies.studio = Some(KraddieStudio::new(id.clone(), design));
        cx.notify();
    }

    fn close_kraddie_studio(&mut self, cx: &mut Context<Self>) {
        self.kraddies.studio = None;
        cx.notify();
    }

    fn save_kraddie_studio(&mut self, cx: &mut Context<Self>) {
        let Some(studio) = self.kraddies.studio.take() else {
            return;
        };
        let Some(mut kraddie) = self
            .kraddies
            .entry(&studio.kraddie)
            .map(|entry| entry.kraddie.clone())
        else {
            return cx.notify();
        };
        kraddie.design = studio.design;
        self.save_kraddie(kraddie, Some(&studio.kraddie), cx);
    }

    fn edit_studio(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut KraddieStudio)) {
        if let Some(studio) = &mut self.kraddies.studio {
            edit(studio);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: u8, y: u8) -> CanvasPoint {
        CanvasPoint::new(x, y).unwrap_or_else(|| unreachable!())
    }

    fn studio() -> KraddieStudio {
        let id = KraddieId::try_from("kraddie".to_owned()).unwrap_or_else(|_| unreachable!());
        let mut design = KraddieDesign::kraddie(BodyStyle::default());
        for layer in &mut design.layers {
            layer.pixels.clear();
        }
        KraddieStudio::new(id, design)
    }

    #[test]
    fn a_drag_paints_a_line_and_one_undo_takes_the_whole_stroke_back() {
        let mut studio = studio();
        studio.press(point(0, 0));
        studio.drag(point(3, 0));
        studio.drag(point(3, 3));
        studio.release();
        let grid = studio
            .design
            .grid(PaintTarget::Layer(0))
            .cloned()
            .unwrap_or_default();
        assert_eq!(grid.painted().count(), 7);
        studio.undo();
        let grid = studio
            .design
            .grid(PaintTarget::Layer(0))
            .cloned()
            .unwrap_or_default();
        assert_eq!(grid.painted().count(), 0);
        studio.redo();
        let grid = studio
            .design
            .grid(PaintTarget::Layer(0))
            .cloned()
            .unwrap_or_default();
        assert_eq!(grid.painted().count(), 7);
    }

    #[test]
    fn a_shape_follows_the_pointer_and_keeps_only_its_last_outline() {
        let mut studio = studio();
        studio.tool = PaintTool::Rectangle;
        studio.press(point(2, 2));
        studio.drag(point(10, 10));
        studio.drag(point(4, 4));
        studio.release();
        let grid = studio
            .design
            .grid(PaintTarget::Layer(0))
            .cloned()
            .unwrap_or_default();
        assert_eq!(grid.painted().count(), 8);
        assert_eq!(grid.get(point(10, 10)), None);
    }

    #[test]
    fn mixing_and_animating_are_undoable_edits() {
        let mut studio = studio();
        let red = KraddieColor::rgb(0x00ff_0000);
        studio.mix(red);
        assert_eq!(studio.design.palette[0], red);
        studio.animate(KraddieMood::Done, KraddieAnimation::Still);
        assert_eq!(studio.design.moods.done.animation, KraddieAnimation::Still);
        studio.undo();
        studio.undo();
        assert_ne!(studio.design.palette[0], red);
        let before = studio.design.palette.len();
        studio.add_color();
        assert_eq!(studio.design.palette.len(), before + 1);
        assert_eq!(studio.color.get(), before);
    }
}
