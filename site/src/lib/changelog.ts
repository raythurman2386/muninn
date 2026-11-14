// Changelog helpers. The page is rendered from the repo's CHANGELOG.md;
// every `## [x.y.z]` (and `## [Unreleased]`) gets a stable heading id.

const VERSION_HEADING = /^## \[([^\]]+)\](?:\s+-\s+(\S+))?/;

export interface ChangelogVersion {
  id: string;
  version: string;
  date: string | null;
  label: string;
}

export function versionAnchor(version: string): string {
  return version.toLowerCase() === 'unreleased' ? 'unreleased' : version;
}

export function changelogVersions(markdown: string): ChangelogVersion[] {
  const versions: ChangelogVersion[] = [];
  for (const line of markdown.split('\n')) {
    const match = VERSION_HEADING.exec(line);
    if (!match) continue;
    const version = match[1];
    const date = match[2] ?? null;
    versions.push({
      id: versionAnchor(version),
      version,
      date,
      label: date ? `${version} — ${date}` : version,
    });
  }
  return versions;
}

// Add `{#0.8.2}` (and `{#unreleased}`) so the heading id is the version, not
// a slug of the whole "0.8.2 - 2026-11-13" line.
export function addChangelogAnchors(markdown: string): string {
  return markdown.replace(/^## \[([^\]]+)\](.*)$/gm, (_full, version: string, rest: string) => {
    return `## [${version}]${rest} {#${versionAnchor(version)}}`;
  });
}
