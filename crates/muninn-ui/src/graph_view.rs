//! The graph canvas: world, edges, keyed nodes, camera gestures and floating controls.

use rune::{
    Color, Cursor, Drag, El, LineCap, MagnifyEvent, PointerButton, Shape, Tx, WheelEvent, canvas,
    col, el, icon, row, text,
};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use muninn_graph::{NODE_H, NODE_W, WORLD};

use crate::{Scope, State};

pub fn graph(s: &State, k: &Kit) -> El {
    let visible = s.visible_notes();
    let edges = s.graph.visible_edges(&visible);
    let mut world = el()
        .key("graph-world")
        .size(WORLD[0], WORLD[1])
        .pin(Some(0.0), Some(0.0), None, None)
        .live_offset(&s.cam.live_pan)
        .live_scale(&s.cam.live_zoom)
        .child(grid(k))
        .child(edge_canvas(s, k, &edges));
    for id in &visible {
        if let Some(node) = s.graph.node(*id) {
            world = world.child(node_button(s, k, *id, node.pos));
        }
    }

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
        .child(stats(s, k))
        .child(info(s, k))
        .child(hint(k))
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

fn edge_canvas(s: &State, k: &Kit, edges: &[(usize, usize)]) -> El {
    let base = k.alpha(k.c.text_muted, 0.55);
    let hot = k.alpha(k.c.text_accent, 0.72);
    let mut shape = Shape::new();
    for &(a, b) in edges {
        let (Some(from), Some(to)) = (s.graph.node(a), s.graph.node(b)) else {
            continue;
        };
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

fn node_button(s: &State, k: &Kit, id: usize, pos: [f32; 2]) -> El {
    let note = &s.vault.notes[id];
    let selected = s.selected == Some(id);
    let neighbour = s.selected.is_none_or(|sel| {
        sel == id
            || s.vault.notes[sel].links.contains(&id)
            || s.vault.notes[sel].backlinks.contains(&id)
    });
    let dim = s.selected.is_some() && !neighbour;
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
    let degree = note.links.len() + note.backlinks.len();
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
    let mut node = row()
        .key(format!("graph-node-{id}"))
        .pin(
            Some(pos[0] - NODE_W / 2.0),
            Some(pos[1] - NODE_H / 2.0),
            None,
            None,
        )
        .size(NODE_W, NODE_H)
        .px(9.0)
        .gap(7.0)
        .radius(9.0)
        .center_y()
        .fill(fill)
        .border(1.0, border)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| s.select_graph_node(id))
        .on_double_press(move |s: &mut State| s.open_note(id))
        .on_drag(move |s: &mut State, d: &mut Drag, _: &mut Tx| s.drag_node(id, d))
        .child(el().size(6.0, 6.0).radius(3.0).fill(dot))
        .child(
            text(label)
                .font(rune::TextStyle::new(12.0, 500.0))
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

fn controls(s: &State, k: &Kit) -> El {
    row()
        .pin(Some(14.0), Some(14.0), None, None)
        .gap(6.0)
        .child(scope_switch(s, k))
        .child(chip(k, "Fit", &ic::EXPAND, "graph-fit", |s: &mut State| {
            s.fit_graph()
        }))
        .child(chip(
            k,
            "Reset",
            &ic::REFRESH,
            "graph-reset",
            |s: &mut State| s.reset_graph(),
        ))
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

fn chip(
    k: &Kit,
    label: &str,
    glyph: &'static rune::Icon,
    key: &str,
    press: impl Fn(&mut State) + 'static,
) -> El {
    row()
        .key(key)
        .h(30.0)
        .px(9.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(1.0, k.c.border_base)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press(press)
        .child(icon(glyph, 12.0).color(k.c.icon_muted))
        .child(text(label).font(k.ty.small_medium).color(k.c.text_muted))
}

fn stats(s: &State, k: &Kit) -> El {
    let nodes = s.visible_notes().len();
    let edges = s.visible_edge_count();
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
                    move |s: &mut State| s.open_note(id),
                ))
                .child(if s.scope == Scope::Local {
                    chip(
                        k,
                        "Global",
                        &ic::SHARE,
                        "graph-scope",
                        move |s: &mut State| s.set_scope(Scope::Global),
                    )
                } else {
                    chip(
                        k,
                        "Local",
                        &ic::SHARE,
                        "graph-scope",
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
            text("Drag to pan · Scroll to zoom · Double-click a node to open")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}
