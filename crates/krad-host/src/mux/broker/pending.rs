use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};

const MAX_PENDING: usize = 4096;

#[derive(Default)]
pub(super) struct Pending {
    scripts: HashMap<String, String>,
    order: VecDeque<String>,
    issued: u64,
}

impl Pending {
    pub(super) fn insert(&mut self, script: String) -> String {
        self.issued += 1;
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(self.issued);
        let id = format!("{:016x}{:x}", hasher.finish(), self.issued);
        while self.order.len() >= MAX_PENDING {
            if let Some(oldest) = self.order.pop_front() {
                self.scripts.remove(&oldest);
            }
        }
        self.order.push_back(id.clone());
        self.scripts.insert(id.clone(), script);
        id
    }

    pub(super) fn take(&mut self, id: &str) -> Option<String> {
        let script = self.scripts.remove(id)?;
        self.order.retain(|queued| queued != id);
        Some(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_is_handed_out_once() {
        let mut pending = Pending::default();
        let id = pending.insert("true".to_owned());
        assert_eq!(pending.take(&id).as_deref(), Some("true"));
        assert_eq!(pending.take(&id), None);
    }

    #[test]
    fn the_oldest_scripts_give_way_when_too_many_wait() {
        let mut pending = Pending::default();
        let first = pending.insert("first".to_owned());
        for _ in 0..MAX_PENDING {
            pending.insert("later".to_owned());
        }
        assert_eq!(pending.take(&first), None);
    }
}
