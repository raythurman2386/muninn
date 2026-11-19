//! The window's chrome: vault sidebar, note title bar, editor, Markdown preview and links rail.

use rune::{
    Align, Color, Cursor, Cx, El, Icon, Key, KeyEvent, LineCap, LinkPaint, LiveRegion, ScrollSpec,
    Shape, Span, TextStyle, Transition, Tx, canvas, col, el, icon, row, spacer, text, text_input,
};
use rune_kit::{
    Button, ButtonVariant, Kit, Markdown, MarkdownStyle, SegmentedControl, Switch, TextArea,
};
use rune_theme_opencode::icons as ic;

use muninn_core::{KanbanView, Vault, block_anchors, headings};
use muninn_i18n::{
    Key as TextKey, Lang, autosave_help, broken_links, deleted, files_skipped, focus_exit_help,
    focus_help, link_impact, login_item_error, more_edits, new_folder_in, new_note_in, note_links,
    note_links_in_folder, palette_kanban_folder, recovered_copies, recovered_for, referrers,
    results, scan_ms, scan_progress, shortcut_failed, shortcut_on, show_with_chord,
    sidebar_summary, status_counts, t, targets, theme_label, trash_empty, trash_summary,
    tray_error, version, welcome_vault_created,
};

use crate::{
    BurnSpace, DeleteConfirm, GraphEffectIntensity, GraphEffects, LanguageChoice, LockPromptKind,
    MenuTarget, Mode, PaletteItem, ReduceMotion, SettingsTab, State, ThemeMode,
};

pub(crate) const SIDEBAR: f32 = 258.0;
const RAIL: f32 = 286.0;

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    // One resolved reduced-motion value (`State::reduced_motion`) drives
    // Rune's transitions here and the graph layout/camera in `poll`.
    s.set_system_reduced_motion(cx.system_reduced_motion());
    cx.reduced_motion(s.reduced_motion());
    let k = crate::omarchy::resolve_kit(s, cx.system_dark());
    // The FX compositor runs outside the view's borrow of `State`, so publish
    // the theme colours it paints with here, once per frame.
    s.fx_accent
        .set([k.c.text_accent.r, k.c.text_accent.g, k.c.text_accent.b]);
    s.fx_muted
        .set([k.c.text_muted.r, k.c.text_muted.g, k.c.text_muted.b]);
    let mut base = row()
        .key("root")
        .w_full()
        .h_full()
        .fill(k.c.bg_base)
        .on_drawn_layout_tx(|s: &mut State, r: [f32; 4], tx: &mut Tx| {
            if (s.window_width - r[2]).abs() > 0.5 || (s.window_height - r[3]).abs() > 0.5 {
                s.window_width = r[2];
                s.window_height = r[3];
                tx.instant();
            }
        })
        // DES-02: dropping files anywhere in the window imports them into the
        // current folder. Markdown becomes a note; images/PDFs are copied but
        // stay out of the index (the reader's attachment strip can open them).
        .on_drop_files(|s: &mut State, files: &[std::path::PathBuf], tx: &mut Tx| {
            if s.import_files(files).changed() {
                tx.instant();
            }
        })
        .accepts_extensions(crate::state::IMPORT_EXTENSIONS)
        .on_drop_rejected(|s: &mut State, files: &[std::path::PathBuf], tx: &mut Tx| {
            s.reject_drop(files);
            tx.instant();
        })
        // Slice 8: the sidebar can be hidden. Its two slots stay in the tree
        // (empty when hidden) so the main column keeps its place and the
        // toggle moves things at once instead of sliding them. #112: Focus
        // mode hides it the same way, without touching the choice underneath.
        .child(if s.sidebar_visible() {
            sidebar(s, &k)
        } else {
            el().key("sidebar-hidden")
        })
        .child(if s.sidebar_visible() {
            divider(&k)
        } else {
            el()
        })
        .child(main_column(s, &k));
    // #135: a delete outside the graph burns the note's sidebar row instead.
    // The row rect is in window points (the sidebar is not inside the main
    // column), so the ghost rides above both columns at the root, whose origin
    // is the window's. Neither element takes input, so presses still land on
    // the rows beneath; the modal layers below are added after it and cover it.
    if let Some(burn) = s
        .burn
        .as_ref()
        .filter(|burn| burn.alive(s.now) && burn.space == BurnSpace::Row)
    {
        base = base
            .child(crate::graph_view::burn_ghost(s, &k, burn))
            .child(crate::graph_view::burn_effect(s, burn));
    }
    // Slice 16 fix: Kanban menus and a menu-opened date picker are real
    // modals, drawn last so Tab and the pointer cannot reach the page behind
    // them.
    if s.mode == Mode::Kanban {
        base = base
            .child(kanban_menu_layer(s, &k))
            .child(crate::kanban_dates::picker_layer(s, &k));
    }
    if s.palette.is_some() {
        base = base.child(palette_layer(s, &k));
    }
    // #134: the password card sits above the palette: it can be opened from a
    // palette action or a sidebar menu, and owns Escape while it is up.
    if s.lock_prompt.is_some() {
        base = base.child(lock_prompt_layer(s, &k));
    }
    base
}

fn sidebar(s: &State, k: &Kit) -> El {
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
        if s.folder_rename_confirm
            .as_ref()
            .is_some_and(|c| c.folder == folder)
        {
            list = list.child(folder_rename_confirm_row(s, k));
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
            (
                t(TextKey::SidebarNoNotesTitle),
                t(TextKey::SidebarNoNotesHint),
            )
        } else {
            (
                t(TextKey::SidebarNoMatchTitle),
                t(TextKey::SidebarNoMatchHint),
            )
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

    let mut brand = row()
        .h(58.0)
        .px(16.0)
        .gap(9.0)
        .center_y()
        .child(logo_mark(24.0))
        .child(
            text(t(TextKey::CommonAppName))
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .grow(1.0),
        )
        .child(import_button(k))
        .child(help_button(k))
        .child(settings_button(k));
    // Following the shared Omarchy palette hides the switcher: Omarchy is the
    // system theme, not a cycle stop. A diverged (pinned/system) or
    // non-Omarchy session keeps the button.
    if !crate::omarchy::is_following(s) {
        brand = brand.child(theme_button(k, s.theme));
    }

    let search = row()
        .key("search-box")
        .label(t(TextKey::SidebarSearch))
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
                .label(t(TextKey::SidebarSearch))
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder(t(TextKey::SidebarSearch), k.c.text_faint)
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
        .child(
            text(t(TextKey::CommonNewNote))
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
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
                // #135: the delete burn is placed from the row's own rect, and
                // only while that rect is inside this viewport, so a row
                // scrolled out of the list never pins a ghost elsewhere.
                .drawn_rect_into(&s.note_list_rect)
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
            scroller
                .child(list)
                .child(sidebar_kanbans(s, k, filtering))
                .child(sidebar_boards(s, k, filtering))
        })
        .child(divider_h(k))
        .child(
            row()
                .h(44.0)
                .px(15.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(sidebar_summary(count, links))
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
        (t(TextKey::SidebarLinksOk).to_owned(), k.c.text_faint)
    } else {
        (broken_links(broken), k.c.warning.fg)
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
        t(TextKey::CommonVaultRoot).to_owned()
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
    let place = if folder.is_empty() {
        t(TextKey::SidebarTheVaultRoot).to_owned()
    } else {
        folder.to_owned()
    };
    pressable(k, format!("folder-new-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::PLUS, 12.0).color(k.c.text_accent))
        .label(new_note_in(&place))
        .help(new_note_in(&place))
        .on_press(move |s: &mut State| s.new_note_in(&target))
}

fn folder_new_folder_button(k: &Kit, slug: &str, folder: &str) -> El {
    let target = folder.to_owned();
    let place = if folder.is_empty() {
        t(TextKey::SidebarTheVaultRoot).to_owned()
    } else {
        folder.to_owned()
    };
    pressable(k, format!("folder-create-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::FOLDER_ADD_LEFT, 12.0).color(k.c.icon_muted))
        .label(new_folder_in(&place))
        .help(new_folder_in(&place))
        .on_press(move |s: &mut State| s.begin_new_folder(&target))
}

/// Shared cursor/hover/press base for small pointer targets.
pub(crate) fn pressable(k: &Kit, key: String) -> El {
    let mut target = el().key(key);
    target = target.radius(6.0);
    target = target.hover(k.c.hover);
    target = target.press(k.c.pressed);
    target.cursor(Cursor::Pointer)
}

/// The canonical page/panel **Close** control (#185).
///
/// Every surface that replaces the main column — Help, Settings, Board
/// settings, Archive and Broken links — leaves through this one bordered text
/// button, so a page close always reads the same: 28 pt high, 10 pt of
/// horizontal padding, a muted border and the compact UI face. It is a real
/// `button` with the visible "Close" as its accessible name, focusable with a
/// focus ring, and right-aligned in its header (put a `spacer()` before it).
/// Never draw an icon-only ✕ or a differently-styled close for a page; callers
/// keep their own element key.
pub(crate) fn close_button(
    k: &Kit,
    key: impl Into<String>,
    on_press: impl Fn(&mut State) + 'static,
) -> El {
    let label = t(TextKey::Close);
    pressable(k, key.into())
        .h(28.0)
        .px(10.0)
        .center_y()
        .border(1.0, k.c.border_muted)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.border_focus)
        .role("button")
        .label(label)
        .help(label)
        .on_press(on_press)
        .child(
            text(t(TextKey::Close))
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
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
    let links = note_links(note.links.len(), note.backlinks.len());
    // Duplicate titles are only distinguishable by folder, so the subtitle
    // carries the vault-relative folder for those rows.
    let subtitle = if s.vault.is_duplicate(id) {
        note_links_in_folder(note.folder(), note.links.len(), note.backlinks.len())
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
        // A row shown for a search query is a search result.
        .on_press(move |s: &mut State| {
            if s.search.text.trim().is_empty() {
                s.open_from_list(id);
            } else {
                s.open_from_search(id);
            }
        })
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
        // Kanban slice 3: a kanban note is still a note; the glyph says so.
        .child(if note.is_kanban() {
            icon(&ic::CHECKLIST, 13.0)
                .key(format!("note-row-{id}-kanban"))
                .color(k.c.icon_muted)
                .label(t(TextKey::ModeKanban))
        } else {
            el()
        })
        // #134: a locked note keeps its glyph while it is unlocked, so the row
        // says what the row will do again when the session ends.
        .child(if note.is_locked() {
            icon(&ic::LOCK, 13.0)
                .key(format!("note-row-{id}-lock"))
                .color(k.c.icon_muted)
                .label(t(TextKey::LockedNoteBadge))
        } else {
            el()
        })
        .px(10.0)
        .pl(20.0)
        // #135: publish the row's window rect so a delete can burn it in place.
        .drawn_rect_into(&s.note_row_rect(id))
}

fn theme_button(k: &Kit, mode: ThemeMode) -> El {
    let label = theme_label(mode.label());
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
        .label(label.clone())
        .help(label)
        .on_press(|s: &mut State| s.cycle_theme())
}

/// #189: import a document as a new note. A 26 pt icon beside Help and
/// Settings in the sidebar's brand row; opening the desktop file dialog is the
/// same path the palette action and ⌘I take.
fn import_button(k: &Kit) -> El {
    pressable(k, "import-document".to_owned())
        .size(26.0, 26.0)
        .center()
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .role("button")
        .on_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if e.key == Key::Enter {
                s.open_import_document(tx);
                true
            } else {
                false
            }
        })
        .child(icon(&ic::FILE_DOCUMENT, 15.0).color(k.c.icon_muted))
        .label(t(TextKey::ImportDocument))
        .help(t(TextKey::ImportDocumentHelp))
        .on_press_tx(|s: &mut State, tx: &mut Tx| s.open_import_document(tx))
}

/// DES-01: the sidebar Help button, next to the theme toggle.
fn help_button(k: &Kit) -> El {
    pressable(k, "help-open".to_owned())
        .size(28.0, 28.0)
        .center()
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                s.open_help();
                true
            } else {
                false
            }
        })
        .child(
            text("?")
                .font(TextStyle::new(14.0, 600.0))
                .color(k.c.icon_muted),
        )
        .label(t(TextKey::KeyboardShortcuts))
        .help(t(TextKey::KeyboardShortcuts))
        .on_press(|s: &mut State| s.open_help())
}

/// v0.5: the sidebar Settings button, next to Help. At 28 pt it matches the
/// `?` and theme buttons and stays reachable from the keyboard.
fn settings_button(k: &Kit) -> El {
    pressable(k, "settings-open".to_owned())
        .size(28.0, 28.0)
        .center()
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                s.open_settings();
                true
            } else {
                false
            }
        })
        .child(icon(&ic::SETTINGS_GEAR, 15.0).color(k.c.icon_muted))
        .label(t(TextKey::SettingsTitle))
        .help(t(TextKey::TopbarSettingsHelp))
        .on_press(|s: &mut State| s.open_settings())
}

/// Manual sync. One provider per vault. Nothing here starts a background sync.
fn sync_section(s: &State, k: &Kit) -> El {
    let provider = t(match s.sync_ui.provider_index {
        0 => TextKey::SyncProviderGithubName,
        1 => TextKey::SyncProviderDriveName,
        _ => TextKey::SyncProviderTailscaleName,
    });
    let fields = if s.sync_ui.provider_index == 2 {
        tailnet_fields(s, k)
    } else if s.sync_ui.provider_index == 0 {
        col()
            .w_full()
            .gap(4.0)
            .child(sync_field(
                k,
                TextKey::SyncFieldGithubOwner,
                &s.sync_ui.github_owner,
                |state| &mut state.sync_ui.github_owner,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldRepository,
                &s.sync_ui.github_repo,
                |state| &mut state.sync_ui.github_repo,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldBranch,
                &s.sync_ui.github_branch,
                |state| &mut state.sync_ui.github_branch,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldPrefix,
                &s.sync_ui.github_prefix,
                |state| &mut state.sync_ui.github_prefix,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldGithubClientId,
                &s.sync_ui.github_client_id,
                |state| &mut state.sync_ui.github_client_id,
            ))
    } else {
        col()
            .w_full()
            .gap(4.0)
            .child(sync_field(
                k,
                TextKey::SyncFieldStoreName,
                &s.sync_ui.drive_store,
                |state| &mut state.sync_ui.drive_store,
            ))
            .child(
                text(t(TextKey::SyncProviderDriveCaption))
                    .font(k.ty.caption())
                    .color(k.c.text_faint)
                    .wrap(),
            )
            .child(sync_field(
                k,
                TextKey::SyncFieldGoogleClientId,
                &s.sync_ui.google_client_id,
                |state| &mut state.sync_ui.google_client_id,
            ))
    };
    settings_card(k)
        .key("settings-sync")
        .child(settings_heading(
            k,
            t(TextKey::SyncHeadingProvider),
            Some(t(TextKey::SyncHeadingProviderCaption)),
        ))
        .child(
            SegmentedControl::new(
                &[
                    t(TextKey::SyncProviderGithubName),
                    t(TextKey::SyncProviderDriveShortName),
                    t(TextKey::SyncProviderTailscaleName),
                ],
                Some(s.sync_ui.provider_index),
            )
            .label(t(TextKey::SyncHeadingProvider))
            .on_pick(|state: &mut State, index, _: &mut Tx| state.set_sync_provider(index))
            .build(k),
        )
        .child(fields)
        .child(divider_h(k))
        .child(
            col()
                .w_full()
                .gap(10.0)
                .child(
                    text(format!("{provider}. {}", s.sync_status()))
                        .key("settings-sync-status")
                        .font(k.ty.caption())
                        .color(k.c.text_base)
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .gap(6.0)
                        .flex_wrap()
                        .child(
                            Button::new(t(TextKey::SyncActionConnect))
                                .variant(ButtonVariant::Ghost)
                                .on_press(|state: &mut State| state.connect_sync())
                                .build(k)
                                .key("settings-sync-connect"),
                        )
                        .child(
                            Button::new(t(TextKey::SyncActionSyncNow))
                                .variant(ButtonVariant::Ghost)
                                .on_press(|state: &mut State| state.sync_now())
                                .build(k)
                                .key("settings-sync-now"),
                        )
                        .child(
                            Button::new(t(TextKey::Cancel))
                                .variant(ButtonVariant::Ghost)
                                .on_press(|state: &mut State| state.cancel_sync())
                                .build(k)
                                .key("settings-sync-cancel"),
                        )
                        .child(
                            Button::new(t(TextKey::SyncActionDisconnect))
                                .variant(ButtonVariant::Ghost)
                                .on_press(|state: &mut State| state.disconnect_sync())
                                .build(k)
                                .key("settings-sync-disconnect"),
                        ),
                ),
        )
}

fn tailnet_fields(s: &State, k: &Kit) -> El {
    let role = SegmentedControl::new(
        &[
            t(TextKey::SyncTailnetRoleHost),
            t(TextKey::SyncTailnetRoleJoin),
        ],
        Some(s.sync_ui.tailnet_role_index),
    )
    .label(t(TextKey::SyncTailnetRoleLabel))
    .on_pick(|state: &mut State, index, _: &mut Tx| state.set_tailnet_role(index))
    .build(k);
    let fields = col().w_full().gap(4.0).child(role);
    if s.sync_ui.tailnet_role_index == 0 {
        let fields = fields
            .child(sync_field(
                k,
                TextKey::SyncFieldLoopbackPort,
                &s.sync_ui.tailnet_port,
                |state| &mut state.sync_ui.tailnet_port,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldAllowedLogins,
                &s.sync_ui.tailnet_allowed,
                |state| &mut state.sync_ui.tailnet_allowed,
            ));
        if s.hosting_tailnet() {
            fields.child(tailnet_key_row(s, k))
        } else {
            fields
        }
    } else {
        fields
            .child(sync_field(
                k,
                TextKey::SyncFieldMagicDnsName,
                &s.sync_ui.tailnet_host,
                |state| &mut state.sync_ui.tailnet_host,
            ))
            .child(sync_field(
                k,
                TextKey::SyncFieldSharedKey,
                &s.sync_ui.tailnet_key,
                |state| &mut state.sync_ui.tailnet_key,
            ))
    }
}

/// Host: the shared key stays masked unless Show key is on. Copy puts it on
/// the clipboard as a secret (kept out of clipboard history, cleared after a
/// minute); Rotate replaces it.
fn tailnet_key_row(s: &State, k: &Kit) -> El {
    let shown = s.sync_ui.revealed_key.as_ref();
    let value = match shown {
        Some(key) => text(key.expose().to_owned()).key("settings-sync-key-value"),
        None => text("••••••••••••").key("settings-sync-key-value"),
    };
    col()
        .w_full()
        .gap(2.0)
        .child(
            text(t(TextKey::SyncTailnetKeyLabel))
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(value.font(k.ty.ui_compact).color(k.c.text_base))
        .child(
            row()
                .w_full()
                .gap(6.0)
                .flex_wrap()
                .child(
                    Button::new(t(TextKey::SyncTailnetKeyCopy))
                        .variant(ButtonVariant::Ghost)
                        .on_press_tx(|state: &mut State, tx: &mut Tx| {
                            if let Some(key) = state.tailnet_key_for_copy() {
                                tx.copy_secret(
                                    key.expose().to_owned(),
                                    crate::sync_ui::TAILNET_KEY_CLIPBOARD,
                                );
                            }
                        })
                        .build(k)
                        .key("settings-sync-key-copy"),
                )
                .child(
                    Button::new(if shown.is_some() {
                        t(TextKey::SyncTailnetKeyHide)
                    } else {
                        t(TextKey::SyncTailnetKeyShow)
                    })
                    .variant(ButtonVariant::Ghost)
                    .on_press(|state: &mut State| state.toggle_tailnet_key())
                    .build(k)
                    .key("settings-sync-key-show"),
                )
                .child(
                    Button::new(t(TextKey::SyncTailnetKeyRotate))
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.rotate_tailnet_key())
                        .build(k)
                        .key("settings-sync-key-rotate"),
                ),
        )
}

/// One labelled Sync text field. The visible label and the accessible name are
/// the same catalog text; the element key comes from the key's id, so a copy
/// edit moves the text without moving the key.
fn sync_field(
    k: &Kit,
    label: TextKey,
    field: &rune::TextEdit,
    edit: impl Fn(&mut State) -> &mut rune::TextEdit + 'static,
) -> El {
    col()
        .w_full()
        .gap(2.0)
        .child(text(t(label)).font(k.ty.caption()).color(k.c.text_muted))
        .child(
            text_input(field, k.ty.ui_compact)
                .key(sync_field_key(label))
                .label(t(label))
                .single_line()
                .w_full()
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .on_edit(edit),
        )
}

/// The element key of a Sync field: `sync.field.github_owner` becomes
/// `sync-field-github-owner`, so the key follows the catalog id rather than the
/// English label.
fn sync_field_key(label: TextKey) -> String {
    let slug = label.id().rsplit('.').next().unwrap_or_default();
    format!("sync-field-{}", slug.replace('_', "-"))
}

/// Horizontal padding of the Settings page. The header, the tab strip and the
/// tab bodies share it, so the page lines up with the Board page's gutter.
const SETTINGS_PAD_X: f32 = 28.0;

/// The Settings page. It replaces the main column like Help and the Kanban
/// Board settings panel (the sidebar stays), uses the whole width and height
/// left after it, and scrolls its body. Preferences still write through their
/// existing `State` setters, so a click or key press is persisted before the
/// next frame; there is no separate Save button.
///
/// The six tabs ([`SettingsTab`]) keep their old bodies. The tab bar keyed
/// `settings-tabs` wraps its segments instead of overflowing a narrow column,
/// and each tab's body is still keyed `settings-tab-<name>`, so a later
/// section (sync, graph effects) lands by editing one function rather than
/// the page shell.
fn settings_panel(s: &State, k: &Kit) -> El {
    let content = match s.settings_tab {
        SettingsTab::Vaults => settings_vaults_tab(s, k),
        SettingsTab::Editor => settings_editor_tab(s, k),
        SettingsTab::Appearance => settings_appearance_tab(s, k),
        SettingsTab::Graph => settings_graph_tab(s, k),
        SettingsTab::Sync => settings_sync_tab(s, k),
        SettingsTab::Kanban => crate::kanban_settings::app_tab(s, k),
    };
    col()
        .key("settings-panel")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(settings_header(k))
        .child(divider_h(k))
        // The tab strip is page navigation, not a dialog footer: it sits on
        // the page surface with one hairline under the header, and the cards
        // below scroll on their own.
        .child(
            col()
                .key("settings-tab-strip")
                .w_full()
                .min_w(0.0)
                .px(SETTINGS_PAD_X)
                .pt(14.0)
                .pb(6.0)
                .child(settings_tab_bar(s, k)),
        )
        // One inner scroll for every tab, the way `help-scroll` fills its
        // page. The Kanban tab has many rows but no dialog-height cap now.
        .child(
            col()
                .key("settings-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .key("settings-body")
                        .w_full()
                        .min_w(0.0)
                        .px(SETTINGS_PAD_X)
                        .pt(16.0)
                        .pb(32.0)
                        .gap(16.0)
                        .child(
                            col()
                                .key("settings-tab-content")
                                .w_full()
                                .min_w(0.0)
                                .child(content),
                        ),
                ),
        )
}

/// The page heading, at the same height and rhythm as the Board and Help
/// pages: title and a faint "saved automatically" line on the left, an
/// affordance on the right. The close control is the shared
/// [`close_button`] — the same text "Close" every page uses — with its Esc
/// chord as help, so it reads as leaving a page rather than dismissing a
/// dialog.
fn settings_header(k: &Kit) -> El {
    row()
        .h(58.0)
        .px(SETTINGS_PAD_X)
        .gap(10.0)
        .center_y()
        .child(
            text(t(TextKey::SettingsTitle))
                .font(TextStyle::new(16.0, 600.0).lh(22.0))
                .color(k.c.text_base),
        )
        .child(
            text(t(TextKey::SettingsChangesSave))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        )
        .child(spacer())
        .child(
            close_button(k, "settings-close", |s: &mut State| {
                s.close_settings();
            })
            .help(t(TextKey::SettingsClose)),
        )
}

/// The six-section tab bar: one raised lane holds the segments, the way the
/// kit's `SegmentedControl` does, but the lane wraps onto another line in a
/// narrow main column instead of squeezing or painting past the page.
fn settings_tab_bar(s: &State, k: &Kit) -> El {
    let mut bar = row()
        .key("settings-tabs")
        .w_full()
        .min_w(0.0)
        .padding(3.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .gap(2.0)
        .flex_wrap()
        .role("radiogroup")
        .focus_group()
        .label(t(TextKey::SettingsSection));
    for (index, tab) in SettingsTab::ALL.iter().enumerate() {
        let selected = s.settings_tab == *tab;
        bar = bar.child(
            pressable(k, format!("segment{index}"))
                .h(30.0)
                .px(14.0)
                .center_y()
                .radius(k.radius.md)
                .fill(if selected {
                    k.c.bg_base
                } else {
                    Color::TRANSPARENT
                })
                .border(if selected { 1.0 } else { 0.0 }, k.c.border_base)
                .role("radio")
                .focusable(true)
                .focus_selected(selected)
                .focus_ring(2.0, 1.0, k.c.border_focus)
                .label(tab.label())
                .position_in_set(index + 1, SettingsTab::ALL.len())
                .on_press(move |s: &mut State| s.set_settings_tab(*tab))
                .child(text(tab.label()).font(k.ty.ui_compact).color(if selected {
                    k.c.text_base
                } else {
                    k.c.text_muted
                })),
        );
    }
    bar
}

/// The card surface one Settings group sits on, matching the Help page's
/// section cards: `layer_02`, a hairline border and the kit's large radius.
pub(crate) fn settings_card(k: &Kit) -> El {
    col()
        .w_full()
        .min_w(0.0)
        .gap(12.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .px(16.0)
        .pt(14.0)
        .pb(16.0)
}

/// A card's heading: the group title, then its explanation in muted caption
/// text, so every card opens with the same vertical rhythm.
pub(crate) fn settings_heading(k: &Kit, title: &str, caption: Option<&str>) -> El {
    let mut heading = col()
        .w_full()
        .min_w(0.0)
        .gap(3.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base));
    if let Some(caption) = caption {
        heading = heading.child(
            text(caption)
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        );
    }
    heading
}

/// One titled group inside a Settings tab: a card with a heading, an optional
/// caption, then the control.
pub(crate) fn settings_section(k: &Kit, title: &str, caption: Option<&str>, control: El) -> El {
    settings_card(k)
        .child(settings_heading(k, title, caption))
        .child(control)
}

/// Vaults tab: the typed-path card and the recent-vault list.
fn settings_vaults_tab(s: &State, k: &Kit) -> El {
    let mut recents = col().key("settings-recents").w_full().gap(2.0);
    if s.recent_vaults().is_empty() {
        recents = recents.child(
            text(t(TextKey::SettingsVaultsRecentEmpty))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .padding(4.0),
        );
    } else {
        for (index, path) in s.recent_vaults().iter().enumerate() {
            recents = recents.child(
                row()
                    .key(format!("settings-recent-{index}"))
                    .w_full()
                    .h(30.0)
                    .px(8.0)
                    .radius(k.radius.md)
                    .center_y()
                    .cursor(Cursor::Pointer)
                    .hover(k.c.hover)
                    .press(k.c.pressed)
                    .child(
                        text(display_path(path))
                            .font(k.ty.ui_compact)
                            .color(k.c.text_base)
                            .grow(1.0)
                            .truncate(),
                    )
                    .on_press(move |s: &mut State| {
                        if s.open_recent_vault(index) {
                            s.close_settings();
                        }
                    }),
            );
        }
    }

    col()
        .key("settings-tab-vaults")
        .w_full()
        .gap(14.0)
        .child(settings_section(
            k,
            t(TextKey::SettingsVaultsOpenTitle),
            Some(t(TextKey::SettingsVaultsOpenCaption)),
            vault_prompt_row(s, k),
        ))
        .child(settings_section(
            k,
            t(TextKey::SettingsVaultsRecentTitle),
            Some(t(TextKey::SettingsVaultsRecentCaption)),
            recents,
        ))
}

/// Editor tab: autosave delay and the default note template.
fn settings_editor_tab(s: &State, k: &Kit) -> El {
    let delay = s.autosave_delay();
    let stepper = row()
        .key("settings-autosave")
        .gap(8.0)
        .center_y()
        .child(
            Button::new("−")
                .variant(ButtonVariant::Ghost)
                .on_press(|s: &mut State| s.nudge_autosave_delay(-0.05))
                .build(k)
                .key("settings-autosave-minus"),
        )
        .child(
            text(format!("{delay:.2} s"))
                .key("settings-autosave-value")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            Button::new("+")
                .variant(ButtonVariant::Ghost)
                .on_press(|s: &mut State| s.nudge_autosave_delay(0.05))
                .build(k)
                .key("settings-autosave-plus"),
        );

    let template = TextArea::new(&s.note_template_field)
        .label(t(TextKey::SettingsEditorTemplateLabel))
        .placeholder(t(TextKey::SettingsEditorTemplatePlaceholder))
        .width(None)
        .rows(3)
        .on_edit(|s: &mut State| &mut s.note_template_field)
        .on_change(|s: &mut State, _: &mut Tx| {
            let template = s.note_template_field.text.clone();
            s.set_note_template(&template);
        })
        .build(k);

    col()
        .key("settings-tab-editor")
        .w_full()
        .gap(18.0)
        .child(settings_section(
            k,
            t(TextKey::SettingsEditorAutosaveTitle),
            Some(t(TextKey::SettingsEditorAutosaveCaption)),
            stepper,
        ))
        .child(settings_section(
            k,
            t(TextKey::SettingsEditorTemplateTitle),
            Some(t(TextKey::SettingsEditorTemplateCaption)),
            template,
        ))
}

/// Appearance tab: language, theme, the optional tray icon, start at login
/// and the global shortcut.
fn settings_appearance_tab(s: &State, k: &Kit) -> El {
    let theme = if crate::omarchy::is_following(s) {
        text(t(TextKey::SettingsAppearanceThemeFollowing))
            .key("settings-theme-note")
            .font(k.ty.ui_compact)
            .color(k.c.text_muted)
            .wrap()
    } else {
        let labels: Vec<&str> = ThemeMode::ALL.iter().map(|mode| mode.label()).collect();
        SegmentedControl::new(&labels, Some(s.theme.id() as usize))
            .label(t(TextKey::SettingsAppearanceThemeTitle))
            // Each segment as wide as its widest label, so "Omarchy" and
            // "System" no longer truncate at the default 232 pt width.
            .fit()
            .on_pick(|s: &mut State, index, _: &mut Tx| {
                if let Some(mode) = ThemeMode::ALL.get(index) {
                    s.set_theme(*mode);
                }
            })
            .build(k)
            .key("settings-theme")
    };
    col()
        .key("settings-tab-appearance")
        .w_full()
        .gap(18.0)
        .child(settings_language_section(s, k))
        .child(settings_section(
            k,
            t(TextKey::SettingsAppearanceThemeTitle),
            None,
            theme,
        ))
        .child(settings_tray_section(s, k))
        .child(settings_startup_section(s, k))
}

/// Appearance › Language: System plus every shipped language, named by its
/// endonym (which is never translated). The control is a segmented control
/// like Theme, so one shipped language still leaves a usable System/English
/// pair rather than a disabled list.
fn settings_language_section(s: &State, k: &Kit) -> El {
    let mut labels: Vec<&str> = Vec::with_capacity(1 + Lang::SHIPPED.len());
    labels.push(LanguageChoice::System.label());
    labels.extend(
        Lang::SHIPPED
            .iter()
            .map(|lang| LanguageChoice::Language(*lang).label()),
    );
    let control = SegmentedControl::new(&labels, Some(s.ui_language().index()))
        .label(t(TextKey::SettingsAppearanceLanguageTitle))
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            s.set_ui_language(LanguageChoice::from_index(index));
        })
        .build(k)
        .key("settings-language");
    settings_section(
        k,
        t(TextKey::SettingsAppearanceLanguageTitle),
        Some(t(TextKey::SettingsAppearanceLanguageCaption)),
        control,
    )
}

/// Appearance › Tray icon: Show tray icon and Close to tray, each with a
/// status line. Disabled where Rune has no tray (macOS at the pinned
/// revision); Close to tray also needs the tray icon on.
fn settings_tray_section(s: &State, k: &Kit) -> El {
    let supported = crate::tray_supported();
    let switch = Switch::new(s.tray_icon() && supported)
        .label(t(TextKey::SettingsAppearanceTrayShow))
        .disabled(!supported)
        .on_toggle(|s: &mut State| {
            let on = !s.tray_icon();
            s.set_tray_icon(on);
        })
        .build(k)
        .key("settings-tray-icon");
    let status = if !supported {
        t(TextKey::SettingsAppearanceUnavailable).to_owned()
    } else if let Some(error) = s.tray_error() {
        tray_error(error)
    } else if s.tray_icon() {
        t(TextKey::SettingsAppearanceTrayStatusOn).to_owned()
    } else {
        t(TextKey::SettingsAppearanceStateOff).to_owned()
    };
    let close_enabled = supported && s.tray_icon();
    let close = Switch::new(s.close_to_tray() && close_enabled)
        .label(t(TextKey::SettingsAppearanceCloseToTrayTitle))
        .disabled(!close_enabled)
        .on_toggle(|s: &mut State| {
            let on = !s.close_to_tray();
            s.set_close_to_tray(on);
        })
        .build(k)
        .key("settings-close-to-tray");
    let close_status = if !supported {
        t(TextKey::SettingsAppearanceUnavailable)
    } else if !s.tray_icon() {
        t(TextKey::SettingsAppearanceCloseToTrayNeedsTray)
    } else if !s.close_to_tray() {
        t(TextKey::SettingsAppearanceCloseToTrayOff)
    } else if s.closes_to_tray() {
        t(TextKey::SettingsAppearanceCloseToTrayOn)
    } else {
        t(TextKey::SettingsAppearanceCloseToTrayNoIcon)
    };
    settings_section(
        k,
        t(TextKey::SettingsAppearanceTrayTitle),
        Some(t(TextKey::SettingsAppearanceTrayCaption)),
        col()
            .w_full()
            .gap(8.0)
            .child(switch)
            .child(settings_caption(k, "settings-tray-status", &status))
            .child(close)
            .child(settings_caption(
                k,
                "settings-close-to-tray-status",
                close_status,
            )),
    )
}

/// Appearance › Startup: Start Muninn at login and the global shortcut.
fn settings_startup_section(s: &State, k: &Kit) -> El {
    let autostart = crate::autostart_supported();
    let login = Switch::new(s.auto_start() && autostart)
        .label(t(TextKey::SettingsAppearanceLoginTitle))
        .disabled(!autostart)
        .on_toggle(|s: &mut State| {
            let on = !s.auto_start();
            s.set_auto_start(on);
        })
        .build(k)
        .key("settings-auto-start");
    let login_status = if !autostart {
        t(TextKey::SettingsAppearanceUnavailable).to_owned()
    } else if let Some(error) = s.auto_start_error() {
        login_item_error(error)
    } else if s.auto_start() {
        t(TextKey::SettingsAppearanceLoginOn).to_owned()
    } else {
        t(TextKey::SettingsAppearanceStateOff).to_owned()
    };
    let chord = crate::show_shortcut_label();
    let shortcut = Switch::new(s.show_shortcut())
        .label(show_with_chord(chord))
        .on_toggle(|s: &mut State| {
            let on = !s.show_shortcut();
            s.set_show_shortcut(on);
        })
        .build(k)
        .key("settings-show-shortcut");
    let shortcut_status = match (s.show_shortcut(), s.shortcut_phase()) {
        (_, crate::ShortcutPhase::Failed(error)) => shortcut_failed(chord, error),
        (true, crate::ShortcutPhase::Bound) => shortcut_on(chord),
        (true, crate::ShortcutPhase::Pending) => {
            t(TextKey::SettingsAppearanceShortcutRegistering).to_owned()
        }
        (true, _) => t(TextKey::SettingsAppearanceStateOn).to_owned(),
        (false, _) => t(TextKey::SettingsAppearanceStateOff).to_owned(),
    };
    settings_section(
        k,
        t(TextKey::SettingsAppearanceStartupTitle),
        None,
        col()
            .w_full()
            .gap(8.0)
            .child(login)
            .child(settings_caption(
                k,
                "settings-auto-start-status",
                &login_status,
            ))
            .child(shortcut)
            .child(settings_caption(
                k,
                "settings-show-shortcut-status",
                &shortcut_status,
            )),
    )
}

/// A muted caption under a switch.
fn settings_caption(k: &Kit, key: &'static str, line: &str) -> El {
    text(line.to_owned())
        .key(key)
        .font(k.ty.caption())
        .color(k.c.text_muted)
        .wrap()
}

/// Graph tab: how the knowledge graph looks and moves.
///
/// **Graph effects** picks the node-border and edge-line family together, so a
/// preset always reads as one treatment rather than two unrelated effects.
/// **Effect intensity** (Subtle / Balanced / Intense) scales the Electric
/// preset's shape and glow, and the delete burn; Balanced is the original look.
/// **Reduce motion** is a persisted System / On / Off choice. The status line
/// under it shows the resolved value ([`State::reduced_motion`]), which is what
/// the layout, camera and transitions obey. Effects read it too: a preset never
/// animates when motion is reduced.
fn settings_graph_tab(s: &State, k: &Kit) -> El {
    let labels: Vec<&str> = ReduceMotion::ALL.iter().map(|m| m.label()).collect();
    let picker = SegmentedControl::new(&labels, Some(s.reduce_motion().index()))
        .label(t(TextKey::SettingsGraphReduceMotionTitle))
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            if let Some(mode) = ReduceMotion::ALL.get(index) {
                s.set_reduce_motion(*mode);
            }
        })
        .build(k)
        .key("settings-reduce-motion");
    let status = if s.reduced_motion_env {
        t(TextKey::SettingsGraphReduceMotionEnv)
    } else {
        match (s.reduce_motion(), s.reduced_motion()) {
            (ReduceMotion::System, true) => t(TextKey::SettingsGraphReduceMotionSystemOn),
            (ReduceMotion::System, false) => t(TextKey::SettingsGraphReduceMotionSystemOff),
            (_, true) => t(TextKey::SettingsGraphReduceMotionOn),
            (_, false) => t(TextKey::SettingsGraphReduceMotionOff),
        }
    };
    let effect_labels: Vec<&str> = GraphEffects::ALL.iter().map(|e| e.label()).collect();
    let effects = SegmentedControl::new(&effect_labels, Some(s.graph_effects().index()))
        .label(t(TextKey::SettingsGraphEffectsTitle))
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            if let Some(effects) = GraphEffects::ALL.get(index) {
                s.set_graph_effects(*effects);
            }
        })
        .build(k)
        .key("settings-graph-effects");
    let intensity_labels: Vec<&str> = GraphEffectIntensity::ALL
        .iter()
        .map(|i| i.label())
        .collect();
    let intensity = SegmentedControl::new(&intensity_labels, Some(s.effect_intensity().index()))
        .label(t(TextKey::SettingsGraphIntensityTitle))
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            if let Some(intensity) = GraphEffectIntensity::ALL.get(index) {
                s.set_effect_intensity(*intensity);
            }
        })
        .build(k)
        .key("settings-graph-effect-intensity");
    col()
        .key("settings-tab-graph")
        .w_full()
        .gap(18.0)
        .child(settings_section(
            k,
            t(TextKey::SettingsGraphReduceMotionTitle),
            Some(t(TextKey::SettingsGraphReduceMotionCaption)),
            col().w_full().gap(8.0).child(picker).child(
                text(status)
                    .key("settings-reduced-motion-status")
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .wrap(),
            ),
        ))
        .child(settings_section(
            k,
            t(TextKey::SettingsGraphEffectsTitle),
            Some(t(TextKey::SettingsGraphEffectsCaption)),
            col().w_full().gap(8.0).child(effects),
        ))
        .child(settings_section(
            k,
            t(TextKey::SettingsGraphIntensityTitle),
            Some(t(TextKey::SettingsGraphIntensityCaption)),
            col().w_full().gap(8.0).child(intensity),
        ))
}

/// Sync tab: the vault's sync provider, its fields, the status line, and
/// Connect / Sync now / Cancel / Disconnect ([`sync_section`]).
fn settings_sync_tab(s: &State, k: &Kit) -> El {
    col()
        .key("settings-tab-sync")
        .w_full()
        .child(sync_section(s, k))
}

fn vault_prompt_row(s: &State, k: &Kit) -> El {
    col()
        .key("vault-prompt")
        .w_full()
        .gap(4.0)
        .px(10.0)
        .py(8.0)
        .radius(k.radius.md)
        // Inset well inside the card, so the field reads as the card's input
        // rather than a second card.
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .child(
            text_input(&s.vault_field, k.ty.ui_compact)
                .key("vault-path")
                .label(t(TextKey::SettingsVaultsPathLabel))
                .single_line()
                .w_full()
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("~/notes", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.vault_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        // Esc closes the Settings page even while the field
                        // owns typing.
                        s.close_settings();
                        tx.blur();
                        return true;
                    }
                    crate::shortcut(s, e, tx)
                })
                .on_submit(|s: &mut State, _| s.submit_vault_path()),
        )
        .child(
            text(t(TextKey::SettingsVaultsPathHint))
                .key("vault-hint")
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .wrap()
                .w_full(),
        )
        .child(if let Some(error) = &s.error {
            text(error.clone())
                .key("vault-prompt-error")
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .wrap_anywhere()
                .w_full()
        } else {
            el()
        })
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(6.0)
                .center_y()
                .flex_wrap()
                .child(
                    pressable(k, "vault-open".to_owned())
                        .px(8.0)
                        .py(3.0)
                        .fill(k.c.layer_03)
                        .child(
                            text(t(TextKey::SettingsVaultsOpenAction))
                                .font(k.ty.caption())
                                .color(k.c.text_base),
                        )
                        .on_press(|s: &mut State| s.submit_vault_path()),
                )
                .child(
                    pressable(k, "vault-create".to_owned())
                        .px(8.0)
                        .py(3.0)
                        .child(
                            text(t(TextKey::SettingsVaultsNewAction))
                                .font(k.ty.caption())
                                .color(k.c.text_muted),
                        )
                        .on_press(|s: &mut State| s.submit_new_vault()),
                )
                .child(
                    pressable(k, "vault-cancel".to_owned())
                        .px(8.0)
                        .py(3.0)
                        .child(
                            text(t(TextKey::Cancel))
                                .font(k.ty.caption())
                                .color(k.c.text_muted),
                        )
                        .on_press(|s: &mut State| s.cancel_vault_prompt()),
                ),
        )
}

/// `~/sub/path` when below the home directory, otherwise the full path.
fn display_path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home.map(std::path::PathBuf::from)
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_owned();
        }
        return format!("~/{}", rest.to_string_lossy());
    }
    path.to_string_lossy().into_owned()
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
                .label(t(TextKey::NewNoteName))
                .single_line()
                .grow(1.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder(t(TextKey::NoteNamePlaceholder), k.c.text_faint)
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
                .child(
                    text(t(TextKey::Cancel))
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                )
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
                .label(t(TextKey::RenameNoteName))
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
                .child(
                    text(t(TextKey::Cancel))
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                )
                .on_press(|s: &mut State| s.cancel_rename_note()),
        )
}

/// Inline name field for a folder being created under its parent group.
fn new_folder_row(s: &State, k: &Kit) -> El {
    folder_field_row(
        s,
        k,
        "new-folder-name",
        t(TextKey::FolderNamePlaceholder),
        true,
    )
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
            .label(if is_new {
                t(TextKey::NewFolderName)
            } else {
                t(TextKey::RenameFolderName)
            })
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
pub(crate) fn context_menu(s: &State, k: &Kit) -> El {
    let Some(menu) = &s.menu else {
        return el();
    };
    let items: Vec<(&str, &str)> = match &menu.target {
        // #134: a locked note's envelope cannot be opened externally, renamed
        // or moved (its id binds the filename), so its menu offers only the
        // actions that still make sense. A locked note that is unlocked in
        // this window can be locked again; a locked note with no session asks
        // for its password.
        MenuTarget::Note(id) => {
            let note = s.vault.notes.get(*id);
            let locked = note.is_some_and(|note| note.is_locked());
            let unlock = note.is_some_and(|note| note.readable_source().is_none());
            let mut items = Vec::with_capacity(6);
            if !locked {
                items.push((t(TextKey::MenuOpenExternally), "open_external"));
            }
            items.push((t(TextKey::MenuReveal), "reveal"));
            items.push(if unlock {
                (t(TextKey::MenuUnlockNote), "unlock")
            } else {
                (t(TextKey::MenuLockNote), "lock")
            });
            if !locked {
                items.push((t(TextKey::CommonRename), "rename"));
                items.push((t(TextKey::MenuMoveTo), "move"));
            }
            items.push((t(TextKey::CommonDelete), "delete"));
            items
        }
        MenuTarget::Folder(_) => vec![
            (t(TextKey::MenuNewNoteHere), "new_note"),
            (t(TextKey::MenuNewFolder), "new_folder"),
            (t(TextKey::CommonRename), "rename"),
            (t(TextKey::CommonDelete), "delete"),
        ],
        MenuTarget::Board(..) => vec![(t(TextKey::CommonRename), "rename")],
        MenuTarget::Card(lane, index) => {
            let done = s
                .kanban
                .as_ref()
                .and_then(|shown| shown.board.as_ref())
                .and_then(|board| board.lanes().get(*lane)?.cards().get(*index))
                .is_some_and(muninn_core::Card::is_done);
            let mut items = vec![
                (t(TextKey::CommonEdit), "edit"),
                (t(TextKey::MenuInsertBefore), "insert_before"),
                (t(TextKey::MenuInsertAfter), "insert_after"),
                (t(TextKey::MenuDuplicate), "duplicate"),
            ];
            // Slice 14: the card's checkbox can be hidden by `show-checkboxes`;
            // the menu still toggles the done mark.
            items.push((
                if done {
                    t(TextKey::MenuMarkOpen)
                } else {
                    t(TextKey::MenuMarkDone)
                },
                "toggle_done",
            ));
            items.push((
                if crate::kanban_dates::has_token(s, *lane, *index, crate::PickerKind::Date) {
                    t(TextKey::MenuChangeDate)
                } else {
                    t(TextKey::MenuAddDate)
                },
                "add_date",
            ));
            items.push((
                if crate::kanban_dates::has_token(s, *lane, *index, crate::PickerKind::Time) {
                    t(TextKey::MenuChangeTime)
                } else {
                    t(TextKey::MenuAddTime)
                },
                "add_time",
            ));
            let multi_line = s
                .kanban
                .as_ref()
                .and_then(|shown| shown.board.as_ref())
                .and_then(|board| board.lanes().get(*lane)?.cards().get(*index))
                .is_some_and(|card| card.text().contains('\n'));
            if multi_line {
                items.push((t(TextKey::MenuSplitCard), "split"));
            }
            items.extend(crate::kanban_lanes::card_move_items(s, *lane, *index));
            items.push((t(TextKey::MenuCopyLink), "copy_link"));
            items.push((t(TextKey::MenuNewNoteFromCard), "new_note"));
            items.push((t(TextKey::Archive), "archive"));
            items.push((t(TextKey::CommonDelete), "delete"));
            items
        }
        MenuTarget::Lane(lane) => {
            let mut items = vec![
                (t(TextKey::AddCard), "add_card"),
                (t(TextKey::MenuMarkAllDone), "mark_done"),
            ];
            items.extend(crate::kanban_lanes::lane_items(s, *lane));
            items
        }
        MenuTarget::DeleteLane(lane) => {
            return crate::kanban_lanes::delete_lane_confirm(s, k, *lane);
        }
        MenuTarget::MoveCard(lane, index) => {
            return crate::kanban_lanes::lane_picker(s, k, *lane, *index);
        }
        MenuTarget::ArchiveRestore(_) => return el(),
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
        .border(1.0, k.c.border_base)
        // Slice 16: a menu of menu items, one Tab stop whose Up/Down rove the
        // rows; closing it gives the card or lane back the keyboard.
        .role("menu")
        .label(menu_label(&menu.target))
        .focus_group()
        // Slice 16 fix: a menu takes the keyboard when it opens (Escape and a
        // press outside close it through the modal), and closing it gives the
        // opener back the keyboard.
        .modal(true, |s: &mut State| s.close_menu());
    if let Some(trigger) = menu_return(s, &menu.target) {
        col_menu = col_menu.focus_return(trigger);
    }
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
                .press(k.c.pressed)
                .role("menuitem")
                .label(label.to_owned())
                .focusable(true)
                .focus_highlight(k.c.hover)
                .child(
                    text(label)
                        .font(k.ty.ui_compact)
                        .color(if action == "delete" {
                            k.c.danger.fg
                        } else {
                            k.c.text_base
                        }),
                )
                .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                    if action == "copy_link" {
                        if let Some(link) = s.copy_open_card_link() {
                            tx.copy(link);
                            s.notice = Some(t(TextKey::StatusLinkCopied).to_owned());
                        }
                    } else {
                        s.pick_menu(action);
                    }
                }),
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
            .press(k.c.pressed)
            .role("menuitem")
            .label(t(TextKey::Cancel))
            .focusable(true)
            .focus_highlight(k.c.hover)
            .child(
                text(t(TextKey::Cancel))
                    .font(k.ty.ui_compact)
                    .color(k.c.text_muted),
            )
            .on_press(|s: &mut State| s.close_menu()),
    )
}

/// The name a menu announces, from what it acts on.
fn menu_label(target: &MenuTarget) -> &'static str {
    match target {
        MenuTarget::Note(_) => t(TextKey::MenuNoteActions),
        MenuTarget::Folder(_) => t(TextKey::MenuFolderActions),
        MenuTarget::Board(..) => t(TextKey::MenuBoardActions),
        MenuTarget::Card(..) | MenuTarget::MoveCard(..) => t(TextKey::MenuCardActions),
        MenuTarget::Lane(_) | MenuTarget::DeleteLane(_) => t(TextKey::MenuLaneActions),
        MenuTarget::ArchiveRestore(_) => t(TextKey::MenuArchiveActions),
    }
}

/// A Kanban menu as the page's root overlay. Inline, the menu's flat document
/// position left the rest of the board "inside" its modal; drawn last, the
/// menu is a real modal that takes the keyboard, traps Tab and the pointer,
/// closes on Escape or a press outside it, and gives the opener its focus
/// back. It sits under the lane header, as the board used to draw it.
pub(crate) fn kanban_menu_layer(s: &State, k: &Kit) -> El {
    let Some(menu) = &s.menu else {
        return el();
    };
    if !matches!(
        menu.target,
        MenuTarget::Card(..)
            | MenuTarget::Lane(_)
            | MenuTarget::DeleteLane(_)
            | MenuTarget::MoveCard(..)
    ) {
        return el();
    }
    let rect = menu
        .target
        .lane()
        .and_then(|lane| s.kanban_rects.lane_rect(lane));
    let (left, top) = rect.map_or((28.0, 72.0), |r| (r[0], r[1] + 52.0));
    let width = 260.0;
    // A menu taller than the window's remaining height starts higher, so its
    // last row stays reachable.
    let x = left.min(s.window_width - width - 8.0).max(8.0);
    let y = top.min((s.window_height - 360.0).max(8.0));
    context_menu(s, k).absolute(x, y).w(width).min_w(0.0)
}

/// The control a menu gives the keyboard back to when it closes, for the
/// menus drawn under a row (the notes list and Kanban cards and lanes). The
/// Kanban key follows the view that drew the menu, since List and Table use
/// their own rows and lane-menu buttons.
pub(crate) fn menu_return(s: &State, target: &MenuTarget) -> Option<String> {
    match target {
        MenuTarget::Card(lane, index) | MenuTarget::MoveCard(lane, index) => {
            Some(match s.kanban_view() {
                KanbanView::Table => format!("kanban-table-{lane}-{index}"),
                KanbanView::Board | KanbanView::List => format!("kanban-card-{lane}-{index}"),
            })
        }
        MenuTarget::Lane(lane) | MenuTarget::DeleteLane(lane) => match s.kanban_view() {
            KanbanView::Board => Some(format!("kanban-lane-{lane}-menu")),
            KanbanView::List => Some(format!("kanban-list-lane-{lane}-menu")),
            // The table draws no lane menu; nothing to return to.
            KanbanView::Table => None,
        },
        MenuTarget::Note(id) => Some(format!("note-row-{id}")),
        MenuTarget::Folder(folder) => Some(format!("folder-header-{}", folder_slug(folder))),
        MenuTarget::Board(..) | MenuTarget::ArchiveRestore(_) => None,
    }
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
            t(TextKey::CommonVaultRoot).to_owned()
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
        &link_impact(confirm.referrers, t(TextKey::LinkImpactHere)),
        confirm.rewrite,
        State::toggle_rename_rewrite,
        State::confirm_rename_note,
        State::cancel_rename_confirm,
        (
            t(TextKey::CommonRename),
            "rename-confirm-yes",
            "rename-confirm-cancel",
        ),
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
        &link_impact(confirm.referrers, t(TextKey::LinkImpactHere)),
        confirm.rewrite,
        State::toggle_move_rewrite,
        State::confirm_move_note,
        State::cancel_move_confirm,
        (
            t(TextKey::CommonMove),
            "move-confirm-yes",
            "move-confirm-cancel",
        ),
    )
}

/// SAF-06: link-impact confirmation under a folder being renamed. Rewriting
/// points every link to the notes' new path-qualified stems.
fn folder_rename_confirm_row(s: &State, k: &Kit) -> El {
    let Some(confirm) = &s.folder_rename_confirm else {
        return el();
    };
    link_impact_row(
        k,
        "folder-rename-confirm",
        &link_impact(confirm.referrers, t(TextKey::LinkImpactIntoFolder)),
        confirm.rewrite,
        State::toggle_folder_rename_rewrite,
        State::confirm_folder_rename,
        State::cancel_folder_rename_confirm,
        (
            t(TextKey::CommonRename),
            "folder-rename-confirm-yes",
            "folder-rename-confirm-cancel",
        ),
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
                        t(TextKey::RenameRewriteLinks)
                    } else {
                        t(TextKey::RenameLeaveDangling)
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
                        .child(
                            text(t(TextKey::Cancel))
                                .font(k.ty.caption())
                                .color(k.c.text_muted),
                        )
                        .on_press(move |s: &mut State| cancel(s)),
                ),
        )
}

/// Inline delete confirmation under the row being deleted.
fn delete_confirm_row(s: &State, k: &Kit, target: &DeleteConfirm) -> El {
    let _ = s;
    let label = match target {
        DeleteConfirm::Note(_) => t(TextKey::DeleteNoteQuestion),
        DeleteConfirm::Folder(_) => t(TextKey::DeleteFolderQuestion),
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
                .child(
                    text(t(TextKey::CommonDelete))
                        .font(k.ty.caption())
                        .color(Color::WHITE),
                )
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
                .child(
                    text(t(TextKey::Cancel))
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                )
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
    let focus = s.focus_active();
    let mut page = col().key("main").grow(1.0).h_full().min_w(0.0);
    // #112: Focus mode drops the note title bar (and with it the mode
    // switcher and the history buttons) so the note is the page.
    if !focus {
        page = page.child(topbar(s, k)).child(divider_h(k));
    }
    if s.find.is_some() && s.mode != Mode::Kanban {
        page = page.child(find_bar(s, k));
    }
    if s.startup_error.is_some() && !s.vault.notes.is_empty() {
        // A partial failure still lets the notes that did load stay usable.
        page = page.child(startup_banner(s, k));
    }
    if !s.recovered.is_empty() {
        page = page.child(recovery_banner(s, k));
    }
    let mut page = page.child(main_body(s, k));
    if focus {
        // The one piece of chrome Focus keeps: a faint way out with the
        // pointer, since the button that entered it went with the title bar.
        page = page.child(focus_exit(k));
    }
    page
}

/// #134: the note page for a locked note that is not unlocked in this window.
///
/// It replaces the editor and reader rather than showing them empty: the note
/// has content, it just cannot be read without the password. The envelope is
/// never put in a buffer, so there is nothing here to select, search or spell
/// check. Recovery is off while the note is locked, and the page says so.
fn locked_note_page(s: &State, k: &Kit, id: usize) -> El {
    let note = &s.vault.notes[id];
    let folder = note.folder();
    let subtitle = if folder.is_empty() {
        t(TextKey::CommonVaultRoot).to_owned()
    } else {
        folder.to_owned()
    };
    col()
        .key("locked-note-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(28.0)
        .py(24.0)
        .center()
        .items_center()
        .child(
            col()
                .key("locked-note-card")
                .w(460.0)
                .max_w(460.0)
                .min_w(0.0)
                .gap(8.0)
                .padding(24.0)
                .radius(k.radius.lg)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_base)
                .items_center()
                .child(icon(&ic::LOCK, 26.0).color(k.c.icon_muted))
                .child(
                    text(note.title.clone())
                        .key("locked-note-title")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .wrap()
                        .w_full()
                        .align(Align::Center),
                )
                .child(
                    text(subtitle)
                        .key("locked-note-folder")
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .truncate()
                        .max_w(360.0),
                )
                .child(
                    text(t(TextKey::LockedNoteHeading))
                        .key("locked-note-heading")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_accent)
                        .wrap()
                        .w_full()
                        .align(Align::Center)
                        .pt(6.0),
                )
                .child(
                    text(t(TextKey::LockedNoteBody))
                        .key("locked-note-body")
                        .font(k.ty.caption())
                        .color(k.c.text_muted)
                        .wrap()
                        .w_full()
                        .align(Align::Center),
                )
                .child(
                    text(t(TextKey::LockedNoteRecoveryOff))
                        .key("locked-note-recovery")
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .wrap()
                        .w_full()
                        .align(Align::Center),
                )
                .child(
                    Button::new(t(TextKey::UnlockSubmit))
                        .variant(ButtonVariant::Submit)
                        .on_press(move |s: &mut State| s.begin_unlock_note(id))
                        .build(k)
                        .key("locked-note-unlock"),
                ),
        )
}

/// Pick the body under the chrome: the loading progress page first (no partial
/// vault exists yet), then the error page, dashboards, graph or note surface.
fn main_body(s: &State, k: &Kit) -> El {
    if s.loading() {
        return loading_page(s, k);
    }
    if s.vault.notes.is_empty() && s.startup_error.is_some() {
        return vault_error_page(s, k);
    }
    if s.broken_panel {
        return broken_links_panel(s, k);
    }
    if s.mode == Mode::Graph {
        return crate::graph_view::graph(s, k);
    }
    if s.mode == Mode::Board {
        return crate::board_view::board(s, k);
    }
    // #134: a locked note has no text to edit or preview, and its envelope must
    // never reach Kanban's parser. Graph and Board stay usable (they index the
    // note as empty); Write and Read show the lock page instead.
    if let Some(id) = s.selected
        && s.vault
            .notes
            .get(id)
            .is_some_and(|note| note.is_locked() && note.readable_source().is_none())
    {
        return locked_note_page(s, k, id);
    }
    if s.mode == Mode::Kanban {
        return crate::kanban_view::kanban(s, k);
    }
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
    // A tiled or narrow window keeps the page; the rail returns when there is
    // room. #112: Focus mode hides it at any width.
    if s.window_width >= 980.0 && s.selected.is_some() && !s.focus_active() {
        content = content.child(divider(k)).child(rail(s, k));
    }
    content
}

/// M3 wrapper around [`main`]: Settings and Help take the whole main column
/// while the sidebar stays, and a first-run welcome banner sits above the
/// ordinary page.
fn main_column(s: &State, k: &Kit) -> El {
    if s.settings_open {
        return settings_panel(s, k);
    }
    if s.help {
        return help_panel(s, k);
    }
    if s.welcome && s.vault.notes.is_empty() && s.startup_error.is_none() {
        return col()
            .key("main-with-welcome")
            .grow(1.0)
            .h_full()
            .min_w(0.0)
            .child(welcome_banner(s, k))
            .child(main(s, k));
    }
    main(s, k)
}

/// LIF-01: first run created the vault folder. Point at the next action rather
/// than an empty editor that reads as "your notes are gone".
fn welcome_banner(s: &State, k: &Kit) -> El {
    row()
        .key("welcome-banner")
        .w_full()
        .px(20.0)
        .py(9.0)
        .gap(9.0)
        .center_y()
        .fill(k.alpha(k.c.text_accent, 0.10))
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.text_accent))
        .child(
            text(welcome_vault_created(&display_path(&s.vault.root)))
                .font(k.ty.caption())
                .color(k.c.text_base)
                .grow(1.0)
                .truncate(),
        )
        .child(welcome_graph_hint(k))
        .child(status_button(
            k,
            t(TextKey::CommonNewNote),
            "welcome-new-note",
            |s| {
                s.new_note();
                true
            },
        ))
        .child(status_button(
            k,
            t(TextKey::CommonDismiss),
            "welcome-dismiss",
            |s| {
                s.dismiss_welcome();
                true
            },
        ))
}

/// A one-pill preview of what a `[[link]]` unlocks: open the link graph.
/// It sits in the first-run banner so the empty vault still shows the
/// headline feature without adding a second page.
fn welcome_graph_hint(k: &Kit) -> El {
    pressable(k, "welcome-graph-hint".to_owned())
        .px(9.0)
        .py(4.0)
        .radius(999.0)
        .fill(k.alpha(k.c.text_accent, 0.12))
        .border(1.0, k.alpha(k.c.text_accent, 0.35))
        .center_y()
        .label(t(TextKey::WelcomeGraphLabel))
        .help(t(TextKey::WelcomeGraphHelp))
        .child(
            text(t(TextKey::WelcomeGraphPill))
                .font(k.ty.caption())
                .color(k.c.text_accent),
        )
        .on_press(|s: &mut State| {
            s.set_mode(Mode::Graph);
        })
}

const ISSUES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");

/// Horizontal padding of the help page. Column count is chosen from the width
/// left after this inset, so the grid and the footer share one measure.
const HELP_PAD_X: f32 = 28.0;

/// How the shortcut map fills the main column.
///
/// The sidebar keeps its own width rule (`window_width < 900` shrinks it).
/// What remains, minus [`HELP_PAD_X`], picks the grid: one column while a
/// second card would be narrower than a key column plus a wrapping
/// description, two columns on a normal desktop, and four once each card can
/// still hold a wrapped key row (about 300pt).
struct HelpLayout {
    columns: usize,
    /// Keys sit above the description. A four-column card is too narrow for a
    /// side-by-side key column.
    stacked: bool,
    /// Width of the key column when [`HelpLayout::stacked`] is false, so every
    /// row in a card lines up.
    key_width: f32,
}

fn help_layout(window_width: f32) -> HelpLayout {
    let sidebar = if window_width < 900.0 {
        SIDEBAR - 30.0
    } else {
        SIDEBAR
    };
    let content = (window_width - sidebar - 1.0).max(0.0);
    let inner = (content - HELP_PAD_X * 2.0).max(0.0);
    let columns = if inner >= 1200.0 {
        4
    } else if inner >= 760.0 {
        2
    } else {
        1
    };
    HelpLayout {
        columns,
        stacked: columns >= 4,
        key_width: if columns == 1 { 220.0 } else { 196.0 },
    }
}

/// One shortcut label: chords separated by ` / `, each chord split on `+` or a
/// leading `⌘`, plus an optional `(context)` suffix.
struct ShortcutLabel {
    chords: Vec<Vec<String>>,
    note: Option<String>,
}

fn shortcut_label(raw: &str) -> ShortcutLabel {
    let (body, note) = match raw.rfind(" (") {
        Some(start) if raw.ends_with(')') && start + 2 < raw.len() - 1 => (
            &raw[..start],
            Some(raw[start + 2..raw.len() - 1].to_owned()),
        ),
        _ => (raw, None),
    };
    let chords = body
        .split(" / ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(split_chord)
        .collect();
    ShortcutLabel { chords, note }
}

fn split_chord(raw: &str) -> Vec<String> {
    // "Ctrl+K" and "Shift+Tab" are chords. A lone "+" is the zoom-in key, and
    // splitting it would throw the plus away.
    let parts: Vec<&str> = raw
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 2 {
        return parts.into_iter().map(str::to_owned).collect();
    }
    if let Some(rest) = raw.strip_prefix('⌘')
        && !rest.is_empty()
    {
        return vec!["⌘".to_owned(), rest.to_owned()];
    }
    vec![raw.to_owned()]
}

/// A single letter draws in uppercase, the way a keycap is printed. Names
/// (`Ctrl`, `Enter`, `Space`) and symbols stay as written.
fn keycap_text(key: &str) -> String {
    let upper = key.to_uppercase();
    if key.chars().count() == 1 && upper.to_lowercase() == key {
        upper
    } else {
        key.to_owned()
    }
}

/// DES-01: the shortcut map. A section title and each row are catalog keys;
/// the chords are geometry, not copy, so they stay literals here.
fn shortcut_groups() -> [(TextKey, &'static [(&'static str, TextKey)]); 6] {
    [
        (
            TextKey::HelpGroupNavigation,
            &[
                ("⌘K / Ctrl+K", TextKey::HelpNavFocusSearch),
                ("↑ / ↓ (search)", TextKey::HelpNavMoveHighlightedResult),
                ("Enter (search)", TextKey::HelpNavOpenHighlightedResult),
                ("⌘[ / Ctrl+[", TextKey::HelpNavBackInNoteHistory),
                ("⌘] / Ctrl+]", TextKey::HelpNavForwardInNoteHistory),
                (
                    "⌘1 / ⌘2 / ⌘3 / ⌘4 / ⌘5",
                    TextKey::HelpNavWriteReadGraphBoardKanban,
                ),
                ("⌘+⇧+F / Ctrl+Shift+F", TextKey::HelpNavFocusMode),
                ("Esc", TextKey::HelpNavCloseOutermostLayer),
                ("⌘+⇧+P / Ctrl+Shift+P", TextKey::HelpNavOpenCommandPalette),
            ],
        ),
        (
            TextKey::HelpGroupEdit,
            &[
                ("⌘F / Ctrl+F", TextKey::HelpEditFindInOpenNote),
                ("⌘F / Ctrl+F (Kanban)", TextKey::HelpEditFindInBoard),
                (
                    "Filter (Kanban header)",
                    TextKey::HelpEditHideCardsByTextTagDateOrState,
                ),
                (
                    "Board settings (Kanban header)",
                    TextKey::HelpEditPerBoardOverrides,
                ),
                ("⌘P / Ctrl+P", TextKey::HelpEditInsertWikilink),
                ("⌘N / Ctrl+N", TextKey::HelpEditNewNote),
                ("⌘I / Ctrl+I", TextKey::HelpEditImportDocument),
                ("⌘S / Ctrl+S", TextKey::HelpEditSaveNow),
                ("⌘E / Ctrl+E", TextKey::HelpEditToggleWriteRead),
                ("[[", TextKey::HelpEditWikilinkCompletion),
                ("# (card editor)", TextKey::HelpEditVaultTagCompletion),
                ("Tab (editor)", TextKey::HelpEditIndentFourSpaces),
                ("Paste (editor)", TextKey::HelpEditPasteRichText),
                ("F2", TextKey::HelpEditRenameSelectedNote),
                ("Del", TextKey::HelpEditDeleteSelectedNote),
            ],
        ),
        (
            TextKey::HelpGroupKanban,
            &[
                ("Tab / Shift+Tab", TextKey::HelpKanbanMoveThroughControls),
                ("↑ / ↓ (lane)", TextKey::HelpKanbanMoveBetweenLaneCards),
                ("← / → (board)", TextKey::HelpKanbanMoveSelectionAcrossLanes),
                ("Space / Enter", TextKey::HelpKanbanToggleCardDone),
                ("Alt+Arrows", TextKey::HelpKanbanMoveSelectedCard),
                ("Alt+Shift+Arrows", TextKey::HelpKanbanMoveCardToEdgeOrLane),
                ("Shift+F10 / Menu", TextKey::HelpKanbanOpenCardMenu),
                ("Double-click", TextKey::HelpKanbanEditCard),
                ("Esc", TextKey::HelpKanbanCloseMenuOrConfirmation),
            ],
        ),
        (
            TextKey::HelpGroupGraph,
            &[
                ("⌘G / Ctrl+G", TextKey::HelpGraphToggleGraph),
                ("⌘L / Ctrl+L", TextKey::HelpGraphToggleGlobalLocal),
                ("Arrows", TextKey::HelpGraphMoveFocusRing),
                ("Enter", TextKey::HelpGraphOpenFocusedNote),
                ("+ / -", TextKey::HelpGraphZoomInOut),
                ("0 / F", TextKey::HelpGraphFitVisibleNodes),
                ("I", TextKey::HelpGraphToggleIsolate),
                ("Esc", TextKey::HelpGraphLeaveGraph),
            ],
        ),
        (
            TextKey::HelpGroupBoard,
            &[
                ("H / Shift+wheel", TextKey::HelpBoardPanWithDrag),
                ("Wheel / pinch", TextKey::HelpBoardZoomAtPointer),
                ("+ / -", TextKey::HelpBoardZoomAtCanvasCentre),
                ("Arrows (nothing selected)", TextKey::HelpBoardPanTheBoard),
                ("0 / F / double-click", TextKey::HelpBoardFitTheDrawing),
                ("R / D / O / A / L / P / X", TextKey::HelpBoardShapeTools),
                ("Drag with a tool", TextKey::HelpBoardDrawShape),
                ("T / 8", TextKey::HelpBoardPlaceText),
                ("Double-click a shape", TextKey::HelpBoardEditTextLabel),
                ("V / 1", TextKey::HelpBoardDropTheTool),
                (
                    "Click / Shift+click",
                    TextKey::HelpBoardSelectElementOrToggle,
                ),
                ("Arrows (selection)", TextKey::HelpBoardNudgeSelection),
                ("Delete / Backspace", TextKey::HelpBoardDeleteSelection),
                ("⌘A / Ctrl+A", TextKey::HelpBoardSelectEveryElement),
                ("⌘L / Ctrl+L", TextKey::HelpBoardLinkToNote),
                ("⌘Enter / Ctrl+Enter", TextKey::HelpBoardOpenLinkedNote),
                (
                    "⌘click / Ctrl+click",
                    TextKey::HelpBoardOpenClickedLinkedNote,
                ),
                ("⌘Z / ⌘⇧Z / Ctrl+Y", TextKey::HelpBoardUndoRedo),
                ("⌘S / Ctrl+S", TextKey::HelpBoardSaveDrawing),
                ("Esc", TextKey::HelpBoardEscapeClearsSelection),
            ],
        ),
        (
            TextKey::HelpGroupSidebar,
            &[
                ("⌘?", TextKey::HelpSidebarOpenThisHelp),
                ("?", TextKey::HelpSidebarOpenHelpWhenNotTyping),
                ("Tab / Shift+Tab", TextKey::HelpSidebarMoveBetweenPanes),
                ("↑ / ↓ (list)", TextKey::HelpSidebarMoveThroughAList),
                ("Enter / Space", TextKey::HelpSidebarOpenFocusedRow),
                ("Right-click", TextKey::HelpSidebarNoteAndFolderActions),
                ("⌘, / Ctrl+,", TextKey::HelpSidebarSettings),
            ],
        ),
    ]
}

/// #134: the lock/unlock password card.
///
/// Drawn at the window root above the palette, so the sidebar, title bar and
/// note page behind it are inert while it is open. `modal` gives Escape and a
/// press outside the card the same meaning as Cancel.
fn lock_prompt_layer(s: &State, k: &Kit) -> El {
    let Some(prompt) = &s.lock_prompt else {
        return el();
    };
    let lock = prompt.kind == LockPromptKind::Lock;
    let title = if lock {
        t(TextKey::LockNoteTitle)
    } else {
        t(TextKey::UnlockNoteTitle)
    };
    let note_title = s
        .vault
        .notes
        .get(prompt.id)
        .map_or_else(String::new, |note| note.title.clone());
    let mut card = col()
        .key("lock-prompt")
        .w(460.0)
        .max_w(460.0)
        .min_w(0.0)
        .gap(10.0)
        .padding(22.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .shadow(10.0, 32.0, k.alpha(k.c.black, 0.35))
        .role("dialog")
        .label(title)
        .modal(true, |s: &mut State| s.cancel_lock_prompt())
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(8.0)
                .center_y()
                .child(icon(&ic::LOCK, 16.0).color(k.c.icon_muted).a11y_hidden())
                .child(
                    text(title)
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        )
        .child(
            text(note_title)
                .key("lock-prompt-note")
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate()
                .w_full(),
        );
    if lock {
        // The one sentence that matters most in the flow: it is the only
        // warning, and it wraps so no language truncates it.
        card = card.child(
            text(t(TextKey::LockLostPasswordWarning))
                .key("lock-warning")
                .font(k.ty.caption())
                .color(k.c.warning.fg)
                .wrap()
                .w_full(),
        );
    }
    card = card
        .child(lock_field_label(
            k,
            "lock-password-label",
            TextKey::LockPassword,
        ))
        .child(
            row()
                .key("lock-password-box")
                .w_full()
                .h(30.0)
                .px(9.0)
                .center_y()
                .radius(k.radius.md)
                .fill(k.c.layer_02)
                .border(1.0, k.c.border_muted)
                .child(
                    text_input(&prompt.password, k.ty.ui_compact)
                        .key("lock-password")
                        .label(t(TextKey::LockPassword))
                        .single_line()
                        .w_full()
                        .color(k.c.text_base)
                        .caret(k.c.text_accent, k.selection())
                        .on_edit(|s: &mut State| {
                            &mut s
                                .lock_prompt
                                .as_mut()
                                .expect("the lock prompt is open")
                                .password
                        })
                        .on_input_key(lock_prompt_key)
                        .on_submit(|s: &mut State, _| s.submit_lock_prompt()),
                ),
        );
    if lock {
        card = card
            .child(lock_field_label(
                k,
                "lock-confirm-label",
                TextKey::LockConfirmPassword,
            ))
            .child(
                row()
                    .key("lock-confirm-box")
                    .w_full()
                    .h(30.0)
                    .px(9.0)
                    .center_y()
                    .radius(k.radius.md)
                    .fill(k.c.layer_02)
                    .border(1.0, k.c.border_muted)
                    .child(
                        text_input(&prompt.confirm, k.ty.ui_compact)
                            .key("lock-confirm")
                            .label(t(TextKey::LockConfirmPassword))
                            .single_line()
                            .w_full()
                            .color(k.c.text_base)
                            .caret(k.c.text_accent, k.selection())
                            .on_edit(|s: &mut State| {
                                &mut s
                                    .lock_prompt
                                    .as_mut()
                                    .expect("the lock prompt is open")
                                    .confirm
                            })
                            .on_input_key(lock_prompt_key)
                            .on_submit(|s: &mut State, _| s.submit_lock_prompt()),
                    ),
            );
    }
    card = card.child(if let Some(error) = prompt.error {
        text(t(error))
            .key("lock-prompt-error")
            .font(k.ty.caption())
            .color(k.c.danger.fg)
            .wrap()
            .w_full()
    } else {
        el()
    });
    card = card.child(
        row()
            .key("lock-prompt-actions")
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .justify_end()
            .child(
                Button::new(t(TextKey::Cancel))
                    .variant(ButtonVariant::Ghost)
                    .on_press(|s: &mut State| s.cancel_lock_prompt())
                    .build(k)
                    .key("lock-prompt-cancel"),
            )
            .child(
                Button::new(if lock {
                    t(TextKey::LockSubmit)
                } else {
                    t(TextKey::UnlockSubmit)
                })
                .variant(ButtonVariant::Submit)
                .on_press(|s: &mut State| s.submit_lock_prompt())
                .build(k)
                .key("lock-prompt-submit"),
            ),
    );
    el().key("lock-prompt-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(k.c.scrim)
        .center()
        .child(card)
}

/// The caption above one prompt field.
fn lock_field_label(k: &Kit, key: &str, label: TextKey) -> El {
    text(t(label))
        .key(key.to_owned())
        .font(k.ty.caption())
        .color(k.c.text_muted)
        .w_full()
}

/// Keys inside a prompt field.
///
/// Escape closes the prompt (the secure field would otherwise blur and swallow
/// it), and a command chord is swallowed: Help and the palette are modal
/// layers behind this one, so a chord must not open them over the password
/// card. Every other key is the field's — typing, arrows, Backspace and Enter.
fn lock_prompt_key(s: &mut State, e: &KeyEvent, _: &mut Tx) -> bool {
    if e.key == Key::Escape {
        s.cancel_lock_prompt();
        return true;
    }
    e.mods.cmd
}

/// v0.5: the command palette, mounted above the whole window. One input
/// searches notes (title-first) and the fixed action list; the card animates in
/// with the same offset + fade pair as the kit-gallery's stagger list.
fn palette_layer(s: &State, k: &Kit) -> El {
    if s.palette.is_none() {
        return el();
    }
    let items = s.palette_items();
    let selected = s.palette_selected();
    let mut rows = col()
        .key("palette-items")
        .w_full()
        .min_w(0.0)
        .gap(1.0)
        .pb(6.0);
    let mut section = "";
    for (index, item) in items.iter().enumerate() {
        if item.section() != section {
            section = item.section();
            rows = rows.child(palette_section(k, section));
        }
        rows = rows.child(palette_row(s, k, index, item, index == selected));
    }
    if items.is_empty() {
        rows = rows.child(
            row()
                .key("palette-empty")
                .w_full()
                .h(64.0)
                .center()
                .a11y_hidden()
                .child(
                    text(t(TextKey::PaletteEmpty))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        );
    }
    let mut card = col()
        .key("palette")
        .w(560.0)
        .max_w(560.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .shadow(10.0, 32.0, k.alpha(k.c.black, 0.35))
        .transition(Transition::offset(0.0, 12.0).combined(Transition::opacity()))
        .role("dialog")
        .label(t(TextKey::PaletteTitle))
        // Slice 16: a real modal, so Escape and Tab never reach the page
        // behind it and the background is inert.
        .modal(true, |s: &mut State| s.close_palette())
        .child(
            row()
                .key("palette-search")
                .w_full()
                .h(52.0)
                .px(16.0)
                .gap(10.0)
                .center_y()
                .child(icon(&ic::MAGNIFYING_GLASS, 15.0).color(k.c.icon_muted))
                .child(
                    text_input(&s.palette_field, k.ty.ui_compact)
                        .key("palette-input")
                        .label(t(TextKey::PaletteTitle))
                        .active_descendant(format!("palette-item-{selected}"))
                        .single_line()
                        .grow(1.0)
                        .color(k.c.text_base)
                        .caret(k.c.text_accent, k.selection())
                        .placeholder(t(TextKey::PaletteTitle), k.c.text_faint)
                        .on_edit(|s: &mut State| &mut s.palette_field)
                        .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                            match e.key {
                                Key::Escape => {
                                    s.close_palette();
                                    return true;
                                }
                                Key::Down => {
                                    s.palette_step(1);
                                    return true;
                                }
                                Key::Up => {
                                    s.palette_step(-1);
                                    return true;
                                }
                                _ => {}
                            }
                            crate::shortcut(s, e, tx)
                        })
                        .on_submit(|s: &mut State, tx: &mut Tx| {
                            s.palette_run(tx);
                            // Focus mode (the only chrome-changing action) comes
                            // and goes at once under reduced motion.
                            if s.reduced_motion() {
                                tx.instant();
                            }
                        }),
                ),
        )
        .child(divider_h(k))
        .child(
            col()
                .key("palette-scroll")
                .w_full()
                .min_w(0.0)
                .min_h(0.0)
                .max_h(360.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .role("listbox")
                .label(if items.is_empty() {
                    t(TextKey::PaletteEmpty)
                } else {
                    t(TextKey::CommonResults)
                })
                .child(rows),
        )
        .child(
            row()
                .key("palette-footer")
                .w_full()
                .h(34.0)
                .px(14.0)
                .gap(10.0)
                .center_y()
                .fill(k.c.layer_02)
                .child(
                    text(results(items.len()))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                )
                .child(spacer())
                .child(
                    text(t(TextKey::PaletteFooter))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        );
    // Slice 16: the palette is a modal. When it closes onto the Kanban page
    // (an item that opens a kanban, or Escape while the page is up) the page
    // takes the keyboard, rather than the field the palette was opened from;
    // Rune restores that field when a modal goes, so the hint is always set
    // and only resolves when `kanban-page` is in the new tree.
    card = card.focus_return("kanban-page");
    el().key("palette-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(k.c.scrim)
        .center()
        .child(card)
}

fn palette_section(k: &Kit, title: &str) -> El {
    row()
        .key(format!("palette-section-{}", title.to_lowercase()))
        .w_full()
        .px(14.0)
        .pt(8.0)
        .pb(3.0)
        // Slice 16 fix: the listbox holds options only; a section heading is
        // visual grouping, not an option.
        .a11y_hidden()
        .child(
            text(title.to_uppercase())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

fn palette_row(s: &State, k: &Kit, index: usize, item: &PaletteItem, selected: bool) -> El {
    let (title, subtitle, hint, dot) = match item {
        PaletteItem::Recent(id) | PaletteItem::Note(id) => {
            let note = s.vault.notes.get(*id);
            let title = note.map_or_else(
                || t(TextKey::PaletteMissingNote).to_owned(),
                |n| n.title.clone(),
            );
            let folder = note.map(|n| n.folder()).unwrap_or_default();
            let subtitle = if folder.is_empty() {
                t(TextKey::CommonVaultRoot).to_owned()
            } else {
                folder.to_owned()
            };
            (title, subtitle, String::new(), k.c.text_accent)
        }
        PaletteItem::Kanban(id) => {
            let note = s.vault.notes.get(*id);
            let title = note.map_or_else(
                || t(TextKey::PaletteMissingNote).to_owned(),
                |n| n.title.clone(),
            );
            let folder = note.map(|n| n.folder()).unwrap_or_default();
            let subtitle = if folder.is_empty() {
                t(TextKey::PaletteKanbanRoot).to_owned()
            } else {
                palette_kanban_folder(folder)
            };
            (title, subtitle, String::new(), k.c.text_accent)
        }
        PaletteItem::Action(action) => (
            action.label().to_owned(),
            String::new(),
            action.hint().to_owned(),
            k.c.text_muted,
        ),
    };
    let mut text_column = col().grow(1.0).min_w(0.0).gap(1.0).child(
        text(title.clone())
            .font(k.ty.ui_medium)
            .color(if selected {
                k.c.text_accent
            } else {
                k.c.text_base
            })
            .truncate(),
    );
    if !subtitle.is_empty() {
        text_column = text_column.child(
            text(subtitle)
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .truncate(),
        );
    }
    let mut row = row()
        .key(format!("palette-item-{index}"))
        .w_full()
        .min_h(42.0)
        .px(14.0)
        .py(6.0)
        .gap(10.0)
        .radius(6.0)
        .center_y()
        .fill(if selected {
            k.alpha(k.c.text_accent, 0.12)
        } else {
            Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .cursor(Cursor::Pointer)
        .role("option")
        .label(title.clone())
        .focus_selected(selected)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.palette_run_index(index, tx))
        .child(
            el().size(6.0, 6.0)
                .radius(3.0)
                .fill(if selected { k.c.text_accent } else { dot }),
        )
        .child(text_column);
    if !hint.is_empty() {
        row = row.child(text(hint).font(k.ty.caption()).color(k.c.text_faint));
    }
    row
}

/// DES-01: the in-window Help overlay. Native menu bars are unavailable on
/// Linux (the desktop verification wave blocked DES-01's menubar path), so the
/// full shortcut map lives here, reachable from the sidebar `?` button,
/// `Cmd/Ctrl+?`, or plain `?` when no text field owns the keyboard.
fn help_panel(s: &State, k: &Kit) -> El {
    let layout = help_layout(s.window_width);
    let mut sections = el()
        .key("help-sections")
        .w_full()
        .min_w(0.0)
        .equal_columns(layout.columns)
        .gap(12.0)
        .items_stretch();
    for (index, (title, shortcuts)) in shortcut_groups().into_iter().enumerate() {
        sections = sections.child(help_section(k, index, title, shortcuts, &layout));
    }
    col()
        .key("help-panel")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(help_header(k))
        .child(divider_h(k))
        .child(
            col()
                .key("help-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .w_full()
                        .min_w(0.0)
                        .px(HELP_PAD_X)
                        .pt(18.0)
                        .pb(28.0)
                        .gap(14.0)
                        .child(sections)
                        .child(help_about(s, k))
                        .child(help_diagnostics(s, k))
                        .child(help_trash(s, k)),
                ),
        )
}

fn help_header(k: &Kit) -> El {
    row()
        // #185: the same 58 pt header and 28 pt inset as Settings, Board and
        // the Kanban panels, so the shared Close lands on the same centre line.
        .h(58.0)
        .px(HELP_PAD_X)
        .gap(12.0)
        .center_y()
        .child(
            text(t(TextKey::KeyboardShortcuts))
                .font(TextStyle::new(16.0, 600.0).lh(22.0))
                .color(k.c.text_base),
        )
        .child(spacer())
        .child(close_button(k, "help-close", |s: &mut State| {
            s.close_help();
        }))
}

fn help_section(
    k: &Kit,
    index: usize,
    title: TextKey,
    shortcuts: &[(&str, TextKey)],
    layout: &HelpLayout,
) -> El {
    let title = t(title);
    let mut rows = col().w_full().min_w(0.0).gap(2.0);
    for (row_index, (keys, action)) in shortcuts.iter().enumerate() {
        rows = rows.child(help_row(k, index, row_index, keys, t(*action), layout));
    }
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    col()
        .key(format!("help-section-{slug}"))
        .w_full()
        .min_w(0.0)
        .self_stretch()
        .gap(10.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .px(14.0)
        .pt(12.0)
        .pb(10.0)
        .role("group")
        .label(title.to_owned())
        .child(help_section_head(k, title))
        .child(rows)
}

fn help_section_head(k: &Kit, title: &str) -> El {
    col()
        .w_full()
        .gap(8.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(el().w(3.0).h(14.0).radius(2.0).fill(k.c.text_accent))
                .child(
                    text(title.to_owned())
                        .font(TextStyle::new(14.0, 620.0).lh(18.0))
                        .color(k.c.text_base),
                ),
        )
        .child(el().w_full().h(1.0).fill(k.c.border_muted))
}

/// The autosave sentence and the version, in their own card so they are not
/// a caption glued to the log.
fn help_about(s: &State, k: &Kit) -> El {
    let delay = s.autosave_delay();
    let app_version = env!("CARGO_PKG_VERSION");
    help_card(k, "help-about", t(TextKey::SettingsAutosave))
        .child(
            text(autosave_help(delay))
                .font(k.ty.ui_compact.lh(18.0))
                .color(k.c.text_base)
                .wrap()
                .w_full(),
        )
        .child(
            text(version(app_version))
                .font(k.ty.ui_compact)
                .color(k.c.text_muted),
        )
}

/// X-01: a selectable diagnostics block plus "Copy diagnostics".
///
/// `Tx::copy` is available in this Rune revision, so the version, counts and
/// last five session-log lines are one click from a bug report. Lines wrap
/// inside the block (a session path is one long token) and the block scrolls
/// once wrapped lines outgrow it. If a future Rune drops `Tx::copy`, the
/// block is still visible and selectable.
fn help_diagnostics(s: &State, k: &Kit) -> El {
    let (summary, logs) = diagnostics_parts(&s.diagnostics());
    let mono = TextStyle {
        size: 11.0,
        line_height: 16.0,
        ..k.ty.mono
    };
    let mut log = col()
        .key("help-log")
        .w_full()
        .min_w(0.0)
        .max_h(148.0)
        .gap(3.0)
        .radius(6.0)
        .padding(10.0)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_base)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    for (index, line) in logs.iter().enumerate() {
        log = log.child(
            text(line.clone())
                .key(format!("help-log-line-{index}"))
                .font(mono)
                .color(k.c.text_muted)
                .wrap_anywhere()
                .w_full()
                .selectable(true),
        );
    }
    let mut body =
        help_card(k, "help-diagnostics", t(TextKey::SettingsDiagnostics)).selection_group(true);
    for (index, line) in summary.iter().enumerate() {
        body = body.child(
            text(line.clone())
                .key(format!("help-diag-line-{index}"))
                .font(k.ty.ui_compact.lh(18.0))
                .color(k.c.text_base)
                .wrap_anywhere()
                .w_full()
                .selectable(true),
        );
    }
    if logs.iter().all(|line| line == "(no log lines yet)") {
        body = body.child(log);
    } else {
        body = body
            .child(
                text(t(TextKey::HelpLastLogLines))
                    .font(k.ty.caption())
                    .color(k.c.text_muted),
            )
            .child(log);
    }
    body.child(
        row()
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .flex_wrap()
            .child(help_link(
                k,
                "help-copy-diagnostics",
                t(TextKey::SettingsCopyDiagnostics),
                true,
            ))
            .child(help_link(
                k,
                "help-report-issue",
                t(TextKey::SettingsReportIssue),
                false,
            )),
    )
}

/// Summary lines, then the log tail. `(no log lines yet)` is the tail, not a
/// summary line: `State::diagnostics` prints it in place of the tail header.
fn diagnostics_parts(text: &str) -> (Vec<String>, Vec<String>) {
    let mut summary = Vec::new();
    let mut logs = Vec::new();
    let mut in_logs = false;
    for line in text.lines() {
        if line == "last log lines:" {
            in_logs = true;
            continue;
        }
        if line == "(no log lines yet)" {
            logs.push(line.to_owned());
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if in_logs {
            logs.push(line.to_owned());
        } else {
            summary.push(line.to_owned());
        }
    }
    if logs.is_empty() {
        logs.push("(no log lines yet)".to_owned());
    }
    (summary, logs)
}

fn help_link(k: &Kit, key: &str, label: &str, copies: bool) -> El {
    let key_owned = key.to_owned();
    let label_owned = label.to_owned();
    let mut button = pressable(k, key_owned)
        .px(10.0)
        .py(5.0)
        .border(1.0, k.c.border_base)
        .child(
            text(label_owned.clone())
                .font(k.ty.ui_compact)
                .color(k.c.text_accent),
        )
        .label(label_owned);
    if copies {
        button = button.on_press_tx(|s: &mut State, tx: &mut Tx| {
            tx.copy(s.diagnostics());
        });
    } else {
        button = button.on_press_tx(|_: &mut State, tx: &mut Tx| {
            // Best-effort: the platform may have no URL opener.
            let _ = tx.open_url(ISSUES_URL);
        });
    }
    button
}

/// Trash readout plus the permanent-empty action. The budget is a fixed
/// constant in `muninn-core` (500 files or 100 MB, oldest timestamp folder
/// first), deliberately not a setting.
fn help_trash(s: &State, k: &Kit) -> El {
    let (files, bytes) = s.trash_size();
    let summary = if files == 0 {
        trash_empty().to_owned()
    } else {
        trash_summary(files, &trash_bytes(bytes))
    };
    let empty = files == 0;
    help_card(k, "help-trash", t(TextKey::TrashTitle))
        .child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(10.0)
                .center_y()
                .child(
                    text(summary)
                        .font(k.ty.ui_compact.lh(18.0))
                        .color(k.c.text_base)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    pressable(k, "help-empty-trash".to_owned())
                        .px(10.0)
                        .py(5.0)
                        .border(1.0, k.c.border_base)
                        .child(
                            text(t(TextKey::TrashEmptyAction))
                                .font(k.ty.ui_compact)
                                .color(if empty {
                                    k.c.text_faint
                                } else {
                                    k.c.text_accent
                                }),
                        )
                        .label(t(TextKey::TrashEmptyAction))
                        .on_press(|s: &mut State| {
                            s.empty_trash();
                        }),
                ),
        )
        .child(
            text(t(TextKey::TrashExplainer))
                .font(k.ty.ui_compact.lh(18.0))
                .color(k.c.text_muted)
                .wrap()
                .w_full(),
        )
}

/// Shared chrome for the autosave, diagnostics and trash blocks.
fn help_card(k: &Kit, key: &str, title: &str) -> El {
    col()
        .key(key.to_owned())
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .padding(14.0)
        .child(
            text(title.to_owned())
                .font(TextStyle::new(12.0, 650.0).tracking(0.4).lh(16.0))
                .color(k.c.text_muted),
        )
}

/// Human byte size for the trash readout. The cap is 100 MB, so one decimal
/// is enough to make growth visible without a wall of digits.
fn trash_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn help_row(
    k: &Kit,
    section: usize,
    row_index: usize,
    keys: &str,
    action: &str,
    layout: &HelpLayout,
) -> El {
    let keys_el = shortcut_keys(
        k,
        section,
        row_index,
        keys,
        if layout.stacked {
            None
        } else {
            Some(layout.key_width)
        },
    );
    let action_el = text(action.to_owned())
        .key(format!("help-action-{section}-{row_index}"))
        .font(k.ty.ui_compact.lh(18.0))
        .color(k.c.text_base)
        .wrap()
        .min_w(0.0);
    if layout.stacked {
        col()
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .py(6.0)
            .child(keys_el)
            .child(action_el.w_full())
    } else {
        row()
            .w_full()
            .min_w(0.0)
            .min_h(32.0)
            .gap(12.0)
            .py(4.0)
            .center_y()
            .child(keys_el)
            .child(action_el.grow(1.0))
    }
}

fn shortcut_keys(k: &Kit, section: usize, row_index: usize, raw: &str, width: Option<f32>) -> El {
    let label = shortcut_label(raw);
    let mut keys = row().gap(6.0).center_y().flex_wrap().min_w(0.0);
    keys = match width {
        Some(width) => keys.w(width),
        None => keys.w_full(),
    };
    let mut nth = 0;
    for (chord_index, chord) in label.chords.iter().enumerate() {
        if chord_index > 0 {
            keys = keys.child(text("/").font(k.ty.caption()).color(k.c.text_faint));
        }
        let mut group = row().gap(3.0).center_y();
        for key in chord {
            group = group.child(keycap(
                k,
                &format!("help-key-{section}-{row_index}-{nth}"),
                key,
            ));
            nth += 1;
        }
        keys = keys.child(group);
    }
    if let Some(note) = label.note {
        keys = keys.child(
            text(format!("({note})"))
                .key(format!("help-note-{section}-{row_index}"))
                .font(k.ty.caption())
                .color(k.c.text_muted),
        );
    }
    keys
}

fn keycap(k: &Kit, key: &str, label: &str) -> El {
    row()
        .key(key.to_owned())
        .h(22.0)
        .min_w(22.0)
        .px(6.0)
        .center()
        .radius(5.0)
        .fill(k.c.layer_03)
        .border(1.0, k.c.border_strong)
        .shadow(
            1.0,
            0.0,
            k.alpha(k.c.black, if k.dark { 0.45 } else { 0.16 }),
        )
        .child(
            text(keycap_text(label))
                .key(format!("{key}-label"))
                .font(TextStyle::new(12.0, 560.0).mono().lh(14.0))
                .color(k.c.text_base),
        )
}

/// A large vault is still being read: show bounded progress and the one action
/// that matters (Cancel, also on Escape). The invisible 1×1 text field is the
/// Escape owner: a focused field sees keys before the window shortcut table,
/// and the loading screen has no editor to hand focus to.
fn loading_page(s: &State, k: &Kit) -> El {
    let (done, total) = s.load_progress().unwrap_or((0, 0));
    let fraction = if total == 0 {
        0.0
    } else {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    };
    let mut bar = row()
        .key("scan-progress-bar")
        .w_full()
        .max_w(420.0)
        .h(6.0)
        .radius(3.0)
        .clip_rounded(3.0)
        .fill(k.alpha(k.c.text_faint, 0.18));
    if fraction > 0.0 {
        bar = bar.child(el().h(6.0).grow(fraction).radius(3.0).fill(k.c.text_accent));
    }
    if fraction < 1.0 {
        bar = bar.child(el().h(6.0).grow(1.0 - fraction));
    }
    col()
        .key("scan-loading")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .items_center()
        .gap(12.0)
        .child(
            text(t(TextKey::StartupScanning))
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(bar)
        .child(
            text(scan_progress(done, total))
                .key("scan-progress-label")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(status_button(
            k,
            t(TextKey::Cancel),
            "scan-cancel",
            State::cancel_scan,
        ))
        .child(
            text_input(&s.scan_field, k.ty.ui_compact)
                .key("scan-escape")
                .a11y_hidden()
                .single_line()
                .size(1.0, 1.0)
                .opacity(0.01)
                .on_edit(|s: &mut State| &mut s.scan_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
                    if e.key == Key::Escape && s.loading() {
                        s.cancel_scan();
                        true
                    } else {
                        false
                    }
                }),
        )
}

/// The whole vault could not be scanned: show the OS error and Retry instead
/// of an empty editor (which would read as "your notes are gone").
fn vault_error_page(s: &State, k: &Kit) -> El {
    let message = s.startup_error.clone().unwrap_or_default();
    let cancelled = message.starts_with("Scan cancelled");
    let title = if cancelled {
        t(TextKey::StartupScanCancelled)
    } else {
        t(TextKey::StartupCouldNotOpen)
    };
    let hint = if cancelled {
        t(TextKey::StartupRetryHint)
    } else {
        t(TextKey::StartupPickFolderHint)
    };
    col()
        .key("vault-error")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(40.0)
        .center()
        .items_center()
        .gap(9.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.danger.fg))
        .child(
            text(message)
                .font(k.ty.caption())
                .max_w(560.0)
                .color(k.c.text_muted),
        )
        .child(
            text(hint)
                .font(k.ty.caption())
                .max_w(560.0)
                .color(k.c.text_faint),
        )
        .child(status_button(
            k,
            t(TextKey::StatusRetry),
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
            text(t(TextKey::BrokenAllResolve))
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
                    text(referrers(count))
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
                    text(t(TextKey::BrokenTitle))
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text(targets(broken.len()))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                )
                .child(spacer())
                .child(close_button(k, "broken-close", |s: &mut State| {
                    s.close_broken_links();
                })),
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
            t(TextKey::StatusRetry),
            "vault-retry",
            State::retry_startup,
        ))
}

/// SAF-05: dirty buffers found in the state directory at startup. Every row
/// restores one buffer; Restore all applies them in order (saving each before
/// the next), and Dismiss discards every listed copy after an explicit click.
fn recovery_banner(s: &State, k: &Kit) -> El {
    let count = s.recovered.len();
    let boards = s
        .recovered
        .iter()
        .filter(|e| crate::board_safety::is_board_rel(&e.rel))
        .count();
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
            text(recovered_for(&recovered_copies(count - boards, boards)))
                .font(k.ty.caption())
                .color(k.c.warning.fg)
                .truncate(),
        );
    for (index, entry) in s.recovered.iter().take(3).enumerate() {
        let rel = entry.rel.clone();
        let slot = entry.slot;
        // One copy keeps the file name, so a single row lays out as before.
        // Two copies of one note say which buffer each row restores.
        let both = s.recovered.iter().filter(|other| other.rel == rel).count() > 1;
        let mut row_el = row()
            .gap(4.0)
            .center_y()
            // The path gives up its 190 pt budget first, so the slot suffix
            // below stays whole and two rows for one note stay distinct.
            .child(
                text(rel.clone())
                    .key(format!("recovery-label-{index}"))
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .max_w(190.0)
                    .truncate()
                    .help_when_truncated(rel.clone()),
            );
        if both {
            let suffix = match slot {
                crate::recovery::RecoverySlot::Kanban => t(TextKey::RecoveryKanbanEdits),
                crate::recovery::RecoverySlot::Editor => t(TextKey::RecoveryWriteEdits),
            };
            row_el = row_el.child(
                text(suffix)
                    .key(format!("recovery-slot-{index}"))
                    .font(k.ty.caption())
                    .color(k.c.text_muted),
            );
        }
        banner = banner.child(
            row_el.child(
                pressable(k, format!("recovery-restore-{index}"))
                    .px(6.0)
                    .py(3.0)
                    .child(
                        text(t(TextKey::RecoveryRestore))
                            .font(k.ty.caption())
                            .color(k.c.text_base),
                    )
                    .on_press(move |s: &mut State| {
                        s.restore_recovery_slot(&rel, slot);
                    }),
            ),
        );
    }
    if count > 3 {
        banner = banner.child(
            text(more_edits(count - 3))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    banner
        .child(spacer())
        .child(status_button(
            k,
            t(TextKey::RecoveryRestoreAll),
            "recovery-restore-all",
            State::restore_all_recovery,
        ))
        .child(status_button(
            k,
            t(TextKey::RecoveryDismiss),
            "recovery-dismiss",
            State::dismiss_recovery,
        ))
}

fn topbar(s: &State, k: &Kit) -> El {
    // Kanban slice 3 (s2 review N4): on the Kanban page the title is the
    // kanban shown, which may not be the selected note.
    let note = match (s.mode, &s.kanban) {
        (Mode::Kanban, Some(shown)) => s.note_id(&shown.rel),
        _ => s.selected,
    };
    let (title, path) = match note.and_then(|id| s.vault.notes.get(id)) {
        Some(note) => (note.title.clone(), note.rel.clone()),
        None => (t(TextKey::TopbarNoNoteSelected).to_owned(), String::new()),
    };
    // A kanban note opened as a note offers its board.
    let open_as_kanban = s.mode.is_note_mode()
        && s.selected
            .and_then(|id| s.vault.notes.get(id))
            .is_some_and(|note| note.is_kanban());
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
                        .key("topbar-title")
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
    let mut bar = row()
        .h(58.0)
        .px(20.0)
        .gap(12.0)
        .center_y()
        .child(sidebar_toggle(s, k));
    // #186: Focus is the note page's own mode, so the button is offered only
    // where it can act. Leaving it out of the tree (rather than mounting an
    // empty slot) keeps the row's 12 pt gap from opening a hole on Graph,
    // Board and Kanban.
    if s.mode.is_note_mode() {
        bar = bar.child(focus_toggle(s, k));
    }
    bar.child(history_button(
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
    .child(if open_as_kanban {
        crate::kanban_view::header_button(
            k,
            "open-as-kanban",
            &ic::CHECKLIST,
            t(TextKey::TopbarOpenAsKanban),
            |s: &mut State| s.set_mode(Mode::Kanban),
        )
    } else {
        el()
    })
    .child(mode_switch(s, k))
}

/// #186: the Focus glyph, flattened for Rune's rasteriser.
///
/// `ic::OUTLINE_EXPAND` is the theme's only 24-unit icon and wraps its path in
/// `<g transform="scale(1.2)" stroke-width="1.25">`. Rune's icon painter reads
/// only `path`/`rect`/`circle`/`ellipse`/`line` and splits the body on `>`, so
/// the group is dropped: the raw 4.583…15.416 coordinates centre the ink at
/// 10/24 instead of 12/24 (1.17 pt up-left at 14 pt), and the stroke draws at
/// 1.25 units instead of the 1.5 the group's scale gives it. This is the same
/// mark with that scale baked into the coordinates, in a 16-unit box, and a
/// 1-unit stroke written on the path — the same weight as `SIDEBAR` beside it.
/// The ink then centres on the box, which [`chrome_icon_button`] draws the
/// target around.
static FOCUS_EXPAND: Icon = Icon {
    name: "focus-expand",
    view: 16.0,
    body: r#"<path d="M3.6667 8.3333V12.3333H7.6667M8.3333 3.6667H12.3333V7.6667" stroke="currentColor" stroke-width="1" stroke-linecap="square"/>"#,
};

/// The title bar's 26 pt bordered icon chip: target, border and centring are
/// defined once so every glyph in the row is framed and placed the same way.
fn chrome_icon_button(k: &Kit, key: &'static str, border: Color) -> El {
    pressable(k, key.to_owned())
        .size(26.0, 26.0)
        .center()
        .border(1.0, border)
        .role("button")
}

/// Slice 8: show or hide the notes sidebar (also Cmd/Ctrl+\\).
fn sidebar_toggle(s: &State, k: &Kit) -> El {
    chrome_icon_button(k, "sidebar-toggle", k.c.border_muted)
        .label(if s.sidebar_shown() {
            t(TextKey::TopbarHideSidebar)
        } else {
            t(TextKey::TopbarShowSidebar)
        })
        .on_press_tx(|s: &mut State, tx: &mut Tx| {
            s.toggle_sidebar();
            tx.instant();
        })
        .child(
            icon(&rune_kit::shell::SIDEBAR, 14.0).color(if s.sidebar_shown() {
                k.c.text_muted
            } else {
                k.c.text_accent
            }),
        )
}

/// #112: enter or leave Focus mode (also ⇧⌘F, and the palette's action).
/// It sits beside the sidebar toggle, the other piece of chrome a writer
/// hides; in Focus the title bar is gone, so [`focus_exit`] is the way back.
fn focus_toggle(s: &State, k: &Kit) -> El {
    chrome_icon_button(k, "focus-toggle", k.c.border_muted)
        .label(if s.focus_active() {
            t(TextKey::FocusExit)
        } else {
            t(TextKey::FocusEnter)
        })
        .help(focus_help(crate::focus::FOCUS_CHORD))
        .on_press_tx(|s: &mut State, tx: &mut Tx| {
            s.toggle_focus_mode();
            if s.reduced_motion() {
                tx.instant();
            }
        })
        .child(icon(&FOCUS_EXPAND, 14.0).color(if s.focus_active() {
            k.c.text_accent
        } else {
            k.c.text_muted
        }))
}

/// #112: the one control Focus keeps — a faint chip pinned to the page's top
/// right, so the mode can be left with the pointer after the title bar (and
/// its button) went away. It enters on Rune's fade, which reduced motion
/// makes instant along with every other chrome change.
fn focus_exit(k: &Kit) -> El {
    chrome_icon_button(k, "focus-exit", k.alpha(k.c.border_muted, 0.7))
        .pin(None, Some(10.0), Some(12.0), None)
        .radius(999.0)
        .fill(k.alpha(k.c.bg_base, 0.72))
        .label(t(TextKey::FocusExit))
        .help(focus_exit_help(crate::focus::FOCUS_CHORD))
        .transition(Transition::opacity())
        .on_press_tx(|s: &mut State, tx: &mut Tx| {
            s.leave_focus_mode();
            if s.reduced_motion() {
                tx.instant();
            }
        })
        .child(icon(&FOCUS_EXPAND, 14.0).color(k.c.text_faint))
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
            t(TextKey::ModeWrite),
            &ic::EDIT,
            "mode-editor",
            Mode::Editor,
            s.mode,
        ))
        .child(mode_button(
            k,
            t(TextKey::ModeRead),
            &ic::OUTLINE_EYE,
            "mode-preview",
            Mode::Preview,
            s.mode,
        ))
        .child(mode_button(
            k,
            t(TextKey::ModeGraph),
            &ic::SHARE,
            "mode-graph",
            Mode::Graph,
            s.mode,
        ))
        .child(mode_button(
            k,
            t(TextKey::ModeBoard),
            &ic::DOT_GRID,
            "mode-board",
            Mode::Board,
            s.mode,
        ))
        .child(mode_button(
            k,
            t(TextKey::ModeKanban),
            &ic::CHECKLIST,
            "mode-kanban",
            Mode::Kanban,
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
                .label(t(TextKey::PreviewFind))
                .single_line()
                .w(280.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder(t(TextKey::PreviewFind), k.c.text_faint)
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
    let mut input = text_input(&s.editor, k.ty.body.lh(25.0))
        .key("editor")
        .label(t(TextKey::PreviewEditor))
        .w_full()
        .color(k.c.text_base)
        .caret(k.c.text_accent, k.selection())
        .placeholder(t(TextKey::PreviewEditorPlaceholder), k.c.text_faint)
        .on_edit(|s: &mut State| &mut s.editor)
        // #111: rich clipboard text becomes Markdown, and a URL pasted over
        // a selection becomes a link. Anything else declines, so Rune's
        // plain-text paste (already-Markdown included) is unchanged.
        .on_input_paste(|s: &mut State, content, _| s.editor_paste(content))
        // EDT-05: Enter closes `[[`, continues lists, or inserts a newline.
        .on_submit(|s: &mut State, _| s.editor_enter())
        .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            // EDT-03: the completion claims Up/Down/Enter/Tab/Escape (and,
            // in the Cmd+P picker, the typed filter) before the field.
            if s.complete_key(e) {
                return true;
            }
            // EDT-05: Tab indents instead of moving focus out of the note.
            if e.key == Key::Tab && !e.mods.shift && !e.mods.cmd && !e.mods.ctrl && !e.mods.alt {
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
        });
    // #112: Focus mode dims every paragraph but the caret's. Rune's
    // `text_input` has no inactive-line style, but `spans` paints byte ranges
    // over the field without touching the buffer, so each other block gets a
    // fainter ink and the active one keeps the field's own colour.
    let dim = s.focus_dim_spans(k.c.text_faint);
    if !dim.is_empty() {
        input = input.spans(dim);
    }
    // A transparent highlight over the caret's character is the keyed target
    // `Tx::scroll_center` centres (see `focus::focus_centre_tick`); it paints
    // nothing itself.
    if let Some(mark) = s.focus_caret_mark() {
        input = input.text_highlight(mark);
    }
    let mut body = col().w_full().max_w(780.0).px(52.0).py(36.0).child(input);
    if let Some(complete) = &s.complete
        && !complete.items.is_empty()
    {
        let rows = complete
            .items
            .iter()
            .map(|&id| {
                (
                    format!("complete-item-{id}"),
                    s.vault.display_label(id),
                    CompletePick::Note(id),
                )
            })
            .collect();
        body = body.child(complete_list(k, "link-complete", rows, complete.index));
    }
    col()
        .key("editor-pane")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            col()
                .key(crate::focus::EDITOR_SCROLL)
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

/// EDT-03: one row of a completion list. Accepting a note inserts a
/// wikilink; accepting a tag (kanban slice 10) inserts `#tag`.
#[derive(Clone, Debug)]
pub(crate) enum CompletePick {
    Note(usize),
    Tag(String),
}

/// EDT-03: the completion/picker list under the editor or the card editor.
/// The highlighted row is moved with Up/Down (and wraps); clicking a row
/// accepts it. Duplicate titles show their folder through `display_label`.
pub(crate) fn complete_list(
    k: &Kit,
    key: &str,
    rows: Vec<(String, String, CompletePick)>,
    index: usize,
) -> El {
    let mut list = col()
        .key(key)
        .mt(8.0)
        .w(420.0)
        .gap(1.0)
        .padding(4.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        // Slice 16: the open completions are a listbox of options; the field
        // points at the highlighted one (its active descendant) and a change
        // is announced.
        .role("listbox")
        .label(t(TextKey::PreviewCompletions))
        .live_region(LiveRegion::Polite);
    for (row_index, (row_key, label, pick)) in rows.into_iter().enumerate() {
        let on = index == row_index;
        list = list.child(
            row()
                .key(row_key)
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
                .role("option")
                .label(label.clone())
                .focus_selected(on)
                .on_press(move |s: &mut State| match &pick {
                    CompletePick::Note(id) => {
                        s.complete_choose(*id);
                    }
                    CompletePick::Tag(tag) => {
                        s.tag_complete_choose(tag);
                    }
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
        return empty_page(k, t(TextKey::PreviewEmptyNote));
    }
    let source = s.vault.preview_with_attachments(&s.editor.text, id);
    // EDT-04: top-level heading blocks get stable keys so a deep link can
    // scroll one into view. Offsets are computed against the preview source
    // (link rewriting changes byte lengths), matching `Markdown::block_keys`.
    let block_keys = heading_keys(&source);
    let accent = k.c.text_accent;
    // Palette identity in the memo version: a theme switch rebuilds the styled preview.
    let body = rune::memo(
        format!("preview-{id}"),
        (source.clone(), crate::omarchy::kit_id(s, k.dark)),
        move || {
            let style = MarkdownStyle::from_kit(k)
                .body_font(k.ty.body.lh(25.0))
                .link(accent)
                .link_underline(k.alpha(accent, 0.35));
            Markdown::new(&source)
                .style(style)
                .block_keys(block_keys)
                .on_link_tx(|s: &mut State, url: &str, tx: &mut Tx| s.open_link(url, tx))
                .link_style(missing_link_style(k))
                .build(k)
        },
    );
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
        .child(
            scroller.child(
                col()
                    .w_full()
                    .max_w(744.0)
                    .px(52.0)
                    .py(36.0)
                    .child(body)
                    .child(attachment_strip(s, id, k)),
            ),
        )
}

/// A wikilink to no note (`Vault::preview_with_attachments` points it at
/// `missing:`) reads dimmed, in Read and on kanban cards alike.
pub(crate) fn missing_link_style(k: &Kit) -> impl Fn(&str) -> Option<(Span, LinkPaint)> + 'static {
    let faint = k.c.text_faint;
    let rule = k.alpha(faint, 0.5);
    let hover = k.alpha(faint, 0.6);
    move |url| {
        url.strip_prefix("missing:").map(|_| {
            (
                Span::new(0, 0).color(faint).underline(rule),
                LinkPaint::new().color(faint).underline(hover),
            )
        })
    }
}

/// EDT-04: stable block keys for the preview's headings and `^id` blocks.
fn heading_keys(source: &str) -> Vec<(usize, String)> {
    let mut keys: Vec<(usize, String)> = headings(source)
        .into_iter()
        .map(|(offset, _)| (offset, format!("heading-{offset}")))
        .collect();
    for (offset, _) in block_anchors(source) {
        let key = format!("heading-{offset}");
        if keys.iter().all(|(_, existing)| existing != &key) {
            keys.push((offset, key));
        }
    }
    keys
}

/// Kanban slice 3: the sidebar's Kanbans section, between the notes and the
/// Boards. Every kanban note (cached in the index), by title; a row opens it
/// in Kanban mode. Hidden while searching notes and in a vault with none.
fn sidebar_kanbans(s: &State, k: &Kit, filtering: bool) -> El {
    let kanbans: Vec<_> = s.vault.notes.iter().filter(|n| n.is_kanban()).collect();
    if filtering || kanbans.is_empty() {
        return el();
    }
    let shown = s
        .kanban
        .as_ref()
        .filter(|_| s.mode == Mode::Kanban)
        .map(|shown| shown.rel.as_str());
    let mut list = col()
        .key("sidebar-kanbans")
        .w_full()
        .px(8.0)
        .pb(8.0)
        .gap(2.0)
        .role("list")
        .child(sidebar_section_head(
            k,
            t(TextKey::SidebarKanbans),
            kanbans.len(),
        ));
    for note in kanbans {
        let id = note.id;
        let current = shown == Some(note.rel.as_str());
        let mut body = row().grow(1.0).min_w(0.0).gap(6.0).center_y().child(
            text(&note.title)
                .key(format!("sidebar-kanban-title-{id}"))
                .font(k.ty.ui_compact)
                .color(if current {
                    k.c.text_accent
                } else {
                    k.c.text_base
                })
                .truncate(),
        );
        if !note.folder().is_empty() {
            body = body.child(
                text(note.folder())
                    .font(k.ty.caption())
                    .color(k.c.text_faint)
                    .truncate(),
            );
        }
        list = list.child(
            pressable(k, format!("sidebar-kanban-{id}"))
                .h(30.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
                .role("button")
                .label(note.title.clone())
                .child(icon(&ic::CHECKLIST, 14.0).color(k.c.icon_muted))
                .child(body)
                .on_press(move |s: &mut State| s.open_kanban(id)),
        );
    }
    list
}

/// A sidebar section's caps title and count (Kanbans, Boards).
fn sidebar_section_head(k: &Kit, title: &'static str, count: usize) -> El {
    row()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .child(
            text(title)
                .font(TextStyle::new(10.0, 650.0).tracking(0.9))
                .color(k.c.text_muted),
        )
        .child(
            text(count.to_string())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

/// Slice 7: the sidebar's Boards section, below the notes. A row opens the
/// board on Board. Hidden while searching notes and in a vault with none.
fn sidebar_boards(s: &State, k: &Kit, filtering: bool) -> El {
    if filtering || s.sidebar_boards.is_empty() {
        return el();
    }
    let mut list = col()
        .key("sidebar-boards")
        .w_full()
        .px(8.0)
        .pb(8.0)
        .gap(2.0)
        .role("list")
        .child(sidebar_section_head(
            k,
            t(TextKey::SidebarBoards),
            s.sidebar_boards.len(),
        ));
    let open = s.board_open.as_ref().map(|open| open.path.as_path());
    for (i, board) in s.sidebar_boards.iter().enumerate() {
        let path = board.path.clone();
        if s.renaming_board_at(&path, crate::BoardPlace::Sidebar) {
            list = list.child(crate::board_rename::rename_field(s, k));
            continue;
        }
        let menu_path = path.clone();
        let current = s.mode == Mode::Board && open == Some(board.path.as_path());
        // #188: one row builder for the page's picker and this section, so a
        // board's title, folder and detail read the same in both.
        let body = crate::board_view::board_row_body(
            k,
            &board.title,
            board.folder(),
            (Some(format!("sidebar-board-title-{i}")), None),
            current,
            false,
        );
        list = list.child(
            pressable(k, format!("sidebar-board-{i}"))
                .h(30.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
                // #188: as the Kanban rows are, a named button to a screen
                // reader rather than an anonymous row.
                .role("button")
                .label(board.title.clone())
                .child(icon(&ic::DOT_GRID, 14.0).color(k.c.icon_muted))
                .child(body)
                .on_press(move |s: &mut State| s.open_board_at(path.clone()))
                .on_secondary_press(move |s: &mut State, press, _| {
                    s.open_board_menu(
                        menu_path.clone(),
                        crate::BoardPlace::Sidebar,
                        press.x,
                        press.y,
                    );
                }),
        );
        if s.menu.as_ref().is_some_and(|m| {
            m.target == MenuTarget::Board(board.path.clone(), crate::BoardPlace::Sidebar)
        }) {
            list = list.child(context_menu(s, k));
        }
    }
    list
}

/// DES-02: attachments are real files but not notes, and Rune's Markdown has
/// no image pipeline. The reader therefore lists each embed with its size and
/// an Open button, which hands the canonical path to the platform opener
/// (`State::open_attachment`) instead of faking a preview.
fn attachment_strip(s: &State, current: usize, k: &Kit) -> El {
    let attachments = s.vault.attachments(&s.editor.text, current);
    if attachments.is_empty() {
        return el();
    }
    let mut rows = col().w_full().gap(4.0);
    for (index, attachment) in attachments.iter().enumerate() {
        let target = attachment.target.clone();
        let board = attachment.is_board();
        let board_path = attachment.path.clone().filter(|_| board);
        let size = attachment
            .size_label()
            .unwrap_or_else(|| t(TextKey::PreviewMissingFile).to_owned());
        let mut body = col()
            .grow(1.0)
            .min_w(0.0)
            .gap(1.0)
            .child(
                text(&attachment.name)
                    .font(k.ty.ui_compact)
                    .color(k.c.text_base)
                    .truncate(),
            )
            .child(text(size).font(k.ty.caption()).color(k.c.text_faint));
        if let Some(alias) = &attachment.alias {
            body = body.child(
                text(alias)
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .truncate(),
            );
        }
        rows = rows.child(
            row()
                .key(format!("attachment-row-{index}"))
                .w_full()
                .px(10.0)
                .py(7.0)
                .gap(9.0)
                .radius(7.0)
                .center_y()
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .child(
                    icon(if board { &ic::DOT_GRID } else { &ic::PHOTO }, 15.0)
                        .color(k.c.icon_muted),
                )
                .child(body)
                .child(
                    pressable(k, format!("attachment-open-{index}"))
                        .px(9.0)
                        .py(4.0)
                        .center_y()
                        .border(1.0, k.c.border_muted)
                        .child(
                            text(if board {
                                t(TextKey::PreviewOpenBoard)
                            } else {
                                t(TextKey::PreviewOpen)
                            })
                            .font(k.ty.caption())
                            .color(k.c.text_accent),
                        )
                        .on_press(move |s: &mut State| {
                            // Slice 7: an embedded board opens on Board.
                            match &board_path {
                                Some(path) => s.open_board_at(path.clone()),
                                None => {
                                    s.open_attachment(&target);
                                }
                            }
                        }),
                ),
        );
    }
    col()
        .key("attachments")
        .w_full()
        .mt(28.0)
        .pt(18.0)
        .gap(9.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    text(t(TextKey::PreviewAttachments))
                        .font(TextStyle::new(10.0, 650.0).tracking(0.9))
                        .color(k.c.text_muted),
                )
                .child(
                    text(attachments.len().to_string())
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                )
                .child(spacer())
                .child(
                    text(t(TextKey::PreviewAttachmentsNote))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        )
        .child(divider_h(k))
        .child(rows)
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
            text(t(TextKey::TopbarNoNoteSelected))
                .font(k.ty.ui_medium)
                .color(k.c.text_muted),
        )
        .child(
            text(t(TextKey::PreviewStartWriting))
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
                .child(
                    text(t(TextKey::CommonNewNote))
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base),
                )
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

/// Visual weight of the save-state indicator. Only failures and in-flight
/// saves colour their text; a clean save stays a calm muted line with a
/// green dot, so the status bar never competes with the note.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Danger,
    Warning,
    Notice,
    Saved,
    Calm,
}

impl StatusTone {
    /// Stable suffix for the indicator's test key (`status-state-danger`).
    fn slug(self) -> &'static str {
        match self {
            StatusTone::Danger => "danger",
            StatusTone::Warning => "warning",
            StatusTone::Notice => "notice",
            StatusTone::Saved => "saved",
            StatusTone::Calm => "autosave",
        }
    }

    fn dot(self, k: &Kit) -> Color {
        match self {
            StatusTone::Danger => k.c.danger.fg,
            StatusTone::Warning => k.c.warning.fg,
            StatusTone::Notice => k.c.text_accent,
            StatusTone::Saved => k.c.success.fg,
            StatusTone::Calm => k.c.text_faint,
        }
    }
}

fn status_bar(s: &State, k: &Kit) -> El {
    if s.conflict.is_some() {
        return conflict_bar(s, k);
    }
    let (tone, label, text_color) = if let Some(error) = &s.error {
        (StatusTone::Danger, error.clone(), Some(k.c.danger.fg))
    } else if s.dirty() {
        (
            StatusTone::Warning,
            t(TextKey::StatusSaving).to_owned(),
            Some(k.c.warning.fg),
        )
    } else if let Some(notice) = &s.notice {
        // DES-02: imported attachments have no sidebar row, so the status bar
        // is where their count is reported.
        (StatusTone::Notice, notice.clone(), None)
    } else if s.last_save.is_some() {
        (StatusTone::Saved, t(TextKey::StatusSaved).to_owned(), None)
    } else {
        (
            StatusTone::Calm,
            t(TextKey::StatusAutosaveOn).to_owned(),
            None,
        )
    };
    // The dot and the words carry the same tone on failure or while saving;
    // a clean save keeps the muted text and only lights the dot.
    let indicator = row()
        .key(format!("status-state-{}", tone.slug()))
        .gap(7.0)
        .center_y()
        .child(
            el().key("status-dot")
                .size(6.0, 6.0)
                .radius(3.0)
                .fill(tone.dot(k)),
        )
        .child(
            text(label)
                .key("status-text")
                .font(k.ty.caption())
                .color(text_color.unwrap_or(k.c.text_faint))
                .truncate(),
        );
    let mut bar = row()
        .key("status-bar")
        .h(36.0)
        .px(20.0)
        .gap(7.0)
        .center_y()
        .child(indicator);
    if s.error.is_some() && s.dirty() {
        bar = bar.child(status_button(
            k,
            t(TextKey::StatusRetry),
            "retry-save",
            State::retry_save,
        ));
    }
    let skipped = s.vault.skipped_count();
    if skipped > 0 {
        bar = bar.child(
            text(files_skipped(skipped))
                .key("skipped-count")
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        );
    }
    if let Some(undo) = &s.trash_undo {
        bar = bar
            .child(
                text(deleted(&undo.label))
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .truncate(),
            )
            .child(status_button(
                k,
                t(TextKey::StatusUndo),
                "trash-undo",
                State::undo_trash,
            ));
    }
    if s.load_ms() >= 5.0 {
        bar = bar.child(
            text(scan_ms(s.load_ms()))
                .key("scan-ms")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
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
        .child(
            el().key("conflict-dot")
                .size(6.0, 6.0)
                .radius(3.0)
                .fill(k.c.danger.fg),
        )
        .child(
            text(conflict.message())
                .key("conflict-text")
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .truncate(),
        )
        .child(spacer())
        .child(status_button(
            k,
            t(TextKey::ConflictKeepMine),
            "conflict-keep-mine",
            State::resolve_conflict_keep_mine,
        ))
        .child(status_button(
            k,
            t(TextKey::ConflictLoadTheirs),
            "conflict-load-theirs",
            State::resolve_conflict_load_theirs,
        ))
        .child(status_button(
            k,
            t(TextKey::ConflictSaveCopy),
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
    text(status_counts(s.words(), count, back))
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
        outgoing = outgoing.child(empty(k, t(TextKey::PreviewNoOutgoing)));
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
        backlinks = backlinks.child(empty(k, t(TextKey::PreviewNoBacklinks)));
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
                    text(t(TextKey::PreviewLinks))
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
                .child(section(
                    k,
                    t(TextKey::PreviewOutgoing),
                    note.links.len(),
                    outgoing,
                ))
                .child(section(
                    k,
                    t(TextKey::PreviewBacklinks),
                    note.backlinks.len(),
                    backlinks,
                ))
                .child(
                    col()
                        .gap(7.0)
                        .child(section(
                            k,
                            t(TextKey::PreviewGraph),
                            0,
                            col().w_full().gap(6.0).child(local_graph_button(k)),
                        ))
                        .child(
                            text(t(TextKey::PreviewGraphHint))
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
            text(t(TextKey::PaletteActionOpenLocalGraph))
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

fn divider(k: &Kit) -> El {
    el().w(1.0).h_full().fill(k.c.border_muted)
}

fn divider_h(k: &Kit) -> El {
    el().w_full().h(1.0).fill(k.c.border_muted)
}

#[cfg(test)]
mod help_layout_tests {
    use super::{diagnostics_parts, help_layout, shortcut_label};

    #[test]
    fn shortcut_labels_keep_every_chord_and_context() {
        let search = shortcut_label("⌘K / Ctrl+K");
        assert_eq!(
            search.chords,
            vec![
                vec!["⌘".to_owned(), "K".to_owned()],
                vec!["Ctrl".to_owned(), "K".to_owned()]
            ]
        );
        assert!(search.note.is_none());

        let noted = shortcut_label("↑ / ↓ (search)");
        assert_eq!(noted.note.as_deref(), Some("search"));
        assert_eq!(
            noted.chords,
            vec![vec!["↑".to_owned()], vec!["↓".to_owned()]]
        );

        assert_eq!(shortcut_label("[[").chords, vec![vec!["[[".to_owned()]]);
        assert_eq!(shortcut_label("⌘1 / ⌘2 / ⌘3 / ⌘4 / ⌘5").chords.len(), 5);
        assert_eq!(
            shortcut_label("Tab / Shift+Tab").chords,
            vec![
                vec!["Tab".to_owned()],
                vec!["Shift".to_owned(), "Tab".to_owned()]
            ]
        );
        assert_eq!(
            shortcut_label("+ / -").chords,
            vec![vec!["+".to_owned()], vec!["-".to_owned()]]
        );
        assert_eq!(
            shortcut_label("Right-click").chords,
            vec![vec!["Right-click".to_owned()]]
        );
        assert_eq!(
            shortcut_label("Plain label").chords,
            vec![vec!["Plain label".to_owned()]]
        );
        assert_eq!(
            shortcut_label("⌘?").chords,
            vec![vec!["⌘".to_owned(), "?".to_owned()]]
        );
    }

    #[test]
    fn column_count_follows_the_main_column() {
        assert_eq!(help_layout(840.0).columns, 1, "narrow window");
        assert!(!help_layout(840.0).stacked);
        assert_eq!(help_layout(1180.0).columns, 2, "default window");
        assert!(!help_layout(1180.0).stacked);
        assert_eq!(help_layout(1600.0).columns, 4, "wide window");
        assert!(help_layout(1600.0).stacked);
        assert_eq!(help_layout(1920.0).columns, 4);
    }

    #[test]
    fn diagnostics_split_keeps_the_log_tail_separate() {
        let (summary, logs) = diagnostics_parts(
            "Muninn 0.3.0 · rune abc\nvault /tmp/notes · 1 notes\nlast log lines:\nline-one\nline-two\n",
        );
        assert_eq!(summary.len(), 2);
        assert_eq!(logs, vec!["line-one".to_owned(), "line-two".to_owned()]);

        let (summary, logs) = diagnostics_parts("Muninn 0.3.0\n(no log lines yet)");
        assert_eq!(summary, vec!["Muninn 0.3.0".to_owned()]);
        assert_eq!(logs, vec!["(no log lines yet)".to_owned()]);
    }
}

/// #186: the title bar's icons must sit in the middle of their 26 pt chips.
///
/// These measure the rasterised ink itself — the same coverage atlas Rune
/// uploads — so a glyph that centres on its path but not on its pixels (or one
/// whose `<g transform>` the painter drops) fails here rather than in a
/// screenshot review. `SIDEBAR` and `SETTINGS_GEAR` are the controls: their
/// chips were measured exactly centred by hand.
#[cfg(test)]
mod icon_centring_tests {
    use super::FOCUS_EXPAND;
    use rune::Icon;
    use rune::text::icon_coverage;

    /// The ink's bounding box at `px` × `px`, in pixels: `(x0, x1, y0, y1)`
    /// with the maxima exclusive, so a box from pixel 3 to pixel 10 ends at 11.
    /// `ink` is the coverage a pixel must reach to count: 1 takes the faint
    /// antialiased fringe, 128 matches the ≥50 % measurement the bug report
    /// used.
    fn ink_bbox(icon: &Icon, px: u32, ink: u8) -> (f32, f32, f32, f32) {
        let cov = icon_coverage(icon, px);
        let (mut x0, mut x1, mut y0, mut y1) = (px as i32, -1, px as i32, -1);
        for y in 0..px as i32 {
            for x in 0..px as i32 {
                if cov[(y * px as i32 + x) as usize] >= ink {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        assert!(
            x1 >= x0 && y1 >= y0,
            "{} draws no ink at {px} pt",
            icon.name
        );
        (x0 as f32, (x1 + 1) as f32, y0 as f32, (y1 + 1) as f32)
    }

    /// How far the ink's centre sits from the box's, on each axis, in points.
    fn centre_error(icon: &Icon, px: u32, ink: u8) -> (f32, f32) {
        let (x0, x1, y0, y1) = ink_bbox(icon, px, ink);
        let middle = px as f32 / 2.0;
        ((x0 + x1) / 2.0 - middle, (y0 + y1) / 2.0 - middle)
    }

    #[test]
    fn the_focus_glyph_centres_like_its_neighbours() {
        // The glyph is drawn at 14 pt; the same coverage at 1x, 2x and 4x
        // scale keeps the centre where the layout put it.
        for px in [14, 28, 56] {
            for ink in [1, 128] {
                let (dx, dy) = centre_error(&FOCUS_EXPAND, px, ink);
                assert!(
                    dx.abs() <= 0.5 && dy.abs() <= 0.5,
                    "FOCUS_EXPAND is off-centre by ({dx}, {dy}) pt at {px} pt (ink ≥ {ink})"
                );
            }
        }

        // The width is the one the old 24-unit asset drew through its group:
        // ~8.5 pt of ink at 14 pt, not the ~7 pt the dropped transform left.
        for px in [14, 28, 56] {
            let (x0, x1, y0, y1) = ink_bbox(&FOCUS_EXPAND, px, 128);
            let (w, h) = (x1 - x0, y1 - y0);
            let expected = 8.35 * px as f32 / 14.0;
            assert!(
                (w - expected).abs() <= 1.0 && (h - expected).abs() <= 1.0,
                "FOCUS_EXPAND ink is {w}×{h} pt at {px} pt, expected ~{expected}"
            );
        }

        // The controls are centred in their own chips, so they hold the same
        // contract this test asks of the Focus glyph.
        let sidebar = centre_error(&rune_kit::shell::SIDEBAR, 14, 1);
        assert!(
            sidebar.0.abs() <= 0.5 && sidebar.1.abs() <= 0.5,
            "SIDEBAR is off-centre by {sidebar:?} pt"
        );
        let gear = centre_error(&rune_theme_opencode::icons::SETTINGS_GEAR, 15, 1);
        assert!(
            gear.0.abs() <= 0.5 && gear.1.abs() <= 0.5,
            "SETTINGS_GEAR is off-centre by {gear:?} pt"
        );
    }
}
