//! Board display and behaviour settings (slice 14).
//!
//! These keys live in the board's settings footer, in the app-wide
//! `kanban_defaults` object of `settings.json`, or both. A board's value wins,
//! then the app default, then the built-in value
//! (`docs/kanban-format.md` section 9).
//!
//! None of these settings is written by viewing a board. The settings panel
//! writes one footer key per change, or one `kanban_defaults` key.

use serde_json::Value;

use super::dates::KanbanDefaults;

/// The footer key: a lane's width in points.
pub const LANE_WIDTH: &str = "lane-width";
/// The footer key: what a new card's `Enter` does in the card editor.
pub const NEW_LINE_TRIGGER: &str = "new-line-trigger";
/// The footer key: where **+ Add card** puts the new card.
pub const NEW_CARD_INSERTION: &str = "new-card-insertion-method";
/// The footer key: draw each card's done checkbox.
pub const SHOW_CHECKBOXES: &str = "show-checkboxes";
/// The footer key: show the header's Filter button.
pub const SHOW_FILTER_BUTTON: &str = "show-filter-button";
/// The footer key: show the header's Archive button.
pub const SHOW_ARCHIVE_BUTTON: &str = "show-archive-button";
/// The footer key: show the header's Open as markdown button.
pub const SHOW_OPEN_MARKDOWN_BUTTON: &str = "show-open-markdown-button";
/// The footer key: the folder **New note from card** writes into.
pub const NEW_NOTE_FOLDER: &str = "new-note-folder";
/// The footer key: a note whose text is the template for that new note.
pub const NEW_NOTE_TEMPLATE: &str = "new-note-template";

/// The built-in lane width, in points (the notes sidebar's width).
pub const DEFAULT_LANE_WIDTH: u64 = 272;
/// The narrowest lane the settings stepper writes.
pub const MIN_LANE_WIDTH: u64 = 120;
/// The widest lane the settings stepper writes.
pub const MAX_LANE_WIDTH: u64 = 720;

/// What a plain `Enter` does in the card editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NewLineTrigger {
    /// `Enter` inserts a line break; `Shift+Enter` commits the card.
    ShiftEnter,
    /// `Enter` commits the card; `Shift+Enter` inserts a line break (the
    /// built-in, and what the editor has always done).
    #[default]
    Enter,
}

impl NewLineTrigger {
    /// The two written values.
    pub const ALL: [NewLineTrigger; 2] = [NewLineTrigger::Enter, NewLineTrigger::ShiftEnter];

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().eq_ignore_ascii_case("enter") {
            Some(Self::Enter)
        } else if text.trim().eq_ignore_ascii_case("shift+enter") {
            Some(Self::ShiftEnter)
        } else {
            None
        }
    }

    /// The value written to the footer.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::ShiftEnter => "shift+enter",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::ShiftEnter => "Shift+Enter",
        }
    }
}

/// Where **+ Add card** puts a new card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NewCardInsertion {
    /// At the lane's top.
    Prepend,
    /// At the lane's foot (the built-in).
    #[default]
    Append,
}

impl NewCardInsertion {
    /// The two written values.
    pub const ALL: [NewCardInsertion; 2] = [NewCardInsertion::Prepend, NewCardInsertion::Append];

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().eq_ignore_ascii_case("prepend") {
            Some(Self::Prepend)
        } else if text.trim().eq_ignore_ascii_case("append") {
            Some(Self::Append)
        } else {
            None
        }
    }

    /// The value written to the footer.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepend => "prepend",
            Self::Append => "append",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Prepend => "Prepend",
            Self::Append => "Append",
        }
    }
}

/// The board display and behaviour settings a board resolves.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KanbanBoardSettings {
    /// Lane width in points, as written; the view clamps it when drawing.
    pub lane_width: u64,
    pub new_line_trigger: NewLineTrigger,
    pub new_card_insertion: NewCardInsertion,
    pub show_checkboxes: bool,
    pub show_filter_button: bool,
    pub show_archive_button: bool,
    pub show_open_markdown_button: bool,
    /// A vault-relative folder, or empty for the folder New note uses.
    pub new_note_folder: String,
    /// A vault-relative note path whose text starts the new note, or empty.
    pub new_note_template: String,
}

impl Default for KanbanBoardSettings {
    fn default() -> Self {
        Self {
            lane_width: DEFAULT_LANE_WIDTH,
            new_line_trigger: NewLineTrigger::Enter,
            new_card_insertion: NewCardInsertion::Append,
            show_checkboxes: true,
            show_filter_button: true,
            show_archive_button: true,
            show_open_markdown_button: true,
            new_note_folder: String::new(),
            new_note_template: String::new(),
        }
    }
}

impl KanbanBoardSettings {
    /// Footer, then app defaults, then the built-in value.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        let mut settings = defaults.to_board_settings();
        if let Some(value) = footer_json.and_then(|json| serde_json::from_str::<Value>(json).ok()) {
            settings.apply_value(&value);
        }
        settings
    }

    /// Read the keys this type owns from one JSON object. Other keys, and a
    /// value of the wrong type, are ignored; the current value stays. An
    /// unknown trigger or insertion value keeps the current one.
    pub fn apply_value(&mut self, value: &Value) {
        if let Some(width) = value.get(LANE_WIDTH).and_then(Value::as_u64) {
            self.lane_width = width;
        }
        if let Some(trigger) = value
            .get(NEW_LINE_TRIGGER)
            .and_then(Value::as_str)
            .and_then(NewLineTrigger::parse)
        {
            self.new_line_trigger = trigger;
        }
        if let Some(insertion) = value
            .get(NEW_CARD_INSERTION)
            .and_then(Value::as_str)
            .and_then(NewCardInsertion::parse)
        {
            self.new_card_insertion = insertion;
        }
        for (key, field) in [
            (SHOW_CHECKBOXES, &mut self.show_checkboxes),
            (SHOW_FILTER_BUTTON, &mut self.show_filter_button),
            (SHOW_ARCHIVE_BUTTON, &mut self.show_archive_button),
            (
                SHOW_OPEN_MARKDOWN_BUTTON,
                &mut self.show_open_markdown_button,
            ),
        ] {
            if let Some(flag) = value.get(key).and_then(Value::as_bool) {
                *field = flag;
            }
        }
        if let Some(folder) = value.get(NEW_NOTE_FOLDER).and_then(Value::as_str) {
            self.new_note_folder = folder.to_owned();
        }
        if let Some(template) = value.get(NEW_NOTE_TEMPLATE).and_then(Value::as_str) {
            self.new_note_template = template.to_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtins_are_the_documented_ones() {
        let settings = KanbanBoardSettings::default();
        assert_eq!(settings.lane_width, 272);
        assert_eq!(settings.new_line_trigger, NewLineTrigger::Enter);
        assert_eq!(settings.new_card_insertion, NewCardInsertion::Append);
        assert!(settings.show_checkboxes);
        assert!(settings.show_filter_button && settings.show_archive_button);
        assert!(settings.show_open_markdown_button);
        assert!(settings.new_note_folder.is_empty() && settings.new_note_template.is_empty());
    }

    #[test]
    fn footer_then_app_defaults_then_builtin() {
        let app = KanbanDefaults {
            lane_width: Some(320),
            new_line_trigger: Some("shift+enter".to_owned()),
            new_card_insertion: Some("prepend".to_owned()),
            show_checkboxes: Some(false),
            show_filter_button: Some(false),
            new_note_folder: Some("Notes".to_owned()),
            ..KanbanDefaults::default()
        };
        let resolved = KanbanBoardSettings::resolve(None, &app);
        assert_eq!(resolved.lane_width, 320);
        assert_eq!(resolved.new_line_trigger, NewLineTrigger::ShiftEnter);
        assert_eq!(resolved.new_card_insertion, NewCardInsertion::Prepend);
        assert!(!resolved.show_checkboxes && !resolved.show_filter_button);
        assert!(
            resolved.show_archive_button,
            "an absent key keeps the builtin"
        );
        assert_eq!(resolved.new_note_folder, "Notes");

        let footer =
            r#"{"lane-width":400,"new-line-trigger":"enter","show-checkboxes":true,"unknown":1}"#;
        let resolved = KanbanBoardSettings::resolve(Some(footer), &app);
        assert_eq!(resolved.lane_width, 400, "the footer wins");
        assert_eq!(resolved.new_line_trigger, NewLineTrigger::Enter);
        assert!(resolved.show_checkboxes);
        assert_eq!(resolved.new_card_insertion, NewCardInsertion::Prepend);
        assert_eq!(resolved.new_note_folder, "Notes");
    }

    #[test]
    fn wrong_types_and_unknown_choices_are_ignored() {
        let footer = json!({
            "lane-width": "wide",
            "new-line-trigger": "control+enter",
            "new-card-insertion-method": 2,
            "show-checkboxes": "yes",
            "show-archive-button": null,
        });
        let resolved =
            KanbanBoardSettings::resolve(Some(&footer.to_string()), &KanbanDefaults::default());
        assert_eq!(resolved, KanbanBoardSettings::default());
    }

    #[test]
    fn trigger_and_insertion_values_round_trip() {
        for trigger in NewLineTrigger::ALL {
            assert_eq!(NewLineTrigger::parse(trigger.as_str()), Some(trigger));
        }
        for insertion in NewCardInsertion::ALL {
            assert_eq!(NewCardInsertion::parse(insertion.as_str()), Some(insertion));
        }
        assert_eq!(
            NewLineTrigger::parse(" Enter "),
            Some(NewLineTrigger::Enter)
        );
        assert_eq!(
            NewLineTrigger::parse("Shift+Enter"),
            Some(NewLineTrigger::ShiftEnter)
        );
        assert_eq!(NewLineTrigger::parse("other"), None);
        assert_eq!(NewCardInsertion::parse("other"), None);
    }
}
