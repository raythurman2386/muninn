//! The window's chrome: vault sidebar, note title bar, editor, Markdown preview and links rail.

use rune::{
    Color, Cursor, Cx, El, Key, KeyEvent, LineCap, LinkPaint, LiveRegion, ScrollSpec, Shape, Span,
    TextStyle, Transition, Tx, canvas, col, el, icon, row, spacer, text, text_input,
};
use rune_kit::{
    Button, ButtonVariant, Dialog, Kit, Markdown, MarkdownStyle, SegmentedControl, Switch, TextArea,
};
use rune_theme_opencode::icons as ic;

use muninn_core::{Vault, block_anchors, headings};

use crate::{
    DeleteConfirm, MenuTarget, Mode, PaletteItem, ReduceMotion, SettingsTab, State, ThemeMode,
};

pub(crate) const SIDEBAR: f32 = 258.0;
const RAIL: f32 = 286.0;

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    // One resolved reduced-motion value (`State::reduced_motion`) drives
    // Rune's transitions here and the graph layout/camera in `poll`.
    s.set_system_reduced_motion(cx.system_reduced_motion());
    cx.reduced_motion(s.reduced_motion());
    let k = crate::omarchy::resolve_kit(s, cx.system_dark());
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
        // toggle moves things at once instead of sliding them.
        .child(if s.sidebar_shown() {
            sidebar(s, &k)
        } else {
            el().key("sidebar-hidden")
        })
        .child(if s.sidebar_shown() { divider(&k) } else { el() })
        .child(main_column(s, &k));
    if s.palette.is_some() {
        base = base.child(palette_layer(s, &k));
    }
    if !s.settings_modal {
        return base;
    }
    // v0.5: rune-kit dialogs render on their own scrim/layer above the whole
    // window, unlike the help page (which replaces the main column). Mount the
    // layer only while open so the ordinary view tree is unchanged otherwise.
    let mut layer = Vec::new();
    settings_dialog(s, &k).build(&k, &mut layer);
    rune_kit::root(base, layer)
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

    let mut brand = row()
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
    let place = if folder.is_empty() {
        "the vault root".to_owned()
    } else {
        folder.to_owned()
    };
    pressable(k, format!("folder-new-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::PLUS, 12.0).color(k.c.text_accent))
        .label(format!("New note in {place}"))
        .help(format!("New note in {place}"))
        .on_press(move |s: &mut State| s.new_note_in(&target))
}

fn folder_new_folder_button(k: &Kit, slug: &str, folder: &str) -> El {
    let target = folder.to_owned();
    let place = if folder.is_empty() {
        "the vault root".to_owned()
    } else {
        folder.to_owned()
    };
    pressable(k, format!("folder-create-{slug}"))
        .size(22.0, 22.0)
        .center()
        .child(icon(&ic::FOLDER_ADD_LEFT, 12.0).color(k.c.icon_muted))
        .label(format!("New folder in {place}"))
        .help(format!("New folder in {place}"))
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
                .label("Kanban")
        } else {
            el()
        })
        .px(10.0)
        .pl(20.0)
}

fn theme_button(k: &Kit, mode: ThemeMode) -> El {
    let label = format!(
        "Theme: {} — click to cycle (System, Dark, Light)",
        mode.label()
    );
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
        .label("Keyboard shortcuts")
        .help("Keyboard shortcuts")
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
        .label("Settings")
        .help("Settings (Ctrl+,)")
        .on_press(|s: &mut State| s.open_settings())
}

/// Manual sync. One provider per vault. Nothing here starts a background sync.
fn sync_section(s: &State, k: &Kit) -> El {
    let provider = match s.sync_ui.provider_index {
        0 => "GitHub",
        1 => "Google Drive",
        _ => "Tailscale",
    };
    let fields = if s.sync_ui.provider_index == 2 {
        tailnet_fields(s, k)
    } else if s.sync_ui.provider_index == 0 {
        col()
            .w_full()
            .gap(4.0)
            .child(sync_field(
                k,
                "GitHub owner",
                &s.sync_ui.github_owner,
                |state| &mut state.sync_ui.github_owner,
            ))
            .child(sync_field(
                k,
                "Repository",
                &s.sync_ui.github_repo,
                |state| &mut state.sync_ui.github_repo,
            ))
            .child(sync_field(k, "Branch", &s.sync_ui.github_branch, |state| {
                &mut state.sync_ui.github_branch
            }))
            .child(sync_field(k, "Prefix", &s.sync_ui.github_prefix, |state| {
                &mut state.sync_ui.github_prefix
            }))
            .child(sync_field(
                k,
                "GitHub client id (optional)",
                &s.sync_ui.github_client_id,
                |state| &mut state.sync_ui.github_client_id,
            ))
    } else {
        col()
            .w_full()
            .gap(4.0)
            .child(sync_field(k, "Store name", &s.sync_ui.drive_store, |state| {
                &mut state.sync_ui.drive_store
            }))
            .child(
                text("Kept in Muninn's private app data in your Drive, not in a visible folder. Use the same store name on each device.")
                    .font(k.ty.caption())
                    .color(k.c.text_faint)
                    .wrap(),
            )
            .child(sync_field(
                k,
                "Google client id (optional)",
                &s.sync_ui.google_client_id,
                |state| &mut state.sync_ui.google_client_id,
            ))
    };
    col()
        .key("settings-sync")
        .w_full()
        .gap(8.0)
        .child(text("Sync").font(k.ty.ui_medium).color(k.c.text_base))
        .child(
            text(
                "Manual Sync Now only. One provider for this vault. Disconnect removes credentials, not notes.",
            )
            .font(k.ty.caption())
            .color(k.c.text_muted)
            .wrap(),
        )
        .child(
            SegmentedControl::new(
                &["GitHub", "Drive", "Tailscale"],
                Some(s.sync_ui.provider_index),
            )
                .label("Sync provider")
                .on_pick(|state: &mut State, index, _: &mut Tx| state.set_sync_provider(index))
                .build(k),
        )
        .child(fields)
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
                    Button::new("Connect")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.connect_sync())
                        .build(k)
                        .key("settings-sync-connect"),
                )
                .child(
                    Button::new("Sync now")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.sync_now())
                        .build(k)
                        .key("settings-sync-now"),
                )
                .child(
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.cancel_sync())
                        .build(k)
                        .key("settings-sync-cancel"),
                )
                .child(
                    Button::new("Disconnect")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.disconnect_sync())
                        .build(k)
                        .key("settings-sync-disconnect"),
                ),
        )
}

fn tailnet_fields(s: &State, k: &Kit) -> El {
    let role = SegmentedControl::new(
        &["Host here", "Join host"],
        Some(s.sync_ui.tailnet_role_index),
    )
    .label("Tailscale role")
    .on_pick(|state: &mut State, index, _: &mut Tx| state.set_tailnet_role(index))
    .build(k);
    let fields = col().w_full().gap(4.0).child(role);
    if s.sync_ui.tailnet_role_index == 0 {
        let fields = fields
            .child(sync_field(
                k,
                "Loopback port",
                &s.sync_ui.tailnet_port,
                |state| &mut state.sync_ui.tailnet_port,
            ))
            .child(sync_field(
                k,
                "Allowed Tailscale logins (optional, comma-separated)",
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
                "Host MagicDNS name (device.tailnet.ts.net)",
                &s.sync_ui.tailnet_host,
                |state| &mut state.sync_ui.tailnet_host,
            ))
            .child(sync_field(
                k,
                "Shared key from the host",
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
            text("Shared key")
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
                    Button::new("Copy key")
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
                        "Hide key"
                    } else {
                        "Show key"
                    })
                    .variant(ButtonVariant::Ghost)
                    .on_press(|state: &mut State| state.toggle_tailnet_key())
                    .build(k)
                    .key("settings-sync-key-show"),
                )
                .child(
                    Button::new("Rotate key")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|state: &mut State| state.rotate_tailnet_key())
                        .build(k)
                        .key("settings-sync-key-rotate"),
                ),
        )
}

fn sync_field(
    k: &Kit,
    label: &str,
    field: &rune::TextEdit,
    edit: impl Fn(&mut State) -> &mut rune::TextEdit + 'static,
) -> El {
    col()
        .w_full()
        .gap(2.0)
        .child(text(label).font(k.ty.caption()).color(k.c.text_muted))
        .child(
            text_input(field, k.ty.ui_compact)
                .single_line()
                .w_full()
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .on_edit(edit),
        )
}

/// v0.5: the settings surface. Preferences write through their existing
/// `State` setters, so a click or key press is persisted before the next
/// frame; there is no separate Save button.
///
/// v0.6: the preferences are split across tabs ([`SettingsTab`]) so the modal
/// shows one group at a time. The tab bar is a segmented control keyed
/// `settings-tabs`; each tab's body is its own function below and is keyed
/// `settings-tab-<name>`, so a later section (sync, graph effects) lands by
/// editing one function rather than the dialog shell.
fn settings_dialog(s: &State, k: &Kit) -> Dialog {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    let tabs = SegmentedControl::new(&labels, Some(s.settings_tab.index()))
        .label("Settings section")
        // Equal cells with a sliding indicator (the theme picker's style);
        // the indicator's travel obeys reduced motion like every transition.
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            if let Some(tab) = SettingsTab::ALL.get(index) {
                s.set_settings_tab(*tab);
            }
        })
        .build(k)
        .key("settings-tabs");

    let content = match s.settings_tab {
        SettingsTab::Vaults => settings_vaults_tab(s, k),
        SettingsTab::Editor => settings_editor_tab(s, k),
        SettingsTab::Appearance => settings_appearance_tab(s, k),
        SettingsTab::Graph => settings_graph_tab(s, k),
        SettingsTab::Sync => settings_sync_tab(s, k),
        SettingsTab::Kanban => crate::kanban_settings::app_tab(s, k),
    };
    // The Kanban defaults are many rows; the other tabs stay short. A scroll
    // bounded by the window keeps the dialog and its Close button on screen,
    // and still shows every row on a tall window.
    let content = if s.settings_tab == SettingsTab::Kanban {
        col()
            .key("settings-kanban-scroll")
            .w_full()
            .max_h((s.window_height - 220.0).max(260.0))
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(content)
    } else {
        content
    };

    let body = col()
        .key("settings-body")
        .w_full()
        .padding(16.0)
        .gap(16.0)
        .child(tabs)
        // A floor on the content height keeps the dialog (and its Close
        // button) from jumping as short tabs swap with tall ones.
        .child(
            col()
                .key("settings-tab-content")
                .w_full()
                .min_h(260.0)
                .child(content),
        )
        .child(
            row().w_full().justify_end().child(
                Button::new("Close")
                    .variant(ButtonVariant::Ghost)
                    .on_press(|s: &mut State| s.close_settings())
                    .build(k)
                    .key("settings-close"),
            ),
        );

    Dialog::new("settings", "Settings")
        .description("Preferences are saved as you change them.")
        .width(540.0)
        .fit()
        .open(true)
        .on_close(|s: &mut State| s.close_settings())
        .child(body)
}

/// One titled group inside a Settings tab: a heading, an optional caption,
/// then the control.
fn settings_section(k: &Kit, title: &str, caption: Option<&str>, control: El) -> El {
    let mut section = col()
        .w_full()
        .gap(8.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base));
    if let Some(caption) = caption {
        section = section.child(
            text(caption)
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        );
    }
    section.child(control)
}

/// Vaults tab: the typed-path card and the recent-vault list.
fn settings_vaults_tab(s: &State, k: &Kit) -> El {
    let mut recents = col().key("settings-recents").w_full().gap(2.0);
    if s.recent_vaults().is_empty() {
        recents = recents.child(
            text("No recent vaults yet")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    } else {
        for (index, path) in s.recent_vaults().iter().enumerate() {
            recents = recents.child(
                row()
                    .key(format!("settings-recent-{index}"))
                    .w_full()
                    .h(28.0)
                    .px(8.0)
                    .radius(6.0)
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
        .gap(6.0)
        .child(text("Vaults").font(k.ty.ui_medium).color(k.c.text_base))
        .child(
            text("Open a folder by path, or create a new vault.")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(vault_prompt_row(s, k))
        .child(
            text("Recent vaults")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .pt(8.0),
        )
        .child(recents)
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
        .label("Default note template")
        .placeholder("Markdown inserted at the top of each new note")
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
            "Autosave delay",
            Some("Idle seconds before the open note is written."),
            stepper,
        ))
        .child(settings_section(
            k,
            "Default note template",
            Some(
                "Markdown inserted at the top of every new note. \
                 Leave empty for a plain heading.",
            ),
            template,
        ))
}

/// Appearance tab: theme, the optional tray icon, start at login and the
/// global shortcut.
fn settings_appearance_tab(s: &State, k: &Kit) -> El {
    let theme = if crate::omarchy::is_following(s) {
        text("Following Omarchy system theme")
            .key("settings-theme-note")
            .font(k.ty.ui_compact)
            .color(k.c.text_muted)
            .wrap()
    } else {
        let labels: Vec<&str> = ThemeMode::ALL.iter().map(|mode| mode.label()).collect();
        SegmentedControl::new(&labels, Some(s.theme.id() as usize))
            .label("Theme")
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
        .child(settings_section(k, "Theme", None, theme))
        .child(settings_tray_section(s, k))
        .child(settings_startup_section(s, k))
}

/// Appearance › Tray icon: Show tray icon and Close to tray, each with a
/// status line. Disabled where Rune has no tray (macOS at the pinned
/// revision); Close to tray also needs the tray icon on.
fn settings_tray_section(s: &State, k: &Kit) -> El {
    let supported = crate::tray_supported();
    let switch = Switch::new(s.tray_icon() && supported)
        .label("Show tray icon")
        .disabled(!supported)
        .on_toggle(|s: &mut State| {
            let on = !s.tray_icon();
            s.set_tray_icon(on);
        })
        .build(k)
        .key("settings-tray-icon");
    let status = if !supported {
        "Not available on macOS yet.".to_owned()
    } else if let Some(error) = s.tray_error() {
        format!("The tray icon could not be added: {error}")
    } else if s.tray_icon() {
        "On. Linux shows it where a tray host runs; GNOME needs an AppIndicator extension."
            .to_owned()
    } else {
        "Off.".to_owned()
    };
    let close_enabled = supported && s.tray_icon();
    let close = Switch::new(s.close_to_tray() && close_enabled)
        .label("Close to tray")
        .disabled(!close_enabled)
        .on_toggle(|s: &mut State| {
            let on = !s.close_to_tray();
            s.set_close_to_tray(on);
        })
        .build(k)
        .key("settings-close-to-tray");
    let close_status = if !supported {
        "Not available on macOS yet."
    } else if !s.tray_icon() {
        "Needs the tray icon. Closing the window quits Muninn."
    } else if !s.close_to_tray() {
        "Off. Closing the window quits Muninn."
    } else if s.closes_to_tray() {
        "On. Closing the window keeps Muninn in the tray; Quit Muninn there quits. \
         If no tray host shows the icon, leave this off."
    } else {
        "On, but there is no tray icon right now, so closing the window quits Muninn."
    };
    settings_section(
        k,
        "Tray icon",
        Some("A tray icon with Show Muninn, New note, Sync now, the sync status and Quit."),
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
        .label("Start Muninn at login")
        .disabled(!autostart)
        .on_toggle(|s: &mut State| {
            let on = !s.auto_start();
            s.set_auto_start(on);
        })
        .build(k)
        .key("settings-auto-start");
    let login_status = if !autostart {
        "Not available on macOS yet.".to_owned()
    } else if let Some(error) = s.auto_start_error() {
        format!("The login item could not be changed: {error}")
    } else if s.auto_start() {
        "On. Muninn opens when you log in.".to_owned()
    } else {
        "Off.".to_owned()
    };
    let chord = crate::show_shortcut_label();
    let shortcut = Switch::new(s.show_shortcut())
        .label(format!("Show Muninn with {chord}"))
        .on_toggle(|s: &mut State| {
            let on = !s.show_shortcut();
            s.set_show_shortcut(on);
        })
        .build(k)
        .key("settings-show-shortcut");
    let shortcut_status = match (s.show_shortcut(), s.shortcut_phase()) {
        (_, crate::ShortcutPhase::Failed(error)) => {
            format!("{chord} could not be registered: {error}")
        }
        (true, crate::ShortcutPhase::Bound) => {
            format!("On. {chord} shows Muninn from any app.")
        }
        (true, crate::ShortcutPhase::Pending) => "Registering…".to_owned(),
        (true, _) => "On.".to_owned(),
        (false, _) => "Off.".to_owned(),
    };
    settings_section(
        k,
        "Startup and shortcut",
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

/// Graph tab: how the knowledge graph moves.
///
/// **Reduce motion** is a persisted System / On / Off choice. The status line
/// under it shows the resolved value ([`State::reduced_motion`]), which is what
/// the layout, camera and transitions obey.
///
/// Integration slot: the graph-effects work (draft PR #20) adds its
/// **Graph effects** picker (Classic default, no Off) as a section *below*
/// Reduce motion in this column. Effects must read `State::reduced_motion()`
/// so reduced motion overrides any effect style.
fn settings_graph_tab(s: &State, k: &Kit) -> El {
    let labels: Vec<&str> = ReduceMotion::ALL.iter().map(|m| m.label()).collect();
    let picker = SegmentedControl::new(&labels, Some(s.reduce_motion().index()))
        .label("Reduce motion")
        .fit()
        .on_pick(|s: &mut State, index, _: &mut Tx| {
            if let Some(mode) = ReduceMotion::ALL.get(index) {
                s.set_reduce_motion(*mode);
            }
        })
        .build(k)
        .key("settings-reduce-motion");
    let status = if s.reduced_motion_env {
        "Currently on: MUNINN_REDUCED_MOTION is set, which overrides this choice."
    } else {
        match (s.reduce_motion(), s.reduced_motion()) {
            (ReduceMotion::System, true) => "Currently on, following the system setting.",
            (ReduceMotion::System, false) => "Currently off, following the system setting.",
            (_, true) => "Currently on.",
            (_, false) => "Currently off.",
        }
    };
    col()
        .key("settings-tab-graph")
        .w_full()
        .gap(18.0)
        .child(settings_section(
            k,
            "Reduce motion",
            Some(
                "Freezes the graph layout, skips camera glides and makes hover and \
                 selection changes instant. System follows your OS setting.",
            ),
            col().w_full().gap(8.0).child(picker).child(
                text(status)
                    .key("settings-reduced-motion-status")
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .wrap(),
            ),
        ))
    // #20 slot: `.child(settings_section(k, "Graph effects", …))` goes here.
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
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .child(
            text_input(&s.vault_field, k.ty.ui_compact)
                .key("vault-path")
                .single_line()
                .w_full()
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("~/notes", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.vault_field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    if e.key == Key::Escape {
                        // Esc closes the Settings modal even while the field
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
            text("Paste a folder path; ~ expands to your home folder.")
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
                        .child(text("Open vault").font(k.ty.caption()).color(k.c.text_base))
                        .on_press(|s: &mut State| s.submit_vault_path()),
                )
                .child(
                    pressable(k, "vault-create".to_owned())
                        .px(8.0)
                        .py(3.0)
                        .child(text("New vault").font(k.ty.caption()).color(k.c.text_muted))
                        .on_press(|s: &mut State| s.submit_new_vault()),
                )
                .child(
                    pressable(k, "vault-cancel".to_owned())
                        .px(8.0)
                        .py(3.0)
                        .child(text("Cancel").font(k.ty.caption()).color(k.c.text_muted))
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
pub(crate) fn context_menu(s: &State, k: &Kit) -> El {
    let Some(menu) = &s.menu else {
        return el();
    };
    let items: Vec<(&str, &str)> = match &menu.target {
        MenuTarget::Note(_) => vec![
            ("Open externally", "open_external"),
            ("Reveal in file manager", "reveal"),
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
        MenuTarget::Board(..) => vec![("Rename", "rename")],
        MenuTarget::Card(lane, index) => {
            let done = s
                .kanban
                .as_ref()
                .and_then(|shown| shown.board.as_ref())
                .and_then(|board| board.lanes().get(*lane)?.cards().get(*index))
                .is_some_and(muninn_core::Card::is_done);
            let mut items = vec![
                ("Edit", "edit"),
                ("Insert card before", "insert_before"),
                ("Insert card after", "insert_after"),
                ("Duplicate", "duplicate"),
            ];
            // Slice 14: the card's checkbox can be hidden by `show-checkboxes`;
            // the menu still toggles the done mark.
            items.push((if done { "Mark open" } else { "Mark done" }, "toggle_done"));
            items.push((
                if crate::kanban_dates::has_token(s, *lane, *index, crate::PickerKind::Date) {
                    "Change date…"
                } else {
                    "Add a date…"
                },
                "add_date",
            ));
            items.push((
                if crate::kanban_dates::has_token(s, *lane, *index, crate::PickerKind::Time) {
                    "Change time…"
                } else {
                    "Add a time…"
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
                items.push(("Split card", "split"));
            }
            items.extend(crate::kanban_lanes::card_move_items(s, *lane, *index));
            items.push(("Copy link to card", "copy_link"));
            items.push(("New note from card", "new_note"));
            items.push(("Archive", "archive"));
            items.push(("Delete", "delete"));
            items
        }
        MenuTarget::Lane(lane) => {
            let mut items = vec![
                ("Add card", "add_card"),
                ("Mark all cards done", "mark_done"),
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
        .focus_group();
    if let Some(trigger) = menu_return(&menu.target) {
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
                            s.notice = Some("Copied a link to the card".to_owned());
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
            .label("Cancel")
            .focusable(true)
            .focus_highlight(k.c.hover)
            .child(text("Cancel").font(k.ty.ui_compact).color(k.c.text_muted))
            .on_press(|s: &mut State| s.close_menu()),
    )
}

/// The name a menu announces, from what it acts on.
fn menu_label(target: &MenuTarget) -> &'static str {
    match target {
        MenuTarget::Note(_) => "Note actions",
        MenuTarget::Folder(_) => "Folder actions",
        MenuTarget::Board(..) => "Board actions",
        MenuTarget::Card(..) | MenuTarget::MoveCard(..) => "Card actions",
        MenuTarget::Lane(_) | MenuTarget::DeleteLane(_) => "Lane actions",
        MenuTarget::ArchiveRestore(_) => "Archive actions",
    }
}

/// The control a menu gives the keyboard back to when it closes, for the
/// menus drawn under a row (the notes list and Kanban cards and lanes).
fn menu_return(target: &MenuTarget) -> Option<String> {
    match target {
        MenuTarget::Card(lane, index) | MenuTarget::MoveCard(lane, index) => {
            Some(format!("kanban-card-{lane}-{index}"))
        }
        MenuTarget::Lane(lane) | MenuTarget::DeleteLane(lane) => {
            Some(format!("kanban-lane-{lane}"))
        }
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
        &referrer_summary(confirm.referrers, "here"),
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
        &referrer_summary(confirm.referrers, "here"),
        confirm.rewrite,
        State::toggle_move_rewrite,
        State::confirm_move_note,
        State::cancel_move_confirm,
        ("Move", "move-confirm-yes", "move-confirm-cancel"),
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
        &referrer_summary(confirm.referrers, "into this folder"),
        confirm.rewrite,
        State::toggle_folder_rename_rewrite,
        State::confirm_folder_rename,
        State::cancel_folder_rename_confirm,
        (
            "Rename",
            "folder-rename-confirm-yes",
            "folder-rename-confirm-cancel",
        ),
    )
}

/// `2 notes link here` / `1 note links here`, pluralised once for both rows.
fn referrer_summary(referrers: usize, place: &str) -> String {
    format!(
        "{} note{} link{} {place}",
        referrers,
        if referrers == 1 { "" } else { "s" },
        if referrers == 1 { "s" } else { "" }
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
    page.child(main_body(s, k))
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
    // A tiled or narrow window keeps the page; the rail returns when there is room.
    if s.window_width >= 980.0 && s.selected.is_some() {
        content = content.child(divider(k)).child(rail(s, k));
    }
    content
}

/// M3 wrapper around [`main`]: the Help overlay takes the whole column, and a
/// first-run welcome banner sits above the ordinary page.
fn main_column(s: &State, k: &Kit) -> El {
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
            text(format!(
                "New vault created at {} — make your first note",
                display_path(&s.vault.root)
            ))
            .font(k.ty.caption())
            .color(k.c.text_base)
            .grow(1.0)
            .truncate(),
        )
        .child(welcome_graph_hint(k))
        .child(status_button(k, "New note", "welcome-new-note", |s| {
            s.new_note();
            true
        }))
        .child(status_button(k, "Dismiss", "welcome-dismiss", |s| {
            s.dismiss_welcome();
            true
        }))
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
        .label("Open the link graph")
        .help("See how notes connect through [[links]]")
        .child(
            text("[[a note]] → graph")
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
const HELP_PAD_X: f32 = 24.0;

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

fn shortcut_groups() -> [(&'static str, &'static [(&'static str, &'static str)]); 5] {
    [
        (
            "Navigation",
            &[
                ("⌘K / Ctrl+K", "Focus search"),
                ("↑ / ↓ (search)", "Move the highlighted result"),
                ("Enter (search)", "Open the highlighted result"),
                ("⌘[ / Ctrl+[", "Back in note history"),
                ("⌘] / Ctrl+]", "Forward in note history"),
                (
                    "⌘1 / ⌘2 / ⌘3 / ⌘4 / ⌘5",
                    "Write / Read / Graph / Board / Kanban",
                ),
                ("Esc", "Close the outermost layer"),
                ("⌘+⇧+P / Ctrl+Shift+P", "Open the command palette"),
            ],
        ),
        (
            "Edit",
            &[
                ("⌘F / Ctrl+F", "Find in the open note"),
                ("⌘F / Ctrl+F (Kanban)", "Find in the board"),
                (
                    "Filter (Kanban header)",
                    "Hide cards by text, tag, date or state",
                ),
                (
                    "Board settings (Kanban header)",
                    "Per-board overrides, one key at a time",
                ),
                ("⌘P / Ctrl+P", "Insert a wikilink"),
                ("⌘N / Ctrl+N", "New note"),
                ("⌘S / Ctrl+S", "Save now (autosave is on)"),
                ("⌘E / Ctrl+E", "Toggle Write / Read"),
                ("[[", "Wikilink completion"),
                ("# (card editor)", "Vault tag completion"),
                ("Tab (editor)", "Indent four spaces"),
                ("F2", "Rename the selected note (on Board: the board)"),
                ("Del", "Delete the selected note"),
            ],
        ),
        (
            "Kanban",
            &[
                ("Tab / Shift+Tab", "Move through the board's controls"),
                ("↑ / ↓ (lane)", "Move between the focused lane's cards"),
                ("← / → (board)", "Move the selection across lanes"),
                ("Space / Enter", "Toggle the selected card's done mark"),
                ("Alt+Arrows", "Move the selected card"),
                (
                    "Alt+Shift+Arrows",
                    "Move it to the top, the bottom or another lane",
                ),
                ("Shift+F10 / Menu", "Open the focused card's menu"),
                ("Double-click", "Edit the card"),
                ("Esc", "Close the menu, confirmation or conflict bar"),
            ],
        ),
        (
            "Graph",
            &[
                ("⌘G / Ctrl+G", "Toggle Graph"),
                ("⌘L / Ctrl+L", "Toggle Global / Local"),
                ("Arrows", "Move the focus ring"),
                ("Enter", "Open the focused note"),
                ("+ / -", "Zoom in / out"),
                ("0 / F", "Fit the visible nodes"),
                ("I", "Toggle Isolate"),
                ("Esc", "Leave the graph"),
            ],
        ),
        (
            "Sidebar",
            &[
                ("⌘?", "Open this Help"),
                ("?", "Open Help when not typing"),
                ("Tab / Shift+Tab", "Move between panes"),
                ("↑ / ↓ (list)", "Move through a list"),
                ("Enter / Space", "Open the focused row"),
                ("Right-click", "Note and folder actions"),
                (
                    "⌘, / Ctrl+,",
                    "Settings (vaults, theme, autosave, Kanban defaults)",
                ),
            ],
        ),
    ]
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
            row().key("palette-empty").w_full().h(64.0).center().child(
                text("No notes or actions match")
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
        .label("Command palette")
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
                        .label("Search notes and actions")
                        .active_descendant(format!("palette-item-{selected}"))
                        .single_line()
                        .grow(1.0)
                        .color(k.c.text_base)
                        .caret(k.c.text_accent, k.selection())
                        .placeholder("Search notes and actions", k.c.text_faint)
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
                        .on_submit(|s: &mut State, _| s.palette_run()),
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
                .label("Results")
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
                    text(plural(items.len(), "result", "results"))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                )
                .child(spacer())
                .child(
                    text("↑↓ move · Enter open · Esc close")
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
            let title = note.map_or_else(|| "Missing note".to_owned(), |n| n.title.clone());
            let folder = note.map(|n| n.folder()).unwrap_or_default();
            let subtitle = if folder.is_empty() {
                "Vault root".to_owned()
            } else {
                folder.to_owned()
            };
            (title, subtitle, String::new(), k.c.text_accent)
        }
        PaletteItem::Kanban(id) => {
            let note = s.vault.notes.get(*id);
            let title = note.map_or_else(|| "Missing note".to_owned(), |n| n.title.clone());
            let folder = note.map(|n| n.folder()).unwrap_or_default();
            let subtitle = if folder.is_empty() {
                "Kanban · Vault root".to_owned()
            } else {
                format!("Kanban · {folder}")
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
        .on_press(move |s: &mut State| s.palette_run_index(index))
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
        .h(54.0)
        .px(HELP_PAD_X)
        .gap(12.0)
        .center_y()
        .child(
            text("Keyboard shortcuts")
                .font(TextStyle::new(16.0, 600.0).lh(22.0))
                .color(k.c.text_base),
        )
        .child(spacer())
        .child(
            pressable(k, "help-close".to_owned())
                .px(10.0)
                .py(5.0)
                .border(1.0, k.c.border_base)
                .child(text("Close").font(k.ty.ui_compact).color(k.c.text_base))
                .on_press(|s: &mut State| s.close_help()),
        )
}

fn help_section(
    k: &Kit,
    index: usize,
    title: &str,
    shortcuts: &[(&str, &str)],
    layout: &HelpLayout,
) -> El {
    let mut rows = col().w_full().min_w(0.0).gap(2.0);
    for (row_index, (keys, action)) in shortcuts.iter().enumerate() {
        rows = rows.child(help_row(k, index, row_index, keys, action, layout));
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
    let version = env!("CARGO_PKG_VERSION");
    help_card(k, "help-about", "Autosave")
        .child(
            text(format!(
                "Autosave writes {delay:.2} s after you stop typing."
            ))
            .font(k.ty.ui_compact.lh(18.0))
            .color(k.c.text_base)
            .wrap()
            .w_full(),
        )
        .child(
            text(format!("Version {version}."))
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
    let mut body = help_card(k, "help-diagnostics", "Diagnostics").selection_group(true);
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
                text("last log lines:")
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
                "Copy diagnostics",
                true,
            ))
            .child(help_link(
                k,
                "help-report-issue",
                "Report an issue on GitHub",
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
    let summary = match files {
        0 => "Trash is empty".to_owned(),
        1 => format!("Trash · 1 note · {}", trash_bytes(bytes)),
        _ => format!("Trash · {files} notes · {}", trash_bytes(bytes)),
    };
    let empty = files == 0;
    help_card(k, "help-trash", "Trash")
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
                        .child(text("Empty trash").font(k.ty.ui_compact).color(if empty {
                            k.c.text_faint
                        } else {
                            k.c.text_accent
                        }))
                        .label("Empty trash")
                        .on_press(|s: &mut State| {
                            s.empty_trash();
                        }),
                ),
        )
        .child(
            text(
                "Deleted notes stay here until undo or permanent removal. At most 500 files \
                 or 100 MB are kept; the oldest timestamp folder is pruned first.",
            )
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
            text("Scanning your vault")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(bar)
        .child(
            text(format!("Scanning {done}/{total} notes — Esc to cancel"))
                .key("scan-progress-label")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(status_button(
            k,
            "Cancel",
            "scan-cancel",
            State::cancel_scan,
        ))
        .child(
            text_input(&s.scan_field, k.ty.ui_compact)
                .key("scan-escape")
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
        "Scan cancelled"
    } else {
        "Could not open the vault"
    };
    let hint = if cancelled {
        "Retry scans the vault again, or pick another folder in Settings (⌘,)."
    } else {
        "Pick another folder in Settings (⌘,), or pass --vault / set MUNINN_VAULT."
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
            text(format!("Recovered unsaved edits for {}", {
                let boards = s
                    .recovered
                    .iter()
                    .filter(|e| crate::board_safety::is_board_rel(&e.rel))
                    .count();
                match (count - boards, boards) {
                    (notes, 0) => plural(notes, "note", "notes"),
                    (0, boards) => plural(boards, "board", "boards"),
                    (notes, boards) => format!(
                        "{} and {}",
                        plural(notes, "note", "notes"),
                        plural(boards, "board", "boards")
                    ),
                }
            }))
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
        let label = match (both, slot) {
            (true, crate::recovery::RecoverySlot::Kanban) => format!("{rel} — Kanban edits"),
            (true, crate::recovery::RecoverySlot::Editor) => format!("{rel} — Write edits"),
            (false, _) => rel.clone(),
        };
        banner = banner.child(
            row()
                .gap(4.0)
                .center_y()
                .child(
                    text(label)
                        .key(format!("recovery-label-{index}"))
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
                            s.restore_recovery_slot(&rel, slot);
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
    // Kanban slice 3 (s2 review N4): on the Kanban page the title is the
    // kanban shown, which may not be the selected note.
    let note = match (s.mode, &s.kanban) {
        (Mode::Kanban, Some(shown)) => s.note_id(&shown.rel),
        _ => s.selected,
    };
    let (title, path) = match note.and_then(|id| s.vault.notes.get(id)) {
        Some(note) => (note.title.clone(), note.rel.clone()),
        None => ("No note selected".to_owned(), String::new()),
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
    row()
        .h(58.0)
        .px(20.0)
        .gap(12.0)
        .center_y()
        .child(sidebar_toggle(s, k))
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
        .child(if open_as_kanban {
            crate::kanban_view::header_button(
                k,
                "open-as-kanban",
                &ic::CHECKLIST,
                "Open as kanban",
                |s: &mut State| s.set_mode(Mode::Kanban),
            )
        } else {
            el()
        })
        .child(mode_switch(s, k))
}

/// Slice 8: show or hide the notes sidebar (also Cmd/Ctrl+\\).
fn sidebar_toggle(s: &State, k: &Kit) -> El {
    pressable(k, "sidebar-toggle".to_owned())
        .size(26.0, 26.0)
        .center()
        .border(1.0, k.c.border_muted)
        .role("button")
        .label(if s.sidebar_shown() {
            "Hide sidebar"
        } else {
            "Show sidebar"
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
        .child(mode_button(
            k,
            "Board",
            &ic::DOT_GRID,
            "mode-board",
            Mode::Board,
            s.mode,
        ))
        .child(mode_button(
            k,
            "Kanban",
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
        .label("Completions")
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
        return empty_page(k, "This note is empty. Write something in the editor.");
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
        .child(sidebar_section_head(k, "KANBANS", kanbans.len()));
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
        .child(sidebar_section_head(k, "BOARDS", s.sidebar_boards.len()));
    let open = s.board_open.as_ref().map(|open| open.path.as_path());
    for (i, board) in s.sidebar_boards.iter().enumerate() {
        let path = board.path.clone();
        if s.renaming_board_at(&path, crate::BoardPlace::Sidebar) {
            list = list.child(crate::board_rename::rename_field(s, k));
            continue;
        }
        let menu_path = path.clone();
        let current = s.mode == Mode::Board && open == Some(board.path.as_path());
        let folder = board.folder();
        let mut body = row().grow(1.0).min_w(0.0).gap(6.0).center_y().child(
            text(&board.title)
                .key(format!("sidebar-board-title-{i}"))
                .font(k.ty.ui_compact)
                .color(if current {
                    k.c.text_accent
                } else {
                    k.c.text_base
                })
                .truncate(),
        );
        if !folder.is_empty() {
            body = body.child(
                text(folder)
                    .font(k.ty.caption())
                    .color(k.c.text_faint)
                    .truncate(),
            );
        }
        list = list.child(
            pressable(k, format!("sidebar-board-{i}"))
                .h(30.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
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
            .unwrap_or_else(|| "missing file".to_owned());
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
                            text(if board { "Open board" } else { "Open" })
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
                    text("ATTACHMENTS")
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
                    text("Files stay outside the note index")
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
            "Saving…".to_owned(),
            Some(k.c.warning.fg),
        )
    } else if let Some(notice) = &s.notice {
        // DES-02: imported attachments have no sidebar row, so the status bar
        // is where their count is reported.
        (StatusTone::Notice, notice.clone(), None)
    } else if s.last_save.is_some() {
        (StatusTone::Saved, "Saved".to_owned(), None)
    } else {
        (StatusTone::Calm, "Autosave on".to_owned(), None)
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
    if s.load_ms() >= 5.0 {
        bar = bar.child(
            text(format!("scan {:.0} ms", s.load_ms()))
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
