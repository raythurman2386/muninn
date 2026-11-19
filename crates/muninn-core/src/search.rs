//! Title and body search over a vault.

use crate::vault::Vault;

impl Vault {
    /// Notes matching `query` by title or body, alphabetically by title with
    /// a deterministic `rel` tie-break for duplicate titles (the sidebar
    /// re-groups these by folder). An empty query matches all.
    ///
    /// A locked note's title is always matched, but its body only when the
    /// session overlay makes it readable ([`crate::Note::readable_source`]).
    /// The envelope is never searched, so a query cannot hit base64 noise.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        let mut ids: Vec<usize> = self
            .notes
            .iter()
            .filter(|n| {
                q.is_empty()
                    || n.title.to_lowercase().contains(&q)
                    || n.readable_source()
                        .is_some_and(|source| source.to_lowercase().contains(&q))
            })
            .map(|n| n.id)
            .collect();
        ids.sort_by(|&a, &b| {
            let (na, nb) = (&self.notes[a], &self.notes[b]);
            (!na.title.to_lowercase().contains(&q))
                .cmp(&!nb.title.to_lowercase().contains(&q))
                .then_with(|| na.title.to_lowercase().cmp(&nb.title.to_lowercase()))
                .then_with(|| na.rel.to_lowercase().cmp(&nb.rel.to_lowercase()))
                .then_with(|| na.rel.cmp(&nb.rel))
        });
        ids
    }
}

#[cfg(test)]
mod tests {
    use crate::note_lock;
    use crate::testing::TempVault;

    #[test]
    fn search_and_snippets_find_notes() {
        let tmp = TempVault::new(&[
            (
                "One.md",
                "the orchard is quiet\n\nsee [[Two|second]] for more",
            ),
            ("Two.md", "nothing here"),
            ("Three.md", "an orchard too"),
        ]);
        let v = tmp.load();
        let found = v.search("orchard");
        let titles: Vec<&str> = found.iter().map(|&id| v.note(id).title.as_str()).collect();
        assert_eq!(titles, ["One", "Three"]);
        let one = v.resolve("One").unwrap();
        let two = v.resolve("Two").unwrap();
        assert!(
            v.snippet(one, two).contains("see second"),
            "{}",
            v.snippet(one, two)
        );
    }

    #[test]
    fn a_locked_note_is_searched_by_title_and_not_by_body() {
        let body = "# Secret\n\nTOP SECRET BODY\n";
        let envelope = note_lock::lock(body, "pw", "Secret").expect("lock");
        let tmp = TempVault::new(&[("Secret.md", envelope.as_str())]);
        let v = tmp.load();
        let secret = v.resolve("Secret").expect("Secret");

        assert_eq!(v.search("secret"), vec![secret], "the title matches");
        assert!(
            v.search("top secret").is_empty(),
            "the body is an envelope, not searchable text"
        );
        assert_eq!(v.snippet(secret, secret), "", "no excerpt from ciphertext");
    }
}
