//! #294: note-facing tags — a `tag:` link in Read filters the sidebar, the
//! sidebar's Clear restores the list, Write completes `#tag`, and the graph
//! draws tag nodes that filter instead of opening a note.

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Scope, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn vault() -> TempVault {
    TempVault::new(&[
        (
            "Alpha.md",
            "# Alpha\n\nWork on #work and #home/garden.\n\n[[Beta]]\n",
        ),
        ("Beta.md", "# Beta\n\nMore #work here.\n"),
        ("Gamma.md", "# Gamma\n\nNo tags here.\n"),
    ])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    // Let Rune's presence/layout springs settle before reading a rect.
    for _ in 0..30 {
        frame(rt);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

fn text_is(rt: &Runtime<State>, words: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(words) && !e.exiting)
}

#[test]
fn a_read_tag_link_filters_the_sidebar_and_clear_restores_the_list() {
    let tmp = vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").unwrap();
    let beta = rt.state.vault.resolve("Beta").unwrap();
    let gamma = rt.state.vault.resolve("Gamma").unwrap();
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Preview);
    frame(&mut rt);

    assert!(rt.find("tag:work").is_some(), "the tag is a Read link");

    click(&mut rt, "tag:work");
    assert_eq!(rt.state.mode, Mode::Preview, "a tag link stays in Read");
    assert_eq!(rt.state.note_tag_filter.as_deref(), Some("work"));
    assert_eq!(rt.state.sidebar_notes(), vec![alpha, beta]);
    assert_eq!(rt.state.vault.notes_with_tag("#WORK"), [alpha, beta]);
    assert!(text_is(&rt, "Notes tagged #work"));
    assert!(rt.find("note-tag-filter-summary").is_some());
    assert!(rt.find(&format!("note-row-{alpha}")).is_some());
    assert!(rt.find(&format!("note-row-{beta}")).is_some());
    assert!(rt.find(&format!("note-row-{gamma}")).is_none());

    click(&mut rt, "note-tag-filter-clear");
    assert_eq!(rt.state.note_tag_filter, None);
    assert_eq!(rt.state.sidebar_notes(), vec![alpha, beta, gamma]);
    assert!(rt.find(&format!("note-row-{gamma}")).is_some());
    assert!(rt.find("note-tag-filter-summary").is_none());
}

#[test]
fn a_tag_link_while_searching_clears_the_search() {
    let tmp = vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").unwrap();
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Preview);
    frame(&mut rt);
    // Drive focus like a user: ⌘K gives the sidebar field the keyboard.
    rt.key(KeyEvent {
        key: Key::Command('k'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    assert!(rt.state.search.focused, "the search field has the keyboard");

    for c in "Gam".chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
        frame(&mut rt);
    }
    assert_eq!(rt.state.search.text, "Gam");

    click(&mut rt, "tag:work");
    assert_eq!(rt.state.search.text, "", "one filter owns the sidebar");
    assert_eq!(rt.state.note_tag_filter.as_deref(), Some("work"));
    assert_eq!(rt.state.search_selected, None);
}

#[test]
fn write_offers_and_inserts_a_vault_tag_without_touching_link_completion() {
    let tmp = vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").unwrap();
    rt.state.open_note(alpha);
    frame(&mut rt);
    rt.state.editor.set_caret(rt.state.editor.text.len());
    rt.state.focus_editor();
    frame(&mut rt);

    for c in "#wo".chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
        frame(&mut rt);
    }
    let complete = rt.state.tag_complete.as_ref().expect("Write tag list");
    assert!(!complete.card, "this list belongs to the Write editor");
    assert_eq!(complete.items.first().map(String::as_str), Some("work"));
    assert!(rt.find("tag-complete").is_some());
    assert!(rt.find("complete-tag-0").is_some());

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(rt.state.editor.text.trim_end().ends_with("#work"));
    assert!(rt.state.tag_complete.is_none());

    // EDT-03: a `[[` mark is still a note completion, not a tag list.
    for c in "[[Al".chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
        frame(&mut rt);
    }
    assert!(rt.state.complete.is_some(), "note completion still opens");
    assert!(rt.state.tag_complete.is_none());
    assert!(rt.find("link-complete").is_some());
}

#[test]
fn graph_tag_nodes_have_edges_click_to_filter_and_stats_count_them() {
    let tmp = vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").unwrap();
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Graph);
    frame(&mut rt);

    let note_count = rt.state.vault.notes.len();
    assert_eq!(rt.state.graph.nodes.len(), note_count + 2);
    assert!(rt.find(&format!("graph-node-{note_count}")).is_some());
    assert!(rt.find(&format!("graph-node-{}", note_count + 1)).is_some());
    // Alpha links Beta; #work joins Alpha and Beta; #home/garden joins Alpha.
    let edges = rt.state.graph.edges.len();
    assert_eq!(edges, 4, "{:?}", rt.state.graph.edges);
    assert!(text_is(&rt, "3 notes · 2 tags · 4 edges"));

    let tag_index = note_count + 1; // tags sort by key: home/garden, work
    click(&mut rt, &format!("graph-node-{tag_index}"));
    assert_eq!(rt.state.note_tag_filter.as_deref(), Some("work"));
    assert_eq!(rt.state.mode, Mode::Graph, "a tag click opens nothing");
    assert_eq!(
        rt.state.sidebar_notes(),
        [alpha, rt.state.vault.resolve("Beta").unwrap()]
    );

    // Hovering a tag swaps the info card to the tag and its note count.
    let (x, y) = rt
        .find(&format!("graph-node-{tag_index}"))
        .expect("the tag node");
    rt.pointer_move(x, y);
    for _ in 0..30 {
        frame(&mut rt);
    }
    assert_eq!(rt.state.graph_tag_hover.as_deref(), Some("work"));
    assert!(
        text_is(&rt, "2 notes"),
        "the info card names its note count"
    );

    // A note node keeps its old behaviour: one click selects and centres it,
    // a double press still opens the note.
    let beta = rt.state.vault.resolve("Beta").unwrap();
    click(&mut rt, &format!("graph-node-{beta}"));
    assert_eq!(rt.state.selected, Some(beta));
    assert_eq!(rt.state.graph_focus, Some(beta));
    assert_eq!(
        rt.state.mode,
        Mode::Graph,
        "one click selects, no note opens"
    );
    let (x, y) = rt
        .find(&format!("graph-node-{beta}"))
        .expect("the note node");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor, "double press opens the note");
    assert_eq!(rt.state.selected, Some(beta));
}

#[test]
fn tags_follow_scope_and_isolate_like_notes() {
    let tmp = vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").unwrap();
    let gamma = rt.state.vault.resolve("Gamma").unwrap();
    rt.state.load_note(gamma, false);
    rt.state.set_mode(Mode::Graph);
    frame(&mut rt);
    let n = rt.state.vault.notes.len();
    let work = n + 1; // tag order: home/garden, then work

    // Global scope: every tag is drawn as a card with its label.
    assert!(text_is(&rt, "#work"), "the tag card shows its label");
    assert!(text_is(&rt, "#home/garden"));

    // Isolate with a tagless selection: the tags dot down, losing their
    // labels, but still filter on a click.
    rt.state.toggle_graph_isolate();
    frame(&mut rt);
    assert!(!text_is(&rt, "#work"), "an isolated-out tag is a dot");
    click(&mut rt, &format!("graph-node-{work}"));
    assert_eq!(rt.state.note_tag_filter.as_deref(), Some("work"));

    // A selection that carries the tag keeps its card under isolate.
    rt.state.select_graph_node(alpha);
    frame(&mut rt);
    assert!(text_is(&rt, "#work"), "the selection carries #work");

    // Local scope around the tagless Gamma draws no tag nodes at all, and
    // the stats readout counts only what the scope shows.
    rt.state.toggle_graph_isolate();
    rt.state.select_graph_node(gamma);
    frame(&mut rt);
    rt.state.set_scope(Scope::Local);
    frame(&mut rt);
    assert_eq!(rt.state.visible_nodes(), vec![gamma]);
    assert!(rt.find(&format!("graph-node-{n}")).is_none());
    assert!(rt.find(&format!("graph-node-{work}")).is_none());
    assert!(!text_is(&rt, "#work"));
    assert!(text_is(&rt, "1 notes · 0 tags · 0 edges"));

    // Back to global: both tag cards return.
    rt.state.set_scope(Scope::Global);
    frame(&mut rt);
    assert!(text_is(&rt, "#work"));
    assert!(rt.find(&format!("graph-node-{work}")).is_some());
}

// ---- #303: drawn cards are packed at card spacing -------------------------

/// A vault past the card/dot boundary (111 nodes) shaped like the one in
/// #303's report: one wide hub whose Local neighbourhood is dozens of cards,
/// plus tags so the drawn set includes tag nodes (and one tag outside it).
fn hub_vault() -> TempVault {
    let notes = 160;
    let hub_links = 55;
    let mut files = Vec::new();
    for i in 0..notes {
        let mut body = format!("# Note {i:03}\n\n");
        if i == 0 {
            for to in 1..=hub_links {
                body.push_str(&format!("[[Note {to:03}]] "));
            }
        } else if i > 1 {
            body.push_str(&format!("[[Note {:03}]] ", i - 1));
        }
        let tag = match i {
            0..=19 => Some("#alpha"),
            20..=39 => Some("#beta"),
            40..=54 => Some("#gamma"),
            100 => Some("#delta"),
            _ => None,
        };
        if let Some(tag) = tag {
            body.push_str(tag);
        }
        body.push('\n');
        files.push((format!("Note {i:03}.md"), body));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    TempVault::new(&refs)
}

/// Open `hub_vault` in Global scope, settled, with `selected` open.
fn hub_graph(tmp: &TempVault, selected: usize) -> Runtime<State> {
    let mut rt = runtime(tmp);
    for _ in 0..400 {
        if !rt.state.loading() {
            break;
        }
        frame(&mut rt);
    }
    assert!(!rt.state.loading(), "the big vault finished scanning");
    rt.state.load_note(selected, false);
    rt.state.scope = Scope::Global;
    rt.state.set_mode(Mode::Graph);
    settle_graph(&mut rt);
    rt.state.auto_fit = false;
    rt.state.needs_fit = false;
    rt
}

/// Run frames until thirty in a row move nothing (the definition the app's
/// poll loop and `crates/muninn-ui/src/graph_view.rs` tests use). The card pack
/// lands on the first still frame and changes positions, so it is inside the
/// window this waits for.
fn settle_graph(rt: &mut Runtime<State>) {
    let mut quiet = 0;
    let mut last: Vec<[f32; 2]> = rt.state.graph.nodes.iter().map(|n| n.pos).collect();
    for _ in 0..6000 {
        frame(rt);
        let now: Vec<[f32; 2]> = rt.state.graph.nodes.iter().map(|n| n.pos).collect();
        if now == last {
            quiet += 1;
            if quiet == 30 {
                return;
            }
        } else {
            quiet = 0;
        }
        last = now;
    }
    panic!("the layout must settle within 6,000 frames");
}

/// The drawn footprints of `ids` that are cards (dots never reach 100 pt).
fn card_footprints(s: &State, ids: &[usize]) -> Vec<(usize, [f32; 2], [f32; 2])> {
    ids.iter()
        .filter_map(|&id| {
            let (pos, size) = muninn_ui::node_footprint(s, id)?;
            (size[0] >= 100.0).then_some((id, pos, size))
        })
        .collect()
}

/// Every pair of drawn cards whose rectangles overlap.
fn overlapping_cards(cards: &[(usize, [f32; 2], [f32; 2])]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (a, &(id_a, pos_a, size_a)) in cards.iter().enumerate() {
        for &(id_b, pos_b, size_b) in &cards[a + 1..] {
            let overlap_x = (pos_a[0] - pos_b[0]).abs() < (size_a[0] + size_b[0]) / 2.0;
            let overlap_y = (pos_a[1] - pos_b[1]).abs() < (size_a[1] + size_b[1]) / 2.0;
            if overlap_x && overlap_y {
                out.push((id_a, id_b));
            }
        }
    }
    out
}

/// #303: above the card/dot boundary the layout only guarantees marker
/// spacing, so the view's cards overlapped — 90 pairs on the reported vault's
/// hub. Selecting the hub in Global and entering Local must both leave every
/// drawn card pair apart.
#[test]
fn drawn_cards_are_packed_at_card_spacing_above_the_card_boundary() {
    let tmp = hub_vault();
    // Settle with a quiet note selected, then click the hub: the click changes
    // Global's drawn card set and must pack it on the next frames.
    let mut rt = hub_graph(&tmp, 100);
    assert!(
        !muninn_graph::cards_fit(rt.state.graph.nodes.len()),
        "the fixture must be past the card/dot boundary"
    );
    click(&mut rt, "graph-node-0");
    settle_graph(&mut rt);
    assert_eq!(rt.state.selected, Some(0), "the hub click selects it");

    let visible = rt.state.visible_notes();
    let cards = card_footprints(&rt.state, &visible);
    assert!(
        cards.len() >= 50,
        "Global draws the hub's neighbourhood as cards, got {}",
        cards.len()
    );
    let bad = overlapping_cards(&cards);
    assert!(
        bad.is_empty(),
        "{} Global card pairs overlap after the hub selection: {:?}",
        bad.len(),
        &bad[..bad.len().min(4)]
    );

    // Local draws the whole neighbourhood as cards; entering it reheats and
    // settles, and the pack lands when the layout stops.
    rt.state.set_scope(Scope::Local);
    settle_graph(&mut rt);
    rt.state.auto_fit = false;
    rt.state.needs_fit = false;
    // Pin a card-country zoom: Local's LOD is zoom-only.
    rt.state.cam.set([0.0, 0.0], 1.0);
    frame(&mut rt);

    let visible = rt.state.visible_notes();
    let cards = card_footprints(&rt.state, &visible);
    assert_eq!(
        cards.len(),
        visible.len(),
        "Local at zoom 1.0 draws every visible note as a card"
    );
    assert!(
        cards.len() >= 50,
        "the hub's Local set is dozens of cards, got {}",
        cards.len()
    );
    let bad = overlapping_cards(&cards);
    assert!(
        bad.is_empty(),
        "{} Local card pairs overlap: {:?}",
        bad.len(),
        &bad[..bad.len().min(4)]
    );
}
