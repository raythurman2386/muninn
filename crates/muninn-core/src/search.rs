//! Title and body search over a vault.

use crate::vault::Vault;

impl Vault {
    /// Notes matching `query` by title or body, alphabetically. An empty query matches all.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        let mut ids: Vec<usize> = self
            .notes
            .iter()
            .filter(|n| {
                q.is_empty()
                    || n.title.to_lowercase().contains(&q)
                    || n.source.to_lowercase().contains(&q)
            })
            .map(|n| n.id)
            .collect();
        ids.sort_by_key(|&id| {
            let note = &self.notes[id];
            (
                !note.title.to_lowercase().contains(&q),
                note.title.to_lowercase(),
            )
        });
        ids
    }
}

#[cfg(test)]
mod tests {
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
}
