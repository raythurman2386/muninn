"""Unit tests for the offline i18n review triage."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from review import Row, find_issue  # noqa: E402


def row(kind: str, ident: str, english: str, target: str) -> Row:
    return Row(kind, ident, english, target)


class ReviewTests(unittest.TestCase):
    def test_clean_row_has_no_finding(self) -> None:
        self.assertIsNone(find_issue(row("key", "common.save", "Save", "Guardar"), "es"))

    def test_missing_placeholder_is_critical(self) -> None:
        finding = find_issue(
            row("msg", "x", "1 \x010\x01", "1 nota"),
            "es",
        )
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding[0], "critical")
        self.assertIn("placeholder", finding[1])

    def test_localized_file_token_is_critical(self) -> None:
        finding = find_issue(
            row(
                "key",
                "kanban.filter.date_error",
                "A date filter is YYYY-MM-DD.",
                "Un filtro usa AAAA-MM-DD.",
            ),
            "es",
        )
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding[0], "critical")
        self.assertIn("YYYY", finding[1])

    def test_french_ascii_apostrophe_is_critical(self) -> None:
        finding = find_issue(
            row("key", "x", "Save", "Enregistrer l'application"),
            "fr",
        )
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding[0], "critical")
        self.assertIn("apostrophe", finding[1])

    def test_pt_pt_vocabulary_is_critical(self) -> None:
        finding = find_issue(row("key", "x", "Save", "Guardar ficheiro"), "pt")
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding[0], "critical")
        self.assertIn("pt-PT", finding[1])

    def test_enter_as_a_verb_is_not_a_keycap_finding(self) -> None:
        self.assertIsNone(
            find_issue(row("msg", "x", "Enter the path", "Introduce la ruta"), "es")
        )

    def test_localized_quotes_and_parentheses_are_not_findings(self) -> None:
        self.assertIsNone(
            find_issue(
                row("msg", "x", "A board named “{0}”", "Un tablero llamado «{0}»"),
                "es",
            )
        )

    def test_english_function_word_leak_is_a_warning(self) -> None:
        finding = find_issue(row("key", "x", "Save", "Save the file"), "es")
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding[0], "warning")
        self.assertIn("English function word", finding[1])


if __name__ == "__main__":
    unittest.main()
