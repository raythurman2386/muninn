#!/usr/bin/env python3
"""Offline triage for a Muninn language pack.

Reads the TSV from `catalog_dump.rs` and writes a Markdown report. It is a
review aid, not a correctness proof: it checks placeholders, invariant tokens,
numbers, punctuation, glossary terms, length, and a few language-specific
typography/vocabulary rules. Semantic quality still needs a bilingual reviewer
(human or LLM).

No network access and no third-party packages: this is deterministic
maintainer tooling, safe to run on a checkout or in CI.

Usage: review.py <tsv> <lang> <out.md> [summary.md]
"""

from __future__ import annotations

import re
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


SENTINEL = re.compile(r"\x01(\d)\x01")
FRENCH_NBSP = "\u00a0"


def unescape(value: str) -> str:
    return (
        value.replace("\\\\", "\x00")
        .replace("\\t", "\t")
        .replace("\\n", "\n")
        .replace("\x00", "\\")
    )


def show(value: str) -> str:
    return SENTINEL.sub(lambda m: "{" + m.group(1) + "}", value)


def last_char(value: str) -> str:
    stripped = value.rstrip()
    return stripped[-1] if stripped else ""


def tokens(text: str) -> set[str]:
    patterns = [
        r"⌘[A-Za-z0-9]?",
        r"⇧[A-Za-z0-9]?",
        r"⌃[A-Za-z0-9]?",
        r"⌥[A-Za-z0-9]?",
        r"\b(?:Ctrl|Shift|Alt)\b",
        r"\bF\d{1,2}\b",
        r"@\{[^}]+\}",
        r"@@\{[^}]+\}",
        r"\b(?:YYYY|MMMM|MMM|MM|DD|HH|mm|Do)\b",
        r"\bM-D\b",
        r"\b127\.0\.0\.1\b",
        r"\bMUNINN_[A-Z0-9_]+\b",
        r"\bxdg-[a-z-]+\b",
        r"#[A-Za-z0-9_-]+",
        r"\b(?:R|D|O|A|L|P)\b(?=(?:\s+[A-Z]\b){2})",
    ]
    found: set[str] = set()
    for pattern in patterns:
        found.update(re.findall(pattern, text))
    return found


NUMBERS = re.compile(r"\d+(?:[.,]\d+)?")

GLOSSARY = {
    "es": {
        "note": ["nota"],
        "board": ["tablero"],
        "card": ["tarjeta"],
        "vault": ["bóveda", "boveda"],
        "folder": ["carpeta"],
        "tag": ["etiqueta"],
        "link": ["enlace"],
        "trash": ["papelera"],
        "settings": ["ajuste", "configuraci"],
        "sync": ["sincroniz"],
        "template": ["plantilla"],
        "archive": ["archiv"],
        "shortcut": ["atajo"],
        "graph": ["gráfic", "grafic"],
        "tray": ["bandeja"],
        "recovery": ["recuperaci"],
        "conflict": ["conflicto"],
        "scan": ["escane"],
        "index": ["índice", "indice"],
    },
    "fr": {
        "note": ["note"],
        "board": ["tableau"],
        "card": ["carte"],
        "vault": ["coffre"],
        "folder": ["dossier"],
        "tag": ["étiquette", "etiquette"],
        "link": ["lien"],
        "trash": ["corbeille"],
        "settings": ["paramètre", "parametre", "réglage", "reglage"],
        "sync": ["synchronis"],
        "template": ["modèle", "modele"],
        "archive": ["archiv"],
        "shortcut": ["raccourci"],
        "graph": ["graphique"],
        "tray": ["notification", "zone de notification"],
        "recovery": ["récupération", "recuperation"],
        "conflict": ["conflit"],
        "scan": ["analys"],
        "index": ["index"],
    },
    "de": {
        "note": ["notiz"],
        "board": ["tafel"],
        "card": ["karte"],
        "vault": ["tresor"],
        "folder": ["ordner"],
        "tag": ["etikett"],
        "link": ["link"],
        "trash": ["papierkorb"],
        "settings": ["einstellung"],
        "sync": ["synchronis"],
        "template": ["vorlage"],
        "archive": ["archiv"],
        "shortcut": ["tastenk", "kürzel", "kurzel"],
        "graph": ["graph"],
        "tray": ["infobereich", "benachrichtigungs"],
        "recovery": ["wiederherstell"],
        "conflict": ["konflikt"],
        "scan": ["scan", "prüf", "pruf"],
        "index": ["index"],
    },
    "pt": {
        "note": ["nota"],
        "board": ["quadro"],
        "card": ["cartão", "cartao", "cartões", "cartoes"],
        "vault": ["cofre"],
        "folder": ["pasta"],
        "tag": ["etiqueta"],
        "link": ["link", "vínculo", "vinculo"],
        "trash": ["lixeira"],
        "settings": ["configuraç", "configurac"],
        "sync": ["sincroniz"],
        "template": ["modelo"],
        "archive": ["arquiv"],
        "shortcut": ["atalho"],
        "graph": ["gráfic", "grafic"],
        "tray": ["bandeja"],
        "recovery": ["recupera"],
        "conflict": ["conflito"],
        "scan": ["verifica", "varredura"],
        "index": ["índice", "indice"],
    },
}

PT_PT_TERMS = re.compile(
    r"\b(ecrã|ecra|ficheiro|ficheiros|utilizador|utilizadores|guardar|eliminar|"
    r"palavra-passe|telemóvel|telemovel|definições|definicoes|equipa|autocarro)\b",
    re.IGNORECASE,
)

ENGLISH_FUNCTION = re.compile(
    r"\b(the|and|with|your|this|that|from|for|not|will|when|then|"
    r"could|should|would|unable|failed|please)\b",
    re.IGNORECASE,
)


@dataclass
class Row:
    kind: str
    id: str
    english: str
    target: str

    @property
    def en(self) -> str:
        return show(self.english)

    @property
    def tgt(self) -> str:
        return show(self.target)


def find_issue(row: Row, lang_key: str) -> tuple[str, str] | None:
    en, tgt = row.en, row.tgt
    # Placeholders: each sentinel must occur exactly once on both sides.
    en_s = Counter(SENTINEL.findall(row.english))
    tgt_s = Counter(SENTINEL.findall(row.target))
    if en_s != tgt_s:
        return ("critical", f"placeholder mismatch: English {dict(en_s)} vs target {dict(tgt_s)}")

    # Invariant keycaps/chords/tokens.
    missing = tokens(en) - tokens(tgt)
    if missing:
        critical_missing = {
            token
            for token in missing
            if token in {"YYYY", "@{", "@@{", "MUNINN_VAULT"} or token.startswith(("MUNINN_", "xdg-", "127.0.0.1"))
        }
        # `⌘` is the English chord; a German pack legitimately writes Ctrl.
        if "⌘" in missing and "Ctrl" in tgt:
            missing.discard("⌘")
            critical_missing.discard("⌘")
        # A pack may spell a tag example as `(#)` rather than `#tag`.
        if "#" in tgt:
            missing = {token for token in missing if not token.startswith("#")}
            critical_missing = {token for token in critical_missing if not token.startswith("#")}
        if missing:
            severity = "critical" if critical_missing else "warning"
            return (severity, "missing invariant token(s): " + ", ".join(sorted(missing)))

    # Numbers: ignore the numbers inside sentinels (none) and known format tokens.
    en_nums = Counter(NUMBERS.findall(SENTINEL.sub("", en)))
    tgt_nums = Counter(NUMBERS.findall(SENTINEL.sub("", tgt)))
    if en_nums and en_nums != tgt_nums:
        return ("critical", f"number mismatch: English {dict(en_nums)} vs target {dict(tgt_nums)}")

    # Terminal punctuation.
    pe, pt = last_char(en), last_char(tgt)
    if pe in ":;?!." and pt not in {pe, "："}:
        return ("warning", f"terminal punctuation differs: {pe!r} vs {pt!r}")
    if pe == "…" and pt not in {"…", "."}:
        return ("warning", f"ellipsis differs: {pe!r} vs {pt!r}")

    # English function words leaking into a non-English string.
    if lang_key != "en":
        leaked = sorted(set(m.group(0).lower() for m in ENGLISH_FUNCTION.finditer(tgt)))
        if leaked:
            return ("warning", "English function word(s): " + ", ".join(leaked))

    # Language-specific rules.
    if lang_key == "fr":
        if "'" in tgt:
            return ("critical", "ASCII apostrophe (use U+2019)")
        for match in re.finditer(r"([:;!?])", tgt):
            pos = match.start()
            before = tgt[:pos]
            if before.endswith(("http", "https", "127.0.0.1", "HH", "mm")) or re.search(r"\d$", before):
                continue
            if match.group(0) in "?!" and re.search(r'[*"]\s*$', before):
                continue
            if not before.endswith((FRENCH_NBSP, "\u202f")):
                return ("warning", f"missing no-break space before {match.group(0)!r}")
    if lang_key == "pt" and PT_PT_TERMS.search(tgt):
        return ("critical", "pt-PT term(s): " + ", ".join(sorted(set(m.group(0) for m in PT_PT_TERMS.finditer(tgt)))))

    # Glossary (only when English uses the term as a word).
    if row.kind == "key":
        for term, allowed in GLOSSARY.get(lang_key, {}).items():
            if not re.search(rf"\b{re.escape(term)}s?\b", en, re.IGNORECASE):
                continue
            low = tgt.lower()
            if not any(a.lower() in low for a in allowed):
                return ("warning", f"glossary: English {term!r} lacks {allowed[0]!r}")

    # Length outlier (short labels matter most; messages with many placeholders are noisy).
    en_len, tgt_len = len(en), len(tgt)
    if en_len >= 10 and en_len <= 80 and not SENTINEL.search(row.english):
        ratio = tgt_len / en_len
        if ratio < 0.45 or ratio > 2.2:
            return ("warning", f"length ratio {ratio:.2f} ({en_len} -> {tgt_len})")
    return None


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    tsv, lang, out = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3])
    summary_path = Path(sys.argv[4]) if len(sys.argv) > 4 else None
    lang_key = "pt" if lang.lower().startswith("pt") else lang.split("-")[0].lower()
    rows: list[Row] = []
    for line in tsv.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("kind\t"):
            continue
        kind, ident, english, target = line.split("\t", 3)
        rows.append(Row(kind, ident, unescape(english), unescape(target)))

    findings: list[tuple[str, Row, str]] = []
    for row in rows:
        issue = find_issue(row, lang_key)
        if issue:
            severity, text = issue
            findings.append((severity, row, text))

    critical = [f for f in findings if f[0] == "critical"]
    warnings = [f for f in findings if f[0] == "warning"]
    lines = [
        f"# {lang} translation triage",
        "",
        f"Rows: {len(rows)}. Critical: {len(critical)}. Warnings: {len(warnings)}.",
        "",
        "This is a heuristic review aid, not a correctness proof. It checks the",
        "mechanical contract and flags likely semantic/typography risks; a bilingual",
        "reviewer still signs off.",
        "",
        "## Critical",
        "",
    ]
    if critical:
        lines += ["| id | English | Target | Issue |", "|---|---|---|---|"]
        for severity, row, text in critical[:80]:
            lines.append(f"| `{row.id}` | {row.en.replace('|', '\\|')} | {row.tgt.replace('|', '\\|')} | {text} |")
    else:
        lines.append("None.")
    lines += ["", "## Warnings", ""]
    if warnings:
        lines += ["| id | English | Target | Issue |", "|---|---|---|---|"]
        for severity, row, text in warnings[:120]:
            lines.append(f"| `{row.id}` | {row.en.replace('|', '\\|')} | {row.tgt.replace('|', '\\|')} | {text} |")
        if len(warnings) > 120:
            lines.append(f"| … | | | {len(warnings) - 120} more warning(s) |")
    else:
        lines.append("None.")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    if summary_path:
        with summary_path.open("a", encoding="utf-8") as handle:
            handle.write(f"- **{lang}**: {len(critical)} critical, {len(warnings)} warning(s) across {len(rows)} rows.\n")
    print(f"{lang}: {len(critical)} critical, {len(warnings)} warnings -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
