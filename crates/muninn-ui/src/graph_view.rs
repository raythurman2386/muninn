//! The graph canvas: world, edges, keyed nodes, camera gestures and floating controls.

use rune::{
    Color, Cursor, Drag, El, LineCap, MagnifyEvent, PointerButton, Shape, Spring, Tx, WheelEvent,
    canvas, col, effect, el, icon, row, text, text_input,
};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use muninn_graph::{GNode, NODE_H, NODE_W, WORLD, cards_fit, marker_extent};
use muninn_i18n::{
    Key as TextKey, filter_by_tag, graph_links, graph_node_parent, graph_stats_with_tags,
    note_label_root, notes, t,
};

use crate::settings::GraphEffects;
use crate::{BurnMark, BurnSpace, Scope, State};

/// Neighbour dimming eases on selection, so a click reads as a change of
/// emphasis rather than a repaint. Reduced motion lands it at once.
const HIGHLIGHT_SPRING: Spring = Spring::new(0.24, 0.0);

/// Below this zoom (Global scope only) nodes render as dots instead of
/// labelled chips: at fit-zoom for hundreds of notes full cards are dense,
/// unreadable, and dominate the instance count. The sample vault fits near
/// zoom 0.9 so it keeps chips and its snapshots do not change.
///
/// Zoom is not the only signal (#190): the layout switches to marker spacing
/// as soon as full cards cannot all fit (`cards_fit`), which happens at 111
/// notes. A vault in the 116–118 band fits near 0.61–0.62 — above this zoom —
/// yet the settled layout only guarantees dot spacing, so drawing every card
/// there stacked 141–153 card pairs over a dot-packed world. Global therefore
/// goes dots-only when either the zoom or the density says so.
const LOD_ZOOM: f32 = 0.6;
/// Local has only one hop around the selection, so labels stay useful a
/// little longer; below this only the selection keeps its card.
const LOCAL_LOD_ZOOM: f32 = 0.55;
/// Cap animated electric edge slots on dense graphs. Cold edges are ranked
/// last, so the selection's own edges always get an arc.
const EDGE_FX_BUDGET: usize = 48;
/// Effect-slot mark prefixes, high half only: the low half carries the node id
/// or the slot's rank as a seed. `muninn-app`'s compositor switches on these.
pub const FX_EDGE_COLD: u32 = 0x4d45_0000;
pub const FX_EDGE_HOT: u32 = 0x4d46_0000;
pub const FX_NODE_SEL: u32 = 0x4d47_0000;
pub const FX_NODE_NEAR: u32 = 0x4d48_0000;
/// The delete burn (one at a time, so no id in the low half).
pub const FX_BURN: u32 = 0x4d49_0000;
/// World size of a low-zoom dot marker (screen size is `DOT * zoom`, so
/// about 12px at typical Global fit zoom).
const DOT: f32 = 28.0;
/// Clearance kept between a drawn dot and the packed envelope: a dot may never
/// be wider than the spacing the settled layout guarantees (#190).
const DOT_PACK_PAD: f32 = 1.0;

/// Whether Global or Local draws this frame as dots only.
///
/// Global is density-aware (#190): the layout only guarantees card spacing
/// while `cards_fit`, so above it the view draws the [marker envelope] every
/// zoom. Local has a handful of notes, so its zoom threshold is unchanged.
///
/// [marker envelope]: muninn_graph::marker_extent
fn graph_dots_only(scope: Scope, zoom: f32, visible: usize) -> bool {
    match scope {
        Scope::Global => zoom < LOD_ZOOM || !cards_fit(visible),
        Scope::Local => zoom < LOCAL_LOD_ZOOM,
    }
}

pub fn graph(s: &State, k: &Kit) -> El {
    let visible = s.visible_notes();
    // #294: edges index nodes, so tag-node indices follow the visible notes.
    let visible_nodes = s.visible_nodes();
    let edges = s.graph.visible_edges(&visible_nodes);
    let viewport = viewport_rect(s);
    let dots_only = graph_dots_only(s.scope, s.cam.zoom(), visible_nodes.len());
    let mut world = el()
        .key("graph-world")
        .size(WORLD[0], WORLD[1])
        .pin(Some(0.0), Some(0.0), None, None)
        .live_offset(&s.cam.live_pan)
        .live_scale(&s.cam.live_zoom)
        .child(grid(s, k))
        .child(edge_canvas(s, k, &edges, viewport));
    // A burn in window points (a sidebar row) is painted over the window by
    // `view`, not here: its rect is not in the graph's world.
    if let Some(burn) = s
        .burn
        .as_ref()
        .filter(|burn| burn.alive(s.now) && burn.space == BurnSpace::Graph)
    {
        world = world.child(burn_ghost(s, k, burn));
        world = world.child(burn_effect(s, burn));
    }
    world = nodes_world(world, s, k, &visible, &visible_nodes, dots_only, viewport);

    let mut canvas_el = el()
        .key("graph-canvas")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .clip_soft(0.0)
        .fill(k.c.bg_base)
        .cursor(Cursor::Grab)
        .on_drawn_layout_tx(|s: &mut State, r: [f32; 4], tx: &mut Tx| {
            let size = [r[2], r[3]];
            if (s.canvas[0] - size[0]).abs() > 0.5 || (s.canvas[1] - size[1]).abs() > 0.5 {
                s.canvas = size;
                if s.needs_fit {
                    s.refit();
                }
                tx.instant();
            }
        })
        .on_wheel(|s: &mut State, e: WheelEvent, tx: &mut Tx| {
            s.auto_fit = false;
            if e.mods.shift {
                // Shift+wheel keeps the old pan behaviour (mostly horizontal
                // on a notched mouse wheel); it never zooms.
                s.cam.pan_by(e.dx, e.dy);
            } else {
                // Plain wheel zooms at the pointer, as the hint promises.
                // Ctrl/Cmd/Alt+wheel (and trackpad pinch-as-wheel) take the
                // same branch. Direction follows the vertical delta, falling
                // back to the horizontal one for sideways-only wheels.
                let delta = if e.dy.abs() > f32::EPSILON {
                    e.dy
                } else {
                    e.dx
                };
                let factor = if delta < 0.0 { 1.08 } else { 0.93 };
                s.cam.zoom_at([e.x, e.y], factor);
            }
            tx.live_only();
            true
        })
        .on_magnify(|s: &mut State, e: MagnifyEvent, tx: &mut Tx| {
            s.auto_fit = false;
            s.cam.zoom_at([e.x, e.y], 1.0 + e.delta);
            tx.live_only();
            true
        })
        .pointer_buttons([PointerButton::Primary])
        .on_drag(|s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_canvas(d);
            tx.live_only();
        })
        .on_double_press(|s: &mut State| s.fit_graph())
        .child(world);

    if visible.is_empty() {
        canvas_el = canvas_el.child(
            col()
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .child(
                    text(t(TextKey::GraphPageEmpty))
                        .font(k.ty.ui)
                        .color(k.c.text_faint),
                ),
        );
    }

    canvas_el
        .child(controls(s, k))
        .child(stats(
            s,
            k,
            visible.len(),
            visible_nodes.len() - visible.len(),
            edges.len(),
        ))
        .child(info(s, k))
        .child(hint(k))
}

/// Append the visible nodes (rings first, then cards/dots) to the world.
fn nodes_world(
    mut world: El,
    s: &State,
    k: &Kit,
    visible: &[usize],
    visible_nodes: &[usize],
    dots_only: bool,
    viewport: [f32; 4],
) -> El {
    for &id in visible {
        let Some(node) = s.graph.node(id) else {
            continue;
        };
        // Off-screen cards cost layout instances for nothing; the selection
        // and keyboard focus survive culling so their rings stay meaningful.
        let focused = s.graph_focus == Some(id);
        if is_culled(node.pos, NODE_W, NODE_H, viewport) && s.selected != Some(id) && !focused {
            continue;
        }
        // Global keeps neighbours labelled at low zoom; Local keeps only the
        // selection. Isolate dims every non-neighbour to a dot.
        let as_dot = node_is_dot(s, id, dots_only, focused);
        let (w, h) = node_size(s, id, as_dot);
        if focused {
            world = world.child(focus_ring(k, node.pos, w, h));
        }
        if s.graph_effects() == GraphEffects::Electric {
            if s.selected == Some(id) {
                world = world.child(node_electric(id, node.pos, w, h, true));
            } else if s.selected.is_some() && is_neighbour(s, id) {
                world = world.child(node_electric(id, node.pos, w, h, false));
            }
        }
        world = world.child(if as_dot {
            node_dot(s, k, id, node.pos)
        } else {
            node_button(s, k, id, node.pos)
        });
    }
    // #294: tag nodes follow the notes, under the same declutter rules.
    world = tags_world(world, s, k, visible, visible_nodes, dots_only, viewport);
    world
}

/// #294: draw the tag nodes — the node indices in `visible_nodes` past the
/// note ids — as cards, or as dots where the same rules would dot a note
/// down. A tag the selected note carries keeps its card; every tag still
/// filters on a click and drags by index.
fn tags_world(
    mut world: El,
    s: &State,
    k: &Kit,
    visible: &[usize],
    visible_nodes: &[usize],
    dots_only: bool,
    viewport: [f32; 4],
) -> El {
    for &index in visible_nodes.iter().skip(visible.len()) {
        let Some(node) = s.graph.node_at(index) else {
            continue;
        };
        let Some(tag) = node.kind.tag() else {
            continue;
        };
        if is_culled(node.pos, NODE_W, NODE_H, viewport) {
            continue;
        }
        // #294: the declutter that dots a note dots a tag too — a zoomed-out
        // graph, or an isolated one whose selection does not carry the tag.
        let carried = s.selected_note_carries(tag);
        if (dots_only || (s.graph_isolate && s.selected.is_some())) && !carried {
            world = world.child(tag_dot(s, k, index, node));
        } else {
            world = world.child(tag_node(k, index, node));
        }
    }
    world
}

fn grid(s: &State, k: &Kit) -> El {
    rune::memo("graph-grid", crate::omarchy::kit_id(s, k.dark), move || {
        let color = k.alpha(k.c.text_base, 0.055);
        let mut shape = Shape::new();
        let mut y = 24.0;
        while y < WORLD[1] {
            let mut x = 24.0;
            while x < WORLD[0] {
                shape = shape.line([x, y], [x + 0.01, y], 2.2, LineCap::Round, color);
                x += 48.0;
            }
            y += 48.0;
        }
        canvas(shape)
            .size(WORLD[0], WORLD[1])
            .pin(Some(0.0), Some(0.0), None, None)
    })
}

fn edge_canvas(s: &State, k: &Kit, edges: &[(usize, usize)], viewport: [f32; 4]) -> El {
    let base = k.alpha(k.c.text_muted, 0.55);
    let hot = k.alpha(k.c.text_accent, 0.72);
    // Arcs only where the cards themselves are drawn: below LOD the graph is a
    // field of dots and a web of lightning would swallow it.
    let electric = s.graph_effects() == GraphEffects::Electric && s.cam.zoom() >= LOD_ZOOM;
    let mut shape = Shape::new();
    let mut fx_slots = Vec::new();
    // Prefer edges that touch the selection when budgeting FX layers, so the
    // arcs the user is looking at are the ones that survive the cap.
    let mut ranked: Vec<(bool, usize, usize)> = edges
        .iter()
        .copied()
        .map(|(a, b)| {
            let touches = s.selected == Some(a) || s.selected == Some(b);
            (touches, a, b)
        })
        .collect();
    ranked.sort_by_key(|a| std::cmp::Reverse(a.0));

    let mut fx_used = 0usize;
    for (touches, a, b) in ranked {
        // #294: edges hold node indices, including tag-node indices, so their
        // endpoints are read by index rather than by note id.
        let (Some(from), Some(to)) = (s.graph.node_at(a), s.graph.node_at(b)) else {
            continue;
        };
        // Skip segments with both ends off-screen; Rune clips the rest.
        if is_culled(from.pos, 1.0, 1.0, viewport) && is_culled(to.pos, 1.0, 1.0, viewport) {
            continue;
        }
        let arc = electric && fx_used < EDGE_FX_BUDGET;
        // An edge that gets a lightning arc does not also draw its plain line:
        // the arc is the edge, and the line underneath would read as a ghost
        // behind it. Edges past the FX budget keep their line so they stay.
        if !arc {
            shape = shape.line(
                from.pos,
                to.pos,
                if touches { 1.7 } else { 1.15 },
                LineCap::Round,
                if touches { hot } else { base },
            );
        }

        if arc {
            // The arc is drawn in the slot's local space: `effect_data` carries
            // the two endpoints as 0..1 fractions of the slot rect.
            let minx = from.pos[0].min(to.pos[0]);
            let miny = from.pos[1].min(to.pos[1]);
            let w = (from.pos[0] - to.pos[0]).abs().max(2.0);
            let h = (from.pos[1] - to.pos[1]).abs().max(2.0);
            let data = [
                (from.pos[0] - minx) / w,
                (from.pos[1] - miny) / h,
                (to.pos[0] - minx) / w,
                (to.pos[1] - miny) / h,
            ];
            // A stable per-edge seed: the bolt's path and strike timing must
            // not re-roll when a selection re-ranks the edges (hot-first) and
            // shifts every rank. Hashing the node pair keeps it fixed while the
            // selection changes only the hot/cold colour, not the shape.
            let lo = a.min(b);
            let hi = a.max(b);
            let seed = lo.wrapping_mul(0x9e37_79b9) ^ hi.wrapping_mul(0x85eb_ca6b);
            fx_slots.push(
                effect(if touches {
                    FX_EDGE_HOT | (seed as u32 & 0xffff)
                } else {
                    FX_EDGE_COLD | (seed as u32 & 0xffff)
                })
                .effect_data(data)
                .effect_bleed(10.0)
                .pin(Some(minx), Some(miny), None, None)
                .size(w, h),
            );
            fx_used += 1;
        }
    }

    let mut canvas_el =
        canvas(shape)
            .size(WORLD[0], WORLD[1])
            .pin(Some(0.0), Some(0.0), None, None);
    for slot in fx_slots {
        canvas_el = canvas_el.child(slot);
    }
    canvas_el
}

/// World-space rectangle currently on screen, expanded so cards straddling
/// the edge are kept.
fn viewport_rect(s: &State) -> [f32; 4] {
    let a = s.cam.world([0.0, 0.0]);
    let b = s.cam.world(s.canvas);
    let m = NODE_W;
    [
        a[0].min(b[0]) - m,
        a[1].min(b[1]) - m,
        a[0].max(b[0]) + m,
        a[1].max(b[1]) + m,
    ]
}

fn is_culled(pos: [f32; 2], w: f32, h: f32, viewport: [f32; 4]) -> bool {
    pos[0] + w / 2.0 < viewport[0]
        || pos[0] - w / 2.0 > viewport[2]
        || pos[1] + h / 2.0 < viewport[1]
        || pos[1] - h / 2.0 > viewport[3]
}

/// True when `id` is the selection or a direct neighbour of it.
fn is_neighbour(s: &State, id: usize) -> bool {
    s.selected.is_none_or(|sel| {
        sel == id
            || s.vault.notes[sel].links.contains(&id)
            || s.vault.notes[sel].backlinks.contains(&id)
    })
}

fn degree(s: &State, id: usize) -> usize {
    let note = &s.vault.notes[id];
    note.links.len() + note.backlinks.len()
}

/// Hub cards grow to 1.18×. The layout's separation envelope keeps 28pt of
/// horizontal and 15pt of vertical slack, so the largest card still fits.
fn card_scale(degree: usize) -> f32 {
    1.0 + (degree.min(9) as f32) * 0.02
}

fn card_size(degree: usize) -> (f32, f32) {
    let scale = card_scale(degree);
    (NODE_W * scale, NODE_H * scale)
}

/// A dot's world size: the degree-scaled marker, capped at the envelope the
/// layout packed to ([`marker_extent`]) minus a small pad, so no zoom can draw
/// a dot wider than its neighbours are spaced. Below card country the envelope
/// is the full 45 pt, so ordinary dots are unchanged.
fn dot_size(n: usize, degree: usize) -> f32 {
    let wanted = DOT * (1.0 + (degree.min(12) as f32) * 0.05);
    let cap = (marker_extent(n) - DOT_PACK_PAD).max(1.0);
    wanted.min(cap)
}

/// Whether the node is drawn as a low-zoom dot rather than a card this frame.
fn node_is_dot(s: &State, id: usize, dots_only: bool, focused: bool) -> bool {
    let labelled =
        !dots_only || s.selected == Some(id) || (s.scope == Scope::Global && is_neighbour(s, id));
    let isolated = s.graph_isolate && s.selected.is_some() && !is_neighbour(s, id) && !focused;
    !labelled || isolated
}

/// The footprint the node is drawn at: a dot below the scope's LOD, a scaled
/// card otherwise.
fn node_size(s: &State, id: usize, as_dot: bool) -> (f32, f32) {
    if as_dot {
        let side = dot_size(s.graph.nodes.len(), degree(s, id));
        (side, side)
    } else {
        card_size(degree(s, id))
    }
}

/// The node's world centre and size as it is drawn right now.
///
/// The delete burn needs this before [`State::confirm_delete`] reloads the
/// vault (which drops the node and rebuilds every id), so it lives here rather
/// than in the delete path: the same LOD and card-size rules decide both. The
/// #303 regression test reads it the same way, so a footprint assertion can
/// never drift from what the frame draws.
pub fn node_footprint(s: &State, id: usize) -> Option<([f32; 2], [f32; 2])> {
    let node = s.graph.node(id)?;
    let dots_only = graph_dots_only(s.scope, s.cam.zoom(), s.visible_notes().len());
    let focused = s.graph_focus == Some(id);
    let as_dot = node_is_dot(s, id, dots_only, focused);
    let (w, h) = node_size(s, id, as_dot);
    Some((node.pos, [w, h]))
}

/// Electric border around a node: `hot` marks the selection, direct
/// neighbours get the calmer charged variant. The compositor paints both.
fn node_electric(id: usize, pos: [f32; 2], w: f32, h: f32, hot: bool) -> El {
    let pad = if hot { 10.0 } else { 6.0 };
    let mark = (if hot { FX_NODE_SEL } else { FX_NODE_NEAR }) | (id as u32 & 0xffff);
    effect(mark)
        .effect_data([if hot { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0])
        .effect_bleed(if hot { 14.0 } else { 10.0 })
        .pin(
            Some(pos[0] - w / 2.0 - pad),
            Some(pos[1] - h / 2.0 - pad),
            None,
            None,
        )
        .size(w + pad * 2.0, h + pad * 2.0)
}

/// The deleted note's card or row, fading under the burn. It is a plain
/// element, not an effect: the fire is [`burn_effect`]'s job, and this keeps
/// the node's or row's place readable for the moment it takes to go.
///
/// Pinned in the burn's own space: the graph world for a node, the window for a
/// sidebar row (#135). Callers place it in that space.
pub(crate) fn burn_ghost(s: &State, k: &Kit, burn: &BurnMark) -> El {
    let fade = 1.0 - burn.progress(s.now).powi(2);
    let [w, h] = burn.size;
    let [x, y] = burn.pos;
    row()
        .key("burn-ghost")
        .pin(Some(x - w / 2.0), Some(y - h / 2.0), None, None)
        .size(w, h)
        .px(9.0)
        .gap(7.0)
        .center_y()
        .radius(9.0)
        .fill(k.alpha(k.c.layer_02, 0.5 * fade))
        .border(1.0, k.alpha(k.c.border_base, 0.35 * fade))
        .child(
            text(burn.label.clone())
                .font(rune::TextStyle::new(12.0, 500.0))
                .color(k.alpha(k.c.text_base, fade))
                .truncate()
                .grow(1.0),
        )
}

/// The fire slot over the deleted node or row. `effect_data[0]` is the burn's
/// progress, which cools and fades the embers in the compositor.
///
/// The compositor paints `El::effect` slots anywhere in the tree, so the same
/// slot works over the graph world and over a sidebar row.
pub(crate) fn burn_effect(s: &State, burn: &BurnMark) -> El {
    let [w, h] = burn.size;
    let [x, y] = burn.pos;
    let progress = burn.progress(s.now);
    effect(FX_BURN)
        .effect_data([progress, 0.0, 0.0, 0.0])
        .effect_bleed(16.0)
        .pin(Some(x - w / 2.0 - 6.0), Some(y - h / 2.0 - 6.0), None, None)
        .size(w + 12.0, h + 12.0)
}

/// GRP-02: an outline outside the card, distinct from the selection's filled
/// accent card and shadow. Non-interactive; the card keeps its own gestures.
fn focus_ring(k: &Kit, pos: [f32; 2], w: f32, h: f32) -> El {
    const INSET: f32 = 3.0;
    el().key("graph-focus-ring")
        .pin(
            Some(pos[0] - w / 2.0 - INSET),
            Some(pos[1] - h / 2.0 - INSET),
            None,
            None,
        )
        .size(w + INSET * 2.0, h + INSET * 2.0)
        .radius(12.0)
        .fill(Color::TRANSPARENT)
        .border(1.5, k.c.text_accent)
}

fn node_button(s: &State, k: &Kit, id: usize, pos: [f32; 2]) -> El {
    let note = &s.vault.notes[id];
    let selected = s.selected == Some(id);
    let dim = s.selected.is_some() && !is_neighbour(s, id);
    let accent = k.c.text_accent;
    let fill = if selected {
        k.c.layer_02.mix(accent, 0.18)
    } else if dim {
        k.c.layer_01
    } else {
        k.c.layer_02
    };
    let border = if selected {
        accent
    } else if dim {
        k.c.border_muted
    } else {
        k.c.border_base
    };
    let ink = if selected {
        accent
    } else if dim {
        k.c.text_faint
    } else {
        k.c.text_base
    };
    let degree = degree(s, id);
    let dot = if degree == 0 {
        k.c.text_faint
    } else if selected {
        accent
    } else {
        k.alpha(accent, if dim { 0.25 } else { 0.7 })
    };
    // Duplicate titles share a graph label; the parent keeps the node
    // readable in the 136pt card (full path is in the info card below).
    let label = if s.vault.is_duplicate(id) {
        let parent = note.parent();
        if parent.is_empty() {
            note_label_root(&note.title)
        } else {
            graph_node_parent(&note.title, parent)
        }
    } else {
        note.title.clone()
    };
    let (w, h) = card_size(degree);
    let scale = card_scale(degree);
    let marker = 6.0 * scale;
    let mut node = row()
        .key(format!("graph-node-{id}"))
        .pin(Some(pos[0] - w / 2.0), Some(pos[1] - h / 2.0), None, None)
        .size(w, h)
        .px(9.0)
        .gap(7.0)
        .radius(9.0 * scale)
        .center_y()
        .fill(fill)
        .border(1.0, border)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.select_graph_node(id);
            graph_press_motion(s, tx);
        })
        .on_double_press(move |s: &mut State| s.open_note(id))
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node(id, d))
        .child(el().size(marker, marker).radius(marker / 2.0).fill(dot))
        .child(
            text(label)
                .font(rune::TextStyle::new(12.0 + (scale - 1.0) * 8.0, 500.0))
                .color(ink)
                .truncate()
                .grow(1.0),
        );
    node = if selected {
        node.shadow(0.0, 12.0, k.alpha(accent, 0.28))
    } else {
        node.shadow(1.0, 4.0, k.alpha(k.c.black, 0.3))
    };
    node
}

/// Low-zoom marker: same key and gestures as [`node_button`], one filled
/// circle instead of a labelled card. Colour keeps the chip's degree and
/// dim encoding (isolated faint, linked accent, dimmed faint border).
fn node_dot(s: &State, k: &Kit, id: usize, pos: [f32; 2]) -> El {
    let dim = s.selected.is_some() && !is_neighbour(s, id);
    let accent = k.c.text_accent;
    let degree = degree(s, id);
    let fill = if degree == 0 {
        k.c.text_faint
    } else if dim {
        k.alpha(accent, 0.25)
    } else {
        k.alpha(accent, 0.7)
    };
    let side = dot_size(s.graph.nodes.len(), degree);
    el().key(format!("graph-node-{id}"))
        .pin(
            Some(pos[0] - side / 2.0),
            Some(pos[1] - side / 2.0),
            None,
            None,
        )
        .size(side, side)
        .radius(side / 2.0)
        .fill(fill)
        .border(
            1.0,
            if dim {
                k.c.border_muted
            } else {
                k.c.border_base
            },
        )
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.select_graph_node(id);
            graph_press_motion(s, tx);
        })
        .on_double_press(move |s: &mut State| s.open_note(id))
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node(id, d))
}

/// #294: a tag node. Same card footprint and gestures as a note node, but its
/// label is `#tag`, a click filters the sidebar instead of loading a note, and
/// hovering it puts the tag and its note count in the info card. There is no
/// double-press (nothing to open) and it takes no keyboard focus.
fn tag_node(k: &Kit, index: usize, node: &GNode) -> El {
    let tag = node
        .kind
        .tag()
        .expect("a tag node carries its tag in its kind");
    let filter = tag.to_owned();
    let hover = filter.clone();
    row()
        .key(format!("graph-node-{index}"))
        .pin(
            Some(node.pos[0] - NODE_W / 2.0),
            Some(node.pos[1] - NODE_H / 2.0),
            None,
            None,
        )
        .size(NODE_W, NODE_H)
        .px(9.0)
        .gap(7.0)
        .radius(9.0)
        .center_y()
        .fill(k.alpha(k.c.text_accent, 0.12))
        .border(1.0, k.alpha(k.c.text_accent, 0.5))
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(filter_by_tag(tag))
        .on_hover_change(move |s: &mut State, on: bool, _: &mut Tx| {
            if on {
                s.graph_tag_hover = Some(hover.clone());
            } else if s.graph_tag_hover.as_deref() == Some(hover.as_str()) {
                s.graph_tag_hover = None;
            }
        })
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.set_note_tag_filter(&filter);
            graph_press_motion(s, tx);
        })
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node_at(index, d))
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.text_accent))
        .child(
            text(format!("#{tag}"))
                .font(rune::TextStyle::new(12.0, 500.0))
                .color(k.c.text_accent)
                .truncate()
                .grow(1.0),
        )
}

/// #294: a tag node drawn as a dot, the same treatment a note gets when the
/// graph is zoomed out or isolated. It still filters on a click and still
/// drags by index; it takes no keyboard focus and shows no hover card.
fn tag_dot(s: &State, k: &Kit, index: usize, node: &GNode) -> El {
    let tag = node
        .kind
        .tag()
        .expect("a tag node carries its tag in its kind")
        .to_owned();
    // Like `node_dot`: a tag the selection does not carry dims down.
    let dim = s.selected.is_some() && !s.selected_note_carries(&tag);
    let accent = k.c.text_accent;
    let degree = s.vault.notes_with_tag(&tag).len();
    let side = dot_size(s.graph.nodes.len(), degree);
    let filter = tag;
    el().key(format!("graph-node-{index}"))
        .pin(
            Some(node.pos[0] - side / 2.0),
            Some(node.pos[1] - side / 2.0),
            None,
            None,
        )
        .size(side, side)
        .radius(side / 2.0)
        .fill(if dim {
            k.alpha(accent, 0.25)
        } else {
            k.alpha(accent, 0.7)
        })
        .border(
            1.0,
            if dim {
                k.c.border_muted
            } else {
                k.c.border_base
            },
        )
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.set_note_tag_filter(&filter);
            graph_press_motion(s, tx);
        })
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node_at(index, d))
}

/// Colour spring for a graph press. Reduced motion lands the dim/selection
/// change in the same frame, matching the camera's snap.
fn graph_press_motion(s: &State, tx: &mut Tx) {
    tx.color_motion(HIGHLIGHT_SPRING);
    if s.reduced_motion() {
        tx.instant();
    }
}

fn controls(s: &State, k: &Kit) -> El {
    row()
        .pin(Some(14.0), Some(14.0), None, None)
        .gap(6.0)
        .child(scope_switch(s, k))
        .child(graph_search(s, k))
        .child(chip(
            k,
            t(TextKey::GraphActionFit),
            &ic::EXPAND,
            "graph-fit",
            false,
            |s: &mut State| s.fit_graph(),
        ))
        .child(chip(
            k,
            t(TextKey::GraphActionReset),
            &ic::REFRESH,
            "graph-reset",
            false,
            |s: &mut State| s.reset_graph(),
        ))
        .child(chip(
            k,
            t(TextKey::GraphActionIsolate),
            &ic::SHARE,
            "graph-isolate",
            s.graph_isolate,
            |s: &mut State| s.toggle_graph_isolate(),
        ))
}

/// GRP-03: type a title or path, Enter selects the best match and centres it.
fn graph_search(s: &State, k: &Kit) -> El {
    row()
        .key("graph-search-box")
        .h(30.0)
        .w(174.0)
        .px(8.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(
            1.0,
            if s.graph_query.focused {
                k.c.border_focus
            } else {
                k.c.border_base
            },
        )
        .cursor(Cursor::Text)
        .child(
            icon(&ic::MAGNIFYING_GLASS, 12.0).color(if s.graph_query.focused {
                k.c.text_accent
            } else {
                k.c.icon_muted
            }),
        )
        .child(
            text_input(&s.graph_query, k.ty.ui_compact)
                .key("graph-search")
                .label(t(TextKey::GraphPageFindNode))
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder(t(TextKey::GraphPageFindPlaceholder), k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.graph_query)
                .on_submit(|s: &mut State, tx: &mut Tx| {
                    s.reveal_graph_search();
                    graph_press_motion(s, tx);
                    // Return keys to the canvas so arrows/I work immediately.
                    tx.blur();
                }),
        )
}

fn scope_switch(s: &State, k: &Kit) -> El {
    row()
        .h(30.0)
        .padding(3.0)
        .gap(2.0)
        .radius(7.0)
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(1.0, k.c.border_base)
        .child(scope_button(
            k,
            t(TextKey::GraphScopeGlobal),
            Scope::Global,
            s.scope,
            "scope-global",
        ))
        .child(scope_button(
            k,
            t(TextKey::GraphScopeLocal),
            Scope::Local,
            s.scope,
            "scope-local",
        ))
}

fn scope_button(k: &Kit, label: &str, scope: Scope, active: Scope, key: &str) -> El {
    let on = scope == active;
    row()
        .key(key)
        .h(24.0)
        .px(9.0)
        .radius(5.0)
        .center_y()
        .fill(if on { k.c.layer_03 } else { Color::TRANSPARENT })
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.set_scope(scope);
            graph_press_motion(s, tx);
        })
        .child(text(label).font(k.ty.small_medium).color(if on {
            k.c.text_base
        } else {
            k.c.text_muted
        }))
}

/// A control chip. `on` is the toggle state; plain chips pass false.
fn chip(
    k: &Kit,
    label: &str,
    glyph: &'static rune::Icon,
    key: &str,
    on: bool,
    press: impl Fn(&mut State) + 'static,
) -> El {
    row()
        .key(key)
        .h(30.0)
        .px(9.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .fill(if on {
            k.c.layer_03
        } else {
            k.alpha(k.c.layer_02, 0.96)
        })
        .border(1.0, if on { k.c.text_accent } else { k.c.border_base })
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press(press)
        .child(icon(glyph, 12.0).color(if on { k.c.text_accent } else { k.c.icon_muted }))
        .child(text(label).font(k.ty.small_medium).color(if on {
            k.c.text_base
        } else {
            k.c.text_muted
        }))
}

fn stats(_s: &State, k: &Kit, notes: usize, tags: usize, edges: usize) -> El {
    row()
        .pin(None, Some(14.0), Some(14.0), None)
        .h(30.0)
        .px(10.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.94))
        .border(1.0, k.c.border_base)
        .child(icon(&ic::SHARE, 12.0).color(k.c.icon_faint))
        .child(
            text(graph_stats_with_tags(notes, tags, edges))
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
}

/// #294: hovering a tag node replaces the note info card's links count with
/// the tag and the number of notes carrying it.
fn tag_info(k: &Kit, tag: &str, count: usize) -> El {
    col()
        .key("graph-info")
        .pin(Some(14.0), None, None, Some(14.0))
        .w(248.0)
        .px(13.0)
        .py(11.0)
        .gap(5.0)
        .radius(10.0)
        .fill(k.alpha(k.c.layer_02, 0.97))
        .border(1.0, k.c.border_base)
        .shadow(10.0, 26.0, k.alpha(k.c.black, 0.34))
        .child(
            text(format!("#{tag}"))
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .truncate(),
        )
        .child(
            text(notes(count))
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(
            text(filter_by_tag(tag))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

fn info(s: &State, k: &Kit) -> El {
    if let Some(tag) = &s.graph_tag_hover {
        return tag_info(k, tag, s.vault.notes_with_tag(tag).len());
    }
    let Some(id) = s.selected else { return el() };
    let note = &s.vault.notes[id];
    let stats = graph_links(note.links.len(), note.backlinks.len());
    col()
        .key("graph-info")
        .pin(Some(14.0), None, None, Some(14.0))
        .w(248.0)
        .px(13.0)
        .py(11.0)
        .gap(5.0)
        .radius(10.0)
        .fill(k.alpha(k.c.layer_02, 0.97))
        .border(1.0, k.c.border_base)
        .shadow(10.0, 26.0, k.alpha(k.c.black, 0.34))
        .child(
            text(t(TextKey::GraphPageSelected))
                .font(rune::TextStyle::new(9.0, 650.0).tracking(0.9))
                .color(k.c.text_faint),
        )
        .child(
            text(note.title.clone())
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .truncate(),
        )
        .child(
            text(note.rel.clone())
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        )
        .child(text(stats).font(k.ty.caption()).color(k.c.text_muted))
        .child(
            row()
                .mt(4.0)
                .gap(6.0)
                .child(chip(
                    k,
                    t(TextKey::GraphActionOpenNote),
                    &ic::OPEN_FILE,
                    "graph-open-note",
                    false,
                    move |s: &mut State| s.open_note(id),
                ))
                .child(if s.scope == Scope::Local {
                    chip(
                        k,
                        t(TextKey::GraphScopeGlobal),
                        &ic::SHARE,
                        "graph-scope",
                        false,
                        move |s: &mut State| s.set_scope(Scope::Global),
                    )
                } else {
                    chip(
                        k,
                        t(TextKey::GraphScopeLocal),
                        &ic::SHARE,
                        "graph-scope",
                        false,
                        move |s: &mut State| s.set_scope(Scope::Local),
                    )
                }),
        )
}

fn hint(k: &Kit) -> El {
    row()
        .pin(None, None, Some(14.0), Some(14.0))
        .h(26.0)
        .px(9.0)
        .radius(6.0)
        .center_y()
        .fill(k.alpha(k.c.bg_deep, 0.78))
        .child(
            text(t(TextKey::GraphPageHint))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    use muninn_core::testing::linked_vault;

    /// #190: the drawn dot must never be wider than the layout's packed
    /// envelope, so the view cannot show dots overlapping a clean layout.
    #[test]
    fn a_dot_never_exceeds_the_packed_envelope() {
        for n in [15usize, 90, 405, 600, 1000, 2000] {
            let envelope = marker_extent(n);
            for degree in [0usize, 1, 4, 9, 12, 40] {
                let side = dot_size(n, degree);
                assert!(
                    side <= envelope - DOT_PACK_PAD + 1e-4,
                    "{n} notes, degree {degree}: dot {side:.2} exceeds envelope {envelope:.2}"
                );
            }
        }
        // Below card country the envelope is the full 45 pt marker, so an
        // ordinary dot keeps its long-standing 28 pt size.
        assert_eq!(dot_size(15, 0), DOT);
    }

    /// #190 slice 2: Global LOD is density-aware, not just zoom-aware. The
    /// 116–118 band fits at 0.607/0.623 — above the 0.6 zoom cliff — while the
    /// layout is already in marker country, so the old rule drew every card
    /// over dot spacing (141/153 overlapping pairs). Local is unchanged.
    #[test]
    fn global_lod_flips_on_density_before_the_zoom_cliff() {
        for (n, fit) in [(116usize, 0.607f32), (118, 0.623)] {
            assert!(!cards_fit(n), "{n} notes are marker country");
            assert!(fit >= LOD_ZOOM, "{n} fits above the zoom cliff at {fit}");
            assert!(
                graph_dots_only(Scope::Global, fit, n),
                "{n} notes at fit zoom {fit} must draw dots"
            );
            assert!(
                graph_dots_only(Scope::Global, 1.5, n),
                "density alone must flip {n} notes to dots, at any zoom"
            );
        }
        // 90 notes still have room for cards, and its ~0.602 fit zoom keeps
        // them; the zoom rule still applies below the cliff.
        assert!(cards_fit(90));
        assert!(!graph_dots_only(Scope::Global, 0.602, 90));
        assert!(graph_dots_only(Scope::Global, 0.599, 90));

        // Local keeps its old zoom-only rule.
        assert!(!graph_dots_only(Scope::Local, 0.607, 116));
        assert!(graph_dots_only(Scope::Local, 0.549, 116));
    }

    /// Run the graph to the app's settled state (30 quiet frames).
    ///
    /// The budget is deliberately far above the ~120 frames Linux needs: the
    /// settle path is platform-sensitive (Windows needs more frames, and the
    /// layout crate already allows 8k–12k in its own long-running tests). The
    /// test still stops at the first 30 quiet frames, so a fast platform pays
    /// nothing for the headroom.
    fn settle(g: &mut muninn_graph::Graph) {
        let mut quiet = 0;
        for _ in 0..12_000 {
            if g.step() {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet == 30 {
                    return;
                }
            }
        }
        panic!("layout must settle within 12,000 frames");
    }

    /// A graph state for `n` notes, settled and in Global scope.
    fn settled_state(tmp: &muninn_core::testing::TempVault) -> State {
        let state_home = tmp.path().with_extension("state");
        let mut s = State::with_state_home(tmp.path().to_path_buf(), state_home);
        // Above 100 notes the startup scan is chunked (as in the app); finish
        // it before reading the vault.
        while s.loading() {
            s.scan_tick();
        }
        s.scope = Scope::Global;
        s.graph_isolate = false;
        settle(&mut s.graph);
        s
    }

    /// Whether two axis-aligned footprints around node centres overlap.
    fn footprints_overlap(a: ([f32; 2], [f32; 2]), b: ([f32; 2], [f32; 2])) -> bool {
        (a.0[0] - b.0[0]).abs() < (a.1[0] + b.1[0]) / 2.0
            && (a.0[1] - b.0[1]).abs() < (a.1[1] + b.1[1]) / 2.0
    }

    /// The drawn footprints of `ids` as `node_footprint` would draw them.
    fn footprints(s: &State, ids: &[usize]) -> Vec<([f32; 2], [f32; 2])> {
        ids.iter()
            .map(|&id| node_footprint(s, id).expect("a visible node has a footprint"))
            .collect()
    }

    fn assert_no_overlaps(what: &str, shapes: &[([f32; 2], [f32; 2])]) {
        for i in 0..shapes.len() {
            for j in i + 1..shapes.len() {
                assert!(
                    !footprints_overlap(shapes[i], shapes[j]),
                    "{what}: footprints {:?} and {:?} overlap",
                    shapes[i],
                    shapes[j]
                );
            }
        }
    }

    /// #190 slice 2: in the 116–118 band only the selected neighbourhood keeps
    /// a card; every other visible note is a dot, and no two drawn dots overlap
    /// at fit zoom.
    #[test]
    fn the_116_to_118_band_draws_only_the_selected_neighbourhood() {
        for n in [116usize, 118] {
            let tmp = linked_vault(n);
            let mut s = settled_state(&tmp);
            let selected = s.vault.notes[0].id;
            s.selected = Some(selected);
            // The reported fit zooms for the band (issue #190 measurement).
            s.cam.set([0.0, 0.0], if n == 116 { 0.607 } else { 0.623 });
            let visible = s.visible_notes();
            let dots_only = graph_dots_only(s.scope, s.cam.zoom(), visible.len());
            assert!(dots_only, "{n} notes at fit zoom must draw dots");
            let mut cards = Vec::new();
            let mut dots = Vec::new();
            for &id in &visible {
                if node_is_dot(&s, id, dots_only, false) {
                    dots.push(id);
                } else {
                    cards.push(id);
                }
            }
            assert!(
                cards.len() <= 10,
                "{n} notes draw {} cards; only the selected neighbourhood (≤10) may",
                cards.len()
            );
            assert!(cards.contains(&selected), "the selection keeps its card");
            assert_eq!(
                cards.len() + dots.len(),
                visible.len(),
                "every visible note is drawn exactly once"
            );
            assert!(
                cards.len() < visible.len(),
                "{n} notes must not draw every card"
            );
            // Dots must not overlap: the layout packs at the marker envelope.
            // The few neighbourhood cards may still overlap each other — card
            // space is only reserved below `cards_fit`, and reserving it at
            // every density is an explicit non-goal of #190.
            assert_no_overlaps(&format!("{n} dots"), &footprints(&s, &dots));
        }
    }

    /// #190 slice 2: 90 notes are still card country, so Global draws all 90
    /// cards at its fit zoom and the packed layout keeps them apart.
    #[test]
    fn a_90_note_global_view_draws_every_card_without_overlap() {
        let tmp = linked_vault(90);
        let mut s = settled_state(&tmp);
        s.selected = None;
        s.cam.set([0.0, 0.0], 0.602);
        let visible = s.visible_notes();
        let dots_only = graph_dots_only(s.scope, s.cam.zoom(), visible.len());
        assert!(!dots_only, "90 notes keep cards at fit zoom");
        for &id in &visible {
            assert!(
                !node_is_dot(&s, id, dots_only, false),
                "no node is a dot at 90 notes"
            );
        }
        assert_eq!(visible.len(), 90);
        assert_no_overlaps("90 cards", &footprints(&s, &visible));
    }
}
