//! A deterministic force-directed layout for a vault's link graph.
//!
//! Nodes start on a golden-angle spiral seeded by their vault-relative path,
//! so the same vault lays out the same way every launch (duplicate titles in
//! different folders seed to different angles). Repulsion, link springs,
//! gravity and damping then settle it. Node ids are note ids; positions
//! survive a save or a new note because [`Graph::sync`] keeps them by `rel`,
//! not by title.

use std::collections::{HashMap, HashSet};

use muninn_core::Vault;

/// The fixed world the layout lives in, in points.
pub const WORLD: [f32; 2] = [1800.0, 1300.0];
pub const NODE_W: f32 = 136.0;
pub const NODE_H: f32 = 38.0;

const REPULSION: f32 = 9000.0;
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
        self.nodes.iter().find(|n| n.note == note)
    }

    pub fn node_mut(&mut self, note: usize) -> Option<&mut GNode> {
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
        let n = self.nodes.len();
        let mut force = vec![[0.0f32; 2]; n];
        for i in 0..n {
            for j in i + 1..n {
                let dx = self.nodes[j].pos[0] - self.nodes[i].pos[0];
                let dy = self.nodes[j].pos[1] - self.nodes[i].pos[1];
                let d2 = (dx * dx + dy * dy).max(36.0);
                let d = d2.sqrt();
                let f = REPULSION / d2;
                let ux = dx / d;
                let uy = dy / d;
                force[i][0] -= ux * f;
                force[i][1] -= uy * f;
                force[j][0] += ux * f;
                force[j][1] += uy * f;
            }
        }
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

/// Keep the wide node cards out of each other's labels. Two passes of rectangle separation
/// per step, along the axis with the smaller overlap.
fn separate(nodes: &mut [GNode]) -> bool {
    const PAD_X: f32 = 28.0;
    const PAD_Y: f32 = 15.0;
    let mut moved = false;
    for _ in 0..2 {
        for i in 0..nodes.len() {
            for j in i + 1..nodes.len() {
                let dx = nodes[j].pos[0] - nodes[i].pos[0];
                let dy = nodes[j].pos[1] - nodes[i].pos[1];
                let ox = NODE_W + PAD_X - dx.abs();
                let oy = NODE_H + PAD_Y - dy.abs();
                if ox <= 0.0 || oy <= 0.0 {
                    continue;
                }
                let (share_i, share_j) = match (nodes[i].fixed, nodes[j].fixed) {
                    (true, true) => continue,
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
                nodes[i].pos[0] =
                    (nodes[i].pos[0] + ux * amount * share_i).clamp(EDGE, WORLD[0] - EDGE);
                nodes[i].pos[1] =
                    (nodes[i].pos[1] + uy * amount * share_i).clamp(EDGE, WORLD[1] - EDGE);
                nodes[j].pos[0] =
                    (nodes[j].pos[0] - ux * amount * share_j).clamp(EDGE, WORLD[0] - EDGE);
                nodes[j].pos[1] =
                    (nodes[j].pos[1] - uy * amount * share_j).clamp(EDGE, WORLD[1] - EDGE);
                moved = true;
            }
        }
    }
    moved
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
