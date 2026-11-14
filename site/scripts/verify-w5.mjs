#!/usr/bin/env node
// W5 checks on a built site/dist:
//   1. Every `## [version]` heading in CHANGELOG.md has a matching id on /changelog/.
//   2. Every curated docs page (and /docs/, /changelog/) was emitted.
// Usage (from site/, after a build): node scripts/verify-w5.mjs
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const REPO_DIR = dirname(SITE_DIR);
const DIST = join(SITE_DIR, 'dist');

const EXPECTED_PAGES = [
  'changelog/index.html',
  'docs/index.html',
  'docs/getting-started/index.html',
  'docs/keyboard-shortcuts/index.html',
  'docs/sync-setup/index.html',
  'docs/kanban-format/index.html',
  'docs/a11y-checklist/index.html',
  'docs/sync-safety/index.html',
];

let failed = false;
function fail(message) {
  failed = true;
  console.log(`FAIL ${message}`);
}

for (const rel of EXPECTED_PAGES) {
  const path = join(DIST, rel);
  if (existsSync(path)) console.log(`ok   emitted ${rel}`);
  else fail(`missing ${rel}`);
}

const changelogMd = readFileSync(join(REPO_DIR, 'CHANGELOG.md'), 'utf8');
const versions = [];
for (const line of changelogMd.split('\n')) {
  const match = /^## \[([^\]]+)\]/.exec(line);
  if (match) {
    const version = match[1];
    versions.push({
      version,
      id: version.toLowerCase() === 'unreleased' ? 'unreleased' : version,
    });
  }
}

const htmlPath = join(DIST, 'changelog/index.html');
if (existsSync(htmlPath)) {
  const html = readFileSync(htmlPath, 'utf8');
  for (const { version, id } of versions) {
    const escaped = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const hasId = new RegExp(`\\sid="${escaped}"`, 'i').test(html);
    const hasToc = html.includes(`href="/changelog/#${id}"`) || html.includes(`href="#${id}"`);
    if (hasId) console.log(`ok   changelog anchor #${id} (## [${version}])`);
    else fail(`changelog missing id="${id}" for ## [${version}]`);
    if (!hasToc) fail(`changelog TOC missing link to #${id}`);
  }
  console.log(`ok   ${versions.length} version headings checked`);
}

if (failed) {
  console.log('W5 VERIFY: FAIL');
  process.exit(1);
}
console.log('W5 VERIFY: PASS');
