use super::super::board::{Cell, Dice, Heading, CELL_COUNT};

pub(super) const MAZE_W: i32 = 47;
pub(super) const MAZE_H: i32 = 11;
pub(super) const START: Cell = Cell::new(1, MAZE_H - 2);

pub(super) const fn inside(cell: Cell) -> bool {
    cell.x >= 1 && cell.y >= 1 && cell.x < MAZE_W - 1 && cell.y < MAZE_H - 1
}

pub(super) fn carve(dice: &mut Dice) -> Vec<bool> {
    let mut walls = vec![true; CELL_COUNT];
    walls[START.index()] = false;
    let mut stack = vec![START];
    while let Some(&at) = stack.last() {
        let options: Vec<Heading> = Heading::ALL
            .into_iter()
            .filter(|heading| {
                let two = at.toward(*heading).toward(*heading);
                inside(two) && walls[two.index()]
            })
            .collect();
        let Some(heading) = options.get(dice.below(options.len())).copied() else {
            stack.pop();
            continue;
        };
        let middle = at.toward(heading);
        let two = middle.toward(heading);
        walls[middle.index()] = false;
        walls[two.index()] = false;
        stack.push(two);
    }
    braid(&mut walls, dice);
    walls
}

fn braid(walls: &mut [bool], dice: &mut Dice) {
    let rooms = (1..MAZE_H - 1)
        .step_by(2)
        .flat_map(|y| (1..MAZE_W - 1).step_by(2).map(move |x| Cell::new(x, y)));
    for room in rooms {
        let open = Heading::ALL
            .iter()
            .filter(|heading| !walls[room.toward(**heading).index()])
            .count();
        if open > 1 {
            continue;
        }
        let closed: Vec<Heading> = Heading::ALL
            .into_iter()
            .filter(|heading| {
                let wall = room.toward(*heading);
                inside(wall) && walls[wall.index()] && inside(wall.toward(*heading))
            })
            .collect();
        if let Some(heading) = closed.get(dice.below(closed.len())) {
            walls[room.toward(*heading).index()] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn open(walls: &[bool], cell: Cell) -> bool {
        inside(cell) && !walls[cell.index()]
    }

    fn reachable(walls: &[bool]) -> usize {
        let mut seen = vec![false; CELL_COUNT];
        let mut queue = VecDeque::from([START]);
        seen[START.index()] = true;
        while let Some(at) = queue.pop_front() {
            for next in Heading::ALL.map(|heading| at.toward(heading)) {
                if open(walls, next) && !seen[next.index()] {
                    seen[next.index()] = true;
                    queue.push_back(next);
                }
            }
        }
        seen.iter().filter(|seen| **seen).count()
    }

    #[test]
    fn every_open_cell_of_a_new_maze_is_reachable_and_none_is_a_dead_end() {
        for seed in 1..20 {
            let walls = carve(&mut Dice::seeded(seed));
            let open_cells: Vec<Cell> = (0..MAZE_H)
                .flat_map(|y| (0..MAZE_W).map(move |x| Cell::new(x, y)))
                .filter(|cell| open(&walls, *cell))
                .collect();
            assert_eq!(reachable(&walls), open_cells.len(), "seed {seed}");
            let dead_ends = open_cells
                .iter()
                .filter(|cell| {
                    Heading::ALL
                        .iter()
                        .filter(|heading| open(&walls, cell.toward(**heading)))
                        .count()
                        < 2
                })
                .count();
            assert_eq!(dead_ends, 0, "seed {seed}");
        }
    }
}
