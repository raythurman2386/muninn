# Security

Muninn reads and writes Markdown files in a folder you choose. It does not run note content,
load remote resources, or send your vault anywhere.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's
[security advisories](https://github.com/raythurman2386/muninn/security/advisories/new) rather
than opening a public issue. Include the affected version or commit, the platform, and a minimal
reproduction. Do not include the contents of private notes.

If the advisory form is unavailable, open a minimal issue asking for a private contact channel
and omit exploit details.

## Scope

Relevant areas include path handling for `--vault` and `MUNINN_VAULT`, note creation names,
Markdown link rewriting, and anything that could write outside the chosen vault root.

Dependency vulnerabilities should be reported upstream to
[Rune](https://github.com/anomalyco/rune) when the issue is in the framework.
