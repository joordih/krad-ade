use super::VaultIndex;

const REPULSION: f32 = 1800.0;
const SPRING: f32 = 0.02;
const SPRING_LENGTH: f32 = 70.0;
const CENTERING: f32 = 0.005;
const DAMPING: f32 = 0.85;
const MAX_SPEED: f32 = 40.0;
const SETTLED_SPEED: f32 = 0.05;
const START_RADIUS: f32 = 12.0;
const GOLDEN_ANGLE: f32 = 2.399_963;
const MIN_DISTANCE: f32 = 1.0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(usize, usize)>,
}

impl NoteGraph {
    #[must_use]
    pub fn of_vault(index: &VaultIndex) -> Self {
        let nodes: Vec<String> = index.notes().iter().map(|note| note.path.clone()).collect();
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (from, note) in index.notes().iter().enumerate() {
            for (link, _) in &note.links {
                let Some(to) = index
                    .resolve(&note.path, link)
                    .and_then(|path| nodes.iter().position(|node| node == path))
                else {
                    continue;
                };
                let edge = (from.min(to), from.max(to));
                if from != to && !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        Self { nodes, edges }
    }

    #[must_use]
    pub fn around(&self, path: &str, hops: usize) -> Self {
        let Some(start) = self.nodes.iter().position(|node| node == path) else {
            return Self::default();
        };
        let mut kept = vec![start];
        let mut frontier = vec![start];
        for _ in 0..hops {
            let mut next = Vec::new();
            for (a, b) in &self.edges {
                for (here, there) in [(*a, *b), (*b, *a)] {
                    if frontier.contains(&here) && !kept.contains(&there) {
                        kept.push(there);
                        next.push(there);
                    }
                }
            }
            frontier = next;
        }
        let edges = self
            .edges
            .iter()
            .filter_map(|(a, b)| {
                let a = kept.iter().position(|node| node == a)?;
                let b = kept.iter().position(|node| node == b)?;
                Some((a, b))
            })
            .collect();
        Self {
            nodes: kept
                .into_iter()
                .map(|node| self.nodes[node].clone())
                .collect(),
            edges,
        }
    }

    #[must_use]
    pub fn degrees(&self) -> Vec<usize> {
        let mut degrees = vec![0; self.nodes.len()];
        for (a, b) in &self.edges {
            degrees[*a] += 1;
            degrees[*b] += 1;
        }
        degrees
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphLayout {
    pub positions: Vec<(f32, f32)>,
    velocities: Vec<(f32, f32)>,
}

impl GraphLayout {
    #[must_use]
    pub fn new(nodes: usize) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let positions = (0..nodes)
            .map(|node| {
                let step = node as f32;
                let radius = START_RADIUS * (step + 1.0).sqrt();
                let angle = step * GOLDEN_ANGLE;
                (radius * angle.cos(), radius * angle.sin())
            })
            .collect();
        Self {
            positions,
            velocities: vec![(0.0, 0.0); nodes],
        }
    }

    pub fn step(&mut self, graph: &NoteGraph) -> bool {
        let count = self.positions.len().min(graph.nodes.len());
        let mut forces = vec![(0.0_f32, 0.0_f32); count];
        for a in 0..count {
            for b in a + 1..count {
                let (dx, dy) = (
                    self.positions[a].0 - self.positions[b].0,
                    self.positions[a].1 - self.positions[b].1,
                );
                let distance_sq = (dx * dx + dy * dy).max(MIN_DISTANCE);
                let distance = distance_sq.sqrt();
                let push = REPULSION / distance_sq;
                let (fx, fy) = (dx / distance * push, dy / distance * push);
                forces[a].0 += fx;
                forces[a].1 += fy;
                forces[b].0 -= fx;
                forces[b].1 -= fy;
            }
        }
        for (a, b) in graph.edges.iter().filter(|(a, b)| *a < count && *b < count) {
            let (dx, dy) = (
                self.positions[*b].0 - self.positions[*a].0,
                self.positions[*b].1 - self.positions[*a].1,
            );
            let distance = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);
            let pull = SPRING * (distance - SPRING_LENGTH);
            let (fx, fy) = (dx / distance * pull, dy / distance * pull);
            forces[*a].0 += fx;
            forces[*a].1 += fy;
            forces[*b].0 -= fx;
            forces[*b].1 -= fy;
        }
        let mut fastest = 0.0_f32;
        for node in 0..count {
            let (x, y) = self.positions[node];
            let force = (
                forces[node].0 - x * CENTERING,
                forces[node].1 - y * CENTERING,
            );
            let velocity = &mut self.velocities[node];
            velocity.0 = (velocity.0 + force.0) * DAMPING;
            velocity.1 = (velocity.1 + force.1) * DAMPING;
            let speed = (velocity.0 * velocity.0 + velocity.1 * velocity.1).sqrt();
            if speed > MAX_SPEED {
                velocity.0 *= MAX_SPEED / speed;
                velocity.1 *= MAX_SPEED / speed;
            }
            fastest = fastest.max(speed.min(MAX_SPEED));
            self.positions[node] = (x + velocity.0, y + velocity.1);
        }
        fastest < SETTLED_SPEED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Note;

    fn graph() -> NoteGraph {
        let index = VaultIndex::new([
            ("a.md".to_owned(), Note::parse("[[b]] [[c]] [[b]]")),
            ("b.md".to_owned(), Note::parse("[[a]] [[d]]")),
            ("c.md".to_owned(), Note::parse("alone but linked")),
            ("d.md".to_owned(), Note::parse("[[e]]")),
            ("e.md".to_owned(), Note::parse("")),
            ("f.md".to_owned(), Note::parse("[[missing]] [[f]]")),
        ]);
        NoteGraph::of_vault(&index)
    }

    #[test]
    fn each_link_between_two_notes_is_one_edge() {
        let graph = graph();
        assert_eq!(graph.edges, [(0, 1), (0, 2), (1, 3), (3, 4)]);
        assert_eq!(graph.degrees(), [2, 2, 1, 2, 1, 0]);
    }

    #[test]
    fn the_local_graph_keeps_the_notes_within_reach() {
        let graph = graph();
        let one = graph.around("a.md", 1);
        assert_eq!(one.nodes, ["a.md", "b.md", "c.md"]);
        let two = graph.around("a.md", 2);
        assert_eq!(two.nodes, ["a.md", "b.md", "c.md", "d.md"]);
        assert_eq!(two.edges.len(), 3);
        assert_eq!(graph.around("missing.md", 2), NoteGraph::default());
    }

    #[test]
    fn the_layout_settles_the_same_way_every_time() {
        let graph = graph();
        let run = || {
            let mut layout = GraphLayout::new(graph.nodes.len());
            let steps = (0..2000).take_while(|_| !layout.step(&graph)).count();
            (steps, layout)
        };
        let (steps, first) = run();
        assert!(steps < 2000, "the layout never settled");
        assert_eq!(run().1, first, "the same vault lays out alike");
        let linked = distance(&first, 0, 1);
        let apart = distance(&first, 0, 5);
        assert!(
            linked < apart,
            "linked notes sit closer: {linked} vs {apart}"
        );
    }

    fn distance(layout: &GraphLayout, a: usize, b: usize) -> f32 {
        let (ax, ay) = layout.positions[a];
        let (bx, by) = layout.positions[b];
        ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
    }
}
