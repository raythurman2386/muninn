//! Kanban slice 14: the Board settings panel and the Settings modal's
//! **Kanban** tab.
//!
//! Both surfaces are generated from one schema ([`muninn_core::KANBAN_KEYS`]):
//! a key, its label, help, control kind and built-in value are written once.
//! The modal edits the app-wide defaults in `settings.json`
//! (`kanban_defaults`, plus the top-level `kanban_autosave`); the page's panel
//! edits the shown board's footer. A footer value wins over an app default,
//! which wins over the built-in value.
//!
//! Every board change writes exactly one footer key through
//! [`State::edit_kanban`], so it is one undo step, marks the board unsaved and
//! follows `⌘S` / autosave. Every app change writes `settings.json` only.
//! Opening or closing either surface writes nothing. Display and behaviour
//! settings read the resolved value each frame, so lane width, the new-line
//! trigger and the header buttons change immediately.

use muninn_core::{
    KANBAN_AUTOSAVE, KANBAN_KEYS, Kanban, KanbanEffective, KanbanKey, KanbanScope, KanbanSection,
    KanbanSettingKind, KanbanSettingValue, MetadataField, kanban_keys_in,
};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl, Switch, TextInput};

use crate::State;

/// Which surface a control writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    /// Settings › Kanban: `settings.json`.
    App,
    /// The Kanban page's Board settings panel: the board footer.
    Board,
}

impl Context {
    fn is_app(self) -> bool {
        self == Context::App
    }
}

impl State {
    /// The shown board's resolved board settings (slice 14): footer, then
    /// `kanban_defaults`, then the built-in value.
    #[must_use]
    pub fn kanban_board_settings(&self) -> muninn_core::KanbanBoardSettings {
        muninn_core::KanbanBoardSettings::resolve(
            self.kanban_board()
                .and_then(muninn_core::Kanban::footer_json),
            &self.settings.kanban_defaults,
        )
    }

    /// The shown board's resolved settings: footer, then `kanban_defaults`,
    /// then the built-in value, with the app-wide autosave choice.
    #[must_use]
    pub fn kanban_effective(&self) -> KanbanEffective {
        let mut effective = KanbanEffective::resolve(
            self.kanban_board().and_then(Kanban::footer_json),
            &self.settings.kanban_defaults,
        );
        effective.autosave = self.settings.kanban_autosave;
        effective
    }

    /// The app-wide defaults resolved without a board footer.
    #[must_use]
    pub fn kanban_app_effective(&self) -> KanbanEffective {
        let mut effective = KanbanEffective::resolve(None, &self.settings.kanban_defaults);
        effective.autosave = self.settings.kanban_autosave;
        effective
    }

    /// The value a control shows for `key`.
    #[must_use]
    pub fn kanban_setting_value(&self, app: bool, key: &str) -> Option<KanbanSettingValue> {
        if app {
            self.kanban_app_effective().get(key)
        } else {
            self.kanban_effective().get(key)
        }
    }

    /// Whether the shown board's footer sets `key`, whatever its value.
    #[must_use]
    pub fn kanban_footer_sets(&self, key: &str) -> bool {
        self.kanban_board()
            .and_then(Kanban::footer_json)
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .is_some_and(|value| value.get(key).is_some())
    }

    /// Whether `settings.json` sets `key` as an app default.
    #[must_use]
    pub fn kanban_app_sets(&self, key: &str) -> bool {
        if key == KANBAN_AUTOSAVE {
            // The built-in is on, so only `false` is a set app choice.
            return !self.settings.kanban_autosave;
        }
        self.settings.kanban_defaults.to_json().get(key).is_some()
    }

    /// The header's **Board settings** button: show the panel, or the lanes.
    pub fn toggle_kanban_settings(&mut self) {
        self.kanban_settings_open = !self.kanban_settings_open;
        if self.kanban_settings_open {
            self.kanban_archive_open = false;
            self.close_kanban_filter();
            self.menu = None;
            self.refresh_kanban_setting_text();
        } else {
            self.blur_kanban_setting_fields();
        }
    }

    /// Close the board panel without changing anything.
    pub fn close_kanban_settings(&mut self) {
        self.kanban_settings_open = false;
        self.blur_kanban_setting_fields();
    }

    fn blur_kanban_setting_fields(&mut self) {
        for field in self.kanban_board_text.values_mut() {
            field.focused = false;
        }
        self.kanban_board_new_metadata.focused = false;
    }

    /// Refill every text field from the resolved values. Called when a surface
    /// opens (never on a plain view), so a field that is not focused shows the
    /// footer/app value it belongs to.
    pub fn refresh_kanban_setting_text(&mut self) {
        let board = self.kanban_effective();
        let app = self.kanban_app_effective();
        for entry in KANBAN_KEYS {
            if entry.kind != KanbanSettingKind::Text {
                continue;
            }
            let text_of = |effective: &KanbanEffective| match effective.get(entry.key) {
                Some(KanbanSettingValue::Text(text)) => text,
                _ => String::new(),
            };
            let board_text = text_of(&board);
            let app_text = text_of(&app);
            self.kanban_board_text
                .entry(entry.key)
                .or_default()
                .replace_text(&board_text);
            self.kanban_app_text
                .entry(entry.key)
                .or_default()
                .replace_text(&app_text);
        }
        self.kanban_board_new_metadata = rune::TextEdit::default();
        self.kanban_app_new_metadata = rune::TextEdit::default();
    }

    /// Write one board footer key as an undoable edit, or do nothing when the
    /// resolved value already equals `value`.
    pub fn set_kanban_board_value(&mut self, key: &'static str, value: KanbanSettingValue) -> bool {
        if self.kanban_setting_value(false, key).as_ref() == Some(&value) {
            return true;
        }
        let json = value.to_json();
        self.edit_kanban("change the board setting", |board| {
            board.set_setting(key, &json)
        })
    }

    /// Remove one footer key, falling back to the app default. Nothing is
    /// written when the footer does not set it.
    pub fn reset_kanban_board_value(&mut self, key: &'static str) {
        if !self.kanban_footer_sets(key) {
            return;
        }
        if self.edit_kanban("reset the board setting", |board| board.remove_setting(key)) {
            self.refill_text_field(false, key);
        }
    }

    /// Write one app-wide default in `settings.json`. `kanban_autosave` is the
    /// one key stored at the top level.
    pub fn set_kanban_app_value(&mut self, key: &'static str, value: KanbanSettingValue) {
        if self.kanban_setting_value(true, key).as_ref() == Some(&value) {
            return;
        }
        if key == KANBAN_AUTOSAVE {
            if let Some(flag) = value.as_bool() {
                self.settings.kanban_autosave = flag;
                self.save_settings();
            }
            return;
        }
        let json = value.to_json();
        if self.settings.kanban_defaults.set_key(key, Some(&json)) {
            self.save_settings();
        }
    }

    /// Remove one app-wide default, falling back to the built-in value.
    pub fn reset_kanban_app_value(&mut self, key: &'static str) {
        if !self.kanban_app_sets(key) {
            return;
        }
        if key == KANBAN_AUTOSAVE {
            self.settings.kanban_autosave = true;
            self.save_settings();
            return;
        }
        if self.settings.kanban_defaults.set_key(key, None) {
            self.save_settings();
            self.refill_text_field(true, key);
        }
    }

    /// Put one text key's field back to its resolved value after a reset.
    fn refill_text_field(&mut self, app: bool, key: &'static str) {
        let Some(entry) = muninn_core::kanban_key(key) else {
            return;
        };
        if entry.kind != KanbanSettingKind::Text {
            return;
        }
        let value = if app {
            self.kanban_app_effective().get(key)
        } else {
            self.kanban_effective().get(key)
        };
        let text = match value {
            Some(KanbanSettingValue::Text(text)) => text,
            _ => String::new(),
        };
        let fields = if app {
            &mut self.kanban_app_text
        } else {
            &mut self.kanban_board_text
        };
        fields.entry(key).or_default().replace_text(&text);
    }
}

/// Settings › Kanban: the app-wide defaults for every board.
pub(crate) fn app_tab(s: &State, k: &Kit) -> El {
    let mut body = col().key("settings-tab-kanban").w_full().gap(18.0).child(
        col()
            .gap(4.0)
            .child(
                text("Kanban defaults")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base),
            )
            .child(
                text("A board's own settings footer wins; otherwise these apply to every board.")
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .wrap(),
            ),
    );
    for section in KanbanSection::ALL {
        body = body.child(section_block(s, k, Context::App, section));
    }
    body
}

/// The Kanban page's Board settings panel: per-board overrides.
pub(crate) fn board_panel(s: &State, k: &Kit) -> El {
    let mut sections = col().key("kanban-settings-sections").w_full().gap(16.0);
    for section in KanbanSection::ALL {
        if section == KanbanSection::App {
            continue;
        }
        sections = sections.child(section_block(s, k, Context::Board, section));
    }
    col()
        .key("kanban-settings-panel")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(12.0)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(10.0)
                .child(
                    text("Board settings")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .grow(1.0),
                )
                .child(
                    Button::new("Close")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|s: &mut State| s.close_kanban_settings())
                        .build(k)
                        .key("kanban-settings-close"),
                ),
        )
        .child(
            text("Changes write only that key to this note's settings footer and can be undone.")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        )
        .child(
            col()
                .key("kanban-settings-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(sections),
        )
}

fn section_block(s: &State, k: &Kit, context: Context, section: KanbanSection) -> El {
    let mut rows = col().w_full().gap(12.0);
    for entry in kanban_keys_in(section) {
        if context == Context::Board && entry.scope == KanbanScope::AppOnly {
            continue;
        }
        rows = rows.child(setting_row(s, k, context, entry));
    }
    col()
        .key(format!(
            "kanban-settings-section-{}",
            section_title_key(section)
        ))
        .w_full()
        .gap(8.0)
        .child(
            text(section.title())
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(section.caption())
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        )
        .child(rows)
}

fn section_title_key(section: KanbanSection) -> &'static str {
    match section {
        KanbanSection::Dates => "dates",
        KanbanSection::Tags => "tags",
        KanbanSection::Metadata => "metadata",
        KanbanSection::Archive => "archive",
        KanbanSection::Board => "board",
        KanbanSection::App => "app",
    }
}

fn setting_row(s: &State, k: &Kit, context: Context, entry: &'static KanbanKey) -> El {
    let value = s
        .kanban_setting_value(context.is_app(), entry.key)
        .or_else(|| entry.builtin.value())
        .unwrap_or_else(|| empty_value(entry.kind));
    let (source, source_color) = source_label(s, k, context, entry.key);
    let mut row = col()
        .key(format!("kanban-setting-{}", entry.key))
        .w_full()
        .gap(4.0)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .child(
                    text(entry.label)
                        .key(format!("kanban-setting-{}-label", entry.key))
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .grow(1.0),
                )
                .child(
                    text(source)
                        .key(format!("kanban-setting-{}-source", entry.key))
                        .font(k.ty.caption())
                        .color(source_color),
                ),
        )
        .child(
            text(entry.help)
                .key(format!("kanban-setting-{}-help", entry.key))
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        )
        .child(control(s, k, context, entry, &value));
    let resets = reset_buttons(s, k, context, entry);
    if has_reset(context, s, entry) {
        row = row.child(resets);
    }
    row
}

fn has_reset(context: Context, s: &State, entry: &KanbanKey) -> bool {
    match context {
        Context::App => s.kanban_app_sets(entry.key),
        Context::Board => s.kanban_footer_sets(entry.key) || s.kanban_app_sets(entry.key),
    }
}

fn reset_buttons(s: &State, k: &Kit, context: Context, entry: &'static KanbanKey) -> El {
    let mut buttons = row().w_full().gap(6.0);
    match context {
        Context::App => {
            if s.kanban_app_sets(entry.key) {
                buttons = buttons.child(
                    small_button(
                        k,
                        format!("kanban-reset-app-{}", entry.key),
                        "Reset to built-in",
                    )
                    .on_press(move |s: &mut State| s.reset_kanban_app_value(entry.key)),
                );
            }
        }
        Context::Board => {
            if s.kanban_footer_sets(entry.key) {
                buttons = buttons.child(
                    small_button(
                        k,
                        format!("kanban-reset-board-{}", entry.key),
                        "Reset board",
                    )
                    .on_press(move |s: &mut State| {
                        s.reset_kanban_board_value(entry.key);
                    }),
                );
            }
            if s.kanban_app_sets(entry.key) {
                buttons = buttons.child(
                    small_button(
                        k,
                        format!("kanban-reset-app-{}", entry.key),
                        "Reset app default",
                    )
                    .on_press(move |s: &mut State| s.reset_kanban_app_value(entry.key)),
                );
            }
        }
    }
    buttons
}

fn small_button(k: &Kit, key: String, label: &'static str) -> El {
    Button::new(label)
        .variant(ButtonVariant::Ghost)
        .build(k)
        .key(key)
}

fn source_label(s: &State, k: &Kit, context: Context, key: &str) -> (&'static str, rune::Color) {
    if context == Context::Board && s.kanban_footer_sets(key) {
        ("Board override", k.c.text_accent)
    } else if s.kanban_app_sets(key) {
        ("App default", k.c.text_muted)
    } else {
        ("Default", k.c.text_muted)
    }
}

fn control(
    s: &State,
    k: &Kit,
    context: Context,
    entry: &'static KanbanKey,
    value: &KanbanSettingValue,
) -> El {
    match entry.kind {
        KanbanSettingKind::Bool => {
            let checked = value.as_bool().unwrap_or(false);
            Switch::new(checked)
                .label(entry.label)
                .on_toggle(move |s: &mut State| {
                    let next = !s
                        .kanban_setting_value(context.is_app(), entry.key)
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    write(s, context, entry.key, KanbanSettingValue::Bool(next));
                })
                .build(k)
                .key(format!("kanban-setting-{}-switch", entry.key))
        }
        KanbanSettingKind::Text => text_control(s, k, context, entry),
        KanbanSettingKind::Choice(choices) => {
            let labels: Vec<&str> = choices.iter().map(|choice| choice.label).collect();
            let selected = value
                .as_text()
                .and_then(|current| choices.iter().position(|choice| choice.value == current))
                .unwrap_or(0);
            SegmentedControl::new(&labels, Some(selected))
                .label(entry.label)
                .fit()
                .on_pick(move |s: &mut State, index, _: &mut rune::Tx| {
                    if let Some(choice) = choices.get(index) {
                        write(
                            s,
                            context,
                            entry.key,
                            KanbanSettingValue::Choice(choice.value.to_owned()),
                        );
                    }
                })
                .build(k)
                .key(format!("kanban-setting-{}-choice", entry.key))
        }
        KanbanSettingKind::Number(number) => number_control(k, context, entry, value, number),
        KanbanSettingKind::MetadataList => metadata_control(s, k, context, entry, value),
        KanbanSettingKind::TagColorList => tag_color_control(k, entry, value),
    }
}

fn text_control(s: &State, k: &Kit, context: Context, entry: &'static KanbanKey) -> El {
    let key = entry.key;
    let field = if context.is_app() {
        s.kanban_app_text.get(key)
    } else {
        s.kanban_board_text.get(key)
    };
    let Some(field) = field else {
        return rune::el();
    };
    TextInput::new(field)
        .label(entry.label)
        .width(None)
        .on_edit(move |s: &mut State| {
            if context.is_app() {
                s.kanban_app_text.entry(key).or_default()
            } else {
                s.kanban_board_text.entry(key).or_default()
            }
        })
        .on_change(move |s: &mut State, _| {
            let text = if context.is_app() {
                s.kanban_app_text.get(key).map(|field| field.text.clone())
            } else {
                s.kanban_board_text.get(key).map(|field| field.text.clone())
            }
            .unwrap_or_default();
            write(s, context, key, KanbanSettingValue::Text(text));
        })
        .build(k)
        .key(format!("kanban-setting-{key}-text"))
}

fn number_control(
    k: &Kit,
    context: Context,
    entry: &'static KanbanKey,
    value: &KanbanSettingValue,
    number: muninn_core::KanbanNumber,
) -> El {
    let current = value.as_number().unwrap_or(number.min);
    let shown = if entry.key == muninn_core::MAX_ARCHIVE_CARDS && current == 0 {
        "Unlimited".to_owned()
    } else {
        current.to_string()
    };
    let key = entry.key;
    row()
        .key(format!("kanban-setting-{key}-number"))
        .gap(8.0)
        .center_y()
        .child(
            small_button(k, format!("kanban-setting-{key}-minus"), "−").on_press(
                move |s: &mut State| {
                    let current = s
                        .kanban_setting_value(context.is_app(), key)
                        .and_then(|value| value.as_number())
                        .unwrap_or(number.min);
                    let next = current.saturating_sub(number.step).max(number.min);
                    write(s, context, key, KanbanSettingValue::Number(next));
                },
            ),
        )
        .child(
            text(shown)
                .key(format!("kanban-setting-{key}-value"))
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
        .child(
            small_button(k, format!("kanban-setting-{key}-plus"), "+").on_press(
                move |s: &mut State| {
                    let current = s
                        .kanban_setting_value(context.is_app(), key)
                        .and_then(|value| value.as_number())
                        .unwrap_or(number.min);
                    let next = current.saturating_add(number.step).min(number.max);
                    write(s, context, key, KanbanSettingValue::Number(next));
                },
            ),
        )
}

fn metadata_control(
    s: &State,
    k: &Kit,
    context: Context,
    entry: &'static KanbanKey,
    value: &KanbanSettingValue,
) -> El {
    let fields = match value {
        KanbanSettingValue::Metadata(fields) => fields.clone(),
        _ => Vec::new(),
    };
    let key = entry.key;
    let mut rows = col()
        .key(format!("kanban-setting-{key}-list"))
        .w_full()
        .gap(4.0);
    if fields.is_empty() {
        rows = rows.child(
            text("No metadata keys")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    for (index, field) in fields.iter().enumerate() {
        let current = field.clone();
        rows = rows.child(
            row()
                .key(format!("kanban-setting-{key}-row-{index}"))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    text(current.key.clone())
                        .key(format!("kanban-setting-{key}-row-{index}-key"))
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .grow(1.0)
                        .truncate(),
                )
                .child(
                    Switch::new(current.contains_markdown)
                        .label("Contains markdown")
                        .on_toggle(move |s: &mut State| {
                            let mut fields = match s.kanban_setting_value(context.is_app(), key) {
                                Some(KanbanSettingValue::Metadata(fields)) => fields,
                                _ => Vec::new(),
                            };
                            if let Some(field) = fields.get_mut(index) {
                                field.contains_markdown = !field.contains_markdown;
                            }
                            write(s, context, key, KanbanSettingValue::Metadata(fields));
                        })
                        .build(k)
                        .key(format!("kanban-setting-{key}-row-{index}-markdown")),
                )
                .child(
                    small_button(
                        k,
                        format!("kanban-setting-{key}-row-{index}-remove"),
                        "Remove",
                    )
                    .on_press(move |s: &mut State| {
                        let mut fields = match s.kanban_setting_value(context.is_app(), key) {
                            Some(KanbanSettingValue::Metadata(fields)) => fields,
                            _ => Vec::new(),
                        };
                        if index < fields.len() {
                            fields.remove(index);
                        }
                        write(s, context, key, KanbanSettingValue::Metadata(fields));
                    }),
                ),
        );
    }
    let new_field = if context.is_app() {
        &s.kanban_app_new_metadata
    } else {
        &s.kanban_board_new_metadata
    };
    rows.child(
        row()
            .key(format!("kanban-setting-{key}-add-row"))
            .w_full()
            .gap(8.0)
            .center_y()
            .child(
                TextInput::new(new_field)
                    .label("New metadata key")
                    .placeholder("status")
                    .width(Some(180.0))
                    .on_edit(move |s: &mut State| {
                        if context.is_app() {
                            &mut s.kanban_app_new_metadata
                        } else {
                            &mut s.kanban_board_new_metadata
                        }
                    })
                    .build(k)
                    .key(format!("kanban-setting-{key}-new")),
            )
            .child(
                small_button(k, format!("kanban-setting-{key}-add"), "Add").on_press(
                    move |s: &mut State| {
                        let field = if context.is_app() {
                            &mut s.kanban_app_new_metadata
                        } else {
                            &mut s.kanban_board_new_metadata
                        };
                        let new_key = field.text.trim().to_owned();
                        if new_key.is_empty() {
                            return;
                        }
                        let mut fields = match s.kanban_setting_value(context.is_app(), key) {
                            Some(KanbanSettingValue::Metadata(fields)) => fields,
                            _ => Vec::new(),
                        };
                        if fields.iter().any(|field| field.key == new_key) {
                            return;
                        }
                        fields.push(MetadataField {
                            key: new_key,
                            contains_markdown: false,
                        });
                        if context.is_app() {
                            s.kanban_app_new_metadata.replace_text("");
                        } else {
                            s.kanban_board_new_metadata.replace_text("");
                        }
                        write(s, context, key, KanbanSettingValue::Metadata(fields));
                    },
                ),
            ),
    )
}

fn tag_color_control(k: &Kit, entry: &'static KanbanKey, value: &KanbanSettingValue) -> El {
    let colors = match value {
        KanbanSettingValue::TagColors(colors) => colors.clone(),
        _ => Vec::new(),
    };
    let mut rows = col()
        .key(format!("kanban-setting-{}-list", entry.key))
        .w_full()
        .gap(4.0);
    if colors.is_empty() {
        rows = rows.child(
            text("No tag colours")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    for (index, color) in colors.iter().enumerate() {
        rows = rows.child(
            row()
                .key(format!("kanban-setting-{}-row-{index}", entry.key))
                .w_full()
                .gap(8.0)
                .child(
                    text(color.tag_key.clone())
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .grow(1.0),
                )
                .child(
                    text(color.color.clone())
                        .key(format!("kanban-setting-{}-row-{index}-color", entry.key))
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                ),
        );
    }
    rows.child(
        text("Kept as written. Viewing never rewrites it.")
            .font(k.ty.caption())
            .color(k.c.text_faint),
    )
}

fn write(s: &mut State, context: Context, key: &'static str, value: KanbanSettingValue) {
    if context.is_app() {
        s.set_kanban_app_value(key, value);
    } else {
        s.set_kanban_board_value(key, value);
    }
}

fn empty_value(kind: KanbanSettingKind) -> KanbanSettingValue {
    match kind {
        KanbanSettingKind::Bool => KanbanSettingValue::Bool(false),
        KanbanSettingKind::Text => KanbanSettingValue::Text(String::new()),
        KanbanSettingKind::Choice(_) => KanbanSettingValue::Choice(String::new()),
        KanbanSettingKind::Number(_) => KanbanSettingValue::Number(0),
        KanbanSettingKind::MetadataList => KanbanSettingValue::Metadata(Vec::new()),
        KanbanSettingKind::TagColorList => KanbanSettingValue::TagColors(Vec::new()),
    }
}
