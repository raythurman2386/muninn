//! A deterministic force-directed layout for a vault's link graph.
//!
//! Nodes start on a golden-angle spiral seeded by their vault-relative path,
//! so the same vault lays out the same way every launch (duplicate titles in
//! different folders seed to different angles). Repulsion, link springs,
//! gravity and damping then settle it. Node ids are note ids; positions
//! survive a save or a new note because [`Graph::sync`] keeps them by `rel`,
//! not by title.
//!
//! Vaults up to [`EXHAUSTIVE_LIMIT`] notes use the original all-pairs passes,
//! so their layout is unchanged. Larger vaults bucket nodes into a uniform
//! grid; cells are scanned in a fixed order and repulsion strength is
//! normalised by density, which keeps a 2000-note graph both fast and finite
//! instead of pinning every node at `MAX_SPEED`.

use std::collections::{HashMap, HashSet};

use muninn_core::Vault;

/// The fixed world the layout lives in, in points.
pub const WORLD: [f32; 2] = [1800.0, 1300.0];
pub const NODE_W: f32 = 136.0;
pub const NODE_H: f32 = 38.0;

const REPULSION: f32 = 9000.0;
/// Small vaults keep the original exhaustive 800 cutoff, bit for bit, so the
/// sample vault, its snapshots and the small deterministic tests do not move.
/// Above this many nodes the cutoff tapers with `1/sqrt(n)`: a 2000-note
/// world cannot afford 800 (it is wider than the world, so no pair pruning),
/// and the omitted far wake is below gravity at ordinary displacements while
/// the grid keeps the result deterministic.
const EXHAUSTIVE_LIMIT: usize = 200;
const REPULSION_CUTOFF_SMALL: f32 = 800.0;
const REPULSION_CUTOFF_MIN: f32 = 220.0;
/// A 5×5 cell window covers a cutoff when the cell is half of it.
const REPULSION_CELL_DIVISOR: f32 = 2.0;
/// A node card may grow to 1.18× for a hub (see the UI), which stays inside
/// the separation pads below.
const SEP_PAD_X: f32 = 28.0;
const SEP_PAD_Y: f32 = 15.0;
/// Separation only acts when both spans overlap, so cells are the overlap
/// reach in each axis: a 3×3 window is the whole candidate set.
const SEP_CELL_X: f32 = NODE_W + SEP_PAD_X;
const SEP_CELL_Y: f32 = NODE_H + SEP_PAD_Y;
const SEP_AREA: f32 = SEP_CELL_X * SEP_CELL_Y;
/// Above this fraction of [`WORLD`], full-size cards cannot all fit. The
/// separation envelope shrinks with density so a large vault can settle into
/// a stable packing instead of kicking cards across the world forever; the
/// view draws those densities as dots (see `LOD_ZOOM`).
const SEP_FULL_FILL: f32 = 0.35;
const SPRING: f32 = 0.008;
const LINK_LEN: f32 = 132.0;
const GRAVITY: f32 = 0.0016;
const DAMPING: f32 = 0.86;
const MAX_SPEED: f32 = 26.0;
const EDGE: f32 = 78.0;

#[derive(Clone, Debug)]
pub struct GNode {
    pub note: usize,
    pub title: String,
    /// Vault-relative path, the stable identity for a node across syncs.
    pub rel: String,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    /// Held still while the pointer drags it.
    pub fixed: bool,
}

pub struct Graph {
    pub nodes: Vec<GNode>,
    /// Undirected, deduplicated links, as pairs of note ids.
    pub edges: Vec<(usize, usize)>,
    /// Extra energy while the layout settles.
    pub heat: f32,
}

impl Graph {
    #[must_use]
    pub fn new(vault: &Vault) -> Graph {
        let mut graph = Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
            heat: 1.0,
        };
        graph.sync(vault);
        graph.heat = 1.0;
        graph
    }

    /// Rebuild nodes and links from the vault, keeping the positions of notes that remain.
    /// Positions are keyed by `rel` so duplicate titles keep distinct nodes.
    pub fn sync(&mut self, vault: &Vault) {
        let old: HashMap<String, ([f32; 2], [f32; 2])> = self
            .nodes
            .iter()
            .map(|n| (n.rel.clone(), (n.pos, n.vel)))
            .collect();
        let old_edges = self.edges.clone();
        let old_len = self.nodes.len();
        self.nodes.clear();
        for note in &vault.notes {
            let (pos, vel) = old
                .get(&note.rel)
                .copied()
                .unwrap_or_else(|| (seed(note.id, &note.rel), [0.0; 2]));
            self.nodes.push(GNode {
                note: note.id,
                title: note.title.clone(),
                rel: note.rel.clone(),
                pos,
                vel,
                fixed: false,
            });
        }
        let mut seen = HashSet::new();
        self.edges.clear();
        for note in &vault.notes {
            for &to in &note.links {
                let pair = (note.id.min(to), note.id.max(to));
                if pair.0 != pair.1 && seen.insert(pair) {
                    self.edges.push(pair);
                }
            }
        }
        if old_len != self.nodes.len() || old_edges != self.edges {
            self.heat = 1.0;
        }
    }

    #[must_use]
    pub fn node(&self, note: usize) -> Option<&GNode> {
        // Nodes are built in Vault order, so `nodes[note].note == note`
        // almost always; fall back to a scan if that ever breaks.
        if let Some(found) = self.nodes.get(note)
            && found.note == note
        {
            return Some(found);
        }
        self.nodes.iter().find(|n| n.note == note)
    }

    pub fn node_mut(&mut self, note: usize) -> Option<&mut GNode> {
        if self.nodes.get(note).is_some_and(|n| n.note == note) {
            return Some(&mut self.nodes[note]);
        }
        self.nodes.iter_mut().find(|n| n.note == note)
    }

    /// Push the layout one fixed step; true only when a node (or separation)
    /// actually moved this step.
    ///
    /// `heat` decays by a fixed amount each call as a “how long since it was
    /// reheated” timer, but heat alone is **not** visible state: pollers must
    /// not report “changed” just because heat has not reached zero yet.
    /// Callers that need the cooling timer read [`Graph::heat`] directly.
    pub fn step(&mut self) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let before: Vec<[f32; 2]> = self.nodes.iter().map(|n| n.pos).collect();
        let mut force = repulsion_force(&self.nodes);
        for &(a, b) in &self.edges {
            let dx = self.nodes[b].pos[0] - self.nodes[a].pos[0];
            let dy = self.nodes[b].pos[1] - self.nodes[a].pos[1];
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = (d - LINK_LEN) * SPRING;
            let ux = dx / d;
            let uy = dy / d;
            force[a][0] += ux * f;
            force[a][1] += uy * f;
            force[b][0] -= ux * f;
            force[b][1] -= uy * f;
        }
        let c = crate::camera::world_centre();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if node.fixed {
                node.vel = [0.0; 2];
                continue;
            }
            let gx = (c[0] - node.pos[0]) * GRAVITY;
            let gy = (c[1] - node.pos[1]) * GRAVITY;
            node.vel[0] = (node.vel[0] + force[i][0] + gx) * DAMPING;
            node.vel[1] = (node.vel[1] + force[i][1] + gy) * DAMPING;
            let speed = (node.vel[0] * node.vel[0] + node.vel[1] * node.vel[1]).sqrt();
            if speed > MAX_SPEED {
                node.vel[0] *= MAX_SPEED / speed;
                node.vel[1] *= MAX_SPEED / speed;
            }
            node.pos[0] = (node.pos[0] + node.vel[0]).clamp(EDGE, WORLD[0] - EDGE);
            node.pos[1] = (node.pos[1] + node.vel[1]).clamp(EDGE, WORLD[1] - EDGE);
        }
        separate(&mut self.nodes);
        self.heat = (self.heat - 0.012).max(0.0);
        // Only visible motion counts: sub-pixel creep from the spring vs.
        // separation tug-of-war or damped velocity tails is invisible and
        // must not keep waking the window (Rune: "returned true but nothing
        // changed"). The threshold matches the old speed gate (0.04 pt/step)
        // so mid-settling motion still reports while the tail stays quiet.
        self.nodes.iter().zip(&before).any(|(node, old)| {
            (node.pos[0] - old[0]).abs() > 0.04 || (node.pos[1] - old[1]).abs() > 0.04
        })
    }

    #[must_use]
    pub fn bounds(&self, visible: &[usize]) -> Option<[f32; 4]> {
        let mut b: Option<[f32; 4]> = None;
        for &id in visible {
            let Some(node) = self.node(id) else { continue };
            let (x, y) = (node.pos[0], node.pos[1]);
            b = Some(match b {
                None => [
                    x - NODE_W / 2.0,
                    y - NODE_H / 2.0,
                    x + NODE_W / 2.0,
                    y + NODE_H / 2.0,
                ],
                Some([x0, y0, x1, y1]) => [
                    x0.min(x - NODE_W / 2.0),
                    y0.min(y - NODE_H / 2.0),
                    x1.max(x + NODE_W / 2.0),
                    y1.max(y + NODE_H / 2.0),
                ],
            });
        }
        b
    }

    /// Links with both ends visible, in a stable order.
    #[must_use]
    pub fn visible_edges(&self, visible: &[usize]) -> Vec<(usize, usize)> {
        // Global scope passes every note: every edge is visible, so skip the
        // set allocation that per-frame view building would otherwise pay.
        if visible.len() == self.nodes.len()
            && visible
                .iter()
                .all(|&id| self.nodes.get(id).is_some_and(|n| n.note == id))
        {
            return self.edges.clone();
        }
        let set: HashSet<usize> = visible.iter().copied().collect();
        self.edges
            .iter()
            .copied()
            .filter(|&(a, b)| set.contains(&a) && set.contains(&b))
            .collect()
    }

    /// Let the layout settle again, e.g. after a drag or a scope change.
    pub fn reheat(&mut self) {
        self.heat = 1.0;
    }

    /// Put every node back on its starting spiral and settle from there.
    pub fn reset(&mut self) {
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.pos = seed(i, &node.rel);
            node.vel = [0.0; 2];
            node.fixed = false;
        }
        self.heat = 1.0;
    }
}

/// A uniform bucket grid over [`WORLD`]. Cells hold node indices in node
/// order, so a scan is deterministic: no hash iteration enters the result.
struct Grid {
    cell: [f32; 2],
    cols: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
}

impl Grid {
    fn build(nodes: &[GNode], cell: [f32; 2]) -> Grid {
        let cols = (WORLD[0] / cell[0]).ceil().max(1.0) as usize;
        let rows = (WORLD[1] / cell[1]).ceil().max(1.0) as usize;
        let mut grid = Grid {
            cell,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
        };
        for (index, node) in nodes.iter().enumerate() {
            let (x, y) = grid.coord(node.pos);
            grid.cells[y * cols + x].push(index);
        }
        grid
    }

    fn coord(&self, pos: [f32; 2]) -> (usize, usize) {
        let x = ((pos[0] / self.cell[0]).floor() as isize).clamp(0, self.cols as isize - 1);
        let y = ((pos[1] / self.cell[1]).floor() as isize).clamp(0, self.rows as isize - 1);
        (x as usize, y as usize)
    }
}

fn repulsion_cutoff(n: usize) -> f32 {
    if n <= EXHAUSTIVE_LIMIT {
        return REPULSION_CUTOFF_SMALL;
    }
    (REPULSION_CUTOFF_SMALL * (EXHAUSTIVE_LIMIT as f32 / n as f32).sqrt()).max(REPULSION_CUTOFF_MIN)
}

/// Repulsive pressure is summed over all neighbours; a fixed constant would
/// grow with density and overpower springs and gravity. Keeping the total
/// roughly constant is what lets a large vault settle instead of pinning
/// every node at `MAX_SPEED`.
fn repulsion_strength(n: usize) -> f32 {
    REPULSION * (EXHAUSTIVE_LIMIT as f32 / n.max(EXHAUSTIVE_LIMIT) as f32)
}

fn add_repulsion(force: &mut [[f32; 2]], nodes: &[GNode], i: usize, j: usize, strength: f32) {
    let dx = nodes[j].pos[0] - nodes[i].pos[0];
    let dy = nodes[j].pos[1] - nodes[i].pos[1];
    let d2 = (dx * dx + dy * dy).max(36.0);
    let d = d2.sqrt();
    let f = strength / d2;
    let ux = dx / d;
    let uy = dy / d;
    force[i][0] -= ux * f;
    force[i][1] -= uy * f;
    force[j][0] += ux * f;
    force[j][1] += uy * f;
}

/// The original all-pairs pass. Small vaults use it so their settled
/// positions (and snapshots) stay byte-identical; it is also the reference
/// the grid path is tested against.
fn pairwise_repulsion(nodes: &[GNode]) -> Vec<[f32; 2]> {
    let mut force = vec![[0.0f32; 2]; nodes.len()];
    let cutoff_sq = repulsion_cutoff(nodes.len()).powi(2);
    let strength = repulsion_strength(nodes.len());
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            let dx = nodes[j].pos[0] - nodes[i].pos[0];
            let dy = nodes[j].pos[1] - nodes[i].pos[1];
            if dx * dx + dy * dy <= cutoff_sq {
                add_repulsion(&mut force, nodes, i, j, strength);
            }
        }
    }
    force
}

/// Pairwise repulsion over a uniform grid. Cells hold node indices in node
/// order and are visited in a fixed order, so the result is deterministic;
/// the summation order differs from [`pairwise_repulsion`] only, within
/// float rounding.
fn repulsion_force(nodes: &[GNode]) -> Vec<[f32; 2]> {
    if nodes.len() <= EXHAUSTIVE_LIMIT {
        return pairwise_repulsion(nodes);
    }
    let mut force = vec![[0.0f32; 2]; nodes.len()];
    let cutoff = repulsion_cutoff(nodes.len());
    let cutoff_sq = cutoff * cutoff;
    let strength = repulsion_strength(nodes.len());
    let cell = cutoff / REPULSION_CELL_DIVISOR;
    let grid = Grid::build(nodes, [cell; 2]);
    let reach = REPULSION_CELL_DIVISOR.ceil() as isize;
    let mut neighbours = Vec::new();
    for i in 0..nodes.len() {
        let (cx, cy) = grid.coord(nodes[i].pos);
        neighbours.clear();
        for gy in cy as isize - reach..=cy as isize + reach {
            if gy < 0 || gy >= grid.rows as isize {
                continue;
            }
            for gx in cx as isize - reach..=cx as isize + reach {
                if gx < 0 || gx >= grid.cols as isize {
                    continue;
                }
                for &j in &grid.cells[gy as usize * grid.cols + gx as usize] {
                    if j <= i {
                        continue;
                    }
                    let dx = nodes[j].pos[0] - nodes[i].pos[0];
                    let dy = nodes[j].pos[1] - nodes[i].pos[1];
                    if dx * dx + dy * dy <= cutoff_sq {
                        neighbours.push(j);
                    }
                }
            }
        }
        for &j in &neighbours {
            add_repulsion(&mut force, nodes, i, j, strength);
        }
    }
    force
}

/// Keep the wide node cards out of each other's labels. Two passes of
/// rectangle separation per step, along the axis with the smaller overlap.
///
/// The grid holds positions from the start of a pass. A pair that only moves
/// into overlap during that pass is caught by the next pass; the old
/// exhaustive scan saw it in the same pass, so this is a documented, bounded
/// approximation rather than an exact reordering.
fn separate(nodes: &mut [GNode]) -> bool {
    let extent = separation_extent(nodes.len());
    if nodes.len() <= EXHAUSTIVE_LIMIT {
        return separate_exhaustive(nodes, extent);
    }
    let mut moved = false;
    let mut candidates = Vec::new();
    for _ in 0..2 {
        if !separate_grid_pass(nodes, extent, &mut candidates) {
            break;
        }
        moved = true;
    }
    moved
}

fn separate_grid_pass(nodes: &mut [GNode], extent: [f32; 2], candidates: &mut Vec<usize>) -> bool {
    let grid = Grid::build(nodes, extent);
    let mut moved = false;
    for i in 0..nodes.len() {
        candidates.clear();
        collect_overlaps(nodes, i, &grid, extent, candidates);
        candidates.sort_unstable();
        for &j in candidates.iter() {
            moved |= separate_pair(nodes, i, j, extent);
        }
    }
    moved
}

fn collect_overlaps(
    nodes: &[GNode],
    i: usize,
    grid: &Grid,
    extent: [f32; 2],
    out: &mut Vec<usize>,
) {
    let (cx, cy) = grid.coord(nodes[i].pos);
    let y0 = cy.saturating_sub(1);
    let y1 = (cy + 1).min(grid.rows - 1);
    let x0 = cx.saturating_sub(1);
    let x1 = (cx + 1).min(grid.cols - 1);
    for gy in y0..=y1 {
        for gx in x0..=x1 {
            for &j in &grid.cells[gy * grid.cols + gx] {
                if j <= i {
                    continue;
                }
                let dx = nodes[j].pos[0] - nodes[i].pos[0];
                let dy = nodes[j].pos[1] - nodes[i].pos[1];
                if dx.abs() < extent[0] && dy.abs() < extent[1] {
                    out.push(j);
                }
            }
        }
    }
}

/// The original all-pairs separation pass, kept for small vaults so their
/// layout is unchanged.
fn separate_exhaustive(nodes: &mut [GNode], extent: [f32; 2]) -> bool {
    let mut moved = false;
    for _ in 0..2 {
        let mut pass_moved = false;
        for i in 0..nodes.len() {
            for j in i + 1..nodes.len() {
                if separate_pair(nodes, i, j, extent) {
                    moved = true;
                    pass_moved = true;
                }
            }
        }
        if !pass_moved {
            break;
        }
    }
    moved
}

/// Full card packing while it fits; below that, shrink both axes so the
/// available world area per node is respected.
fn separation_extent(n: usize) -> [f32; 2] {
    if n == 0 {
        return [SEP_CELL_X, SEP_CELL_Y];
    }
    let fill = n as f32 * SEP_AREA / (WORLD[0] * WORLD[1]);
    let scale = if fill <= SEP_FULL_FILL {
        1.0
    } else {
        (SEP_FULL_FILL / fill).sqrt()
    };
    [SEP_CELL_X * scale, SEP_CELL_Y * scale]
}

/// Separate one pair, if it overlaps. Returns true when a position changed.
fn separate_pair(nodes: &mut [GNode], i: usize, j: usize, extent: [f32; 2]) -> bool {
    let dx = nodes[j].pos[0] - nodes[i].pos[0];
    let dy = nodes[j].pos[1] - nodes[i].pos[1];
    let ox = extent[0] - dx.abs();
    let oy = extent[1] - dy.abs();
    if ox <= 0.0 || oy <= 0.0 {
        return false;
    }
    let (share_i, share_j) = match (nodes[i].fixed, nodes[j].fixed) {
        (true, true) => return false,
        (true, false) => (0.0, 1.0),
        (false, true) => (1.0, 0.0),
        (false, false) => (0.5, 0.5),
    };
    // `u` points from j to i along the axis of smaller overlap.
    let (ux, uy, amount) = if ox < oy {
        (if dx >= 0.0 { -1.0 } else { 1.0 }, 0.0, ox)
    } else {
        (0.0, if dy >= 0.0 { -1.0 } else { 1.0 }, oy)
    };
    nodes[i].pos[0] = (nodes[i].pos[0] + ux * amount * share_i).clamp(EDGE, WORLD[0] - EDGE);
    nodes[i].pos[1] = (nodes[i].pos[1] + uy * amount * share_i).clamp(EDGE, WORLD[1] - EDGE);
    nodes[j].pos[0] = (nodes[j].pos[0] - ux * amount * share_j).clamp(EDGE, WORLD[0] - EDGE);
    nodes[j].pos[1] = (nodes[j].pos[1] - uy * amount * share_j).clamp(EDGE, WORLD[1] - EDGE);
    true
}

fn seed(index: usize, rel: &str) -> [f32; 2] {
    let c = crate::camera::world_centre();
    let angle = index as f32 * 2.399_963_2 + (hash(rel) % 1000) as f32 / 1000.0 * 0.9;
    let radius = 74.0 + 27.0 * (index as f32).sqrt();
    [
        c[0] + angle.cos() * radius * 1.16,
        c[1] + angle.sin() * radius * 0.86,
    ]
}

fn hash(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::testing::TempVault;

    fn vault() -> TempVault {
        TempVault::new(&[
            ("A.md", "[[B]] [[C]]"),
            ("B.md", "[[A]]"),
            ("C.md", "[[B]] [[D]]"),
            ("D.md", "[[C]] [[A]]"),
        ])
    }

    #[test]
    fn edges_are_undirected_and_deduplicated() {
        let tmp = vault();
        let v = tmp.load();
        let g = Graph::new(&v);
        // A-B, A-C, B-C, C-D, A-D are all one pair each; reciprocal links collapse.
        assert_eq!(g.edges.len(), 5, "{:?}", g.edges);
        assert_eq!(g.nodes.len(), 4);
    }

    #[test]
    fn layout_is_deterministic() {
        let tmp = vault();
        let v = tmp.load();
        let mut one = Graph::new(&v);
        let mut two = Graph::new(&v);
        for _ in 0..120 {
            one.step();
            two.step();
        }
        for (a, b) in one.nodes.iter().zip(&two.nodes) {
            assert_eq!(a.title, b.title);
            assert!(
                (a.pos[0] - b.pos[0]).abs() < 1e-4,
                "{:?} {:?}",
                a.pos,
                b.pos
            );
            assert!((a.pos[1] - b.pos[1]).abs() < 1e-4);
        }
    }

    #[test]
    fn layout_finishes_and_stays_in_the_world() {
        let tmp = vault();
        let v = tmp.load();
        let mut g = Graph::new(&v);
        for _ in 0..600 {
            g.step();
        }
        assert!(!g.step() || g.heat == 0.0);
        for node in &g.nodes {
            assert!(node.pos[0] >= EDGE && node.pos[0] <= WORLD[0] - EDGE);
            assert!(node.pos[1] >= EDGE && node.pos[1] <= WORLD[1] - EDGE);
        }
    }

    #[test]
    fn bounds_and_visible_edges_follow_the_selection() {
        let tmp = vault();
        let v = tmp.load();
        let g = Graph::new(&v);
        let a = v.resolve("A").unwrap();
        let b = v.resolve("B").unwrap();
        let c = v.resolve("C").unwrap();
        let bounds = g.bounds(&[a, b]).unwrap();
        assert!(bounds[2] > bounds[0]);
        let edges = g.visible_edges(&[a, b, c]);
        assert!(edges.contains(&(a.min(b), a.max(b))));
        assert_eq!(g.visible_edges(&[a]), Vec::new());
    }
}

#[cfg(test)]
mod grid {
    use super::*;
    use muninn_core::testing::TempVault;

    /// `n` notes mixed from 25-note star groups and a chain: every note links
    /// to its group hub and the previous note, giving realistic high-degree
    /// nodes without a single 80-link hub.
    fn big_vault(n: usize) -> TempVault {
        let mut files = Vec::with_capacity(n);
        for i in 0..n {
            let mut body = String::new();
            if i > 0 {
                body.push_str(&format!("[[Note {:04}]]\n", i - 1));
            }
            let hub = i - i % 25;
            if hub != i {
                body.push_str(&format!("[[Note {hub:04}]]\n"));
            }
            files.push((format!("Note {i:04}.md"), format!("# Note {i:04}\n{body}")));
        }
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, body)| (name.as_str(), body.as_str()))
            .collect();
        TempVault::new(&refs)
    }

    #[test]
    fn grid_repulsion_matches_the_exhaustive_pass() {
        // Above EXHAUSTIVE_LIMIT, so `repulsion_force` takes the grid path.
        let tmp = big_vault(260);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        // Spread the spiral into several grid cells before comparing.
        for _ in 0..40 {
            g.step();
        }
        let grid = repulsion_force(&g.nodes);
        let exhaustive = pairwise_repulsion(&g.nodes);
        for (i, (a, b)) in grid.iter().zip(&exhaustive).enumerate() {
            // Same candidate set and forces; only the float summation order
            // differs, so the tolerance is rounding, not approximation.
            assert!((a[0] - b[0]).abs() < 1e-3, "node {i}: {a:?} vs {b:?}");
            assert!((a[1] - b[1]).abs() < 1e-3, "node {i}: {a:?} vs {b:?}");
        }
    }
}
