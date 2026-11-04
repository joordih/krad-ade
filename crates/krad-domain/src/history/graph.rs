use crate::review::CommitSha;

use super::HistoryCommit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lane(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LaneColor(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub from: Lane,
    pub to: Lane,
    pub color: LaneColor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRow {
    pub node: Lane,
    pub color: LaneColor,
    pub above: Vec<Segment>,
    pub below: Vec<Segment>,
    pub width: usize,
}

#[derive(Clone, Copy, Debug)]
struct Track<'a> {
    expects: &'a CommitSha,
    color: LaneColor,
}

#[derive(Default)]
struct Tracks<'a> {
    lanes: Vec<Option<Track<'a>>>,
    next_color: usize,
}

impl<'a> Tracks<'a> {
    fn fresh_color(&mut self) -> LaneColor {
        let color = LaneColor(self.next_color);
        self.next_color += 1;
        color
    }

    fn free_lane(&mut self) -> usize {
        self.lanes
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.lanes.push(None);
                self.lanes.len() - 1
            })
    }

    fn expecting(&self, sha: &CommitSha) -> Option<usize> {
        self.lanes
            .iter()
            .position(|track| track.is_some_and(|track| track.expects == sha))
    }

    fn passing<'b>(&'b self, opened: &'b [usize]) -> impl Iterator<Item = Segment> + 'b {
        self.lanes
            .iter()
            .enumerate()
            .filter(|(index, _)| !opened.contains(index))
            .filter_map(|(index, track)| {
                track.map(|track| Segment {
                    from: Lane(index),
                    to: Lane(index),
                    color: track.color,
                })
            })
    }

    fn open_lane(&mut self, parent: &'a CommitSha) -> (usize, LaneColor) {
        let lane = self.free_lane();
        let color = self.fresh_color();
        self.lanes[lane] = Some(Track {
            expects: parent,
            color,
        });
        (lane, color)
    }

    fn trim(&mut self) {
        while self.lanes.last().is_some_and(Option::is_none) {
            self.lanes.pop();
        }
    }

    fn row(&mut self, commit: &'a HistoryCommit) -> GraphRow {
        let node = self
            .expecting(&commit.sha)
            .unwrap_or_else(|| self.free_lane());
        let color = self.lanes[node].map_or_else(|| self.fresh_color(), |track| track.color);
        let mut above = Vec::new();
        for (index, track) in self.lanes.iter_mut().enumerate() {
            let Some(seen) = *track else {
                continue;
            };
            if seen.expects == &commit.sha {
                above.push(Segment {
                    from: Lane(index),
                    to: Lane(node),
                    color: seen.color,
                });
                *track = None;
            } else {
                above.push(Segment {
                    from: Lane(index),
                    to: Lane(index),
                    color: seen.color,
                });
            }
        }
        let mut below: Vec<Segment> = Vec::new();
        let mut opened = vec![node];
        let mut parents = commit.parents.iter();
        if let Some(first) = parents.next() {
            self.lanes[node] = Some(Track {
                expects: first,
                color,
            });
            below.push(Segment {
                from: Lane(node),
                to: Lane(node),
                color,
            });
        }
        for parent in parents {
            let (lane, lane_color) = if let Some(lane) = self.expecting(parent) {
                (lane, self.lanes[lane].map_or(color, |track| track.color))
            } else {
                let lane = self.open_lane(parent);
                opened.push(lane.0);
                lane
            };
            below.push(Segment {
                from: Lane(node),
                to: Lane(lane),
                color: lane_color,
            });
        }
        below.extend(self.passing(&opened));
        let width = self.lanes.len().max(node + 1);
        self.trim();
        GraphRow {
            node: Lane(node),
            color,
            above,
            below,
            width,
        }
    }
}

#[must_use]
pub fn commit_graph(commits: &[HistoryCommit]) -> Vec<GraphRow> {
    let mut tracks = Tracks::default();
    commits.iter().map(|commit| tracks.row(commit)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn commit(sha: &str, parents: &[&str]) -> HistoryCommit {
        HistoryCommit {
            sha: CommitSha(sha.to_owned()),
            parents: parents
                .iter()
                .map(|parent| CommitSha((*parent).to_owned()))
                .collect(),
            author: String::new(),
            time: SystemTime::UNIX_EPOCH,
            labels: Vec::new(),
            subject: String::new(),
        }
    }

    fn segment(from: usize, to: usize, color: usize) -> Segment {
        Segment {
            from: Lane(from),
            to: Lane(to),
            color: LaneColor(color),
        }
    }

    #[test]
    fn a_straight_history_stays_in_one_lane() {
        let rows = commit_graph(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
        assert!(rows.iter().all(|row| row.node == Lane(0) && row.width == 1));
        assert_eq!(rows[0].above, []);
        assert_eq!(rows[1].above, [segment(0, 0, 0)]);
        assert_eq!(rows[2].below, []);
    }

    #[test]
    fn a_merge_opens_a_lane_for_its_second_parent_and_closes_it_where_they_meet() {
        let rows = commit_graph(&[
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ]);
        assert_eq!(rows[0].below, [segment(0, 0, 0), segment(0, 1, 1)]);
        assert_eq!((rows[1].node, rows[1].color), (Lane(1), LaneColor(1)));
        assert_eq!(rows[1].above, [segment(0, 0, 0), segment(1, 1, 1)]);
        assert_eq!(rows[2].node, Lane(0));
        assert_eq!(rows[2].below, [segment(0, 0, 0), segment(1, 1, 1)]);
        assert_eq!(rows[3].above, [segment(0, 0, 0), segment(1, 0, 1)]);
        assert_eq!(rows[3].width, 2);
    }

    #[test]
    fn a_branch_tip_without_children_takes_a_free_lane() {
        let rows = commit_graph(&[commit("x", &["a"]), commit("y", &["a"]), commit("a", &[])]);
        assert_eq!(rows[1].node, Lane(1));
        assert_eq!(rows[1].above, [segment(0, 0, 0)]);
        assert_eq!(rows[2].above, [segment(0, 0, 0), segment(1, 0, 1)]);
    }
}
