use crate::ids::TerminalId;
use crate::mosaic::UnitRect;
use serde::{Deserialize, Serialize};

pub const MAX_PANES: usize = 4;
pub const MIN_PANE_SHARE: f32 = 0.15;
pub const DIVIDER_SNAPS: [f32; 5] = [1.0 / 3.0, 0.5, 2.0 / 3.0, 0.25, 0.75];
pub const DIVIDER_SNAP_DISTANCE: f32 = 0.03;
const HALF: f32 = 0.5;
const WHOLE: UnitRect = UnitRect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DividerId(pub u32);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub id: PaneId,
    pub tabs: Vec<TerminalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown: Option<TerminalId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneNode {
    Pane(PaneId),
    Split {
        divider: DividerId,
        axis: Axis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapZone {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl SnapZone {
    pub const ALL: [Self; 8] = [
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    #[must_use]
    pub fn frame(self, pane: UnitRect) -> UnitRect {
        let (half_w, half_h) = (pane.w * HALF, pane.h * HALF);
        let (right, bottom) = (pane.x + half_w, pane.y + half_h);
        let at = |x, y, w, h| UnitRect { x, y, w, h };
        match self {
            Self::Left => at(pane.x, pane.y, half_w, pane.h),
            Self::Right => at(right, pane.y, half_w, pane.h),
            Self::Top => at(pane.x, pane.y, pane.w, half_h),
            Self::Bottom => at(pane.x, bottom, pane.w, half_h),
            Self::TopLeft => at(pane.x, pane.y, half_w, half_h),
            Self::TopRight => at(right, pane.y, half_w, half_h),
            Self::BottomLeft => at(pane.x, bottom, half_w, half_h),
            Self::BottomRight => at(right, bottom, half_w, half_h),
        }
    }

    #[must_use]
    pub fn at(x: f32, y: f32, reach: f32) -> Option<Self> {
        let (left, right) = (x < reach, x > 1.0 - reach);
        let (top, bottom) = (y < reach, y > 1.0 - reach);
        match (left, right, top, bottom) {
            (true, _, true, _) => Some(Self::TopLeft),
            (_, true, true, _) => Some(Self::TopRight),
            (true, _, _, true) => Some(Self::BottomLeft),
            (_, true, _, true) => Some(Self::BottomRight),
            (true, ..) => Some(Self::Left),
            (_, true, ..) => Some(Self::Right),
            (_, _, true, _) => Some(Self::Top),
            (_, _, _, true) => Some(Self::Bottom),
            _ => None,
        }
    }

    const fn is_quarter(self) -> bool {
        matches!(
            self,
            Self::TopLeft | Self::TopRight | Self::BottomLeft | Self::BottomRight
        )
    }

    const fn half_split(self) -> (Axis, bool) {
        match self {
            Self::Left | Self::TopLeft | Self::BottomLeft => (Axis::Row, true),
            Self::Right | Self::TopRight | Self::BottomRight => (Axis::Row, false),
            Self::Top => (Axis::Column, true),
            Self::Bottom => (Axis::Column, false),
        }
    }

    const fn upper(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutTemplate {
    TwoColumns,
    TwoRows,
    MainAndStack,
    Grid,
}

impl LayoutTemplate {
    pub const ALL: [Self; 4] = [
        Self::TwoColumns,
        Self::TwoRows,
        Self::MainAndStack,
        Self::Grid,
    ];

    #[must_use]
    pub fn slots(self) -> Vec<UnitRect> {
        let panes = SessionPanes::with_shape(self, &[]);
        panes.layout().into_iter().map(|(_, rect)| rect).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneRefusal {
    Full,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionPanes {
    pub panes: Vec<Pane>,
    pub root: PaneNode,
    next_id: u32,
}

impl Default for SessionPanes {
    fn default() -> Self {
        Self::single(Vec::new())
    }
}

impl SessionPanes {
    #[must_use]
    pub fn single(tabs: Vec<TerminalId>) -> Self {
        let id = PaneId(0);
        Self {
            panes: vec![Pane {
                id,
                shown: tabs.first().copied(),
                tabs,
            }],
            root: PaneNode::Pane(id),
            next_id: 1,
        }
    }

    fn with_shape(template: LayoutTemplate, tabs: &[TerminalId]) -> Self {
        let mut panes = Self::single(tabs.to_vec());
        let first = PaneId(0);
        match template {
            LayoutTemplate::TwoColumns => {
                panes.split(first, Axis::Row, false);
            }
            LayoutTemplate::TwoRows => {
                panes.split(first, Axis::Column, false);
            }
            LayoutTemplate::MainAndStack => {
                let stack = panes.split(first, Axis::Row, false);
                panes.split(stack, Axis::Column, false);
            }
            LayoutTemplate::Grid => {
                let right = panes.split(first, Axis::Row, false);
                panes.split(first, Axis::Column, false);
                panes.split(right, Axis::Column, false);
            }
        }
        panes
    }

    fn fresh_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    #[must_use]
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.iter().find(|pane| pane.id == id)
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|pane| pane.id == id)
    }

    #[must_use]
    pub fn pane_of(&self, tab: TerminalId) -> Option<PaneId> {
        self.panes
            .iter()
            .find(|pane| pane.tabs.contains(&tab))
            .map(|pane| pane.id)
    }

    #[must_use]
    pub fn layout(&self) -> Vec<(PaneId, UnitRect)> {
        let mut out = Vec::new();
        lay_out(&self.root, WHOLE, &mut out);
        out
    }

    #[must_use]
    pub fn dividers(&self) -> Vec<(DividerId, Axis, f32, UnitRect)> {
        let mut out = Vec::new();
        dividers_of(&self.root, WHOLE, &mut out);
        out
    }

    pub fn sync(&mut self, terminals: &[TerminalId], guests: &[TerminalId], into: Option<PaneId>) {
        let emptied: Vec<PaneId> = self
            .panes
            .iter_mut()
            .filter_map(|pane| {
                let had = !pane.tabs.is_empty();
                pane.tabs
                    .retain(|tab| terminals.contains(tab) || guests.contains(tab));
                if pane.shown.is_some_and(|shown| !pane.tabs.contains(&shown)) {
                    pane.shown = pane.tabs.first().copied();
                }
                (had && pane.tabs.is_empty()).then_some(pane.id)
            })
            .collect();
        for id in emptied {
            self.remove_pane(id);
        }
        let target = into
            .filter(|id| self.pane(*id).is_some())
            .or_else(|| self.panes.first().map(|pane| pane.id));
        let Some(target) = target else {
            *self = Self::single(terminals.to_vec());
            return;
        };
        let missing: Vec<TerminalId> = terminals
            .iter()
            .copied()
            .filter(|tab| self.pane_of(*tab).is_none())
            .collect();
        if let Some(pane) = self.pane_mut(target) {
            pane.tabs.extend(missing);
            if pane.shown.is_none() {
                pane.shown = pane.tabs.first().copied();
            }
        }
    }

    pub fn show(&mut self, tab: TerminalId) {
        if let Some(pane) = self.panes.iter_mut().find(|pane| pane.tabs.contains(&tab)) {
            pane.shown = Some(tab);
        }
    }

    fn take(&mut self, tab: TerminalId) {
        let Some(from) = self.pane_of(tab) else {
            return;
        };
        let emptied = self.pane_mut(from).is_some_and(|pane| {
            pane.tabs.retain(|other| *other != tab);
            if pane.shown == Some(tab) {
                pane.shown = pane.tabs.first().copied();
            }
            pane.tabs.is_empty()
        });
        if emptied {
            self.remove_pane(from);
        }
    }

    /// # Errors
    ///
    /// Returns [`PaneRefusal::Unknown`] when `to` is not a pane of the view.
    pub fn move_tab(
        &mut self,
        tab: TerminalId,
        to: PaneId,
        index: usize,
    ) -> Result<(), PaneRefusal> {
        if self.pane(to).is_none() {
            return Err(PaneRefusal::Unknown);
        }
        if self.pane_of(tab) != Some(to) {
            self.take(tab);
        }
        let pane = self.pane_mut(to).ok_or(PaneRefusal::Unknown)?;
        pane.tabs.retain(|other| *other != tab);
        let at = index.min(pane.tabs.len());
        pane.tabs.insert(at, tab);
        pane.shown = Some(tab);
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`PaneRefusal::Full`] when the split would pass
    /// [`MAX_PANES`], and [`PaneRefusal::Unknown`] for a pane that is not
    /// part of the view.
    pub fn snap(
        &mut self,
        tab: TerminalId,
        target: PaneId,
        zone: SnapZone,
    ) -> Result<PaneId, PaneRefusal> {
        if self.pane(target).is_none() {
            return Err(PaneRefusal::Unknown);
        }
        let alone = self.pane(target).is_some_and(|pane| pane.tabs == [tab]);
        let leaving = usize::from(alone);
        let adding = if zone.is_quarter() { 2 } else { 1 };
        if self.panes.len() - leaving + adding > MAX_PANES {
            return Err(PaneRefusal::Full);
        }
        if alone {
            return Ok(target);
        }
        self.take(tab);
        let (axis, new_first) = zone.half_split();
        let side = self.split(target, axis, new_first);
        let placed = if zone.is_quarter() {
            let other = self.split(side, Axis::Column, !zone.upper());
            if zone.upper() {
                side
            } else {
                other
            }
        } else {
            side
        };
        if let Some(pane) = self.pane_mut(placed) {
            pane.tabs.push(tab);
            pane.shown = Some(tab);
        }
        Ok(placed)
    }

    fn split(&mut self, target: PaneId, axis: Axis, new_first: bool) -> PaneId {
        let new = PaneId(self.fresh_id());
        let divider = DividerId(self.fresh_id());
        self.panes.push(Pane {
            id: new,
            tabs: Vec::new(),
            shown: None,
        });
        replace_leaf(&mut self.root, target, &|old| {
            let (first, second) = if new_first {
                (PaneNode::Pane(new), old)
            } else {
                (old, PaneNode::Pane(new))
            };
            PaneNode::Split {
                divider,
                axis,
                ratio: HALF,
                first: Box::new(first),
                second: Box::new(second),
            }
        });
        new
    }

    pub fn apply_template(&mut self, template: LayoutTemplate, tab: TerminalId, slot: usize) {
        self.take(tab);
        let groups: Vec<Pane> = std::mem::take(&mut self.panes)
            .into_iter()
            .filter(|pane| !pane.tabs.is_empty())
            .collect();
        let mut shaped = Self::with_shape(template, &[]);
        let order: Vec<PaneId> = shaped.layout().into_iter().map(|(id, _)| id).collect();
        let slot = slot.min(order.len().saturating_sub(1));
        let mut rest = groups.into_iter();
        for (index, id) in order.iter().enumerate() {
            let Some(pane) = shaped.pane_mut(*id) else {
                continue;
            };
            if index == slot {
                pane.tabs.push(tab);
                pane.shown = Some(tab);
                continue;
            }
            if let Some(group) = rest.next() {
                pane.tabs = group.tabs;
                pane.shown = group.shown;
            }
        }
        let leftover: Vec<TerminalId> = rest.flat_map(|pane| pane.tabs).collect();
        if let Some(last) = order.last().and_then(|id| shaped.pane_mut(*id)) {
            last.tabs.extend(leftover);
            if last.shown.is_none() {
                last.shown = last.tabs.first().copied();
            }
        }
        *self = shaped;
    }

    pub fn release(&mut self, tab: TerminalId) {
        self.take(tab);
    }

    pub fn close_pane(&mut self, id: PaneId) {
        if self.panes.len() <= 1 {
            return;
        }
        let tabs = self.pane(id).map_or_default(|pane| pane.tabs.clone());
        self.remove_pane(id);
        let heir = self.panes.first().map(|pane| pane.id);
        if let Some(pane) = heir.and_then(|heir| self.pane_mut(heir)) {
            pane.tabs.extend(tabs);
            if pane.shown.is_none() {
                pane.shown = pane.tabs.first().copied();
            }
        }
    }

    fn remove_pane(&mut self, id: PaneId) {
        if self.panes.len() <= 1 {
            return;
        }
        self.panes.retain(|pane| pane.id != id);
        remove_leaf(&mut self.root, id);
    }

    pub fn resize(&mut self, id: DividerId, ratio: f32) {
        let ratio = snap_share(ratio.clamp(MIN_PANE_SHARE, 1.0 - MIN_PANE_SHARE));
        set_ratio(&mut self.root, id, ratio);
    }
}

#[must_use]
pub fn snap_share(ratio: f32) -> f32 {
    DIVIDER_SNAPS
        .iter()
        .copied()
        .find(|share| (ratio - share).abs() < DIVIDER_SNAP_DISTANCE)
        .unwrap_or(ratio)
}

fn parts(rect: UnitRect, axis: Axis, ratio: f32) -> (UnitRect, UnitRect) {
    match axis {
        Axis::Row => {
            let w = rect.w * ratio;
            (
                UnitRect { w, ..rect },
                UnitRect {
                    x: rect.x + w,
                    w: rect.w - w,
                    ..rect
                },
            )
        }
        Axis::Column => {
            let h = rect.h * ratio;
            (
                UnitRect { h, ..rect },
                UnitRect {
                    y: rect.y + h,
                    h: rect.h - h,
                    ..rect
                },
            )
        }
    }
}

fn lay_out(node: &PaneNode, rect: UnitRect, out: &mut Vec<(PaneId, UnitRect)>) {
    match node {
        PaneNode::Pane(id) => out.push((*id, rect)),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
            ..
        } => {
            let (a, b) = parts(rect, *axis, *ratio);
            lay_out(first, a, out);
            lay_out(second, b, out);
        }
    }
}

fn dividers_of(node: &PaneNode, rect: UnitRect, out: &mut Vec<(DividerId, Axis, f32, UnitRect)>) {
    if let PaneNode::Split {
        divider,
        axis,
        ratio,
        first,
        second,
    } = node
    {
        out.push((*divider, *axis, *ratio, rect));
        let (a, b) = parts(rect, *axis, *ratio);
        dividers_of(first, a, out);
        dividers_of(second, b, out);
    }
}

fn replace_leaf(node: &mut PaneNode, target: PaneId, make: &dyn Fn(PaneNode) -> PaneNode) {
    match node {
        PaneNode::Pane(id) if *id == target => {
            let old = std::mem::replace(node, PaneNode::Pane(target));
            *node = make(old);
        }
        PaneNode::Pane(_) => {}
        PaneNode::Split { first, second, .. } => {
            replace_leaf(first, target, make);
            replace_leaf(second, target, make);
        }
    }
}

fn remove_leaf(node: &mut PaneNode, target: PaneId) {
    let PaneNode::Split { first, second, .. } = node else {
        return;
    };
    let keep = match (first.as_ref(), second.as_ref()) {
        (PaneNode::Pane(id), _) if *id == target => Some(second.as_ref().clone()),
        (_, PaneNode::Pane(id)) if *id == target => Some(first.as_ref().clone()),
        _ => None,
    };
    if let Some(keep) = keep {
        *node = keep;
        return;
    }
    remove_leaf(first, target);
    remove_leaf(second, target);
}

fn set_ratio(node: &mut PaneNode, target: DividerId, value: f32) {
    if let PaneNode::Split {
        divider,
        ratio,
        first,
        second,
        ..
    } = node
    {
        if *divider == target {
            *ratio = value;
            return;
        }
        set_ratio(first, target, value);
        set_ratio(second, target, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: TerminalId = TerminalId(1);
    const B: TerminalId = TerminalId(2);
    const C: TerminalId = TerminalId(3);

    fn tabs_of(panes: &SessionPanes) -> Vec<Vec<TerminalId>> {
        panes
            .layout()
            .into_iter()
            .filter_map(|(id, _)| panes.pane(id).map(|pane| pane.tabs.clone()))
            .collect()
    }

    #[test]
    fn a_half_splits_the_pane_and_takes_the_tab() {
        let mut panes = SessionPanes::single(vec![A, B]);
        let right = panes.snap(B, PaneId(0), SnapZone::Right);
        assert!(right.is_ok());
        assert_eq!(tabs_of(&panes), [vec![A], vec![B]]);
        let frames: Vec<UnitRect> = panes.layout().into_iter().map(|(_, rect)| rect).collect();
        assert!((frames[1].x - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_quarter_leaves_an_empty_slot_beside_it() {
        let mut panes = SessionPanes::single(vec![A, B]);
        assert!(panes.snap(B, PaneId(0), SnapZone::TopRight).is_ok());
        assert_eq!(tabs_of(&panes), [vec![A], vec![B], vec![]]);
    }

    #[test]
    fn the_view_holds_at_most_four_panes() {
        let mut panes = SessionPanes::single(vec![A, B, C, TerminalId(4), TerminalId(5)]);
        assert!(panes.snap(B, PaneId(0), SnapZone::Right).is_ok());
        assert!(panes.snap(C, PaneId(0), SnapZone::Bottom).is_ok());
        assert!(panes.snap(TerminalId(4), PaneId(0), SnapZone::Left).is_ok());
        assert_eq!(
            panes.snap(TerminalId(5), PaneId(0), SnapZone::Top),
            Err(PaneRefusal::Full)
        );
    }

    #[test]
    fn a_pane_left_empty_goes_and_its_neighbour_grows() {
        let mut panes = SessionPanes::single(vec![A, B]);
        let Ok(right) = panes.snap(B, PaneId(0), SnapZone::Right) else {
            unreachable!("a pane with two tabs splits");
        };
        assert!(panes.move_tab(B, PaneId(0), 0).is_ok());
        assert_eq!(panes.panes.len(), 1);
        assert_eq!(panes.root, PaneNode::Pane(PaneId(0)));
        assert!(panes.pane(right).is_none());
        assert_eq!(tabs_of(&panes), [vec![B, A]]);
    }

    #[test]
    fn templates_keep_the_other_panes_in_their_slots() {
        let mut panes = SessionPanes::single(vec![A, B, C]);
        panes.apply_template(LayoutTemplate::Grid, C, 3);
        assert_eq!(tabs_of(&panes), [vec![A, B], vec![], vec![], vec![C]]);
        assert_eq!(LayoutTemplate::MainAndStack.slots().len(), 3);
    }

    #[test]
    fn sync_follows_the_session_and_dividers_snap() {
        let mut panes = SessionPanes::single(vec![A]);
        panes.sync(&[A, B], &[], None);
        assert_eq!(tabs_of(&panes), [vec![A, B]]);
        assert!(panes.snap(B, PaneId(0), SnapZone::Bottom).is_ok());
        panes.sync(&[A], &[], None);
        assert_eq!(panes.panes.len(), 1, "a pane whose tab closed goes");
        panes.sync(&[A, B], &[], None);
        assert!(panes.snap(B, PaneId(0), SnapZone::Left).is_ok());
        let Some((divider, ..)) = panes.dividers().first().copied() else {
            unreachable!("a split has a divider");
        };
        panes.resize(divider, 0.49);
        assert!(
            matches!(panes.root, PaneNode::Split { ratio, .. } if (ratio - 0.5).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn guests_stay_where_dropped_but_never_join() {
        const GUEST: TerminalId = TerminalId(9);
        let mut panes = SessionPanes::single(vec![A]);
        panes.sync(&[A], &[GUEST], None);
        assert_eq!(tabs_of(&panes), [vec![A]], "a guest is not pulled in");
        assert!(panes.snap(GUEST, PaneId(0), SnapZone::Right).is_ok());
        panes.sync(&[A], &[GUEST], None);
        assert_eq!(tabs_of(&panes), [vec![A], vec![GUEST]]);
        panes.release(GUEST);
        assert_eq!(tabs_of(&panes), [vec![A]]);
    }

    #[test]
    fn zones_follow_the_pointer_and_corners_win() {
        assert_eq!(SnapZone::at(0.05, 0.5, 0.25), Some(SnapZone::Left));
        assert_eq!(SnapZone::at(0.95, 0.05, 0.25), Some(SnapZone::TopRight));
        assert_eq!(SnapZone::at(0.5, 0.5, 0.25), None);
    }
}
