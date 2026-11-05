use super::entry::IndexEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    ExactId,
    NamePrefix,
    Contains,
}

fn rank(entry: &IndexEntry, query: &str) -> Option<Rank> {
    let id = entry.id.as_ref().to_lowercase();
    let name = entry.name.to_lowercase();
    if id == query {
        return Some(Rank::ExactId);
    }
    if name.starts_with(query) || id.starts_with(query) {
        return Some(Rank::NamePrefix);
    }
    let contains = id.contains(query)
        || name.contains(query)
        || entry.description.to_lowercase().contains(query)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query));
    contains.then_some(Rank::Contains)
}

pub(super) fn search<'a>(entries: &'a [IndexEntry], query: &str) -> Vec<&'a IndexEntry> {
    let query = query.trim().to_lowercase();
    let mut found: Vec<(Rank, &IndexEntry)> = entries
        .iter()
        .filter_map(|entry| {
            if query.is_empty() {
                Some((Rank::Contains, entry))
            } else {
                rank(entry, &query).map(|rank| (rank, entry))
            }
        })
        .collect();
    found.sort_by(|(left_rank, left), (right_rank, right)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    found.into_iter().map(|(_, entry)| entry).collect()
}
