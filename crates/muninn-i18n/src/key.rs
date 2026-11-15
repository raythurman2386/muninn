//! The catalog key type.
//!
//! `keys!` is a local declarative macro rather than a proc macro: the key set
//! is small, and a macro keeps `Key`, `Key::ALL` and `Key::id` in one place
//! with no build-time dependency. Adding a key here without English text in
//! `en_us` fails `cargo check`, because that match is exhaustive.

/// Declares [`Key`], its [`Key::ALL`] slice and its stable ids.
///
/// One row per user-visible string: `Variant => "dotted.id"`. The id is
/// deliberately not the English text, so a copy edit never changes it.
macro_rules! keys {
    ($($variant:ident => $id:literal,)*) => {
        /// The identity of one piece of user-visible copy.
        ///
        /// Call sites name a key; [`crate::t`] resolves it to text in the
        /// current language. A key never carries the text itself, so the same
        /// call site can render any language.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Key {
            $(
                /// See [`Key::id`] for the stable id and [`crate::t`] for the text.
                $variant,
            )*
        }

        impl Key {
            /// Every key in the catalog, in declaration order.
            ///
            /// Tests use this to check for duplicate ids and missing text
            /// without naming each key.
            pub const ALL: &'static [Key] = &[
                $(Key::$variant,)*
            ];

            /// The stable dotted id, for debugging and future catalogs.
            ///
            /// Use this in log lines and test failure messages; use
            /// [`crate::t`] for anything a user reads. The id is part of the
            /// catalog's contract: renaming one is a catalog change, not a
            /// copy edit.
            pub const fn id(self) -> &'static str {
                match self {
                    $(Key::$variant => $id,)*
                }
            }
        }
    };
}

keys! {
    AddCard => "kanban.add_card",
    Archive => "kanban.archive",
    Today => "date.today",
    OverLimit => "kanban.over_limit",
    KeyboardShortcuts => "help.keyboard_shortcuts",
    Close => "common.close",
    Save => "common.save",
    Cancel => "common.cancel",
    LinkedNote => "link.linked_note",
    NoBoards => "board.no_boards",
}
