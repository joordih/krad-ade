use gpui_kit::base::text::TextViewState;
use gpui_kit::{AppContext as _, Context, Entity};
use std::collections::HashMap;
use std::hash::Hash;

use super::super::EnvironmentShell;

const PROSE_VIEWS_KEPT: usize = 384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ProseKey {
    pub(super) message: usize,
    pub(super) part: usize,
    pub(super) block: usize,
}

struct Kept<T> {
    value: T,
    used: u64,
}

pub(super) struct Recent<K, T> {
    entries: HashMap<K, Kept<T>>,
    tick: u64,
    room: usize,
}

impl<K: Copy + Eq + Hash, T> Recent<K, T> {
    pub(super) fn new(room: usize) -> Self {
        Self {
            entries: HashMap::new(),
            tick: 0,
            room,
        }
    }

    pub(super) fn take_or(&mut self, key: K, make: impl FnOnce() -> T) -> &T {
        self.tick += 1;
        let tick = self.tick;
        if !self.entries.contains_key(&key) && self.entries.len() >= self.room {
            self.forget_oldest();
        }
        let kept = self.entries.entry(key).or_insert_with(|| Kept {
            value: make(),
            used: tick,
        });
        kept.used = tick;
        &kept.value
    }

    fn forget_oldest(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, kept)| kept.used)
            .map(|(key, _)| *key);
        if let Some(oldest) = oldest {
            self.entries.remove(&oldest);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }
}

pub(super) type ProseViews = Recent<ProseKey, Entity<TextViewState>>;

impl Default for ProseViews {
    fn default() -> Self {
        Self::new(PROSE_VIEWS_KEPT)
    }
}

impl EnvironmentShell {
    pub(super) fn prose_view(
        views: &mut ProseViews,
        key: ProseKey,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Entity<TextViewState> {
        let mut made = false;
        let state = views
            .take_or(key, || {
                made = true;
                cx.new(|cx| TextViewState::markdown(text, cx))
            })
            .clone();
        if !made {
            state.update(cx, |state, cx| state.set_text(text, cx));
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_view_is_reused_for_the_same_block() {
        let mut views: Recent<u8, String> = Recent::new(2);
        let mut calls = 0;
        let mut build = |text: &str| {
            calls += 1;
            text.to_owned()
        };
        assert_eq!(views.take_or(1, || build("a")), "a");
        assert_eq!(views.take_or(1, || build("b")), "a");
        assert_eq!(calls, 1);
    }

    #[test]
    fn the_least_recently_shown_view_is_forgotten_first() {
        let mut views: Recent<u8, u8> = Recent::new(2);
        views.take_or(1, || 1);
        views.take_or(2, || 2);
        views.take_or(1, || 1);
        views.take_or(3, || 3);
        assert_eq!(views.len(), 2);
        assert!(views.contains(&1));
        assert!(!views.contains(&2));
        assert!(views.contains(&3));
    }

    #[test]
    fn blocks_of_other_parts_or_messages_keep_their_own_views() {
        let mut views: Recent<ProseKey, u8> = Recent::new(8);
        let key = |message, part, block| ProseKey {
            message,
            part,
            block,
        };
        views.take_or(key(1, 0, 0), || 1);
        views.take_or(key(1, 0, 1), || 2);
        views.take_or(key(1, 1, 0), || 3);
        views.take_or(key(2, 0, 0), || 4);
        assert_eq!(views.len(), 4);
        assert_eq!(*views.take_or(key(1, 0, 1), || 9), 2);
    }
}
