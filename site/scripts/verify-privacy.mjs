#!/usr/bin/env node
// Diff the rendered /privacy/ page against PRIVACY.md (normalized text).
// Usage (from site/, after a build): node scripts/verify-privacy.mjs
// Optional: --html path/to/privacy/index.html --md path/to/PRIVACY.md
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const REPO_DIR = dirname(SITE_DIR);

const { values: args } = parseArgs({
  options: {
    html: { type: 'string' },
    md: { type: 'string' },
  },
});

const htmlPath = args.html ?? join(SITE_DIR, 'dist', 'privacy', 'index.html');
const mdPath = args.md ?? join(REPO_DIR, 'PRIVACY.md');

function normalize(text) {
  return text
    .replace(/\r\n/g, '\n')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function markdownToText(md) {
  // PRIVACY.md is simple: ATX headings, paragraphs, backticks, bare URLs. No
  // reference links or images. Strip the syntax and leave the visible text.
  return normalize(
    md
      .replace(/^#{1,6}\s+/gm, '')
      .replace(/`([^`]+)`/g, '$1')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1'),
  );
}

function htmlToText(html) {
  const article = html.match(/<article\b[^>]*>([\s\S]*?)<\/article>/i);
  if (!article) throw new Error(`no <article> in ${htmlPath}`);
  const text = article[1]
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(p|h[1-6]|li|div|tr|section|article)>/gi, '\n')
    .replace(/<li\b[^>]*>/gi, '- ')
    .replace(/<[^>]+>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&mdash;|&#8212;/g, '—')
    .replace(/&ndash;|&#8211;/g, '–');
  return normalize(text);
}

const expected = markdownToText(readFileSync(mdPath, 'utf8'));
const actual = htmlToText(readFileSync(htmlPath, 'utf8'));
if (expected === actual) {
  console.log(`PRIVACY VERIFY: PASS (${expected.split(/\n/).length} lines, ${expected.length} chars)`);
  process.exit(0);
}

const expLines = expected.split('\n');
const actLines = actual.split('\n');
const n = Math.max(expLines.length, actLines.length);
console.log('PRIVACY VERIFY: FAIL');
console.log(`markdown: ${mdPath} (${expLines.length} lines)`);
console.log(`html:     ${htmlPath} (${actLines.length} lines)`);
for (let i = 0; i < n; i++) {
  if (expLines[i] !== actLines[i]) {
    console.log(`first diff at line ${i + 1}:`);
    console.log(`  md:   ${JSON.stringify(expLines[i] ?? '<missing>')}`);
    console.log(`  html: ${JSON.stringify(actLines[i] ?? '<missing>')}`);
    break;
  }
}
process.exit(1);
