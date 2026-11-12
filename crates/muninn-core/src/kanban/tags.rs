//! Card tag display settings (slice 10).
//!
//! `docs/kanban-format.md` section 9 names the keys. Like the date settings,
//! a value resolves from the board's footer, then the app-wide
//! `kanban_defaults`, then the built-in default. A display setting never
//! rewrites a board.

use serde_json::{Map, Value};

use super::dates::KanbanDefaults;

/// The footer key: keep `#tag` tokens out of the card's main text.
pub const HIDE_TAGS_IN_TITLE: &str = "hide-tags-in-title";
/// The footer key: draw tag chips at the card's foot.
pub const SHOW_TAGS_AT_BOTTOM: &str = "show-tags-at-bottom";
/// The footer key: a tag's chip colour, as `{tagKey, color}` objects.
pub const TAG_COLORS: &str = "tag-colors";

/// One `tag-colors` entry. Both fields are kept as written; Muninn has no
/// colour editor yet and does not translate the value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TagColor {
    pub tag_key: String,
    pub color: String,
}

/// The tag settings a board shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TagSettings {
    pub hide_tags_in_title: bool,
    pub show_tags_at_bottom: bool,
    /// In written order. A malformed entry is ignored; the rest stay.
    pub tag_colors: Vec<TagColor>,
}

impl TagSettings {
    /// Footer, then app defaults, then the built-in value.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        let mut settings = defaults.to_tag_settings();
        if let Some(value) = footer_json.and_then(|json| serde_json::from_str::<Value>(json).ok()) {
            settings.apply_value(&value);
        }
        settings
    }

    /// Read the keys this type owns from one JSON object. Other keys, and a
    /// value of the wrong type, are ignored; the current value stays.
    pub fn apply_value(&mut self, value: &Value) {
        if let Some(flag) = value.get(HIDE_TAGS_IN_TITLE).and_then(Value::as_bool) {
            self.hide_tags_in_title = flag;
        }
        if let Some(flag) = value.get(SHOW_TAGS_AT_BOTTOM).and_then(Value::as_bool) {
            self.show_tags_at_bottom = flag;
        }
        if let Some(colors) = tag_colors(value.get(TAG_COLORS)) {
            self.tag_colors = colors;
        }
    }
}

/// Read a `tag-colors` value: an array of objects with a string `tagKey` and
/// a string `color`. Unknown fields and malformed entries are ignored.
#[must_use]
pub fn tag_colors(value: Option<&Value>) -> Option<Vec<TagColor>> {
    let value = value?;
    let items = value.as_array()?;
    let mut colors = Vec::with_capacity(items.len());
    for item in items {
        let Some(tag_key) = item.get("tagKey").and_then(Value::as_str) else {
            continue;
        };
        let Some(color) = item.get("color").and_then(Value::as_str) else {
            continue;
        };
        colors.push(TagColor {
            tag_key: tag_key.to_owned(),
            color: color.to_owned(),
        });
    }
    Some(colors)
}

/// The `tag-colors` value as written: an array of `{tagKey, color}` objects.
#[must_use]
pub fn tag_colors_json(colors: &[TagColor]) -> Value {
    Value::Array(
        colors
            .iter()
            .map(|entry| {
                let mut object = Map::new();
                object.insert("tagKey".to_owned(), Value::String(entry.tag_key.clone()));
                object.insert("color".to_owned(), Value::String(entry.color.clone()));
                Value::Object(object)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_built_in_and_footer_then_app_defaults_win() {
        let builtin = TagSettings::default();
        assert!(!builtin.hide_tags_in_title);
        assert!(!builtin.show_tags_at_bottom);

        let app = KanbanDefaults {
            hide_tags_in_title: Some(true),
            show_tags_at_bottom: Some(true),
            tag_colors: Some(vec![TagColor {
                tag_key: "work".to_owned(),
                color: "1".to_owned(),
            }]),
            ..KanbanDefaults::default()
        };
        let resolved = TagSettings::resolve(None, &app);
        assert!(resolved.hide_tags_in_title);
        assert!(resolved.show_tags_at_bottom);
        assert_eq!(resolved.tag_colors[0].tag_key, "work");

        let footer = r#"{"kanban-plugin":"board","hide-tags-in-title":false,"tag-colors":[{"tagKey":"ops","color":"3"}]}"#;
        let resolved = TagSettings::resolve(Some(footer), &app);
        assert!(!resolved.hide_tags_in_title, "the footer wins");
        assert!(
            resolved.show_tags_at_bottom,
            "an absent footer key keeps the app default"
        );
        assert_eq!(
            resolved.tag_colors,
            vec![TagColor {
                tag_key: "ops".to_owned(),
                color: "3".to_owned()
            }]
        );

        assert_eq!(
            TagSettings::resolve(Some("{not json"), &app),
            TagSettings::resolve(None, &app)
        );
        let wrong = r#"{"hide-tags-in-title":"yes","show-tags-at-bottom":1}"#;
        assert_eq!(
            TagSettings::resolve(Some(wrong), &app),
            TagSettings::resolve(None, &app)
        );
    }

    #[test]
    fn colors_round_trip_and_malformed_entries_are_skipped() {
        let value: Value = serde_json::from_str(
            r#"[{"tagKey":"a","color":"1"},{"tagKey":2,"color":"2"},{"tagKey":"b","color":"4","extra":true}]"#,
        )
        .expect("json");
        let colors = tag_colors(Some(&value)).expect("colors");
        assert_eq!(
            colors,
            [
                TagColor {
                    tag_key: "a".to_owned(),
                    color: "1".to_owned()
                },
                TagColor {
                    tag_key: "b".to_owned(),
                    color: "4".to_owned()
                }
            ]
        );
        assert_eq!(
            tag_colors_json(&colors),
            serde_json::json!([{"tagKey":"a","color":"1"},{"tagKey":"b","color":"4"}])
        );
    }
}
