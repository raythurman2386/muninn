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
