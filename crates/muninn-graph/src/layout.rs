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
//!
//! A vault dense enough that its drawn footprint cannot all fit at the force
//! equilibrium is packed when its heat runs out: the last overlaps are pushed
//! apart to the drawn footprint (cards while they fit, the density-scaled
//! [`marker_extent`] above that), then the layout freezes. Without that final
//! pack the two-pass separation loses a tug-of-war with the forces every step
//! and the settled graph keeps cards or markers on top of each other (#136).
//!
//! The footprint has no density cutoff (#190): at every N a marker packs to
//! `min(DOT_SEP, sqrt(SEP_FULL_FILL × world area / n))`, the same envelope the
//! separation and packing use and the UI's dot size is capped to. A blob that
//! the forces have pressed against the walls cannot make room by growing, so
//! the bounded pack also compacts a wall-bound blob toward a target bbox
//! instead of only growing it.
//!
//! A dense layout can sit in a limit cycle above the dormancy gate (seen on
//! macOS arm64). A stall guard stops it: positions are compared over a 300-step
//! window and frozen when the layout ends where it began — unless the packed
//! envelope is still violated — with a 3,600-step hard bound as the backstop,
//! so no density can keep the window awake forever (#144). The hard bound is
//! the last resort: it records a freeze that still overlaps, and the pack runs
//! first so a fixture it can pack never reaches it.

use std::collections::{HashMap, HashSet};

use muninn_core::Vault;

/// The fixed world the layout lives in, in points.
pub const WORLD: [f32; 2] = [1800.0, 1300.0];
pub const NODE_W: f32 = 136.0;
pub const NODE_H: f32 = 38.0;

const REPULSION: f32 = 13000.0;
/// Small vaults keep the exhaustive 800 cutoff (this constant is unchanged);
/// above this many nodes it tapers with `1/sqrt(n)` so a 2000-note world can
/// afford the pass (800 exceeds the world, leaving no pair pruning). The
/// omitted far wake stays below gravity at ordinary displacements while the
/// grid keeps the result deterministic. (Repulsion and link length were later
/// retuned for spacing, so the settled graph does move from its pre-tuning
/// self; only this cutoff is the pre-grid value.)
const EXHAUSTIVE_LIMIT: usize = 200;
const REPULSION_CUTOFF_SMALL: f32 = 800.0;
const REPULSION_CUTOFF_MIN: f32 = 220.0;
/// A 5×5 cell window covers a cutoff when the cell is half of it.
const REPULSION_CELL_DIVISOR: f32 = 2.0;
/// A node card may grow to 1.18× for a hub (see the UI), which stays inside
/// the separation pads below.
const SEP_PAD_X: f32 = 40.0;
const SEP_PAD_Y: f32 = 22.0;
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
/// The largest card the view draws: a hub grows to 1.18× (see the UI).
const CARD_W: f32 = NODE_W * 1.18;
const CARD_H: f32 = NODE_H * 1.18;
/// Clearance the settled packing leaves between two cards, so a packed pair
/// cannot read as touching after rounding.
const SETTLE_PAD: f32 = 1.0;
/// A pair closer than the envelope by less than this is touching, not
/// overlapping: f32 rounding on a packed pair must not keep the pack awake.
const TOUCH_EPS: f32 = 0.01;
/// The pack moves a pair until its overlap is below this — half the touching
/// epsilon, so the settled centre distance keeps a real margin above
/// `envelope − TOUCH_EPS` instead of landing exactly on it.
const PACK_MOVE_EPS: f32 = TOUCH_EPS * 0.5;
/// The largest low-zoom marker the view draws (28 pt × up to 1.6, see the UI).
const DOT_SEP: f32 = 45.0;
/// A wall-bound blob is compacted toward this fraction of the world's bbox
/// area, rather than grown: at 600+ notes the forces press the blob against
/// both walls, where local nudges and `SETTLE_GROW` only make it wider.
const PACK_TARGET_FILL: f32 = 0.70;
/// A node this close to a wall counts as wall-clamped when choosing between
/// compaction and growth.
const PACK_WALL_EPS: f32 = 0.5;
/// Upper bound on the packing passes a cooling layout may spend. A compact
/// blob with a few dozen overlaps packs clean in tens of passes; the cap is
/// the guarantee that a pathological start still terminates, deterministically.
/// Each pass is [`PACK_SWEEPS_PER_PASS`] directional sweeps, and every
/// [`SETTLE_GROW_EVERY`] passes re-spaces the blob (see [`pack_overlaps`]).
const SETTLE_PASSES: usize = 512;
/// Sweeps per packing pass: forward, reversed, forward, reversed. A single
/// direction biases every pair toward the nodes visited last, and one
/// forward+reverse pair still left a dense hub short inside the pass cap; two
/// pairs unjam it without raising [`SETTLE_PASSES`].
const PACK_SWEEPS_PER_PASS: usize = 4;
/// The pack re-spaces a stalled blob this often: every
/// [`SETTLE_GROW_EVERY`] passes it is either compacted (wall-bound) or given
/// more room (free), so the local pair pushes cannot stall at a wall.
const SETTLE_GROW_EVERY: usize = 64;
/// A free blob is given more room this often by scaling the whole layout about
/// the world centre by this much. A wall-bound blob is instead compacted
/// toward [`PACK_TARGET_FILL`].
const SETTLE_GROW: f32 = 1.04;
const SPRING: f32 = 0.008;
const LINK_LEN: f32 = 175.0;
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
/// Dense steps per stall probe. A layout that ends a window where it began is
/// cycling, not settling, even if its instantaneous speed never drops below
/// [`DORMANT_SPEED_SQ`] (reported on macOS arm64 above the marker-pack limit,
/// #144).
const SETTLE_PROBE_FRAMES: u16 = 300;
/// Net movement (points) under which a probe window counts as stalled. Over
/// 300 steps this is 0.007 pt/step, far below anything visible.
const SETTLE_PROBE_STEP: f32 = 2.0;
/// Hard bound on dense settling once the heat timer runs out; the backstop for
/// a cycle whose amplitude is too large for the probe window to read as still.
/// The slowest observed real fixture (2,000 notes) quiets around frame 2,600.
const SETTLE_MAX_DENSE_FRAMES: u32 = 3600;
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
    /// A cooled near-full layout has run its final card packing: positions are
    /// frozen until something changes, so the packed state cannot drift back
    /// into overlap.
    packed: bool,
    /// #144: positions at the start of the current stall probe, and how many
    /// dense steps it has run.
    probe: Vec<[f32; 2]>,
    probe_frames: u16,
    /// Dense steps since the heat timer reached zero; the hard bound's clock.
    dense_frames: u32,
    /// The stall guard froze this dense layout; positions stay put until a
    /// reheat, sync or reset.
    frozen: bool,
    /// The freeze was the last resort: the packed envelope was still violated.
    /// Recorded rather than hidden; fixtures the pack can clear never set it.
    frozen_overlapping: bool,
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
            packed: false,
            probe: Vec::new(),
            probe_frames: 0,
            dense_frames: 0,
            frozen: false,
            frozen_overlapping: false,
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
            self.packed = false;
            self.probe.clear();
            self.probe_frames = 0;
            self.dense_frames = 0;
            self.frozen = false;
            self.frozen_overlapping = false;
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
    /// Above the exhaustive-layout limit nodes a cooled layout goes dormant
    /// when the fastest node is below `0.4 pt/step`; from then on this
    /// returns false without integrating (positions are frozen). A vault dense
    /// enough to owe a packing is instead packed once when its heat runs out,
    /// and then frozen the same way. [`Graph::reheat`] resumes it, as does a
    /// sync that adds/removes nodes or links.
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
        let mut moved = moved_since(&self.nodes, &before);
        if self.cooling() {
            moved |= self.pack();
        }
        // #144: a dense vault no longer settling by the motion gate still has to
        // stop; the stall probe and the hard bound guarantee it does. The guard
        // also watches a packed layout whose envelope the pack could not meet,
        // so it can still reach the hard bound (#190).
        if dense && self.heat == 0.0 && !self.frozen {
            self.guard_stall();
        }
        moved
    }

    /// Once the cooling timer has run out a near-full layout still overlaps a
    /// little: the force equilibrium is tighter than the separation envelope,
    /// and the two-pass projection cannot win against it every step. Pack the
    /// remains away once (deterministic, bounded) and freeze; without the
    /// freeze the forces would pull the cards back into overlap and the layout
    /// would jitter forever. [#136]
    ///
    /// The pack schedule itself is the bound (#190): it runs at most
    /// [`SETTLE_PASSES`] passes with the fixed compaction/growth schedule and
    /// then leaves the result to the overlap-aware gate. `packed` keeps its old
    /// meaning for small vaults — set only when the pack moved something — so
    /// their settling path stays bit-identical. A dense pack that moved nothing
    /// but still leaves overlaps is marked packed too, so a jammed wall cannot
    /// re-run the whole schedule every cooled step until the hard bound.
    fn pack(&mut self) -> bool {
        let Some(extent) = settle_extent(self.nodes.len()) else {
            return false;
        };
        let moved = pack_overlaps(&mut self.nodes, extent);
        let dense = self.nodes.len() > EXHAUSTIVE_LIMIT;
        if moved || (dense && envelope_violated(&self.nodes, extent)) {
            self.packed = true;
        }
        if moved {
            for node in &mut self.nodes {
                node.vel = [0.0; 2];
            }
        }
        moved
    }

    /// Cooled and unheld: the settled layout may owe a packing. There is no
    /// density cutoff — every N ≥ 2 has a [`settle_extent`] (#190).
    fn cooling(&self) -> bool {
        self.heat == 0.0
            && !self.packed
            && !self.nodes.iter().any(|node| node.fixed)
            && settle_extent(self.nodes.len()).is_some()
    }

    /// Stop a dense layout that is not making progress (#144).
    ///
    /// The speed gate assumes a settling layout slows down; some densities sit
    /// in a limit cycle above the gate instead, and the window never sleeps.
    /// Two cheap, deterministic bounds:
    ///
    /// - **Probe:** every [`SETTLE_PROBE_FRAMES`] steps, compare positions with
    ///   the window's start. Net movement under [`SETTLE_PROBE_STEP`] means the
    ///   layout is cycling in place (or creeping invisibly), so freeze it.
    /// - **Hard bound:** after [`SETTLE_MAX_DENSE_FRAMES`] cooled steps, freeze
    ///   regardless, which catches a cycle too wide for the probe to read as
    ///   still.
    ///
    /// A held node (a drag) suspends both, like the pack.
    fn guard_stall(&mut self) {
        if self.nodes.iter().any(|node| node.fixed) {
            return;
        }
        self.dense_frames = self.dense_frames.saturating_add(1);
        self.probe_frames = self.probe_frames.saturating_add(1);
        if self.probe.len() != self.nodes.len() || self.probe_frames >= SETTLE_PROBE_FRAMES {
            let still = self.probe.len() == self.nodes.len()
                && self.nodes.iter().zip(&self.probe).all(|(node, old)| {
                    (node.pos[0] - old[0]).abs() <= SETTLE_PROBE_STEP
                        && (node.pos[1] - old[1]).abs() <= SETTLE_PROBE_STEP
                });
            self.probe.clear();
            self.probe.extend(self.nodes.iter().map(|node| node.pos));
            self.probe_frames = 0;
            // A layout that is still but still overlaps is not settled: the
            // pack needs its budget first (#190). The hard bound below still
            // stops it if packing cannot meet the envelope.
            if still && !self.overlaps_owed() {
                self.freeze();
                return;
            }
        }
        if self.dense_frames >= SETTLE_MAX_DENSE_FRAMES {
            self.freeze();
        }
    }

    /// Freeze positions and speeds until the next reheat, sync or reset.
    fn freeze(&mut self) {
        self.frozen = true;
        self.frozen_overlapping = self.overlaps_owed();
        for node in &mut self.nodes {
            node.vel = [0.0; 2];
        }
    }

    /// True when pairs are still closer than the packing footprint, with no
    /// node held. A held node (a drag) is outside the guarantee, like the pack
    /// itself. A clean dense layout is not dormant-blocked even before a pack
    /// runs: it owes nothing.
    fn overlaps_owed(&self) -> bool {
        if self.nodes.iter().any(|node| node.fixed) {
            return false;
        }
        let Some(extent) = settle_extent(self.nodes.len()) else {
            return false;
        };
        envelope_violated(&self.nodes, extent)
    }

    /// A cooled layout that owes nothing more is dormant: positions are frozen
    /// and `step` reports no change. Dense vaults stop at the motion gate (their
    /// residual creep is sub-pixel), when the stall guard freezes them, or, for
    /// a packed near-full vault, outright.
    ///
    /// Since #190 the motion gate is overlap-aware: a still dense layout whose
    /// pairs still violate its [`settle_extent`] is not dormant, so it spends
    /// its bounded pack before stopping instead of freezing on top of itself.
    /// A held node keeps the old motion-gate behaviour.
    fn dormant(&self) -> bool {
        if self.heat != 0.0 {
            return false;
        }
        if self.nodes.len() > EXHAUSTIVE_LIMIT {
            if self.frozen {
                return true;
            }
            return self.max_speed_sq() <= DORMANT_SPEED_SQ && !self.overlaps_owed();
        }
        self.packed && !self.nodes.iter().any(|node| node.fixed)
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
        self.packed = false;
        self.probe.clear();
        self.probe_frames = 0;
        self.dense_frames = 0;
        self.frozen = false;
        self.frozen_overlapping = false;
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
        self.packed = false;
        self.probe.clear();
        self.probe_frames = 0;
        self.dense_frames = 0;
        self.frozen = false;
        self.frozen_overlapping = false;
    }

    /// True when the hard bound froze a dense layout whose packed envelope was
    /// still violated. The freeze is still the last resort — it just says so.
    #[must_use]
    pub fn frozen_overlapping(&self) -> bool {
        self.frozen_overlapping
    }

    /// True once this heat cycle's final pack has run. The pack is a one-shot
    /// layout correction, so the frame that ran it has a large deliberate move
    /// even though the layout is settled; measurements that track residual
    /// motion read this to tell a pack jump from a force creep.
    #[must_use]
    pub fn is_packed(&self) -> bool {
        self.packed
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

/// Whether full cards still fit inside [`SEP_FULL_FILL`] of the world. Below
/// this density the layout keeps the original card envelope; above it, the
/// isotropic marker envelope takes over.
fn cards_fit(n: usize) -> bool {
    n as f32 * (CARD_W + SETTLE_PAD) * (CARD_H + SETTLE_PAD) <= SEP_FULL_FILL * WORLD[0] * WORLD[1]
}

/// The density-scaled footprint every marker must have: full marker spacing
/// while it fits, then the square root of the area left per node. There is no
/// cutoff: the envelope shrinks with density instead of disappearing at 405
/// notes, so packing, separation and the drawn dot all use one number (#190).
///
/// The UI caps a drawn dot (plus a small pad) at this value, so a dot can never
/// be wider than the spacing the settled layout guarantees.
#[must_use]
pub fn marker_extent(n: usize) -> f32 {
    if n == 0 {
        return DOT_SEP;
    }
    let area = SEP_FULL_FILL * WORLD[0] * WORLD[1] / n as f32;
    area.sqrt().min(DOT_SEP)
}

/// Full card separation while cards fit; below that, shrink both axes so the
/// available world area per node is respected. Above card country the envelope
/// is the isotropic [`marker_extent`] — the old 176×60 card shrink left a
/// 49×17 reach at 1,000 notes, far below the 28 pt dot it was separating.
fn separation_extent(n: usize) -> [f32; 2] {
    if n == 0 {
        return [SEP_CELL_X, SEP_CELL_Y];
    }
    if cards_fit(n) {
        let fill = n as f32 * SEP_AREA / (WORLD[0] * WORLD[1]);
        let scale = if fill <= SEP_FULL_FILL {
            1.0
        } else {
            (SEP_FULL_FILL / fill).sqrt()
        };
        return [SEP_CELL_X * scale, SEP_CELL_Y * scale];
    }
    let marker = marker_extent(n);
    [marker, marker]
}

/// The envelope a settled layout must respect, or `None` only for a lone node.
/// Full cards while they fit, [`marker_extent`] above that: the view draws those
/// densities as dots (see `LOD_ZOOM`), and demanding card spacing would only
/// wedge the packing.
fn settle_extent(n: usize) -> Option<[f32; 2]> {
    if n < 2 {
        return None;
    }
    if cards_fit(n) {
        return Some([CARD_W + SETTLE_PAD, CARD_H + SETTLE_PAD]);
    }
    let marker = marker_extent(n);
    Some([marker, marker])
}

/// Pack every overlapping pair out to `extent`. Each pair moves the smallest
/// distance along its own centre-to-centre line that clears both rectangles, so
/// a pile spreads evenly instead of stacking along one axis.
///
/// Each pass is [`PACK_SWEEPS_PER_PASS`] directional sweeps. A sweep visits
/// every node in a fixed order and takes its candidates from a [`Grid`] cell of
/// the envelope (a 3×3 window, the same candidate set the separation pass
/// uses), then sorts them, so the O(n²) pair scan is gone and the result stays
/// deterministic. The grid holds sweep-start positions: a pair pushed into
/// overlap mid-sweep is caught by the next sweep, exactly like the separation
/// pass's documented bound.
///
/// The pack has a fixed schedule and budget ([`SETTLE_PASSES`] passes, fixed
/// pair order). Every [`SETTLE_GROW_EVERY`] passes a stalled blob is re-spaced:
/// card country grows a free blob about the world centre (which escapes a jam
/// that local nudging cannot), and marker country — or any wall-bound blob —
/// is **compacted or spread** toward [`PACK_TARGET_FILL`] of the world about its
/// centroid. Growth alone cannot make room at 600+ notes because the blob is
/// wall-bound; compaction is what turns an unruly spread into a packable bbox.
fn pack_overlaps(nodes: &mut [GNode], extent: [f32; 2]) -> bool {
    if !envelope_violated(nodes, extent) {
        return false;
    }
    let mut moved = false;
    let mut candidates = Vec::new();
    for pass in 0..SETTLE_PASSES {
        if pass > 0 && pass % SETTLE_GROW_EVERY == 0 {
            moved |= respace(nodes, extent);
        }
        if pack_pass(nodes, extent, &mut candidates) {
            moved = true;
            continue;
        }
        // A pass that moves nothing is either clean or jammed (typically with
        // part of the blob clamped at a wall). A clean layout stops here; a
        // jammed one is re-spaced once more and given another pass. The pass
        // budget still bounds the whole schedule.
        if !envelope_violated(nodes, extent) {
            break;
        }
        if pass + 1 >= SETTLE_PASSES || !respace(nodes, extent) {
            break;
        }
        moved = true;
    }
    moved
}

/// One grid-backed radial packing pass: [`PACK_SWEEPS_PER_PASS`] directional
/// sweeps, each visiting every node in a fixed order and taking candidates from
/// a 3×3 [`Grid`] window. Alternating direction cancels the systematic bias of
/// a single order (the first nodes always give way to the last), which is what
/// lets a dense hub unjam inside [`SETTLE_PASSES`]; the grid holds sweep-start
/// positions, and a pair pushed into overlap mid-sweep is caught by the next
/// one — the same documented bound as the separation pass.
fn pack_pass(nodes: &mut [GNode], extent: [f32; 2], candidates: &mut Vec<usize>) -> bool {
    let mut moved = false;
    for sweep in 0..PACK_SWEEPS_PER_PASS {
        moved |= pack_sweep(nodes, extent, candidates, sweep % 2 == 1);
    }
    moved
}

/// One directional sweep. Nodes are moved in index order (or its reverse); a
/// moved node's stale grid cell is corrected by the next sweep.
fn pack_sweep(
    nodes: &mut [GNode],
    extent: [f32; 2],
    candidates: &mut Vec<usize>,
    reverse: bool,
) -> bool {
    let grid = Grid::build(nodes, extent);
    let mut moved = false;
    for step in 0..nodes.len() {
        let i = if reverse {
            nodes.len() - 1 - step
        } else {
            step
        };
        candidates.clear();
        collect_overlaps(nodes, i, &grid, extent, PACK_MOVE_EPS, candidates);
        candidates.sort_unstable();
        for &j in candidates.iter() {
            moved |= separate_pair_radial(nodes, i, j, extent, PACK_MOVE_EPS);
        }
    }
    moved
}

/// Re-space a stalled pack: compact a wall-bound blob toward a target bbox,
/// otherwise grow a free one about the world centre. Returns whether any
/// position changed.
fn respace(nodes: &mut [GNode], extent: [f32; 2]) -> bool {
    let Some([x0, y0, x1, y1]) = bbox(nodes) else {
        return false;
    };
    let width = (x1 - x0).max(1.0);
    let height = (y1 - y0).max(1.0);
    let usable_w = WORLD[0] - EDGE * 2.0;
    let usable_h = WORLD[1] - EDGE * 2.0;
    let wall_bound = nodes.iter().any(|node| {
        node.pos[0] <= EDGE + PACK_WALL_EPS
            || node.pos[0] >= WORLD[0] - EDGE - PACK_WALL_EPS
            || node.pos[1] <= EDGE + PACK_WALL_EPS
            || node.pos[1] >= WORLD[1] - EDGE - PACK_WALL_EPS
    }) || width >= usable_w - PACK_WALL_EPS
        || height >= usable_h - PACK_WALL_EPS;

    // Card country grows a free blob (the cards need room), but a marker blob
    // never does: its cells already fit in the target bbox, and growing one is
    // what walked a wall-bound marker blob back into the walls (compaction,
    // growth, compaction — seen on a 2,000-note hub, #190). Marker country
    // always re-spaces toward the target bbox, which shrinks an over-spread
    // blob and gives a small one exactly the room its cells need.
    //
    // The axes are scaled independently. A uniform scale cannot reach a target
    // bbox that is wider than the blob's own aspect once the shorter axis hits
    // the wall clamp; it undershoots, leaving the cells too dense to unjam.
    // Scaling each axis toward its target dimension reaches PACK_TARGET_FILL
    // exactly and never turns an expansion into an overlap (both axes only
    // grow).
    let (centre, scale_x, scale_y) = if !wall_bound && cards_fit(nodes.len()) {
        let grow = SETTLE_GROW;
        (crate::camera::world_centre(), grow, grow)
    } else {
        let target_area = (PACK_TARGET_FILL * WORLD[0] * WORLD[1])
            .max(nodes.len() as f32 * extent[0] * extent[1]);
        let aspect = usable_w / usable_h;
        let target_w = (target_area * aspect).sqrt().min(usable_w);
        let target_h = (target_area / target_w).min(usable_h);
        let scale_x = (target_w / width).clamp(0.25, 2.0);
        let scale_y = (target_h / height).clamp(0.25, 2.0);
        (centroid(nodes), scale_x, scale_y)
    };
    if (scale_x - 1.0).abs() < 1e-4 && (scale_y - 1.0).abs() < 1e-4 {
        return false;
    }
    for node in nodes.iter_mut() {
        node.pos[0] =
            (centre[0] + (node.pos[0] - centre[0]) * scale_x).clamp(EDGE, WORLD[0] - EDGE);
        node.pos[1] =
            (centre[1] + (node.pos[1] - centre[1]) * scale_y).clamp(EDGE, WORLD[1] - EDGE);
    }
    true
}

/// The bounding box of the nodes' centres, or `None` when there are none.
fn bbox(nodes: &[GNode]) -> Option<[f32; 4]> {
    let mut boxed: Option<[f32; 4]> = None;
    for node in nodes {
        boxed = Some(match boxed {
            None => [node.pos[0], node.pos[1], node.pos[0], node.pos[1]],
            Some([x0, y0, x1, y1]) => [
                x0.min(node.pos[0]),
                y0.min(node.pos[1]),
                x1.max(node.pos[0]),
                y1.max(node.pos[1]),
            ],
        });
    }
    boxed
}

/// The mean position of the nodes (the blob's centre for compaction).
fn centroid(nodes: &[GNode]) -> [f32; 2] {
    let mut sum = [0.0f32; 2];
    for node in nodes {
        sum[0] += node.pos[0];
        sum[1] += node.pos[1];
    }
    let n = nodes.len().max(1) as f32;
    [sum[0] / n, sum[1] / n]
}

/// True when any pair still violates `extent` by more than the touching
/// epsilon. Uses the same grid candidate set as the pack, so it is O(n) and
/// safe to poll from the dormancy gate at 2,000 notes.
fn envelope_violated(nodes: &[GNode], extent: [f32; 2]) -> bool {
    if nodes.len() < 2 {
        return false;
    }
    let grid = Grid::build(nodes, extent);
    let mut candidates = Vec::new();
    for i in 0..nodes.len() {
        candidates.clear();
        collect_overlaps(nodes, i, &grid, extent, TOUCH_EPS, &mut candidates);
        if !candidates.is_empty() {
            return true;
        }
    }
    false
}

/// The minimal move along the centre-to-centre direction that clears the two
/// rectangles, split evenly (or given wholly to the free node when one is held)
/// and clamped to the world.
fn separate_pair_radial(
    nodes: &mut [GNode],
    i: usize,
    j: usize,
    extent: [f32; 2],
    min_overlap: f32,
) -> bool {
    let (share_i, share_j) = match (nodes[i].fixed, nodes[j].fixed) {
        (true, true) => return false,
        (true, false) => (0.0, 1.0),
        (false, true) => (1.0, 0.0),
        (false, false) => (0.5, 0.5),
    };
    let dx = nodes[j].pos[0] - nodes[i].pos[0];
    let dy = nodes[j].pos[1] - nodes[i].pos[1];
    let ox = extent[0] - dx.abs();
    let oy = extent[1] - dy.abs();
    if ox <= min_overlap || oy <= min_overlap {
        return false;
    }
    let d = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if d < 1e-3 {
        (1.0f32, 0.0f32)
    } else {
        (dx / d, dy / d)
    };
    let tx = if ux.abs() > 1e-6 {
        ox / ux.abs()
    } else {
        f32::INFINITY
    };
    let ty = if uy.abs() > 1e-6 {
        oy / uy.abs()
    } else {
        f32::INFINITY
    };
    let t = tx.min(ty);
    if !t.is_finite() {
        return false;
    }
    let before = [nodes[i].pos, nodes[j].pos];
    nodes[i].pos[0] = (nodes[i].pos[0] - ux * t * share_i).clamp(EDGE, WORLD[0] - EDGE);
    nodes[i].pos[1] = (nodes[i].pos[1] - uy * t * share_i).clamp(EDGE, WORLD[1] - EDGE);
    nodes[j].pos[0] = (nodes[j].pos[0] + ux * t * share_j).clamp(EDGE, WORLD[0] - EDGE);
    nodes[j].pos[1] = (nodes[j].pos[1] + uy * t * share_j).clamp(EDGE, WORLD[1] - EDGE);
    nodes[i].pos != before[0] || nodes[j].pos != before[1]
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
        // Above EXHAUSTIVE_LIMIT, so the dense settling rules apply. One node is
        // held (as during a drag) so the settle pack does not run: this test is
        // about the residual creep the heat-aware dormancy gate bounds, and the
        // pack replaces that path for dense vaults it can pack. Holding a node
        // is how the pack is suppressed without depending on the density
        // threshold, whose frame count differs by platform float path.
        let tmp = big_vault(260);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        g.nodes[0].fixed = true;
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

/// #136: a settled, dense layout must not draw two nodes on top of each other.
///
/// The measurement is the one the issue used on a settled `--tree` dump, in
/// world space: a screen-space pair overlaps exactly when the same pair overlaps
/// in world space (the camera scales positions and card sizes together), and
/// `CARD_W`×`CARD_H` is the largest card the view can draw, so a settled layout
/// that clears it can never show overlapping cards at any zoom.
#[cfg(test)]
mod settle {
    use super::*;
    use muninn_core::testing::{TempVault, big_vault};

    /// A dense synthetic vault: `n` notes, each linking to four others at a
    /// fixed stride, so the graph is as link-dense as a real vault's and the
    /// fixture is deterministic. The four strides reproduce #136: at 90 notes
    /// the settled layout drew 61 overlapping card pairs before the settle
    /// pack, and this fixture is what the regression test measures.
    fn linked_vault(n: usize) -> TempVault {
        let files: Vec<(String, String)> = (0..n)
            .map(|i| {
                let mut body = format!("# Note {i:03}\n\n");
                for step in [1usize, 7, 13, 29] {
                    body.push_str(&format!("[[Note {:03}]] ", (i + step) % n));
                }
                body.push('\n');
                (format!("Note {i:03}.md"), body)
            })
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, body)| (name.as_str(), body.as_str()))
            .collect();
        TempVault::new(&refs)
    }

    /// The N list the #190 footprint is measured over. 405 is the old
    /// arithmetic cutoff (404 fit, 405 did not); 600/1000 are the densities
    /// that used to be pressed against the walls while overlapping; 2000 is
    /// the perf-style case, gated separately.
    const DENSITIES: [usize; 5] = [150, 300, 405, 600, 1000];

    /// Whether a settled layout keeps every marker pair at the envelope.
    fn assert_packed(g: &Graph, n: usize) {
        let e = marker_extent(n);
        let bad = overlaps(g, e, e);
        assert!(
            bad.is_empty(),
            "{n} notes settle with {} overlapping marker pairs at {e:.3} pt: {:?}",
            bad.len(),
            &bad[..bad.len().min(4)]
        );
        let min_nn = min_nn(g);
        assert!(
            min_nn >= e - 0.01,
            "{n} notes: closest centre pair {min_nn:.3} pt is inside the {e:.3} pt envelope"
        );
    }

    /// The closest centre-to-centre distance in a settled layout.
    fn min_nn(g: &Graph) -> f32 {
        let mut min = f32::INFINITY;
        for i in 0..g.nodes.len() {
            for j in i + 1..g.nodes.len() {
                let dx = g.nodes[j].pos[0] - g.nodes[i].pos[0];
                let dy = g.nodes[j].pos[1] - g.nodes[i].pos[1];
                min = min.min((dx * dx + dy * dy).sqrt());
            }
        }
        min
    }

    /// Bbox area as a fraction of the world — the "fill" the issue measured.
    fn bbox_fill(g: &Graph) -> f32 {
        let Some([x0, y0, x1, y1]) = bbox(&g.nodes) else {
            return 0.0;
        };
        (x1 - x0) * (y1 - y0) / (WORLD[0] * WORLD[1])
    }

    /// Nodes exactly on the world clamp (the integrator writes the wall value).
    fn wall_clamped(g: &Graph) -> usize {
        g.nodes
            .iter()
            .filter(|node| {
                node.pos[0] <= EDGE
                    || node.pos[0] >= WORLD[0] - EDGE
                    || node.pos[1] <= EDGE
                    || node.pos[1] >= WORLD[1] - EDGE
            })
            .count()
    }

    /// Run frames until thirty in a row report no change (the definition of
    /// settled the app's poll loop uses).
    fn settle(g: &mut Graph) -> usize {
        let mut quiet = 0;
        for frame in 1..=8000 {
            if g.step() {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet == 30 {
                    return frame;
                }
            }
        }
        panic!("layout must settle within 8000 frames");
    }

    /// Overlapping pairs of footprints `w`×`h` (both axes must overlap).
    ///
    /// The comparand is `TOUCH_EPS` rather than zero: a packed pair lands with
    /// its edges exactly on the envelope, and f32 rounding then reports an
    /// overlap of ~1e-4 pt on some targets (macOS arm64) — a touching pair, not
    /// a drawn one.
    fn overlaps(g: &Graph, w: f32, h: f32) -> Vec<(usize, usize, f32, f32)> {
        /// Below this the pair is touching, not overlapping.
        const TOUCH_EPS: f32 = 0.01;
        let mut out = Vec::new();
        for i in 0..g.nodes.len() {
            for j in i + 1..g.nodes.len() {
                let ox = w - (g.nodes[j].pos[0] - g.nodes[i].pos[0]).abs();
                let oy = h - (g.nodes[j].pos[1] - g.nodes[i].pos[1]).abs();
                if ox > TOUCH_EPS && oy > TOUCH_EPS {
                    out.push((i, j, ox, oy));
                }
            }
        }
        out
    }

    fn assert_inside_world(g: &Graph) {
        for node in &g.nodes {
            assert!(
                node.pos[0] >= EDGE && node.pos[0] <= WORLD[0] - EDGE,
                "{} left the world at {:?}",
                node.rel,
                node.pos
            );
            assert!(
                node.pos[1] >= EDGE && node.pos[1] <= WORLD[1] - EDGE,
                "{} left the world at {:?}",
                node.rel,
                node.pos
            );
        }
    }

    #[test]
    fn a_dense_vault_settles_without_overlapping_cards() {
        // 90 notes: dense (above SEP_FULL_FILL, so the separation envelope
        // shrinks) yet still card country. This is the reported fixture.
        let tmp = linked_vault(90);
        let v = tmp.load();
        let fill = v.notes.len() as f32 * SEP_AREA / (WORLD[0] * WORLD[1]);
        assert!(fill > SEP_FULL_FILL, "the fixture must be dense: {fill}");
        let mut g = Graph::new(&v);
        let frames = settle(&mut g);
        assert!(
            frames < 400,
            "a dense vault settles in a few seconds: {frames}"
        );
        assert_inside_world(&g);

        let bad = overlaps(&g, CARD_W, CARD_H);
        assert!(
            bad.is_empty(),
            "a settled dense vault draws overlapping cards: {bad:?}"
        );

        // Frozen, not oscillating: the settled positions do not move again.
        let frozen: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        for _ in 0..30 {
            assert!(!g.step(), "a settled layout must report no change");
        }
        let after: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        assert_eq!(after, frozen, "a settled layout must keep its positions");
    }

    #[test]
    fn dense_markers_settle_apart_at_scale() {
        // Past the card country the view draws dots (LOD_ZOOM), but the settled
        // positions still have to keep them apart: 260 notes used to settle with
        // dozens of overlapping markers.
        for tmp in [big_vault(260), linked_vault(300)] {
            let v = tmp.load();
            let mut g = Graph::new(&v);
            settle(&mut g);
            assert_inside_world(&g);
            let bad = overlaps(&g, DOT_SEP, DOT_SEP);
            assert!(
                bad.is_empty(),
                "{} notes settle with overlapping markers: {bad:?}",
                g.nodes.len()
            );
        }
    }

    #[test]
    fn a_vault_above_the_old_pack_limit_settles_clean_and_freezes() {
        // #144: 405 notes used to skip the settle pack entirely and sit in a
        // limit cycle on macOS arm64. #190 removed the cutoff, so 410 now packs
        // to its marker envelope, stops well inside the bound, and freezes.
        let tmp = big_vault(410);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        let frames = settle(&mut g);
        assert!(
            frames < 6000,
            "a vault above the old pack limit must stop settling: {frames}"
        );
        assert_inside_world(&g);
        assert!(
            !g.frozen_overlapping(),
            "the pack must meet the envelope before the layout stops"
        );
        assert_packed(&g, 410);
        let frozen: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        for _ in 0..30 {
            assert!(!g.step(), "a stalled layout must report no change");
        }
        let after: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        assert_eq!(after, frozen, "the stall guard must freeze positions");
    }

    #[test]
    fn settling_is_deterministic_and_reheats() {
        let tmp = linked_vault(90);
        let v = tmp.load();
        let mut one = Graph::new(&v);
        let mut two = Graph::new(&v);
        settle(&mut one);
        settle(&mut two);
        for (a, b) in one.nodes.iter().zip(&two.nodes) {
            assert_eq!(a.pos, b.pos, "{} settled off its twin", a.rel);
        }

        // A reheat wakes the frozen layout, and the resumed run settles just as
        // deterministically (and just as overlap-free).
        for g in [&mut one, &mut two] {
            g.reheat();
            assert!(g.step(), "reheat must resume the step");
            settle(g);
        }
        for (a, b) in one.nodes.iter().zip(&two.nodes) {
            assert_eq!(a.pos, b.pos, "reheat must settle deterministically");
        }
        let bad = overlaps(&one, CARD_W, CARD_H);
        assert!(bad.is_empty(), "a reheated layout settles clean: {bad:?}");
    }

    /// #190: the footprint has no density cutoff. The table is the issue's
    /// counterfactual, ±0.1: 45.0 at 405, 36.9 at 600, 28.6 at 1000, 20.2 at
    /// 2000.
    #[test]
    fn marker_extent_is_density_scaled() {
        for (n, want) in [(405usize, 45.0f32), (600, 36.9), (1000, 28.6), (2000, 20.2)] {
            let got = marker_extent(n);
            assert!(
                (got - want).abs() <= 0.1,
                "marker_extent({n}) = {got:.3}, want {want} ±0.1"
            );
        }
        // Full marker spacing while it fits, then monotone shrink, never above
        // the drawn dot's largest size.
        assert!(marker_extent(1) <= DOT_SEP);
        assert!(marker_extent(150) <= DOT_SEP);
        let mut last = marker_extent(1);
        for n in [2usize, 100, 404, 405, 600, 1000, 2000, 5000] {
            let now = marker_extent(n);
            assert!(now <= last + 1e-6, "footprint grew at {n}");
            last = now;
        }
    }

    /// Every density packs to the same envelope separation uses; 405 is no
    /// worse than 404 (the old arithmetic cliff).
    #[test]
    fn dense_vaults_pack_to_the_marker_envelope() {
        for n in DENSITIES {
            let tmp = linked_vault(n);
            let v = tmp.load();
            let mut g = Graph::new(&v);
            settle(&mut g);
            assert_inside_world(&g);
            assert!(
                !g.frozen_overlapping(),
                "{n} notes froze with a violated envelope"
            );
            assert_packed(&g, n);
        }
    }

    #[test]
    fn a_405_note_vault_is_no_worse_than_404() {
        let mut counts = Vec::new();
        for n in [404usize, 405] {
            let tmp = linked_vault(n);
            let v = tmp.load();
            let mut g = Graph::new(&v);
            settle(&mut g);
            assert_inside_world(&g);
            let e = marker_extent(n);
            let bad = overlaps(&g, e, e);
            assert!(
                bad.is_empty(),
                "{n} notes overlap at their {e:.3} pt envelope: {} pairs",
                bad.len()
            );
            counts.push(bad.len());
        }
        assert!(
            counts[1] <= counts[0],
            "405 ({}) must be no worse than 404 ({})",
            counts[1],
            counts[0]
        );
    }

    /// The spread gate from the issue: the settled bbox fills at most 0.75 of
    /// the world and at most 2% of nodes are wall-clamped. Measured #190 on
    /// this fixture: fill 0.47/0.70/0.70/0.70/0.70, walls 3/0/0/0/0 at
    /// 150/300/405/600/1000.
    #[test]
    fn dense_vaults_spread_without_pressing_the_walls() {
        for n in DENSITIES {
            let tmp = linked_vault(n);
            let v = tmp.load();
            let mut g = Graph::new(&v);
            settle(&mut g);
            let fill = bbox_fill(&g);
            assert!(
                fill <= 0.75,
                "{n} notes fill {fill:.3} of the world, over the 0.75 gate"
            );
            let walls = wall_clamped(&g);
            let limit = (g.nodes.len() as f32 * 0.02).ceil() as usize;
            assert!(
                walls <= limit,
                "{n} notes leave {walls} nodes wall-clamped (limit {limit})"
            );
        }
    }

    /// Determinism and reheat on the new N list: two identical vaults settle
    /// to the same positions, and a reheated settled layout does too, still
    /// inside the envelope.
    #[test]
    fn dense_settling_is_deterministic_and_reheats() {
        let tmp = linked_vault(405);
        let v = tmp.load();
        let mut one = Graph::new(&v);
        let mut two = Graph::new(&v);
        settle(&mut one);
        settle(&mut two);
        for (a, b) in one.nodes.iter().zip(&two.nodes) {
            assert_eq!(a.pos, b.pos, "{} settled off its twin", a.rel);
        }
        for g in [&mut one, &mut two] {
            g.reheat();
            assert!(g.step(), "reheat must resume the step");
            settle(g);
        }
        for (a, b) in one.nodes.iter().zip(&two.nodes) {
            assert_eq!(a.pos, b.pos, "reheat must settle deterministically");
        }
        assert_packed(&one, 405);
    }

    /// The small-vault path must not move: 15 notes never take a dense branch
    /// (the 90-note card test covers the boundary), so #190 must leave this
    /// regime and the final pack untouched.
    ///
    /// This used to pin a hash of the settled positions. Those positions start
    /// from `seed`'s `angle.cos()/sin()`, and libm may differ by an ULP across
    /// Linux/macOS/Windows, so a bit hash proves portability as much as it
    /// proves the pack. Instead this proves the portable claim directly: the
    /// fixture stays in card country, and the new pack is a bit-for-bit no-op
    /// on the settled sample.
    #[test]
    fn a_15_note_vault_layout_is_untouched_by_the_pack() {
        let tmp = linked_vault(15);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        settle(&mut g);

        // The density-scaled marker envelope must not reach this regime: 15
        // notes keep the full card envelope.
        let extent = settle_extent(15).expect("15 notes are in card country");
        assert_eq!(extent, [CARD_W + SETTLE_PAD, CARD_H + SETTLE_PAD]);

        // "The sample does not move" means the pack owes nothing here. Run the
        // real pack on a copy of the settled nodes and require a no-op, with
        // every f32 bit unchanged.
        let before: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        let mut nodes = g.nodes.clone();
        assert!(
            !pack_overlaps(&mut nodes, extent),
            "the 15-note sample must not owe a packing"
        );
        for (index, (node, old)) in nodes.iter().zip(&before).enumerate() {
            assert_eq!(
                node.pos.map(f32::to_bits),
                old.map(f32::to_bits),
                "node {index} moved in the pack; small vaults must stay bit-identical"
            );
        }
    }

    /// The 2,000-note case, perf-gated: the grid pack must clear its 20.2 pt
    /// envelope within the settling bound, not freeze overlapping.
    #[test]
    #[ignore = "perf: 2000-note settle; run with --release -- --include-ignored"]
    fn a_2000_note_vault_packs_to_its_envelope() {
        let tmp = linked_vault(2000);
        let v = tmp.load();
        let mut g = Graph::new(&v);
        let frames = settle(&mut g);
        println!(
            "2000-note settle: {frames} frames, {} overlaps at {:.3} pt, min NN {:.3}, \
             fill {:.3}, {} wall-clamped",
            overlaps(&g, marker_extent(2000), marker_extent(2000)).len(),
            marker_extent(2000),
            min_nn(&g),
            bbox_fill(&g),
            wall_clamped(&g),
        );
        assert!(
            !g.frozen_overlapping(),
            "2000 notes froze with a violated envelope at frame {frames}"
        );
        assert_inside_world(&g);
        assert_packed(&g, 2000);
    }
}

#[cfg(test)]
mod grid {
    use super::*;
    use muninn_core::testing::big_vault;
    #[test]
    fn grid_repulsion_matches_the_exhaustive_pass() {
        // Above EXHAUSTIVE_LIMIT, so `repulsion_force` takes the grid path.
        let tmp = big_vault(410);
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
