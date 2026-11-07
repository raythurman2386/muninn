//! The window's chrome: vault sidebar, note title bar, editor, Markdown preview and links rail.

use rune::{
    Color, Cursor, Cx, El, Key, KeyEvent, LinkPaint, ScrollSpec, Span, TextStyle, Tx, col, el,
    icon, row, spacer, text, text_input,
};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use muninn_core::Vault;

use crate::{Mode, State};

const SIDEBAR: f32 = 258.0;
const RAIL: f32 = 286.0;

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = s.dark.unwrap_or_else(|| cx.system_dark());
    let k = rune_theme_opencode::kit(dark);
    row()
        .key("root")
        .w_full()
        .h_full()
        .fill(k.c.bg_base)
        .on_drawn_layout_tx(|s: &mut State, r: [f32; 4], tx: &mut Tx| {
            if (s.window_width - r[2]).abs() > 0.5 {
                s.window_width = r[2];
                tx.instant();
            }
        })
        .child(sidebar(s, k, dark))
        .child(divider(k))
        .child(main(s, k))
}

fn sidebar(s: &State, k: &Kit, dark: bool) -> El {
    let results = s.vault.search(&s.search.text);
    let empty = results.is_empty();
    let mut list = col().key("note-list").w_full().px(8.0).pb(8.0).gap(2.0);
    for id in results {
        list = list.child(note_row(s, k, id));
    }
    if empty {
        let (title, hint) = if s.vault.notes.is_empty() {
            ("No notes yet", "Add .md files here, or make one below.")
        } else {
            ("No notes match", "Try a different search.")
        };
        list = list.child(
            col()
                .py(24.0)
                .px(12.0)
                .gap(5.0)
                .items_center()
                .child(text(title).font(k.ty.ui_compact).color(k.c.text_muted))
                .child(text(hint).font(k.ty.caption()).color(k.c.text_faint)),
        );
    }

    let brand = row()
        .h(58.0)
        .px(16.0)
        .gap(9.0)
        .center_y()
        .child(
            el().size(24.0, 24.0)
                .radius(7.0)
                .fill(k.alpha(k.c.text_accent, 0.15))
                .center()
                .child(icon(&ic::DOT_GRID, 14.0).color(k.c.text_accent)),
        )
        .child(
            text("Muninn")
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .grow(1.0),
        )
        .child(theme_button(k, dark));

    let search = row()
        .key("search-box")
        .h(34.0)
        .mx(12.0)
        .px(9.0)
        .gap(7.0)
        .radius(7.0)
        .center_y()
        .fill(k.c.layer_01)
        .border(
            1.0,
            if s.search.focused {
                k.c.border_focus
            } else {
                k.c.border_muted
            },
        )
        .cursor(Cursor::Text)
        .on_press(|s: &mut State| s.focus_search())
        .child(
            icon(&ic::MAGNIFYING_GLASS, 14.0).color(if s.search.focused {
                k.c.text_accent
            } else {
                k.c.icon_muted
            }),
        )
        .child(
            text_input(&s.search, k.ty.ui_compact)
                .key("search")
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("Search notes", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.search)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        s.search.focused = false;
                        s.editor.focused = false;
                        tx.blur();
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| {
                    if let Some(id) = s.vault.search(&s.search.text).first().copied() {
                        s.open_from_list(id);
                    }
                }),
        );

    let new_note = row()
        .key("new-note")
        .h(34.0)
        .mx(12.0)
        .mt(8.0)
        .px(10.0)
        .gap(7.0)
        .radius(7.0)
        .center_y()
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .child(icon(&ic::PLUS, 14.0).color(k.c.text_accent))
        .child(text("New note").font(k.ty.ui_compact).color(k.c.text_base))
        .on_press(|s: &mut State| s.new_note());

    let count = s.vault.notes.len();
    let links = s.link_total();
    let width = if s.window_width < 900.0 {
        SIDEBAR - 30.0
    } else {
        SIDEBAR
    };
    col()
        .key("sidebar")
        .w(width)
        .h_full()
        .fill(k.c.bg_deep)
        .child(brand)
        .child(search)
        .child(new_note)
        .child(divider_h(k).mt(8.0))
        .child({
            let mut scroller = col()
                .key("note-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(4.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                });
            if let Some(id) = s.reveal {
                scroller =
                    scroller.on_drawn_layout_tx(move |s: &mut State, _: [f32; 4], tx: &mut Tx| {
                        if s.reveal == Some(id) {
                            s.reveal = None;
                            tx.scroll_into_view_in("note-scroll", format!("note-row-{id}"));
                        }
                    });
            }
            scroller.child(list)
        })
        .child(divider_h(k))
        .child(
            row()
                .h(44.0)
                .px(15.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(format!(
                        "{} · {}",
                        plural(count, "note", "notes"),
                        plural(links, "link", "links")
                    ))
                    .font(k.ty.caption())
                    .color(k.c.text_faint),
                )
                .child(spacer())
                .child(text("⌘K").font(k.ty.caption()).color(k.c.text_faint)),
        )
}

fn note_row(s: &State, k: &Kit, id: usize) -> El {
    let note = &s.vault.notes[id];
    let on = s.selected == Some(id);
    let accent = k.c.text_accent;
    let dot = if note.links.is_empty() {
        k.c.text_faint
    } else if on {
        accent
    } else {
        k.alpha(accent, 0.55)
    };
    let subtitle = match (note.links.len(), note.backlinks.len()) {
        (0, 0) => "no links".to_owned(),
        (0, b) => plural(b, "backlink", "backlinks"),
        (l, 0) => plural(l, "link", "links"),
        (l, b) => format!(
            "{} · {}",
            plural(l, "link", "links"),
            plural(b, "backlink", "backlinks")
        ),
    };
    row()
        .key(format!("note-row-{id}"))
        .w_full()
        .h(48.0)
        .px(10.0)
        .gap(9.0)
        .radius(7.0)
        .center_y()
        .fill(if on {
            k.alpha(accent, 0.10)
        } else {
            Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| s.open_from_list(id))
        .child(el().size(6.0, 6.0).radius(3.0).fill(dot))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(note.title.clone())
                        .font(k.ty.ui_medium)
                        .color(if on { accent } else { k.c.text_base })
                        .truncate(),
                )
                .child(
                    text(subtitle)
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .truncate(),
                ),
        )
}

fn theme_button(k: &Kit, dark: bool) -> El {
    el().key("theme")
        .size(28.0, 28.0)
        .radius(7.0)
        .center()
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .child(
            text("◐")
                .font(TextStyle::new(14.0, 500.0))
                .color(k.c.icon_muted),
        )
        .on_press(move |s: &mut State| s.dark = Some(!dark))
}

fn main(s: &State, k: &Kit) -> El {
    let body = if s.mode == Mode::Graph {
        crate::graph_view::graph(s, k)
    } else {
        let mut content = row().key("note-body").w_full().grow(1.0).min_h(0.0).child(
            col()
                .key("page")
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .child(if s.mode == Mode::Preview {
                    preview(s, k)
                } else {
                    editor(s, k)
                })
                .child(status_bar(s, k)),
        );
        // A tiled or narrow window keeps the page; the rail returns when there is room.
        if s.window_width >= 980.0 && s.selected.is_some() {
            content = content.child(divider(k)).child(rail(s, k));
        }
        content
    };
    col()
        .key("main")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .child(topbar(s, k))
        .child(divider_h(k))
        .child(body)
}

fn topbar(s: &State, k: &Kit) -> El {
    let (title, path) = match s.selected.and_then(|id| s.vault.notes.get(id)) {
        Some(note) => (note.title.clone(), note.rel.clone()),
        None => ("No note selected".to_owned(), String::new()),
    };
    let dirty = s.dirty();
    let title_col = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    text(title)
                        .font(k.ty.ui_medium.lh(20.0))
                        .color(k.c.text_base)
                        .truncate(),
                )
                .child(if dirty {
                    el().size(6.0, 6.0).radius(3.0).fill(k.c.warning.fg)
                } else {
                    el()
                }),
        )
        .child(
            text(path)
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        );
    row()
        .h(58.0)
        .px(20.0)
        .gap(12.0)
        .center_y()
        .child(title_col)
        .child(mode_switch(s, k))
}

fn mode_switch(s: &State, k: &Kit) -> El {
    row()
        .h(32.0)
        .padding(3.0)
        .gap(2.0)
        .radius(8.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .child(mode_button(
            k,
            "Write",
            &ic::EDIT,
            "mode-editor",
            Mode::Editor,
            s.mode,
        ))
        .child(mode_button(
            k,
            "Read",
            &ic::OUTLINE_EYE,
            "mode-preview",
            Mode::Preview,
            s.mode,
        ))
        .child(mode_button(
            k,
            "Graph",
            &ic::SHARE,
            "mode-graph",
            Mode::Graph,
            s.mode,
        ))
}

fn mode_button(
    k: &Kit,
    label: &str,
    glyph: &'static rune::Icon,
    key: &str,
    mode: Mode,
    active: Mode,
) -> El {
    let on = mode == active;
    row()
        .key(key)
        .h(26.0)
        .px(9.0)
        .gap(6.0)
        .radius(6.0)
        .center_y()
        .fill(if on { k.c.layer_03 } else { Color::TRANSPARENT })
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .child(icon(glyph, 13.0).color(if on { k.c.text_accent } else { k.c.icon_muted }))
        .child(text(label).font(k.ty.small_medium).color(if on {
            k.c.text_base
        } else {
            k.c.text_muted
        }))
        .on_press(move |s: &mut State| s.set_mode(mode))
}

fn editor(s: &State, k: &Kit) -> El {
    if s.selected.is_none() {
        return no_note(k);
    }
    let body = col().w_full().max_w(780.0).px(52.0).py(36.0).child(
        text_input(&s.editor, k.ty.body.lh(25.0))
            .key("editor")
            .w_full()
            .color(k.c.text_base)
            .caret(k.c.text_accent, k.selection())
            .placeholder(
                "Start writing. Type [[ to link another note.",
                k.c.text_faint,
            )
            .on_edit(|s: &mut State| &mut s.editor)
            .on_submit(|s: &mut State, _| s.editor.insert("\n"))
            .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| crate::shortcut(s, e, tx)),
    );
    col()
        .key("editor-pane")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            col()
                .key("editor-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

fn preview(s: &State, k: &Kit) -> El {
    let Some(id) = s.selected else {
        return no_note(k);
    };
    if s.editor.text.trim().is_empty() {
        return empty_page(k, "This note is empty. Write something in the editor.");
    }
    let source = s.vault.preview(&s.editor.text, id);
    let accent = k.c.text_accent;
    let faint = k.c.text_faint;
    let faint_rule = k.alpha(faint, 0.5);
    let faint_hover = k.alpha(faint, 0.6);
    let body = rune::memo(format!("preview-{id}"), source.clone(), move || {
        let style = MarkdownStyle::from_kit(k)
            .body_font(k.ty.body.lh(25.0))
            .link(accent)
            .link_underline(k.alpha(accent, 0.35));
        Markdown::new(&source)
            .style(style)
            .on_link_tx(|s: &mut State, url: &str, tx: &mut Tx| s.open_link(url, tx))
            .link_style(move |url| {
                url.strip_prefix("missing:").map(|_| {
                    (
                        Span::new(0, 0).color(faint).underline(faint_rule),
                        LinkPaint::new().color(faint).underline(faint_hover),
                    )
                })
            })
            .build(k)
    });
    col()
        .key("preview-pane")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            col()
                .key("preview-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(col().w_full().max_w(744.0).px(52.0).py(36.0).child(body)),
        )
}

fn no_note(k: &Kit) -> El {
    col()
        .key("empty-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(10.0)
        .child(
            text("No note selected")
                .font(k.ty.ui_medium)
                .color(k.c.text_muted),
        )
        .child(
            text("Make a note to start writing.")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(
            row()
                .key("empty-new-note")
                .h(34.0)
                .px(13.0)
                .gap(7.0)
                .radius(8.0)
                .center_y()
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .cursor(Cursor::Pointer)
                .child(icon(&ic::PLUS, 14.0).color(k.c.text_accent))
                .child(text("New note").font(k.ty.ui_compact).color(k.c.text_base))
                .on_press(|s: &mut State| s.new_note()),
        )
}

fn empty_page(k: &Kit, message: &str) -> El {
    col()
        .key("empty-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(text(message).font(k.ty.ui).color(k.c.text_faint))
}

fn status_bar(s: &State, k: &Kit) -> El {
    let (dot, label) = if let Some(error) = &s.error {
        (k.c.danger.fg, error.clone())
    } else if s.dirty() {
        (k.c.warning.fg, "Saving…".to_owned())
    } else if s.last_save.is_some() {
        (k.c.success.fg, "Saved".to_owned())
    } else {
        (k.c.text_faint, "Autosave on".to_owned())
    };
    let count = s
        .selected
        .and_then(|id| s.vault.notes.get(id))
        .map_or(0, |n| n.links.len());
    let back = s
        .selected
        .and_then(|id| s.vault.notes.get(id))
        .map_or(0, |n| n.backlinks.len());
    row()
        .h(36.0)
        .px(20.0)
        .gap(7.0)
        .center_y()
        .child(el().size(6.0, 6.0).radius(3.0).fill(dot))
        .child(
            text(label)
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        )
        .child(spacer())
        .child(
            text(format!(
                "{} · {} · {}",
                plural(s.words(), "word", "words"),
                plural(count, "link", "links"),
                plural(back, "backlink", "backlinks")
            ))
            .font(k.ty.caption())
            .color(k.c.text_faint),
        )
}

fn rail(s: &State, k: &Kit) -> El {
    let Some(id) = s.selected else {
        return col().w(RAIL).h_full().fill(k.c.bg_deep);
    };
    let note = &s.vault.notes[id];
    let mut outgoing = col().w_full().gap(2.0);
    for &target in &note.links {
        outgoing = outgoing.child(rail_link(k, target, &s.vault, false, id));
    }
    if note.links.is_empty() {
        outgoing = outgoing.child(empty(k, "No outgoing links"));
    }
    let mut backlinks = col().w_full().gap(2.0);
    for &from in &note.backlinks {
        backlinks = backlinks.child(rail_link(k, from, &s.vault, true, id));
    }
    if note.backlinks.is_empty() {
        backlinks = backlinks.child(empty(k, "No backlinks yet"));
    }
    col()
        .key("rail")
        .w(RAIL)
        .h_full()
        .fill(k.c.bg_deep)
        .child(
            row()
                .h(48.0)
                .px(15.0)
                .gap(8.0)
                .center_y()
                .child(
                    text("LINKS")
                        .font(TextStyle::new(10.0, 650.0).tracking(0.9))
                        .color(k.c.text_muted),
                )
                .child(spacer()),
        )
        .child(divider_h(k))
        .child(
            col()
                .key("rail-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(11.0)
                .py(12.0)
                .gap(18.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(section(k, "OUTGOING", note.links.len(), outgoing))
                .child(section(k, "BACKLINKS", note.backlinks.len(), backlinks))
                .child(
                    col()
                        .gap(7.0)
                        .child(section(
                            k,
                            "GRAPH",
                            0,
                            col().w_full().gap(6.0).child(local_graph_button(k)),
                        ))
                        .child(
                            text("See this note and everything it touches.")
                                .font(k.ty.caption())
                                .color(k.c.text_faint),
                        ),
                ),
        )
}

fn section(k: &Kit, title: &str, count: usize, body: El) -> El {
    let mut head = row().w_full().px(4.0).gap(7.0).center_y().child(
        text(title)
            .font(TextStyle::new(10.0, 650.0).tracking(0.9))
            .color(k.c.text_muted),
    );
    if count > 0 {
        head = head.child(
            text(count.to_string())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    col().w_full().gap(5.0).child(head).child(body)
}

fn rail_link(k: &Kit, id: usize, vault: &Vault, with_snippet: bool, current: usize) -> El {
    let note = &vault.notes[id];
    let title = note.title.clone();
    let mut body = col().grow(1.0).min_w(0.0).gap(3.0).child(
        text(title)
            .font(k.ty.ui_compact)
            .color(k.c.text_base)
            .truncate(),
    );
    if with_snippet {
        body = body.child(
            text(vault.snippet(id, current))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        );
    }
    row()
        .w_full()
        .px(8.0)
        .py(if with_snippet { 7.0 } else { 6.0 })
        .gap(8.0)
        .radius(6.0)
        .center_y()
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| s.open_from_link(id))
        .child(icon(if with_snippet { &ic::BRANCH } else { &ic::LINK }, 13.0).color(k.c.icon_faint))
        .child(body)
}

fn local_graph_button(k: &Kit) -> El {
    row()
        .key("open-local-graph")
        .h(33.0)
        .px(9.0)
        .gap(7.0)
        .radius(7.0)
        .center_y()
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .child(icon(&ic::SHARE, 13.0).color(k.c.text_accent))
        .child(
            text("Open local graph")
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
        .on_press(|s: &mut State| s.open_local_graph())
}

fn empty(k: &Kit, message: &str) -> El {
    text(message)
        .font(k.ty.caption())
        .color(k.c.text_faint)
        .padding(8.0)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn divider(k: &Kit) -> El {
    el().w(1.0).h_full().fill(k.c.border_muted)
}

fn divider_h(k: &Kit) -> El {
    el().w_full().h(1.0).fill(k.c.border_muted)
}
