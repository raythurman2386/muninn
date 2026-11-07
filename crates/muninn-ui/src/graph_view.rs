//! The graph canvas: world, edges, keyed nodes, camera gestures and floating controls.

use rune::{
    Color, Cursor, Drag, El, LineCap, MagnifyEvent, PointerButton, Shape, Tx, WheelEvent, canvas,
    col, el, icon, row, text, text_input,
};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use muninn_graph::{NODE_H, NODE_W, WORLD};

use crate::{Scope, State};

/// Below this zoom (Global scope only) nodes render as dots instead of
/// labelled chips: at fit-zoom for hundreds of notes full cards are dense,
/// unreadable, and dominate the instance count. The sample vault fits near
/// zoom 0.9 so it keeps chips and its snapshots do not change.
const LOD_ZOOM: f32 = 0.6;
/// Local has only one hop around the selection, so labels stay useful a
/// little longer; below this only the selection keeps its card.
const LOCAL_LOD_ZOOM: f32 = 0.55;
/// World size of a low-zoom dot marker (screen size is `DOT * zoom`, so
/// about 12px at typical Global fit zoom).
const DOT: f32 = 28.0;

pub fn graph(s: &State, k: &Kit) -> El {
    let visible = s.visible_notes();
    let edges = s.graph.visible_edges(&visible);
    let viewport = viewport_rect(s);
    let dots_only = match s.scope {
        Scope::Global => s.cam.zoom() < LOD_ZOOM,
        Scope::Local => s.cam.zoom() < LOCAL_LOD_ZOOM,
    };
    let mut world = el()
        .key("graph-world")
        .size(WORLD[0], WORLD[1])
        .pin(Some(0.0), Some(0.0), None, None)
        .live_offset(&s.cam.live_pan)
        .live_scale(&s.cam.live_zoom)
        .child(grid(k))
        .child(edge_canvas(s, k, &edges, viewport));
    world = nodes_world(world, s, k, &visible, dots_only, viewport);

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
                .child(text("No notes to draw").font(k.ty.ui).color(k.c.text_faint)),
        );
    }

    canvas_el
        .child(controls(s, k))
        .child(stats(s, k, visible.len(), edges.len()))
        .child(info(s, k))
        .child(hint(k))
}

/// Append the visible nodes (rings first, then cards/dots) to the world.
fn nodes_world(
    mut world: El,
    s: &State,
    k: &Kit,
    visible: &[usize],
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
        let labelled = !dots_only
            || s.selected == Some(id)
            || (s.scope == Scope::Global && is_neighbour(s, id));
        let isolated = s.graph_isolate && s.selected.is_some() && !is_neighbour(s, id) && !focused;
        let as_dot = !labelled || isolated;
        if focused {
            let (w, h) = if as_dot {
                let side = dot_size(degree(s, id));
                (side, side)
            } else {
                card_size(degree(s, id))
            };
            world = world.child(focus_ring(k, node.pos, w, h));
        }
        world = world.child(if as_dot {
            node_dot(s, k, id, node.pos)
        } else {
            node_button(s, k, id, node.pos)
        });
    }
    world
}

fn grid(k: &Kit) -> El {
    rune::memo("graph-grid", k.dark, move || {
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
    let mut shape = Shape::new();
    for &(a, b) in edges {
        let (Some(from), Some(to)) = (s.graph.node(a), s.graph.node(b)) else {
            continue;
        };
        // Skip segments with both ends off-screen; Rune clips the rest.
        if is_culled(from.pos, 1.0, 1.0, viewport) && is_culled(to.pos, 1.0, 1.0, viewport) {
            continue;
        }
        let touches = s.selected == Some(a) || s.selected == Some(b);
        shape = shape.line(
            from.pos,
            to.pos,
            if touches { 1.7 } else { 1.15 },
            LineCap::Round,
            if touches { hot } else { base },
        );
    }
    canvas(shape)
        .size(WORLD[0], WORLD[1])
        .pin(Some(0.0), Some(0.0), None, None)
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

fn dot_size(degree: usize) -> f32 {
    DOT * (1.0 + (degree.min(12) as f32) * 0.05)
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
            format!("{} (root)", note.title)
        } else {
            format!("{} ({parent})", note.title)
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
        .on_press(move |s: &mut State| s.select_graph_node(id))
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
    let side = dot_size(degree);
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
        .on_press(move |s: &mut State| s.select_graph_node(id))
        .on_double_press(move |s: &mut State| s.open_note(id))
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node(id, d))
}

fn controls(s: &State, k: &Kit) -> El {
    row()
        .pin(Some(14.0), Some(14.0), None, None)
        .gap(6.0)
        .child(scope_switch(s, k))
        .child(graph_search(s, k))
        .child(chip(
            k,
            "Fit",
            &ic::EXPAND,
            "graph-fit",
            false,
            |s: &mut State| s.fit_graph(),
        ))
        .child(chip(
            k,
            "Reset",
            &ic::REFRESH,
            "graph-reset",
            false,
            |s: &mut State| s.reset_graph(),
        ))
        .child(chip(
            k,
            "Isolate",
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
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("Find node · Enter", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.graph_query)
                .on_submit(|s: &mut State, tx: &mut Tx| {
                    s.reveal_graph_search();
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
            "Global",
            Scope::Global,
            s.scope,
            "scope-global",
        ))
        .child(scope_button(
            k,
            "Local",
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
        .on_press(move |s: &mut State| s.set_scope(scope))
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

fn stats(_s: &State, k: &Kit, nodes: usize, edges: usize) -> El {
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
            text(format!("{nodes} notes · {edges} edges"))
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
}

fn info(s: &State, k: &Kit) -> El {
    let Some(id) = s.selected else { return el() };
    let note = &s.vault.notes[id];
    let stats = format!(
        "{} · {}",
        plural(note.links.len(), "outgoing link", "outgoing links"),
        plural(note.backlinks.len(), "backlink", "backlinks")
    );
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
            text("SELECTED")
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
                    "Open note",
                    &ic::OPEN_FILE,
                    "graph-open-note",
                    false,
                    move |s: &mut State| s.open_note(id),
                ))
                .child(if s.scope == Scope::Local {
                    chip(
                        k,
                        "Global",
                        &ic::SHARE,
                        "graph-scope",
                        false,
                        move |s: &mut State| s.set_scope(Scope::Global),
                    )
                } else {
                    chip(
                        k,
                        "Local",
                        &ic::SHARE,
                        "graph-scope",
                        false,
                        move |s: &mut State| s.set_scope(Scope::Local),
                    )
                }),
        )
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
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
            text(
                "Arrows focus · Enter open · +/- zoom · 0/F fit · I isolate · Drag pan · Scroll zoom",
            )
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}
