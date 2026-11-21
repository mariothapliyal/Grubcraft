use super::*;

// Binary-heap reference for the sector-link search.
fn reach_binary(s: &mut Search, grid: &Grid, sec: u32, from: u32, wanted: &[u32]) {
    s.next_stamp();
    let (x0, y0, x1, y1) = grid.sector_bounds(sec);
    let (w, h) = (x1 - x0, y1 - y0);
    let local = |c: u32| {
        let (x, y) = grid.xy(c);
        ((y - y0) * SECTOR + (x - x0)) as usize
    };
    let mut unsettled = 0;
    for &c in wanted {
        let l = local(c);
        if s.want[l] != s.stamp {
            s.want[l] = s.stamp;
            unsettled += 1;
        }
    }
    s.heap.clear();
    let l0 = local(from);
    s.seen[l0] = s.stamp;
    s.g[l0] = 0;
    s.heap.push(Reverse((0, l0 as u32)));
    let open = |weights: &[u32], x: i32, y: i32| {
        x >= 0 && y >= 0 && x < w && y < h && weights[(y * SECTOR + x) as usize] > 0
    };
    while let Some(Reverse((g, l))) = s.heap.pop() {
        let l = l as usize;
        if g != s.g[l] {
            continue;
        }
        if s.want[l] == s.stamp {
            s.want[l] = 0;
            unsettled -= 1;
            if unsettled == 0 {
                return;
            }
        }
        let (x, y) = (l as i32 % SECTOR, l as i32 / SECTOR);
        let here = s.weights[l];
        for (k, &(dx, dy)) in DIRS.iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if !open(&s.weights, nx, ny) {
                continue;
            }
            let diagonal = k >= 4;
            if diagonal && !(open(&s.weights, x + dx, y) && open(&s.weights, x, y + dy)) {
                continue;
            }
            let nl = (ny * SECTOR + nx) as usize;
            let ng = g + if diagonal { 7 } else { 5 } * (here + s.weights[nl]);
            if (s.seen[nl] != s.stamp || ng < s.g[nl]) && s.heap.len() < s.heap.capacity() {
                s.seen[nl] = s.stamp;
                s.g[nl] = ng;
                s.heap.push(Reverse((ng, nl as u32)));
            }
        }
    }
}

fn binary_costs(grid: &Grid, s: &mut Search, sec: usize) -> Vec<u32> {
    let nodes = grid.nodes_in(sec);
    let n = nodes.len();
    let mut costs = vec![NONE; n * n];
    s.weigh(grid, sec as u32, GRAPH_BERTH);
    for (a, &u) in nodes.iter().enumerate() {
        let cell = grid.node_cell[u as usize];
        let area = grid.area.get(cell as usize);
        let wanted: Vec<_> = nodes[a + 1..]
            .iter()
            .map(|&v| grid.node_cell[v as usize])
            .filter(|&c| grid.area.get(c as usize) == area)
            .collect();
        if wanted.is_empty() {
            continue;
        }
        reach_binary(s, grid, sec as u32, cell, &wanted);
        for (b, &to) in nodes.iter().enumerate().skip(a + 1) {
            let to = grid.node_cell[to as usize];
            if grid.area.get(to as usize) == area
                && let Some(c) = s.cost_to(grid, sec as u32, to)
            {
                costs[a * n + b] = c;
                costs[b * n + a] = c;
            }
        }
    }
    costs
}

#[test]
fn sector_cost_matrices_match_binary_heap() {
    for side in [32, 65, 96] {
        for pattern in 0..5 {
            let ground: Vec<_> = (0..side * side)
                .map(|c| {
                    let (x, y) = (c % side, c / side);
                    match pattern {
                        0 => OPEN,
                        1 => 1 + ((x * 7 + y * 11) % 9) as u8,
                        2 if x % 13 < 2 && y % 17 > 3 => BLOCKED,
                        3 if x == side / 2 => BLOCKED,
                        4 => BLOCKED,
                        _ => OPEN,
                    }
                })
                .collect();
            let grid = Grid::build(&ground, side);
            for capacity in [8, (SECTOR * SECTOR * 8 + 8) as usize] {
                let mut s = Search::for_grid(&grid);
                s.heap = BinaryHeap::with_capacity(capacity);
                let mut old = Search::for_grid(&grid);
                old.heap = BinaryHeap::with_capacity(capacity);
                for sec in 0..(grid.sectors * grid.sectors) as usize {
                    assert_eq!(
                        grid.sector_costs(&mut s, sec),
                        binary_costs(&grid, &mut old, sec),
                        "side {side}, pattern {pattern}, sector {sec}, capacity {capacity}"
                    );
                }
            }
        }
    }
}

#[test]
fn reach_matches_binary_for_empty_and_unreachable_targets() {
    let mut ground = vec![OPEN; (SECTOR * SECTOR) as usize];
    for y in 0..SECTOR {
        ground[(y * SECTOR + SECTOR / 2) as usize] = BLOCKED;
    }
    let grid = Grid::build(&ground, SECTOR);
    let mut s = Search::for_grid(&grid);
    let mut old = Search::for_grid(&grid);
    for wanted in [&[][..], &[0, 0][..], &[(SECTOR - 1) as u32][..]] {
        s.weigh(&grid, 0, GRAPH_BERTH);
        old.weigh(&grid, 0, GRAPH_BERTH);
        s.reach(&grid, 0, 0, wanted);
        reach_binary(&mut old, &grid, 0, 0, wanted);
        assert_eq!(s.g, old.g);
        assert_eq!(s.seen, old.seen);
        assert_eq!(s.want, old.want);
    }
}
