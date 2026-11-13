//! One schema for every kanban setting (slice 14).
//!
//! The settings modal's **Kanban** tab and the Kanban page's **Board
//! settings** panel are both generated from [`KANBAN_KEYS`], so a key, its
//! label and its built-in value are written once. Every key resolves from the
//! board's settings footer, then the app-wide `kanban_defaults` object in
//! `settings.json`, then its built-in value
//! (`docs/kanban-format.md` section 9).
//!
//! A key is one row. `key` is both the footer key and the `kanban_defaults`
//! key; the app-only row [`KANBAN_AUTOSAVE`] is stored at the top level of
//! `settings.json` and has no footer meaning. A later key is one entry here
//! plus its field in [`KanbanDefaults`] and the resolver that owns it.

use serde_json::{Value, json};

use super::archive::{
    ARCHIVE_DATE_FORMAT, ARCHIVE_DATE_POSITION, ARCHIVE_DATE_SEPARATOR, ARCHIVE_WITH_DATE,
    ArchiveSettings, MAX_ARCHIVE_CARDS,
};
use super::dates::{
    DATE_DISPLAY_FORMAT, DATE_FORMAT, DATE_TRIGGER, DateSettings, HIDE_DATE_IN_TITLE,
    KanbanDefaults, LINK_DATE_TO_DAILY_NOTE, SHOW_RELATIVE_DATE, TIME_FORMAT, TIME_TRIGGER,
};
use super::layout::{
    DEFAULT_LANE_WIDTH, KanbanBoardSettings, LANE_WIDTH, MAX_LANE_WIDTH, MIN_LANE_WIDTH,
    NEW_CARD_INSERTION, NEW_LINE_TRIGGER, NEW_NOTE_FOLDER, NEW_NOTE_TEMPLATE, SHOW_ARCHIVE_BUTTON,
    SHOW_CHECKBOXES, SHOW_FILTER_BUTTON, SHOW_OPEN_MARKDOWN_BUTTON,
};
use super::metadata::{
    LINKED_PAGE_METADATA, MetadataField, MetadataSettings, SHOW_MISSING_METADATA,
};
use super::tags::{HIDE_TAGS_IN_TITLE, SHOW_TAGS_AT_BOTTOM, TAG_COLORS, TagColor, TagSettings};

/// The top-level `settings.json` key for kanban autosave. It is not a footer
/// or `kanban_defaults` key: autosave is a whole-app choice.
pub const KANBAN_AUTOSAVE: &str = "kanban_autosave";

/// A group of keys in the settings UIs, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KanbanSection {
    Dates,
    Tags,
    Metadata,
    Archive,
    Board,
    App,
}

impl KanbanSection {
    pub const ALL: [KanbanSection; 6] = [
        KanbanSection::Dates,
        KanbanSection::Tags,
        KanbanSection::Metadata,
        KanbanSection::Archive,
        KanbanSection::Board,
        KanbanSection::App,
    ];

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            KanbanSection::Dates => "Dates and times",
            KanbanSection::Tags => "Tags",
            KanbanSection::Metadata => "Linked note metadata",
            KanbanSection::Archive => "Archive",
            KanbanSection::Board => "Board and cards",
            KanbanSection::App => "App",
        }
    }

    /// The one-line description under the group's heading.
    #[must_use]
    pub fn caption(self) -> &'static str {
        match self {
            KanbanSection::Dates => "How dates and times are typed, written and shown.",
            KanbanSection::Tags => "Tag chips and the kept tag-colour list.",
            KanbanSection::Metadata => "Keys drawn at a card's foot from its first linked note.",
            KanbanSection::Archive => "What archiving writes and how many cards the archive keeps.",
            KanbanSection::Board => "Lane width, card editing and the header buttons.",
            KanbanSection::App => "Choices for every board; there is no per-board override.",
        }
    }
}

/// Where a key can be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KanbanScope {
    /// The board footer and the app-wide `kanban_defaults`.
    BoardAndApp,
    /// Only `settings.json`; the global tab shows it, a board panel does not.
    AppOnly,
}

/// The bounds of a number setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KanbanNumber {
    pub min: u64,
    pub max: u64,
    pub step: u64,
}

/// One choice of an enum setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KanbanChoice {
    pub value: &'static str,
    pub label: &'static str,
}

/// The kind of control a key's value needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KanbanSettingKind {
    /// A switch.
    Bool,
    /// A single-line text field.
    Text,
    /// A segmented control over [`KanbanChoice`] rows.
    Choice(&'static [KanbanChoice]),
    /// A stepper.
    Number(KanbanNumber),
    /// `linked-page-metadata`: rows of a key and `containsMarkdown`.
    MetadataList,
    /// `tag-colors`: read-only rows, kept as written.
    TagColorList,
}

/// A key's built-in value. `Absent` means the built-in is "not written".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KanbanBuiltin {
    Bool(bool),
    Text(&'static str),
    Number(u64),
    Choice(&'static str),
    Absent,
}

impl KanbanBuiltin {
    /// The built-in as a resolved value; `None` for [`KanbanBuiltin::Absent`].
    #[must_use]
    pub fn value(self) -> Option<KanbanSettingValue> {
        match self {
            KanbanBuiltin::Bool(value) => Some(KanbanSettingValue::Bool(value)),
            KanbanBuiltin::Text(value) => Some(KanbanSettingValue::Text(value.to_owned())),
            KanbanBuiltin::Number(value) => Some(KanbanSettingValue::Number(value)),
            KanbanBuiltin::Choice(value) => Some(KanbanSettingValue::Choice(value.to_owned())),
            KanbanBuiltin::Absent => None,
        }
    }
}

/// One resolved setting value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KanbanSettingValue {
    Bool(bool),
    Text(String),
    Number(u64),
    Choice(String),
    Metadata(Vec<MetadataField>),
    TagColors(Vec<TagColor>),
}

impl KanbanSettingValue {
    /// The JSON value written to a footer or to `kanban_defaults`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            KanbanSettingValue::Bool(value) => Value::Bool(*value),
            KanbanSettingValue::Text(value) | KanbanSettingValue::Choice(value) => {
                Value::String(value.clone())
            }
            KanbanSettingValue::Number(value) => Value::from(*value),
            KanbanSettingValue::Metadata(fields) => Value::Array(
                fields
                    .iter()
                    .map(|field| {
                        json!({
                            "key": field.key,
                            "containsMarkdown": field.contains_markdown,
                        })
                    })
                    .collect(),
            ),
            KanbanSettingValue::TagColors(colors) => Value::Array(
                colors
                    .iter()
                    .map(|color| json!({"tagKey": color.tag_key, "color": color.color}))
                    .collect(),
            ),
        }
    }

    /// Read a value of `kind` from JSON, when its type is right.
    #[must_use]
    pub fn from_json(kind: KanbanSettingKind, value: &Value) -> Option<Self> {
        match kind {
            KanbanSettingKind::Bool => value.as_bool().map(Self::Bool),
            KanbanSettingKind::Text => value.as_str().map(|text| Self::Text(text.to_owned())),
            KanbanSettingKind::Choice(_) => {
                value.as_str().map(|text| Self::Choice(text.to_owned()))
            }
            KanbanSettingKind::Number(_) => value.as_u64().map(Self::Number),
            KanbanSettingKind::MetadataList => {
                super::metadata::metadata_fields(Some(value)).map(Self::Metadata)
            }
            KanbanSettingKind::TagColorList => {
                super::tags::tag_colors(Some(value)).map(Self::TagColors)
            }
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::Choice(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_number(&self) -> Option<u64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

/// Every resolved setting a board or the app defaults show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KanbanEffective {
    pub dates: DateSettings,
    pub tags: TagSettings,
    pub metadata: MetadataSettings,
    pub archive: ArchiveSettings,
    pub board: KanbanBoardSettings,
    /// The app-wide `kanban_autosave` choice (not a footer key).
    pub autosave: bool,
}

impl KanbanEffective {
    /// Footer, then `kanban_defaults`, then the built-in value. `autosave`
    /// starts at its built-in and is set by the caller from `settings.json`.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        Self {
            dates: DateSettings::resolve(footer_json, defaults),
            tags: TagSettings::resolve(footer_json, defaults),
            metadata: MetadataSettings::resolve(footer_json, defaults),
            archive: ArchiveSettings::resolve(footer_json, defaults),
            board: KanbanBoardSettings::resolve(footer_json, defaults),
            autosave: true,
        }
    }

    /// The value of `key`, or `None` for an unknown key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<KanbanSettingValue> {
        super::schema::kanban_key(key).map(|key| (key.get)(self))
    }

    /// Set `key` in this resolved copy, reporting whether it applied: `false`
    /// for an unknown key, a value of the wrong kind for the key, or a value
    /// the key refuses (an unknown choice). A refused value changes nothing.
    pub fn set(&mut self, key: &str, value: &KanbanSettingValue) -> bool {
        let Some(entry) = super::schema::kanban_key(key) else {
            return false;
        };
        (entry.set)(self, value)
    }
}

/// One row of the schema.
pub struct KanbanKey {
    /// The footer key and the `kanban_defaults` key; see [`KANBAN_AUTOSAVE`]
    /// for the one app-only row.
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub section: KanbanSection,
    pub scope: KanbanScope,
    pub kind: KanbanSettingKind,
    pub builtin: KanbanBuiltin,
    pub get: fn(&KanbanEffective) -> KanbanSettingValue,
    /// Apply `value`; `false` when its kind is wrong for this key or the key
    /// refuses its content (an unknown choice).
    pub set: fn(&mut KanbanEffective, &KanbanSettingValue) -> bool,
}

/// The key row with this name, when the schema owns it.
#[must_use]
pub fn kanban_key(key: &str) -> Option<&'static KanbanKey> {
    KANBAN_KEYS.iter().find(|entry| entry.key == key)
}

impl KanbanKey {
    /// The footer key this setting writes, when the board can override it.
    /// An app-only row has none.
    #[must_use]
    pub fn footer_key(&self) -> Option<&'static str> {
        (self.scope == KanbanScope::BoardAndApp).then_some(self.key)
    }

    /// The `kanban_defaults` key this setting writes (the top-level
    /// `kanban_autosave` for the app-only row).
    #[must_use]
    pub fn app_default_key(&self) -> &'static str {
        self.key
    }
}

/// Every key in `section`, in drawn order.
pub fn kanban_keys_in(section: KanbanSection) -> impl Iterator<Item = &'static KanbanKey> {
    KANBAN_KEYS
        .iter()
        .filter(move |entry| entry.section == section)
}

macro_rules! get_bool {
    ($group:ident . $field:ident) => {
        |e: &KanbanEffective| KanbanSettingValue::Bool(e.$group.$field)
    };
}

macro_rules! set_bool {
    ($group:ident . $field:ident) => {
        |e: &mut KanbanEffective, v: &KanbanSettingValue| {
            if let KanbanSettingValue::Bool(value) = v {
                e.$group.$field = *value;
                true
            } else {
                false
            }
        }
    };
}

macro_rules! get_text {
    ($group:ident . $field:ident) => {
        |e: &KanbanEffective| KanbanSettingValue::Text(e.$group.$field.clone())
    };
}

macro_rules! set_text {
    ($group:ident . $field:ident) => {
        |e: &mut KanbanEffective, v: &KanbanSettingValue| {
            if let KanbanSettingValue::Text(text) = v {
                e.$group.$field.clone_from(text);
                true
            } else {
                false
            }
        }
    };
}

macro_rules! get_number {
    ($group:ident . $field:ident) => {
        |e: &KanbanEffective| KanbanSettingValue::Number(e.$group.$field)
    };
}

macro_rules! set_number {
    ($group:ident . $field:ident) => {
        |e: &mut KanbanEffective, v: &KanbanSettingValue| {
            if let KanbanSettingValue::Number(value) = v {
                e.$group.$field = *value;
                true
            } else {
                false
            }
        }
    };
}

macro_rules! get_choice {
    ($group:ident . $field:ident) => {
        |e: &KanbanEffective| KanbanSettingValue::Choice(e.$group.$field.as_str().to_owned())
    };
}

macro_rules! set_choice {
    ($group:ident . $field:ident, $ty:ty) => {
        |e: &mut KanbanEffective, v: &KanbanSettingValue| {
            if let KanbanSettingValue::Choice(text) = v
                && let Some(value) = <$ty>::parse(text)
            {
                e.$group.$field = value;
                true
            } else {
                false
            }
        }
    };
}

const NEW_LINE_CHOICES: &[KanbanChoice] = &[
    KanbanChoice {
        value: "enter",
        label: "Enter",
    },
    KanbanChoice {
        value: "shift+enter",
        label: "Shift+Enter",
    },
];

const ARCHIVE_POSITION_CHOICES: &[KanbanChoice] = &[
    KanbanChoice {
        value: "start",
        label: "Start",
    },
    KanbanChoice {
        value: "end",
        label: "End",
    },
];

const INSERTION_CHOICES: &[KanbanChoice] = &[
    KanbanChoice {
        value: "prepend",
        label: "Prepend",
    },
    KanbanChoice {
        value: "append",
        label: "Append",
    },
];

/// Every kanban setting, in the order the settings UIs draw them.
pub const KANBAN_KEYS: &[KanbanKey] = &[
    KanbanKey {
        key: DATE_TRIGGER,
        label: "Date trigger",
        help: "Typed before a date, as in @{2026-11-12}.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("@"),
        get: get_text!(dates.date_trigger),
        set: set_text!(dates.date_trigger),
    },
    KanbanKey {
        key: TIME_TRIGGER,
        label: "Time trigger",
        help: "Typed before a time, as in @@{14:30}.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("@@"),
        get: get_text!(dates.time_trigger),
        set: set_text!(dates.time_trigger),
    },
    KanbanKey {
        key: DATE_FORMAT,
        label: "Date format",
        help: "Written inside a new date token; YYYY, MM, DD, MMMM and Do are tokens.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("YYYY-MM-DD"),
        get: get_text!(dates.date_format),
        set: set_text!(dates.date_format),
    },
    KanbanKey {
        key: TIME_FORMAT,
        label: "Time format",
        help: "Written inside a new time token; HH, mm, h and A are tokens.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("HH:mm"),
        get: get_text!(dates.time_format),
        set: set_text!(dates.time_format),
    },
    KanbanKey {
        key: DATE_DISPLAY_FORMAT,
        label: "Date display format",
        help: "How a date is shown at the card's foot.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("YYYY-MM-DD"),
        get: get_text!(dates.date_display_format),
        set: set_text!(dates.date_display_format),
    },
    KanbanKey {
        key: SHOW_RELATIVE_DATE,
        label: "Show relative date",
        help: "Adds today or in N days beside the date.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(dates.show_relative_date),
        set: set_bool!(dates.show_relative_date),
    },
    KanbanKey {
        key: HIDE_DATE_IN_TITLE,
        label: "Hide date in title",
        help: "Keeps date and time tokens out of the card's main text.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: get_bool!(dates.hide_date_in_title),
        set: set_bool!(dates.hide_date_in_title),
    },
    KanbanKey {
        key: LINK_DATE_TO_DAILY_NOTE,
        label: "Link date to daily note",
        help: "Makes the shown date a link to its daily note.",
        section: KanbanSection::Dates,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(dates.link_date_to_daily_note),
        set: set_bool!(dates.link_date_to_daily_note),
    },
    KanbanKey {
        key: HIDE_TAGS_IN_TITLE,
        label: "Hide tags in title",
        help: "Keeps #tag tokens out of the card's main text.",
        section: KanbanSection::Tags,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(tags.hide_tags_in_title),
        set: set_bool!(tags.hide_tags_in_title),
    },
    KanbanKey {
        key: SHOW_TAGS_AT_BOTTOM,
        label: "Show tags at bottom",
        help: "Draws tag chips at the card's foot.",
        section: KanbanSection::Tags,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(tags.show_tags_at_bottom),
        set: set_bool!(tags.show_tags_at_bottom),
    },
    KanbanKey {
        key: TAG_COLORS,
        label: "Tag colours",
        help: "Kept as written. Muninn has no colour editor yet.",
        section: KanbanSection::Tags,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::TagColorList,
        builtin: KanbanBuiltin::Absent,
        get: |e| KanbanSettingValue::TagColors(e.tags.tag_colors.clone()),
        set: |e, v| {
            if let KanbanSettingValue::TagColors(colors) = v {
                e.tags.tag_colors.clone_from(colors);
                true
            } else {
                false
            }
        },
    },
    KanbanKey {
        key: LINKED_PAGE_METADATA,
        label: "Linked note metadata",
        help: "Keys shown at the card's foot from its first linked note.",
        section: KanbanSection::Metadata,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::MetadataList,
        builtin: KanbanBuiltin::Absent,
        get: |e| KanbanSettingValue::Metadata(e.metadata.linked_page_metadata.clone()),
        set: |e, v| {
            if let KanbanSettingValue::Metadata(fields) = v {
                e.metadata.linked_page_metadata.clone_from(fields);
                true
            } else {
                false
            }
        },
    },
    KanbanKey {
        key: SHOW_MISSING_METADATA,
        label: "Show missing metadata",
        help: "Draws key: — when the linked note or the key is missing.",
        section: KanbanSection::Metadata,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(metadata.show_missing_metadata),
        set: set_bool!(metadata.show_missing_metadata),
    },
    KanbanKey {
        key: ARCHIVE_WITH_DATE,
        label: "Archive with date",
        help: "Adds the date to a card's text when it is archived.",
        section: KanbanSection::Archive,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(false),
        get: get_bool!(archive.with_date),
        set: set_bool!(archive.with_date),
    },
    KanbanKey {
        key: ARCHIVE_DATE_FORMAT,
        label: "Archive date format",
        help: "The date format used when archiving; the same tokens as Date format.",
        section: KanbanSection::Archive,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text("YYYY-MM-DD"),
        get: get_text!(archive.date_format),
        set: set_text!(archive.date_format),
    },
    KanbanKey {
        key: ARCHIVE_DATE_POSITION,
        label: "Archive date position",
        help: "Start or end of the archived card's text.",
        section: KanbanSection::Archive,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Choice(ARCHIVE_POSITION_CHOICES),
        builtin: KanbanBuiltin::Choice("end"),
        get: |e| KanbanSettingValue::Choice(e.archive.date_position.as_str().to_owned()),
        set: |e, v| {
            if let KanbanSettingValue::Choice(text) = v
                && let Some(position) = super::ArchiveDatePosition::parse(text)
            {
                e.archive.date_position = position;
                true
            } else {
                false
            }
        },
    },
    KanbanKey {
        key: ARCHIVE_DATE_SEPARATOR,
        label: "Archive date separator",
        help: "Text between the card and its date; an empty value joins them.",
        section: KanbanSection::Archive,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text(" "),
        get: get_text!(archive.date_separator),
        set: set_text!(archive.date_separator),
    },
    KanbanKey {
        key: MAX_ARCHIVE_CARDS,
        label: "Max archive cards",
        help: "Oldest archive cards beyond this many go. 0 is unlimited.",
        section: KanbanSection::Archive,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Number(KanbanNumber {
            min: 0,
            max: 10_000,
            step: 1,
        }),
        builtin: KanbanBuiltin::Absent,
        get: |e| KanbanSettingValue::Number(e.archive.max_cards.map_or(0, |n| n as u64)),
        set: |e, v| {
            if let KanbanSettingValue::Number(value) = v {
                e.archive.max_cards = (*value > 0).then_some(*value as usize);
                true
            } else {
                false
            }
        },
    },
    KanbanKey {
        key: LANE_WIDTH,
        label: "Lane width",
        help: "Lane width in points; the board clamps what it draws to 120–720.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Number(KanbanNumber {
            min: MIN_LANE_WIDTH,
            max: MAX_LANE_WIDTH,
            step: 8,
        }),
        builtin: KanbanBuiltin::Number(DEFAULT_LANE_WIDTH),
        get: get_number!(board.lane_width),
        set: set_number!(board.lane_width),
    },
    KanbanKey {
        key: NEW_LINE_TRIGGER,
        label: "New line trigger",
        help: "In the card editor, Enter commits and Shift+Enter breaks the line, or the reverse.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Choice(NEW_LINE_CHOICES),
        builtin: KanbanBuiltin::Choice("enter"),
        get: get_choice!(board.new_line_trigger),
        set: set_choice!(board.new_line_trigger, super::NewLineTrigger),
    },
    KanbanKey {
        key: NEW_CARD_INSERTION,
        label: "New card position",
        help: "Where + Add card puts the new card.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Choice(INSERTION_CHOICES),
        builtin: KanbanBuiltin::Choice("append"),
        get: get_choice!(board.new_card_insertion),
        set: set_choice!(board.new_card_insertion, super::NewCardInsertion),
    },
    KanbanKey {
        key: SHOW_CHECKBOXES,
        label: "Show checkboxes",
        help: "Draws each card's done checkbox.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: get_bool!(board.show_checkboxes),
        set: set_bool!(board.show_checkboxes),
    },
    KanbanKey {
        key: SHOW_FILTER_BUTTON,
        label: "Filter button",
        help: "Shows the header's Filter button.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: get_bool!(board.show_filter_button),
        set: set_bool!(board.show_filter_button),
    },
    KanbanKey {
        key: SHOW_ARCHIVE_BUTTON,
        label: "Archive button",
        help: "Shows the header's Archive button.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: get_bool!(board.show_archive_button),
        set: set_bool!(board.show_archive_button),
    },
    KanbanKey {
        key: SHOW_OPEN_MARKDOWN_BUTTON,
        label: "Open as markdown button",
        help: "Shows the header's Open as markdown button.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: get_bool!(board.show_open_markdown_button),
        set: set_bool!(board.show_open_markdown_button),
    },
    KanbanKey {
        key: NEW_NOTE_FOLDER,
        label: "New note folder",
        help: "Folder for New note from card; empty uses the folder New note uses.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text(""),
        get: get_text!(board.new_note_folder),
        set: set_text!(board.new_note_folder),
    },
    KanbanKey {
        key: NEW_NOTE_TEMPLATE,
        label: "New note template",
        help: "A note path whose text starts the new note; empty uses the app note template.",
        section: KanbanSection::Board,
        scope: KanbanScope::BoardAndApp,
        kind: KanbanSettingKind::Text,
        builtin: KanbanBuiltin::Text(""),
        get: get_text!(board.new_note_template),
        set: set_text!(board.new_note_template),
    },
    KanbanKey {
        key: KANBAN_AUTOSAVE,
        label: "Autosave kanban edits",
        help: "Saves unsaved kanban edits once they rest. Off leaves saving to ⌘S.",
        section: KanbanSection::App,
        scope: KanbanScope::AppOnly,
        kind: KanbanSettingKind::Bool,
        builtin: KanbanBuiltin::Bool(true),
        get: |e| KanbanSettingValue::Bool(e.autosave),
        set: |e, v| {
            if let KanbanSettingValue::Bool(value) = v {
                e.autosave = *value;
                true
            } else {
                false
            }
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kanban;
    use serde_json::json;

    fn footer() -> String {
        json!({}).to_string()
    }

    #[test]
    fn keys_are_unique_and_have_one_section() {
        let mut seen = std::collections::HashSet::new();
        for entry in KANBAN_KEYS {
            assert!(seen.insert(entry.key), "duplicate key {}", entry.key);
            assert!(!entry.key.is_empty() && !entry.label.is_empty());
        }
        for section in KanbanSection::ALL {
            assert!(kanban_keys_in(section).count() > 0, "{section:?} is empty");
        }
        // Every board key's footer name is also its app-default name.
        for entry in KANBAN_KEYS {
            assert_eq!(
                entry.scope == KanbanScope::AppOnly,
                entry.key == KANBAN_AUTOSAVE
            );
            assert_eq!(entry.app_default_key(), entry.key);
            assert_eq!(
                entry.footer_key().is_some(),
                entry.scope == KanbanScope::BoardAndApp
            );
        }
    }

    #[test]
    fn every_builtin_resolves_to_a_typed_value() {
        let effective = KanbanEffective::resolve(Some(&footer()), &KanbanDefaults::default());
        for entry in KANBAN_KEYS {
            let value = (entry.get)(&effective);
            match entry.builtin {
                KanbanBuiltin::Absent => {
                    assert!(
                        matches!(
                            (&entry.kind, &value),
                            (KanbanSettingKind::MetadataList, KanbanSettingValue::Metadata(v)) if v.is_empty()
                        ) || matches!(
                            (&entry.kind, &value),
                            (KanbanSettingKind::TagColorList, KanbanSettingValue::TagColors(v)) if v.is_empty()
                        ) || matches!(
                            (&entry.kind, &value),
                            (KanbanSettingKind::Number(_), KanbanSettingValue::Number(0))
                        ),
                        "{} has no builtin but resolved to {value:?}",
                        entry.key
                    );
                }
                builtin => assert_eq!(
                    builtin.value().as_ref(),
                    Some(&value),
                    "{} resolves to its builtin",
                    entry.key
                ),
            }
        }
    }

    #[test]
    fn every_kind_round_trips_through_json() {
        let fields = vec![MetadataField {
            key: "status".to_owned(),
            contains_markdown: true,
        }];
        let colors = vec![TagColor {
            tag_key: "work".to_owned(),
            color: "1".to_owned(),
        }];
        for (kind, value) in [
            (KanbanSettingKind::Bool, KanbanSettingValue::Bool(true)),
            (
                KanbanSettingKind::Text,
                KanbanSettingValue::Text("DD.MM.YYYY".to_owned()),
            ),
            (
                KanbanSettingKind::Choice(NEW_LINE_CHOICES),
                KanbanSettingValue::Choice("shift+enter".to_owned()),
            ),
            (
                KanbanSettingKind::Number(KanbanNumber {
                    min: 0,
                    max: 9,
                    step: 1,
                }),
                KanbanSettingValue::Number(3),
            ),
            (
                KanbanSettingKind::MetadataList,
                KanbanSettingValue::Metadata(fields),
            ),
            (
                KanbanSettingKind::TagColorList,
                KanbanSettingValue::TagColors(colors),
            ),
        ] {
            let json = value.to_json();
            assert_eq!(KanbanSettingValue::from_json(kind, &json), Some(value));
        }
        assert_eq!(
            KanbanSettingValue::from_json(KanbanSettingKind::Bool, &json!("yes")),
            None
        );
        assert_eq!(
            KanbanSettingValue::from_json(
                KanbanSettingKind::Number(KanbanNumber {
                    min: 0,
                    max: 9,
                    step: 1
                }),
                &json!(-1)
            ),
            None
        );
    }

    #[test]
    fn the_schema_sets_a_resolved_copy_without_touching_other_keys() {
        let mut effective = KanbanEffective::resolve(Some(&footer()), &KanbanDefaults::default());
        assert!(effective.set(LANE_WIDTH, &KanbanSettingValue::Number(320)));
        assert_eq!(effective.board.lane_width, 320);
        assert!(!effective.set("not-a-kanban-key", &KanbanSettingValue::Bool(true)));
        assert!(
            !effective.set(LANE_WIDTH, &KanbanSettingValue::Bool(true)),
            "a value of the wrong kind is refused"
        );
        assert_eq!(
            effective.board.lane_width, 320,
            "a refused value changes nothing"
        );
        assert!(
            !effective.set(
                NEW_LINE_TRIGGER,
                &KanbanSettingValue::Choice("tab".to_owned())
            ),
            "a choice the key does not know is refused"
        );
        assert_eq!(
            effective.board.new_line_trigger,
            super::super::NewLineTrigger::Enter
        );
    }

    #[test]
    fn every_key_accepts_its_own_kind_and_refuses_another() {
        let mut effective = KanbanEffective::resolve(Some(&footer()), &KanbanDefaults::default());
        for entry in KANBAN_KEYS {
            let own = entry
                .builtin
                .value()
                .unwrap_or_else(|| empty_for_test(entry.kind));
            assert!(
                effective.set(entry.key, &own),
                "{} accepts a value of its kind",
                entry.key
            );
            let wrong = wrong_kind(entry.kind);
            assert!(
                !effective.set(entry.key, &wrong),
                "{} refuses {wrong:?}",
                entry.key
            );
            assert_eq!(
                effective.get(entry.key),
                Some(
                    entry
                        .builtin
                        .value()
                        .unwrap_or_else(|| empty_for_test(entry.kind))
                ),
                "{} keeps its value after the refusal",
                entry.key
            );
        }
    }

    #[test]
    fn app_default_then_board_override_then_reset() {
        let mut defaults = KanbanDefaults::default();
        assert!(defaults.set_key(SHOW_RELATIVE_DATE, Some(&json!(true))));
        let app = KanbanEffective::resolve(None, &defaults);
        assert_eq!(
            app.get(SHOW_RELATIVE_DATE),
            Some(KanbanSettingValue::Bool(true))
        );

        let mut board = Kanban::parse("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n")
            .expect("board");
        board
            .set_setting(SHOW_RELATIVE_DATE, &json!(false))
            .expect("set");
        let resolved = KanbanEffective::resolve(board.footer_json(), &defaults);
        assert_eq!(
            resolved.get(SHOW_RELATIVE_DATE),
            Some(KanbanSettingValue::Bool(false)),
            "the board wins"
        );
        board
            .remove_setting(SHOW_RELATIVE_DATE)
            .expect("remove footer");
        let resolved = KanbanEffective::resolve(board.footer_json(), &defaults);
        assert_eq!(
            resolved.get(SHOW_RELATIVE_DATE),
            Some(KanbanSettingValue::Bool(true)),
            "removing the footer key falls back to the app default"
        );
        assert!(defaults.set_key(SHOW_RELATIVE_DATE, None));
        assert_eq!(defaults.to_json().get(SHOW_RELATIVE_DATE), None);
        let resolved = KanbanEffective::resolve(board.footer_json(), &defaults);
        assert_eq!(
            resolved.get(SHOW_RELATIVE_DATE),
            Some(KanbanSettingValue::Bool(false)),
            "clearing the app default falls back to the builtin"
        );
    }

    #[test]
    fn absent_keys_and_wrong_types_fall_back_and_unknown_keys_keep_their_bytes() {
        let source = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n\
                      %% kanban:settings\n```\n\
                      {\"kanban-plugin\":\"board\",\"lane-width\":\"wide\",\"unknown\":{\"x\":1}}\n\
                      ```\n%%\n";
        let mut board = Kanban::parse(source).expect("board");
        assert_eq!(
            KanbanEffective::resolve(board.footer_json(), &KanbanDefaults::default())
                .get(LANE_WIDTH),
            Some(KanbanSettingValue::Number(272)),
            "a wrong-typed footer value falls back to the builtin"
        );
        board
            .set_setting(NEW_LINE_TRIGGER, &json!("shift+enter"))
            .expect("set");
        let footer = board.footer_json().expect("footer");
        assert!(footer.contains("\"lane-width\":\"wide\""), "{footer}");
        assert!(footer.contains("\"unknown\":{\"x\":1}"), "{footer}");
        assert!(
            footer.contains("\"new-line-trigger\":\"shift+enter\""),
            "{footer}"
        );

        // An app default with the wrong type is kept until the panel clears it.
        let mut defaults = KanbanDefaults::from_json(Some(&json!({"lane-width": "wide"})));
        assert_eq!(defaults.to_json()["lane-width"], "wide");
        assert!(defaults.set_key(LANE_WIDTH, None));
        assert_eq!(defaults.to_json().get(LANE_WIDTH), None);
        assert!(
            !defaults.set_key("unknown", None),
            "unknown keys are not ours"
        );
    }

    #[test]
    fn app_defaults_set_every_schema_key_and_clear_again() {
        let mut defaults = KanbanDefaults::default();
        for entry in KANBAN_KEYS {
            if entry.scope == KanbanScope::AppOnly {
                continue;
            }
            let value = entry
                .builtin
                .value()
                .unwrap_or_else(|| empty_for_test(entry.kind));
            let json = value.to_json();
            assert!(
                defaults.set_key(entry.key, Some(&json)),
                "{} is settable",
                entry.key
            );
            assert!(
                KanbanSettingValue::from_json(entry.kind, &defaults.to_json()[entry.key]).is_some(),
                "{} writes a value of its kind",
                entry.key
            );
        }
        assert!(!defaults.is_empty());
        for entry in KANBAN_KEYS {
            if entry.scope == KanbanScope::AppOnly {
                continue;
            }
            assert!(defaults.set_key(entry.key, None));
        }
        assert!(
            defaults.is_empty(),
            "every key clears and the object is absent"
        );
    }

    /// A resolved value of the key's kind with nothing set: the shape an
    /// absent built-in takes.
    fn empty_for_test(kind: KanbanSettingKind) -> KanbanSettingValue {
        match kind {
            KanbanSettingKind::Bool => KanbanSettingValue::Bool(false),
            KanbanSettingKind::Number(_) => KanbanSettingValue::Number(0),
            KanbanSettingKind::MetadataList => KanbanSettingValue::Metadata(Vec::new()),
            KanbanSettingKind::TagColorList => KanbanSettingValue::TagColors(Vec::new()),
            KanbanSettingKind::Text | KanbanSettingKind::Choice(_) => {
                KanbanSettingValue::Text(String::new())
            }
        }
    }

    /// A value of some other kind than `kind`.
    fn wrong_kind(kind: KanbanSettingKind) -> KanbanSettingValue {
        match kind {
            KanbanSettingKind::Bool => KanbanSettingValue::Text("true".to_owned()),
            _ => KanbanSettingValue::Bool(true),
        }
    }
}
