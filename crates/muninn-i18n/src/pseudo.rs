//! The test-only pseudo-localisation catalog.
//!
//! This is not a translation. Every key renders its English text wrapped in
//! [`PSEUDO_OPEN`] and [`PSEUDO_CLOSE`], so a render check can assert that a
//! drawn string came through the catalog and not from a literal assembled at
//! draw time. The markers are deliberately unmistakable: no shipped string
//! starts with `⟦`.
//!
//! The catalog is built once per process, from [`Key::ALL`] in declaration
//! order, so a render loop does no allocation and adds no entry per frame. The
//! strings are leaked on purpose: they live for the process, exactly like the
//! `&'static str` an `en-US` lookup returns, and there are as many as there are
//! keys.

use std::sync::OnceLock;

use crate::{Key, PSEUDO_CLOSE, PSEUDO_OPEN, en_us};

/// The pseudo text for `key`.
///
/// Indexed by [`Key::index`] into a catalog laid out in [`Key::ALL`] order, so
/// the lookup is a slice index rather than a scan or a hash.
pub(crate) fn text(key: Key) -> &'static str {
    catalog()[key.index()]
}

/// The whole pseudo catalog, built on first use.
fn catalog() -> &'static [&'static str] {
    static PSEUDO: OnceLock<Vec<&'static str>> = OnceLock::new();
    PSEUDO.get_or_init(|| {
        Key::ALL
            .iter()
            .map(|&key| bracketed(en_us::text(key)))
            .collect()
    })
}

/// `text` wrapped in the pseudo markers, leaked for the process lifetime.
fn bracketed(text: &str) -> &'static str {
    Box::leak(format!("{PSEUDO_OPEN}{text}{PSEUDO_CLOSE}").into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lang, t_in};

    #[test]
    fn every_key_is_bracketed() {
        for &key in Key::ALL {
            let text = t_in(Lang::Pseudo, key);
            assert_eq!(
                text.chars().next(),
                Some(PSEUDO_OPEN),
                "{} is not bracketed: {text:?}",
                key.id()
            );
            assert_eq!(
                text.chars().last(),
                Some(PSEUDO_CLOSE),
                "{} is not bracketed: {text:?}",
                key.id()
            );
            assert_eq!(
                text.trim_matches(|c| c == PSEUDO_OPEN || c == PSEUDO_CLOSE),
                en_us::text(key),
                "{} lost its English text",
                key.id()
            );
        }
    }

    #[test]
    fn the_mapping_is_shared_between_calls() {
        let first = t_in(Lang::Pseudo, Key::Close);
        let second = t_in(Lang::Pseudo, Key::Close);
        assert_eq!(first.as_ptr(), second.as_ptr(), "the catalog was rebuilt");
    }

    #[test]
    fn index_matches_the_all_order() {
        for (position, &key) in Key::ALL.iter().enumerate() {
            assert_eq!(key.index(), position, "{} is out of order", key.id());
        }
    }
}
