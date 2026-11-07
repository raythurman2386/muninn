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
//!
//! Dense vaults also seed inside the world instead of clamping a spiral that is
//! wider than the world against the walls, and they settle under a heat-aware
//! dormancy rule: once heat has decayed to zero and no node still moves faster
//! than `0.4 pt/step`, the simulation stops (`Graph::step` returns false) with a
//! bounded, sub-pixel residual. [`Graph::reheat`] wakes it again. Small vaults
//! never take these branches, so their settled positions stay bit-identical.

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
/// Velocity retained per step. Small vaults keep the original constant so their
/// settled positions stay bit-identical; dense vaults are damped a little more
/// so the spring/repulsion/separation tail loses energy instead of orbiting.
const DAMPING: f32 = 0.86;
const DAMPING_DENSE: f32 = 0.80;
const MAX_SPEED: f32 = 26.0;
/// Dense vaults are not allowed to fling a node across the world in one step;
/// the lower cap keeps a pile at a wall from oscillating at the full speed.
const MAX_SPEED_DENSE: f32 = 18.0;
/// Below this speed (squared) a cooled dense layout is treated as dormant: the
/// remaining creep is sub-pixel and invisible, so the simulation stops instead
/// of waking the window forever. `0.4 pt/step` is bounded residual motion.
const DORMANT_SPEED_SQ: f32 = 0.16;
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

#[derive(Clone, Copy)]
struct StepParams {
    centre: [f32; 2],
    damping: f32,
    max_speed: f32,
    /// Dense vaults zero a velocity component that would leave the world; the
    /// small-vault path keeps the original clamp-only behaviour.
    wall: bool,
}

/// Add link-spring forces to `force` (one pass over the deduplicated edges).
fn add_springs(force: &mut [[f32; 2]], nodes: &[GNode], edges: &[(usize, usize)]) {
    for &(a, b) in edges {
        pair_force(force, nodes, a, b, PairForce::Spring);
    }
}

/// Integrate one node: gravity, damping, speed cap, wall absorption (dense
/// only) and the world clamp.
fn integrate(node: &mut GNode, force: [f32; 2], p: StepParams) {
    if node.fixed {
        node.vel = [0.0; 2];
        return;
    }
    let gx = (p.centre[0] - node.pos[0]) * GRAVITY;
    let gy = (p.centre[1] - node.pos[1]) * GRAVITY;
    node.vel[0] = (node.vel[0] + force[0] + gx) * p.damping;
    node.vel[1] = (node.vel[1] + force[1] + gy) * p.damping;
    let speed = (node.vel[0] * node.vel[0] + node.vel[1] * node.vel[1]).sqrt();
    if speed > p.max_speed {
        node.vel[0] *= p.max_speed / speed;
        node.vel[1] *= p.max_speed / speed;
    }
    let max_x = WORLD[0] - EDGE;
    let max_y = WORLD[1] - EDGE;
    if p.wall {
        // A wall is not a spring: without this the clamped position is static
        // while velocity keeps pointing out of the world, which pins a pile at
        // the edge at `MAX_SPEED` forever.
        if node.pos[0] + node.vel[0] <= EDGE || node.pos[0] + node.vel[0] >= max_x {
            node.vel[0] = 0.0;
        }
        if node.pos[1] + node.vel[1] <= EDGE || node.pos[1] + node.vel[1] >= max_y {
            node.vel[1] = 0.0;
        }
    }
    node.pos[0] = (node.pos[0] + node.vel[0]).clamp(EDGE, max_x);
    node.pos[1] = (node.pos[1] + node.vel[1]).clamp(EDGE, max_y);
}

/// The visible-motion gate: did any node move more than 0.04 pt in an axis?
fn moved_since(nodes: &[GNode], before: &[[f32; 2]]) -> bool {
    nodes.iter().zip(before).any(|(node, old)| {
        (node.pos[0] - old[0]).abs() > 0.04 || (node.pos[1] - old[1]).abs() > 0.04
    })
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
                .unwrap_or_else(|| (seed(note.id, &note.rel, vault.notes.len()), [0.0; 2]));
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
    ///
    /// Above [`EXHAUSTIVE_LIMIT`] nodes a cooled layout goes dormant when the
    /// fastest node is below `0.4 pt/step`; from then on this returns false
    /// without integrating (positions are frozen). [`Graph::reheat`] resumes
    /// it, as does a sync that adds/removes nodes or links.
    pub fn step(&mut self) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        if self.dormant() {
            for node in &mut self.nodes {
                node.vel = [0.0; 2];
            }
            return false;
        }
        let dense = self.nodes.len() > EXHAUSTIVE_LIMIT;
        let params = StepParams {
            centre: crate::camera::world_centre(),
            damping: if dense { DAMPING_DENSE } else { DAMPING },
            max_speed: if dense { MAX_SPEED_DENSE } else { MAX_SPEED },
            wall: dense,
        };
        let before: Vec<[f32; 2]> = self.nodes.iter().map(|n| n.pos).collect();
        let mut force = repulsion_force(&self.nodes);
        add_springs(&mut force, &self.nodes, &self.edges);
        for (i, node) in self.nodes.iter_mut().enumerate() {
            integrate(node, force[i], params);
        }
        separate(&mut self.nodes);
        if dense {
            // Position-based damping for dense vaults: separation moves positions
            // after the force pass, so a node at a wall otherwise re-applies its
            // pre-separation outward velocity every step while separation pushes
            // it back — a limit cycle that never falls under the motion gate.
            // Deriving velocity from the actual displacement lets the correction
            // absorb it; the free-flight case is unchanged (displacement is the
            // velocity the force pass just wrote).
            for (node, old) in self.nodes.iter_mut().zip(&before) {
                if !node.fixed {
                    node.vel = [node.pos[0] - old[0], node.pos[1] - old[1]];
                }
            }
        }
        self.heat = (self.heat - 0.012).max(0.0);
        // Only visible motion counts: sub-pixel creep from the spring vs.
        // separation tug-of-war or damped velocity tails is invisible and
        // must not keep waking the window (Rune: "returned true but nothing
        // changed"). The threshold matches the old speed gate (0.04 pt/step)
        // so mid-settling motion still reports while the tail stays quiet.
        moved_since(&self.nodes, &before)
    }

    /// A cooled dense layout whose fastest node is below the dormant threshold:
    /// integration stops and the residual creep (bounded, sub-pixel) is frozen.
    fn dormant(&self) -> bool {
        self.nodes.len() > EXHAUSTIVE_LIMIT
            && self.heat == 0.0
            && self.max_speed_sq() <= DORMANT_SPEED_SQ
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

    /// Largest per-node speed, squared. Used by the dense-vault dormancy gate.
    fn max_speed_sq(&self) -> f32 {
        self.nodes
            .iter()
            .map(|n| n.vel[0] * n.vel[0] + n.vel[1] * n.vel[1])
            .fold(0.0, f32::max)
    }

    /// Put every node back on its starting spiral and settle from there.
    pub fn reset(&mut self) {
        let count = self.nodes.len();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.pos = seed(i, &node.rel, count);
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
    pair_force(force, nodes, i, j, PairForce::Repel(strength));
}

/// The two force kernels share their delta/unit-vector arithmetic; keeping one
/// function makes the float operations identical for the exhaustive path while
/// avoiding a near-clone.
#[derive(Clone, Copy)]
enum PairForce {
    Spring,
    /// Inverse-square repulsion with the original 36 pt² floor.
    Repel(f32),
}

/// Apply one pair force from `i` to `j`; a negative magnitude pushes apart.
fn pair_force(force: &mut [[f32; 2]], nodes: &[GNode], i: usize, j: usize, kind: PairForce) {
    let dx = nodes[j].pos[0] - nodes[i].pos[0];
    let dy = nodes[j].pos[1] - nodes[i].pos[1];
    let raw = dx * dx + dy * dy;
    let (d, f) = match kind {
        PairForce::Spring => {
            let d = raw.sqrt().max(0.01);
            (d, (d - LINK_LEN) * SPRING)
        }
        PairForce::Repel(strength) => {
            let d2 = raw.max(36.0);
            (d2.sqrt(), -strength / d2)
        }
    };
    push_pair(force, i, j, dx / d, dy / d, f);
}

/// Apply one equal-and-opposite pair force along the unit vector `(ux, uy)`:
/// `i` is pushed by `+f`, `j` by `-f`.
fn push_pair(force: &mut [[f32; 2]], i: usize, j: usize, ux: f32, uy: f32, f: f32) {
    force[i][0] += ux * f;
    force[i][1] += uy * f;
    force[j][0] -= ux * f;
    force[j][1] -= uy * f;
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
        return separate_exhaustive(nodes, extent, 0.0);
    }
    // Dense cards are drawn as dots, so a sub-visible overlap is not worth a
    // position change. Chasing it is what keeps the tail above the 0.04 pt
    // motion gate forever; the threshold is below that gate for one pair.
    let min_overlap = 0.04;
    let mut moved = false;
    let mut candidates = Vec::new();
    for _ in 0..6 {
        if !separate_grid_pass(nodes, extent, min_overlap, &mut candidates) {
            break;
        }
        moved = true;
    }
    moved
}

fn separate_grid_pass(
    nodes: &mut [GNode],
    extent: [f32; 2],
    min_overlap: f32,
    candidates: &mut Vec<usize>,
) -> bool {
    let grid = Grid::build(nodes, extent);
    let mut moved = false;
    for i in 0..nodes.len() {
        candidates.clear();
        collect_overlaps(nodes, i, &grid, extent, min_overlap, candidates);
        candidates.sort_unstable();
        for &j in candidates.iter() {
            moved |= separate_pair(nodes, i, j, extent, min_overlap);
        }
    }
    moved
}

fn collect_overlaps(
    nodes: &[GNode],
    i: usize,
    grid: &Grid,
    extent: [f32; 2],
    min_overlap: f32,
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
                if dx.abs() < extent[0] - min_overlap && dy.abs() < extent[1] - min_overlap {
                    out.push(j);
                }
            }
        }
    }
}

/// The original all-pairs separation pass, kept for small vaults so their
/// layout is unchanged.
fn separate_exhaustive(nodes: &mut [GNode], extent: [f32; 2], min_overlap: f32) -> bool {
    let mut moved = false;
    for _ in 0..2 {
        let mut pass_moved = false;
        for i in 0..nodes.len() {
            for j in i + 1..nodes.len() {
                if separate_pair(nodes, i, j, extent, min_overlap) {
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
fn separate_pair(
    nodes: &mut [GNode],
    i: usize,
    j: usize,
    extent: [f32; 2],
    min_overlap: f32,
) -> bool {
    let dx = nodes[j].pos[0] - nodes[i].pos[0];
    let dy = nodes[j].pos[1] - nodes[i].pos[1];
    let ox = extent[0] - dx.abs();
    let oy = extent[1] - dy.abs();
    if ox <= min_overlap || oy <= min_overlap {
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

fn seed(index: usize, rel: &str, count: usize) -> [f32; 2] {
    let c = crate::camera::world_centre();
    let angle = index as f32 * 2.399_963_2 + (hash(rel) % 1000) as f32 / 1000.0 * 0.9;
    let radius = (74.0 + 27.0 * (index as f32).sqrt()) * seed_scale(count);
    [
        c[0] + angle.cos() * radius * 1.16,
        c[1] + angle.sin() * radius * 0.86,
    ]
}

/// Keep the starting spiral inside the world once it is wider than the world.
///
/// A 2000-note spiral reaches ~1100 pt below the centre, so without this most
/// nodes start clamped against the top and bottom walls; the pile's own
/// repulsion then holds them there and the layout never settles. The scale is
/// exactly `1.0` up to [`EXHAUSTIVE_LIMIT`], so small vaults (the sample and the
/// snapshot tests) keep their original seeds.
fn seed_scale(count: usize) -> f32 {
    if count <= EXHAUSTIVE_LIMIT {
        return 1.0;
    }
    let max_radius = 74.0 + 27.0 * ((count - 1) as f32).sqrt();
    let available = ((WORLD[0] * 0.5 - EDGE) / 1.16).min((WORLD[1] * 0.5 - EDGE) / 0.86);
    (available / max_radius).clamp(0.15, 1.0)
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
mod dense {
    use super::*;
    use muninn_core::testing::big_vault;

    #[test]
    fn dense_layout_goes_dormant_with_bounded_residual() {
        // Above EXHAUSTIVE_LIMIT, so the dense settling rules apply.
        let tmp = big_vault(260);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        let mut quiet = 0;
        let mut last_delta = 0.0f32;
        let mut dormant_at = None;
        for frame in 1..=4000 {
            let before: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
            let moved = g.step();
            let delta = g
                .nodes
                .iter()
                .zip(&before)
                .fold(0.0f32, |max, (node, old)| {
                    max.max((node.pos[0] - old[0]).abs())
                        .max((node.pos[1] - old[1]).abs())
                });
            if moved {
                quiet = 0;
                last_delta = delta;
            } else {
                quiet += 1;
                if quiet == 30 {
                    dormant_at = Some(frame);
                    break;
                }
            }
        }
        let frame = dormant_at.expect("dense layout must quiesce within 4000 frames");
        assert!(
            last_delta < 0.5,
            "residual motion at dormancy must be sub-pixel, got {last_delta} pt at frame {frame}"
        );
        // Heat-aware dormancy stops integration; reheat resumes it.
        assert_eq!(g.heat, 0.0);
        g.reheat();
        let _ = g.step();
        assert!(g.heat > 0.0 && g.heat < 1.0, "reheat must resume the step");
    }

    #[test]
    fn dense_seeds_stay_inside_the_world() {
        // The old spiral reached ~1100 pt below centre at 2000 notes and wedged
        // most nodes against a wall. Scaling only above EXHAUSTIVE_LIMIT keeps
        // the small-vault sample exact.
        for count in [201usize, 500, 2000] {
            for index in [0usize, count / 2, count - 1] {
                let p = seed(index, "folder/note", count);
                assert!(p[0] > EDGE && p[0] < WORLD[0] - EDGE, "{count}: {p:?}");
                assert!(p[1] > EDGE && p[1] < WORLD[1] - EDGE, "{count}: {p:?}");
            }
        }
        // At and below the exhaustive limit the scale is exactly 1.0.
        assert_eq!(seed_scale(EXHAUSTIVE_LIMIT), 1.0);
        assert_eq!(seed(5, "x", EXHAUSTIVE_LIMIT), seed(5, "x", 2));
    }
}

#[cfg(test)]
mod grid {
    use super::*;
    use muninn_core::testing::big_vault;

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
