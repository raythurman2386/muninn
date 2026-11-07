//! The window's chrome: vault sidebar, note title bar, editor, Markdown preview and links rail.

use rune::{
    Color, Cursor, Cx, El, Key, KeyEvent, LineCap, LinkPaint, ScrollSpec, Shape, Span, TextStyle,
    Tx, canvas, col, el, icon, row, spacer, text, text_input,
};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use muninn_core::{Vault, headings};

use crate::{DeleteConfirm, LinkComplete, MenuTarget, Mode, State};

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
    // No debounce: the filter is an in-memory linear scan (~1–3 ms at 2,000
    // notes), so a per-keystroke rebuild stays cheap; revisit if a much larger
    // vault ever makes typing lag.
    let results = s.search_results();
    // NAV-01: highlight the row the search field's Up/Down selection points at.
    let picked = s
        .search_selected
        .and_then(|index| results.get(index).copied());
    let empty = results.is_empty();
    let filtering = !s.search.text.trim().is_empty();
    // Without a query, show every folder (including empty ones so they can
    // be targeted); with a query, only folders holding a match.
    let groups: Vec<(String, Vec<usize>)> = if filtering {
        s.vault.group_by_folder(&results)
    } else {
        s.vault
            .folders()
            .into_iter()
            .map(|f| {
                let ids = s.vault.notes_in_folder(&f);
                (f, ids)
            })
            .collect::<Vec<_>>()
    };
    // M2 A11Y-01: one Tab stop for the whole note list; Up/Down rove and
    // Enter/Space open the focused row, as in a macOS source list.
    let mut list = col()
        .key("note-list")
        .w_full()
        .px(8.0)
        .pb(8.0)
        .gap(2.0)
        .focus_group()
        .role("list");
    for (folder, ids) in groups {
        let collapsed = s.is_collapsed(&folder) && !filtering;
        list = list.child(folder_header(s, k, &folder, ids.len(), collapsed));
        if s.menu
            .as_ref()
            .is_some_and(|m| m.target == MenuTarget::Folder(folder.clone()))
        {
            list = list.child(context_menu(s, k));
        }
        if s.delete_confirm.as_ref() == Some(&DeleteConfirm::Folder(folder.clone())) {
            list = list.child(delete_confirm_row(
                s,
                k,
                &DeleteConfirm::Folder(folder.clone()),
            ));
        }
        if s.rename_folder_prompt
            .as_ref()
            .is_some_and(|p| p.folder == folder)
        {
            list = list.child(rename_folder_row(s, k));
        }
        if s.new_folder_prompt
            .as_ref()
            .is_some_and(|p| p.parent == folder)
        {
            list = list.child(new_folder_row(s, k));
        }
        if !collapsed {
            if s.new_note_prompt
                .as_ref()
                .is_some_and(|p| p.folder == folder)
            {
                list = list.child(new_note_row(s, k));
            }
            for id in ids {
                if s.rename_prompt.as_ref().is_some_and(|p| p.id == id) {
                    list = list.child(rename_note_row(s, k));
                } else if s.delete_confirm.as_ref() == Some(&DeleteConfirm::Note(id)) {
                    list = list.child(delete_confirm_row(s, k, &DeleteConfirm::Note(id)));
                } else {
                    list = list.child(note_row(s, k, id, picked == Some(id)));
                }
                if s.menu
                    .as_ref()
                    .is_some_and(|m| m.target == MenuTarget::Note(id))
                {
                    list = list.child(context_menu(s, k));
                }
                if s.rename_confirm.as_ref().is_some_and(|c| c.id == id) {
                    list = list.child(rename_confirm_row(s, k));
                }
                if s.move_confirm.as_ref().is_some_and(|c| c.id == id) {
                    list = list.child(move_confirm_row(s, k));
                }
                if s.move_picker == Some(id) {
                    list = list.child(move_picker(s, k, id));
                }
            }
        }
    }
    if empty && !(s.vault.notes.is_empty() && s.startup_error.is_some()) {
        let (title, hint) = if s.vault.notes.is_empty() {
            ("No notes yet", "Add .md files here, or make one below.")
        } else {
            ("No notes match", "Try a different search.")
        };
        list = list.child(
            col()
                .key("no-notes-hint")
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
        .child(logo_mark(24.0))
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
                    // M2 NAV-01: Up/Down walk the ranked results and wrap.
                    // Enter opens the highlighted row (or the first result).
                    match e.key {
                        Key::Escape => {
                            s.search.focused = false;
                            s.editor.focused = false;
                            tx.blur();
                            return true;
                        }
                        Key::Down => {
                            s.search_step(1);
                            return true;
                        }
                        Key::Up => {
                            s.search_step(-1);
                            return true;
                        }
                        _ => {}
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.open_search_selected()),
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
                .child(broken_chip(s, k))
                .child(text("⌘K").font(k.ty.caption()).color(k.c.text_faint)),
        )
}

/// NAV-02: sidebar footer chip that opens the broken-links dashboard. It is
/// always present, so the "All links resolve" empty state is reachable too.
fn broken_chip(s: &State, k: &Kit) -> El {
    let broken = s.broken_count();
    let (label, color) = if broken == 0 {
        ("links ok".to_owned(), k.c.text_faint)
    } else {
        (format!("{broken} broken"), k.c.warning.fg)
    };
    pressable(k, "broken-chip".to_owned())
        .px(6.0)
        .py(3.0)
        .center_y()
        // M2 A11Y-01: a focused button presses on Space for free; Enter needs
        // an explicit handler on a control outside a list group.
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                s.open_broken_links();
                true
            } else {
                false
            }
        })
        .child(text(label).font(k.ty.caption()).color(color))
        .on_press(|s: &mut State| s.open_broken_links())
}

/// Sidebar folder header: chevron + name + count toggles the group, `+`
/// creates a note in that folder, the folder icon starts a new subfolder.
/// Right-click offers the folder menu. Root shows as “Vault root”.
fn folder_header(_s: &State, k: &Kit, folder: &str, count: usize, collapsed: bool) -> El {
    let slug = folder_slug(folder);
    let name = if folder.is_empty() {
        "Vault root".to_owned()
    } else {
        folder.to_owned()
    };
    let chevron = if collapsed { "▸" } else { "▾" };
    let toggle = folder.to_owned();
    let press_folder = folder.to_owned();
    row()
        .key(format!("folder-header-{slug}"))
        .w_full()
        .h(30.0)
        .px(6.0)
        .gap(6.0)
        .radius(6.0)
        .center_y()
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| s.toggle_folder(&toggle))
        .on_secondary_press(move |s: &mut State, press, _| {
            s.open_folder_menu(&press_folder, press.x, press.y);
        })
        .child(
            text(chevron)
                .font(TextStyle::new(10.0, 600.0))
                .color(k.c.text_faint),
        )
        .child(
            text(name)
                .font(k.ty.small_medium)
                .color(k.c.text_muted)
                .grow(1.0)
                .truncate(),
        )
        .child(
            text(count.to_string())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(folder_new_button(k, &slug, folder))
        .child(folder_new_folder_button(k, &slug, folder))
}

fn folder_new_button(k: &Kit, slug: &str, folder: &str) -> El {
    let target = folder.to_owned();
    pressable(k, format!("folder-new-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::PLUS, 12.0).color(k.c.text_accent))
        .on_press(move |s: &mut State| s.new_note_in(&target))
}

fn folder_new_folder_button(k: &Kit, slug: &str, folder: &str) -> El {
    let target = folder.to_owned();
    pressable(k, format!("folder-create-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::FOLDER_ADD_LEFT, 12.0).color(k.c.icon_muted))
        .on_press(move |s: &mut State| s.begin_new_folder(&target))
}

/// Shared cursor/hover/press base for small pointer targets.
fn pressable(k: &Kit, key: String) -> El {
    let mut target = el().key(key);
    target = target.radius(6.0);
    target = target.hover(k.c.hover);
    target = target.press(k.c.pressed);
    target.cursor(Cursor::Pointer)
}

/// Stable test key for a folder (`""` → `root`).
fn folder_slug(folder: &str) -> String {
    if folder.is_empty() {
        return "root".to_owned();
    }
    folder
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn note_row(s: &State, k: &Kit, id: usize, picked: bool) -> El {
    let note = &s.vault.notes[id];
    let on = s.selected == Some(id);
    // NAV-01: the keyboard search highlight uses the same accent fill as the
    // open note, so Up/Down reads as a selection moving through the list.
    let lit = on || picked;
    let accent = k.c.text_accent;
    let dot = if note.links.is_empty() {
        k.c.text_faint
    } else if on {
        accent
    } else {
        k.alpha(accent, 0.55)
    };
    let links = match (note.links.len(), note.backlinks.len()) {
        (0, 0) => "no links".to_owned(),
        (0, b) => plural(b, "backlink", "backlinks"),
        (l, 0) => plural(l, "link", "links"),
        (l, b) => format!(
            "{} · {}",
            plural(l, "link", "links"),
            plural(b, "backlink", "backlinks")
        ),
    };
    // Duplicate titles are only distinguishable by folder, so the subtitle
    // carries the vault-relative folder for those rows.
    let subtitle = if s.vault.is_duplicate(id) {
        let folder = note.folder();
        if folder.is_empty() {
            format!("vault root · {links}")
        } else {
            format!("{folder} · {links}")
        }
    } else {
        links
    };
    row()
        .key(format!("note-row-{id}"))
        .w_full()
        .h(48.0)
        .gap(9.0)
        .radius(7.0)
        .center_y()
        .fill(if lit {
            k.alpha(accent, 0.10)
        } else {
            Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 1.0, accent)
        .on_press(move |s: &mut State| s.open_from_list(id))
        .on_secondary_press(move |s: &mut State, press, _| {
            s.open_note_menu(id, press.x, press.y);
        })
        .child(el().size(6.0, 6.0).radius(3.0).fill(dot))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(note.title.clone())
                        .font(k.ty.ui_medium)
                        .color(if lit { accent } else { k.c.text_base })
                        .truncate(),
                )
                .child(
                    text(subtitle)
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .truncate(),
                ),
        )
        .px(10.0)
        .pl(20.0)
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

/// Inline name field for a note being created in its folder group.
/// Enter submits, Escape cancels; an empty name creates nothing.
fn new_note_row(s: &State, k: &Kit) -> El {
    row()
        .key("new-note-prompt")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(9.0)
        .center_y()
        .child(
            text_input(&s.new_note_field, k.ty.ui_compact)
                .key("new-note-name")
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("Note name", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.new_note_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        s.cancel_new_note();
                        tx.blur();
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.submit_new_note()),
        )
        .child(
            row()
                .key("new-note-cancel")
                .px(8.0)
                .py(4.0)
                .radius(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .child(text("Cancel").font(k.ty.caption()).color(k.c.text_muted))
                .on_press(|s: &mut State| s.cancel_new_note()),
        )
}

/// Inline rename field replacing a note row. Enter saves, Escape undoes.
fn rename_note_row(s: &State, k: &Kit) -> El {
    row()
        .key("rename-note-prompt")
        .w_full()
        .h(48.0)
        .px(10.0)
        .pl(20.0)
        .gap(9.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.text_accent, 0.10))
        .child(
            text_input(&s.rename_field, k.ty.ui_compact)
                .key("rename-note-name")
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .on_edit(|s: &mut State| &mut s.rename_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        s.cancel_rename_note();
                        tx.blur();
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.submit_rename_note()),
        )
        .child(
            row()
                .key("rename-note-cancel")
                .px(8.0)
                .py(4.0)
                .radius(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .child(text("Cancel").font(k.ty.caption()).color(k.c.text_muted))
                .on_press(|s: &mut State| s.cancel_rename_note()),
        )
}

/// Inline name field for a folder being created under its parent group.
fn new_folder_row(s: &State, k: &Kit) -> El {
    folder_field_row(s, k, "new-folder-name", "Folder name", true)
}

/// Inline rename field for a folder, shown under its header.
fn rename_folder_row(s: &State, k: &Kit) -> El {
    folder_field_row(s, k, "rename-folder-name", "", false)
}

fn folder_field_row(s: &State, k: &Kit, key: &str, placeholder: &str, is_new: bool) -> El {
    let key = key.to_owned();
    row()
        .key(format!("{key}-row"))
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(9.0)
        .center_y()
        .child(
            text_input(
                if is_new {
                    &s.new_folder_field
                } else {
                    &s.rename_folder_field
                },
                k.ty.ui_compact,
            )
            .key(key)
            .single_line()
            .grow(1.0)
            .color(k.c.text_base)
            .caret(k.c.text_accent, k.selection())
            .placeholder(placeholder, k.c.text_faint)
            .on_edit(move |s: &mut State| -> &mut rune::TextEdit {
                if is_new {
                    &mut s.new_folder_field
                } else {
                    &mut s.rename_folder_field
                }
            })
            .on_input_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                if e.key == Key::Escape {
                    if is_new {
                        s.cancel_new_folder();
                    } else {
                        s.cancel_rename_folder();
                    }
                    tx.blur();
                    return true;
                }
                crate::shortcut(s, e, tx)
            })
            .on_submit(move |s: &mut State, _| {
                if is_new {
                    s.submit_new_folder();
                } else {
                    s.submit_rename_folder();
                }
            }),
        )
}

/// The open right-click menu, rendered inline under the row that asked.
/// Right-click opens it; Escape, an action, or Cancel closes it. Primary
/// behaviour (open note / toggle folder) is untouched.
fn context_menu(s: &State, k: &Kit) -> El {
    let Some(menu) = &s.menu else {
        return el();
    };
    let items: Vec<(&str, &str)> = match &menu.target {
        MenuTarget::Note(_) => vec![
            ("Rename", "rename"),
            ("Move to…", "move"),
            ("Delete", "delete"),
        ],
        MenuTarget::Folder(_) => vec![
            ("New note here", "new_note"),
            ("New folder…", "new_folder"),
            ("Rename", "rename"),
            ("Delete", "delete"),
        ],
    };
    let mut col_menu = col()
        .key("context-menu")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(2.0)
        .py(4.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base);
    for (label, action) in items {
        col_menu = col_menu.child(
            row()
                .key(format!("menu-{action}"))
                .w_full()
                .h(30.0)
                .px(10.0)
                .radius(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .child(
                    text(label)
                        .font(k.ty.ui_compact)
                        .color(if action == "delete" {
                            k.c.danger.fg
                        } else {
                            k.c.text_base
                        }),
                )
                .on_press(move |s: &mut State| s.pick_menu(action)),
        );
    }
    col_menu.child(
        row()
            .key("menu-cancel")
            .w_full()
            .h(30.0)
            .px(10.0)
            .radius(6.0)
            .center_y()
            .cursor(Cursor::Pointer)
            .hover(k.c.hover)
            .child(text("Cancel").font(k.ty.ui_compact).color(k.c.text_muted))
            .on_press(|s: &mut State| s.close_menu()),
    )
}

/// Inline folder targets under a note being moved.
fn move_picker(s: &State, k: &Kit, id: usize) -> El {
    let current = s
        .vault
        .notes
        .get(id)
        .map(|n| n.folder().to_owned())
        .unwrap_or_default();
    let mut list = col()
        .key("move-picker")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(2.0)
        .py(4.0);
    for folder in s.vault.folders() {
        let label = if folder.is_empty() {
            "Vault root".to_owned()
        } else {
            folder.clone()
        };
        list = list.child(
            row()
                .key(format!("move-to-{}", folder_slug(&folder)))
                .w_full()
                .h(30.0)
                .px(10.0)
                .radius(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .child(
                    text(if folder == current {
                        format!("{label} ✓")
                    } else {
                        label
                    })
                    .font(k.ty.ui_compact)
                    .color(k.c.text_base),
                )
                .on_press(move |s: &mut State| s.move_note_to(id, &folder)),
        );
    }
    list
}

/// SAF-06: link-impact confirmation under a note being renamed. Default is to
/// rewrite inbound wikilinks; the toggle switches to leaving them dangling.
fn rename_confirm_row(s: &State, k: &Kit) -> El {
    let Some(confirm) = &s.rename_confirm else {
        return el();
    };
    link_impact_row(
        k,
        "rename-confirm",
        &format!(
            "{} note{} link{} here",
            confirm.referrers,
            if confirm.referrers == 1 { "" } else { "s" },
            if confirm.referrers == 1 { "s" } else { "" }
        ),
        confirm.rewrite,
        State::toggle_rename_rewrite,
        State::confirm_rename_note,
        State::cancel_rename_confirm,
        ("Rename", "rename-confirm-yes", "rename-confirm-cancel"),
    )
}

/// SAF-06: the same choice before moving a note to another folder.
fn move_confirm_row(s: &State, k: &Kit) -> El {
    let Some(confirm) = &s.move_confirm else {
        return el();
    };
    link_impact_row(
        k,
        "move-confirm",
        &format!(
            "{} note{} link{} here",
            confirm.referrers,
            if confirm.referrers == 1 { "" } else { "s" },
            if confirm.referrers == 1 { "s" } else { "" }
        ),
        confirm.rewrite,
        State::toggle_move_rewrite,
        State::confirm_move_note,
        State::cancel_move_confirm,
        ("Move", "move-confirm-yes", "move-confirm-cancel"),
    )
}

#[allow(clippy::too_many_arguments)]
fn link_impact_row(
    k: &Kit,
    key: &str,
    summary: &str,
    rewrite: bool,
    toggle: fn(&mut State),
    confirm: fn(&mut State),
    cancel: fn(&mut State),
    (confirm_label, confirm_key, cancel_key): (&str, &str, &str),
) -> El {
    let confirm_label = confirm_label.to_owned();
    let confirm_key = confirm_key.to_owned();
    let cancel_key = cancel_key.to_owned();
    let toggle_key = format!("{key}-toggle");
    col()
        .key(key.to_owned())
        .w_full()
        .px(10.0)
        .pl(20.0)
        .py(6.0)
        .gap(4.0)
        .radius(7.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .child(
            text(summary.to_owned())
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        )
        .child(
            row()
                .key(toggle_key)
                .w_full()
                .h(26.0)
                .gap(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .on_press(move |s: &mut State| toggle(s))
                .child(
                    text(if rewrite { "▣" } else { "▢" })
                        .font(TextStyle::new(12.0, 600.0))
                        .color(if rewrite {
                            k.c.text_accent
                        } else {
                            k.c.text_faint
                        }),
                )
                .child(
                    text(if rewrite {
                        "Rewrite links to new name"
                    } else {
                        "Leave links dangling"
                    })
                    .font(k.ty.caption())
                    .color(k.c.text_muted),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(6.0)
                .center_y()
                .child(
                    pressable(k, confirm_key)
                        .px(8.0)
                        .py(3.0)
                        .fill(k.c.layer_03)
                        .child(
                            text(confirm_label)
                                .font(k.ty.caption())
                                .color(k.c.text_base),
                        )
                        .on_press(move |s: &mut State| confirm(s)),
                )
                .child(
                    pressable(k, cancel_key)
                        .px(8.0)
                        .py(3.0)
                        .child(text("Cancel").font(k.ty.caption()).color(k.c.text_muted))
                        .on_press(move |s: &mut State| cancel(s)),
                ),
        )
}

/// Inline delete confirmation under the row being deleted.
fn delete_confirm_row(s: &State, k: &Kit, target: &DeleteConfirm) -> El {
    let _ = s;
    let label = match target {
        DeleteConfirm::Note(_) => "Delete this note?",
        DeleteConfirm::Folder(_) => "Delete folder? (empty only)",
    };
    row()
        .key("delete-confirm")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(8.0)
        .center_y()
        .child(
            text(label)
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .grow(1.0),
        )
        .child(
            row()
                .key("delete-yes")
                .px(10.0)
                .py(5.0)
                .radius(6.0)
                .center_y()
                .fill(k.c.danger.fg)
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .child(text("Delete").font(k.ty.caption()).color(Color::WHITE))
                .on_press(|s: &mut State| s.confirm_delete()),
        )
        .child(
            row()
                .key("delete-cancel")
                .px(10.0)
                .py(5.0)
                .radius(6.0)
                .center_y()
                .cursor(Cursor::Pointer)
                .hover(k.c.hover)
                .child(text("Cancel").font(k.ty.caption()).color(k.c.text_muted))
                .on_press(|s: &mut State| s.cancel_delete()),
        )
}

/// The Muninn mark at `size` points, matching `assets/logo.svg`: dark
/// rounded tile, white geometric M and a violet core node. Gradients are
/// solid colours on the canvas (the tile itself uses a radial gradient).
fn logo_mark(size: f32) -> El {
    let bg = rune::color::hex("#12141A").unwrap_or(Color::BLACK);
    let edge = rune::color::hex("#272A30").unwrap_or(Color::BLACK);
    let ink = Color::WHITE;
    let node = rune::color::hex("#7C4DFF").unwrap_or(Color::WHITE);
    let corner = size * 112.0 / 512.0;
    let scale = size / 512.0;
    let shape = Shape::new()
        .view_box(512.0, 512.0)
        .stroke(
            vec![
                [136.0, 360.0],
                [136.0, 152.0],
                [256.0, 272.0],
                [376.0, 152.0],
                [376.0, 360.0],
            ],
            12.0 * scale,
            ink,
        )
        .line(
            [256.0, 272.0],
            [256.0, 272.0],
            28.0 * scale,
            LineCap::Round,
            node,
        )
        .line(
            [256.0, 272.0],
            [256.0, 272.0],
            12.0 * scale,
            LineCap::Round,
            ink,
        );
    el().key("brand-logo")
        .size(size, size)
        .radius(corner)
        .fill(bg)
        .radial_gradient(
            [0.5, 0.5],
            size * 0.75,
            [
                (0.0, rune::color::hex("#18191E").unwrap_or(bg)),
                (1.0, rune::color::hex("#0B0C0E").unwrap_or(bg)),
            ],
        )
        .border(1.0, edge)
        .clip_rounded(corner)
        .child(canvas(shape).size(size, size))
}

fn main(s: &State, k: &Kit) -> El {
    let mut page = col()
        .key("main")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .child(topbar(s, k))
        .child(divider_h(k));
    if s.find.is_some() {
        page = page.child(find_bar(s, k));
    }
    if s.startup_error.is_some() && !s.vault.notes.is_empty() {
        // A partial failure still lets the notes that did load stay usable.
        page = page.child(startup_banner(s, k));
    }
    if !s.recovered.is_empty() {
        page = page.child(recovery_banner(s, k));
    }
    let body = if s.vault.notes.is_empty() && s.startup_error.is_some() {
        vault_error_page(s, k)
    } else if s.broken_panel {
        broken_links_panel(s, k)
    } else if s.mode == Mode::Graph {
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
    page.child(body)
}

/// The whole vault could not be scanned: show the OS error and Retry instead
/// of an empty editor (which would read as "your notes are gone").
fn vault_error_page(s: &State, k: &Kit) -> El {
    let message = s.startup_error.clone().unwrap_or_default();
    col()
        .key("vault-error")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(40.0)
        .center()
        .items_center()
        .gap(9.0)
        .child(
            text("Could not open the vault")
                .font(k.ty.ui_medium)
                .color(k.c.danger.fg),
        )
        .child(
            text(message)
                .font(k.ty.caption())
                .max_w(560.0)
                .color(k.c.text_muted),
        )
        .child(
            text("Choosing another folder is not available yet; pass --vault or set MUNINN_VAULT.")
                .font(k.ty.caption())
                .max_w(560.0)
                .color(k.c.text_faint),
        )
        .child(status_button(
            k,
            "Retry",
            "vault-retry",
            State::retry_startup,
        ))
}

/// NAV-02: vault-hygiene dashboard listing unresolved wikilink targets.
///
/// Clicking a target creates the note (folder-aware, like a reader click);
/// clicking a referrer opens that note. The list comes from the cache built by
/// `Vault::reindex`, so opening this surface never scans notes.
fn broken_links_panel(s: &State, k: &Kit) -> El {
    let broken = s.vault.broken_links();
    // M2 A11Y-01: Up/Down rove the dashboard as one list, and Enter opens the
    // focused target or referrer.
    let mut list = col()
        .key("broken-list")
        .w_full()
        .gap(4.0)
        .px(28.0)
        .pb(24.0)
        .focus_group()
        .role("list");
    if broken.is_empty() {
        list = list.child(
            text("All links resolve")
                .key("broken-empty")
                .font(k.ty.ui_compact)
                .color(k.c.success.fg)
                .padding(8.0),
        );
    }
    for entry in broken {
        let slug = folder_slug(&entry.target);
        let target = entry.target.clone();
        let count = entry.referrers.len();
        list = list.child(
            row()
                .key(format!("broken-target-{slug}"))
                .w_full()
                .h(38.0)
                .px(10.0)
                .gap(8.0)
                .radius(7.0)
                .center_y()
                .hover(k.c.hover)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 1.0, k.c.warning.fg)
                .on_press(move |s: &mut State| s.open_broken_target(&target))
                .child(icon(&ic::LINK, 13.0).color(k.c.warning.fg))
                .child(
                    text(entry.target.clone())
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .grow(1.0)
                        .truncate(),
                )
                .child(
                    text(plural(count, "referrer", "referrers"))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        );
        for &id in &entry.referrers {
            let label = s.vault.display_label(id);
            let rel = s.vault.notes[id].rel.clone();
            list = list.child(
                row()
                    .key(format!("broken-ref-{slug}-{id}"))
                    .w_full()
                    .h(30.0)
                    .pl(40.0)
                    .pr(10.0)
                    .gap(7.0)
                    .radius(6.0)
                    .center_y()
                    .hover(k.c.hover)
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .focus_ring(2.0, 1.0, k.c.warning.fg)
                    .on_press(move |s: &mut State| s.open_broken_referrer(id))
                    .child(text("↳").font(k.ty.caption()).color(k.c.text_faint))
                    .child(
                        text(label)
                            .font(k.ty.caption())
                            .color(k.c.text_muted)
                            .truncate(),
                    )
                    .child(
                        text(rel)
                            .font(k.ty.caption())
                            .color(k.c.text_faint)
                            .truncate(),
                    ),
            );
        }
    }
    col()
        .key("broken-panel")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            row()
                .h(58.0)
                .px(28.0)
                .gap(10.0)
                .center_y()
                .child(
                    text("Broken links")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text(plural(broken.len(), "target", "targets"))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                )
                .child(spacer())
                .child(
                    pressable(k, "broken-close".to_owned())
                        .px(10.0)
                        .py(4.0)
                        .border(1.0, k.c.border_muted)
                        .child(text("Close").font(k.ty.caption()).color(k.c.text_muted))
                        .on_press(|s: &mut State| s.close_broken_links()),
                ),
        )
        .child(divider_h(k))
        .child(
            col()
                .key("broken-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(list),
        )
}

/// A banner above a still-usable vault when the last scan partly failed.
fn startup_banner(s: &State, k: &Kit) -> El {
    let message = s.startup_error.clone().unwrap_or_default();
    row()
        .key("startup-banner")
        .w_full()
        .px(20.0)
        .py(7.0)
        .gap(8.0)
        .center_y()
        .fill(k.alpha(k.c.danger.fg, 0.08))
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.danger.fg))
        .child(
            text(message)
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .grow(1.0)
                .truncate(),
        )
        .child(status_button(
            k,
            "Retry",
            "vault-retry",
            State::retry_startup,
        ))
}

/// SAF-05: dirty buffers found in the state directory at startup. Every row
/// restores one buffer; Restore all applies them in order (saving each before
/// the next), and Dismiss discards every listed copy after an explicit click.
fn recovery_banner(s: &State, k: &Kit) -> El {
    let count = s.recovered.len();
    let mut banner = row()
        .key("recovery-banner")
        .w_full()
        .px(20.0)
        .py(7.0)
        .gap(8.0)
        .center_y()
        .fill(k.alpha(k.c.warning.fg, 0.10))
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.warning.fg))
        .child(
            text(format!(
                "Recovered unsaved edits for {}",
                plural(count, "note", "notes")
            ))
            .font(k.ty.caption())
            .color(k.c.warning.fg)
            .truncate(),
        );
    for (index, entry) in s.recovered.iter().take(3).enumerate() {
        let rel = entry.rel.clone();
        banner = banner.child(
            row()
                .gap(4.0)
                .center_y()
                .child(
                    text(entry.rel.clone())
                        .font(k.ty.caption())
                        .color(k.c.text_muted)
                        .max_w(190.0)
                        .truncate(),
                )
                .child(
                    pressable(k, format!("recovery-restore-{index}"))
                        .px(6.0)
                        .py(3.0)
                        .child(text("Restore").font(k.ty.caption()).color(k.c.text_base))
                        .on_press(move |s: &mut State| {
                            s.restore_recovery(&rel);
                        }),
                ),
        );
    }
    if count > 3 {
        banner = banner.child(
            text(format!("+{} more", count - 3))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    banner
        .child(spacer())
        .child(status_button(
            k,
            "Restore all",
            "recovery-restore-all",
            State::restore_all_recovery,
        ))
        .child(status_button(
            k,
            "Dismiss",
            "recovery-dismiss",
            State::dismiss_recovery,
        ))
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
        .child(history_button(
            k,
            "history-back",
            "‹",
            s.can_back(),
            State::back,
        ))
        .child(history_button(
            k,
            "history-forward",
            "›",
            s.can_forward(),
            State::forward,
        ))
        .child(title_col)
        .child(mode_switch(s, k))
}

/// M2 NAV-03: a compact Back/Forward chip. Disabled chips dim but keep their
/// place, so the title bar does not reflow as history grows.
fn history_button(
    k: &Kit,
    key: &'static str,
    glyph: &'static str,
    enabled: bool,
    action: fn(&mut State) -> bool,
) -> El {
    let mut button = pressable(k, key.to_owned())
        .size(26.0, 26.0)
        .center()
        .border(1.0, k.c.border_muted)
        .child(
            text(glyph)
                .font(TextStyle::new(15.0, 600.0))
                .color(if enabled {
                    k.c.text_muted
                } else {
                    k.alpha(k.c.text_faint, 0.4)
                }),
        );
    if enabled {
        button = button.on_press(move |s: &mut State| {
            let _ = action(s);
        });
    }
    button
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

/// EDT-01: the in-note find bar under the title bar.
///
/// Matches are recomputed in `State::find_tick` (called from `poll`), not here:
/// `view` only borrows `State`. The count reads `index + 1 / total`; Enter and
/// Shift+Enter step and wrap, Escape closes and returns typing to the editor.
fn find_bar(s: &State, k: &Kit) -> El {
    let Some(find) = &s.find else {
        return el();
    };
    let total = find.matches.len();
    let count = if total == 0 {
        "0/0".to_owned()
    } else {
        format!("{}/{}", find.index + 1, total)
    };
    row()
        .key("find-bar")
        .w_full()
        .h(40.0)
        .px(20.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.layer_01)
        .child(icon(&ic::MAGNIFYING_GLASS, 13.0).color(k.c.text_muted))
        .child(
            text_input(&find.field, k.ty.ui_compact)
                .key("find")
                .single_line()
                .w(280.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("Find in note", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.find.as_mut().expect("find bar").field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    match e.key {
                        Key::Escape => {
                            s.close_find();
                            return true;
                        }
                        Key::Enter => {
                            if e.mods.shift {
                                s.find_prev();
                            } else {
                                s.find_next();
                            }
                            return true;
                        }
                        _ => {}
                    }
                    crate::shortcut(s, e, tx)
                }),
        )
        .child(
            text(count)
                .key("find-count")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(
            pressable(k, "find-prev".to_owned())
                .size(24.0, 24.0)
                .center()
                .child(
                    text("‹")
                        .font(TextStyle::new(14.0, 600.0))
                        .color(k.c.text_muted),
                )
                .on_press(|s: &mut State| s.find_prev()),
        )
        .child(
            pressable(k, "find-next".to_owned())
                .size(24.0, 24.0)
                .center()
                .child(
                    text("›")
                        .font(TextStyle::new(14.0, 600.0))
                        .color(k.c.text_muted),
                )
                .on_press(|s: &mut State| s.find_next()),
        )
        .child(spacer())
        .child(
            pressable(k, "find-close".to_owned())
                .size(24.0, 24.0)
                .center()
                .child(text("✕").font(k.ty.caption()).color(k.c.text_faint))
                .on_press(|s: &mut State| s.close_find()),
        )
}

fn editor(s: &State, k: &Kit) -> El {
    if s.selected.is_none() {
        return no_note(k);
    }
    let mut body = col().w_full().max_w(780.0).px(52.0).py(36.0).child(
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
            // EDT-05: Enter closes `[[`, continues lists, or inserts a newline.
            .on_submit(|s: &mut State, _| s.editor_enter())
            .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                // EDT-03: the completion claims Up/Down/Enter/Tab/Escape (and,
                // in the Cmd+P picker, the typed filter) before the field.
                if s.complete_key(e) {
                    return true;
                }
                // EDT-05: Tab indents instead of moving focus out of the note.
                if e.key == Key::Tab && !e.mods.shift && !e.mods.cmd && !e.mods.ctrl && !e.mods.alt
                {
                    s.editor_tab();
                    return true;
                }
                // Let menus/prompts/graph shortcuts claim Escape first; an
                // unclaimed Escape blurs in the same event (EDT-06) so the
                // typing gate cannot leave F2/Delete disabled until the next
                // frame. Mirrors the search field's handler above.
                let handled = crate::shortcut(s, e, tx);
                if e.key == Key::Escape && !handled {
                    s.blur_text();
                    tx.blur();
                    return true;
                }
                handled
            }),
    );
    if let Some(complete) = &s.complete
        && !complete.items.is_empty()
    {
        body = body.child(link_complete_list(s, k, complete));
    }
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

/// EDT-03: the completion/picker list under the editor. The highlighted row is
/// moved with Up/Down (and wraps); clicking a row accepts it. Duplicate titles
/// show their folder through `display_label`.
fn link_complete_list(s: &State, k: &Kit, complete: &LinkComplete) -> El {
    let mut list = col()
        .key("link-complete")
        .mt(8.0)
        .w(420.0)
        .gap(1.0)
        .padding(4.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base);
    for (index, &id) in complete.items.iter().enumerate() {
        let on = complete.index == index;
        let label = s.vault.display_label(id);
        let key = format!("complete-item-{id}");
        list = list.child(
            row()
                .key(key)
                .w_full()
                .h(28.0)
                .px(8.0)
                .radius(5.0)
                .center_y()
                .fill(if on {
                    k.alpha(k.c.text_accent, 0.12)
                } else {
                    Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut State| {
                    s.complete_choose(id);
                })
                .child(
                    text(label)
                        .font(k.ty.ui_compact)
                        .color(if on { k.c.text_accent } else { k.c.text_base })
                        .truncate(),
                ),
        );
    }
    list
}

fn preview(s: &State, k: &Kit) -> El {
    let Some(id) = s.selected else {
        return no_note(k);
    };
    if s.editor.text.trim().is_empty() {
        return empty_page(k, "This note is empty. Write something in the editor.");
    }
    let source = s.vault.preview(&s.editor.text, id);
    // EDT-04: top-level heading blocks get stable keys so a deep link can
    // scroll one into view. Offsets are computed against the preview source
    // (link rewriting changes byte lengths), matching `Markdown::block_keys`.
    let block_keys: Vec<(usize, String)> = headings(&source)
        .into_iter()
        .map(|(offset, _)| (offset, format!("heading-{offset}")))
        .collect();
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
            .block_keys(block_keys)
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
    let scroller = col()
        .key("preview-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .items_center()
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    col()
        .key("preview-pane")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(scroller.child(col().w_full().max_w(744.0).px(52.0).py(36.0).child(body)))
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
    if s.conflict.is_some() {
        return conflict_bar(s, k);
    }
    let (dot, label) = if let Some(error) = &s.error {
        (k.c.danger.fg, error.clone())
    } else if s.dirty() {
        (k.c.warning.fg, "Saving…".to_owned())
    } else if s.last_save.is_some() {
        (k.c.success.fg, "Saved".to_owned())
    } else {
        (k.c.text_faint, "Autosave on".to_owned())
    };
    let mut bar = row()
        .key("status-bar")
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
        );
    if s.error.is_some() && s.dirty() {
        bar = bar.child(status_button(k, "Retry", "retry-save", State::retry_save));
    }
    let skipped = s.vault.skipped_count();
    if skipped > 0 {
        bar = bar.child(
            text(plural(skipped, "file", "files") + " skipped")
                .key("skipped-count")
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        );
    }
    if let Some(undo) = &s.trash_undo {
        bar = bar
            .child(
                text(format!("Deleted {}", undo.label))
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .truncate(),
            )
            .child(status_button(k, "Undo", "trash-undo", State::undo_trash));
    }
    bar.child(spacer()).child(status_meta(s, k))
}

/// A conflict outranks the ordinary status line: while one is pending the
/// only actions are the three explicit resolutions.
fn conflict_bar(s: &State, k: &Kit) -> El {
    let Some(conflict) = &s.conflict else {
        return el();
    };
    row()
        .key("conflict-bar")
        .h(36.0)
        .px(20.0)
        .gap(7.0)
        .center_y()
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.danger.fg))
        .child(
            text(conflict.message())
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .truncate(),
        )
        .child(spacer())
        .child(status_button(
            k,
            "Keep mine",
            "conflict-keep-mine",
            State::resolve_conflict_keep_mine,
        ))
        .child(status_button(
            k,
            "Load theirs",
            "conflict-load-theirs",
            State::resolve_conflict_load_theirs,
        ))
        .child(status_button(
            k,
            "Save copy",
            "conflict-save-copy",
            State::resolve_conflict_save_copy,
        ))
}

fn status_meta(s: &State, k: &Kit) -> El {
    let count = s
        .selected
        .and_then(|id| s.vault.notes.get(id))
        .map_or(0, |n| n.links.len());
    let back = s
        .selected
        .and_then(|id| s.vault.notes.get(id))
        .map_or(0, |n| n.backlinks.len());
    text(format!(
        "{} · {} · {}",
        plural(s.words(), "word", "words"),
        plural(count, "link", "links"),
        plural(back, "backlink", "backlinks")
    ))
    .font(k.ty.caption())
    .color(k.c.text_faint)
}

fn status_button(k: &Kit, label: &str, key: &str, action: fn(&mut State) -> bool) -> El {
    let label = label.to_owned();
    let key = key.to_owned();
    pressable(k, key)
        .px(8.0)
        .py(4.0)
        .center_y()
        .child(text(label).font(k.ty.caption()).color(k.c.text_base))
        .on_press(move |s: &mut State| {
            let _ = action(s);
        })
}

fn rail(s: &State, k: &Kit) -> El {
    let Some(id) = s.selected else {
        return col().w(RAIL).h_full().fill(k.c.bg_deep);
    };
    let note = &s.vault.notes[id];
    let outgoing_len = note.links.len();
    let mut outgoing = col().w_full().gap(2.0);
    for (index, &target) in note.links.iter().enumerate() {
        outgoing = outgoing.child(rail_link(k, target, &s.vault, false, id, index));
    }
    if note.links.is_empty() {
        outgoing = outgoing.child(empty(k, "No outgoing links"));
    }
    let mut backlinks = col().w_full().gap(2.0);
    for (offset, &from) in note.backlinks.iter().enumerate() {
        backlinks = backlinks.child(rail_link(
            k,
            from,
            &s.vault,
            true,
            id,
            outgoing_len + offset,
        ));
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
                .focus_group()
                .role("list")
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

fn rail_link(
    k: &Kit,
    id: usize,
    vault: &Vault,
    with_snippet: bool,
    current: usize,
    index: usize,
) -> El {
    // Duplicate titles need their folder to be distinguishable here too.
    let title = vault.display_label(id);
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
        .key(format!("rail-link-{index}-{id}"))
        .w_full()
        .px(8.0)
        .py(if with_snippet { 7.0 } else { 6.0 })
        .gap(8.0)
        .radius(6.0)
        .center_y()
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        // M2 A11Y-01: the editor owns Tab (EDT-05), so the rail's rows are
        // the first stops in the window and Tab from any non-editor field
        // lands here. Up/Down rove the whole rail; Enter/Space open a row.
        .focusable(true)
        .tab_index(1)
        .focus_ring(2.0, 1.0, k.c.text_accent)
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
