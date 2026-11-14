#!/usr/bin/env node
// Offline internal link check over site/dist.
// Usage (from site/, after a build): node scripts/check-links.mjs
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, posix, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const DIST = join(SITE_DIR, 'dist');

function walk(dir, files = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, files);
    else files.push(path);
  }
  return files;
}

function fileForUrlPath(urlPath) {
  const clean = urlPath.replace(/#.*$/, '').replace(/\?.*$/, '');
  if (!clean || !clean.startsWith('/')) return null;
  const rel = clean.replace(/^\//, '');
  const candidates = rel.endsWith('/')
    ? [join(DIST, rel, 'index.html'), join(DIST, rel.slice(0, -1) + '.html')]
    : [join(DIST, rel), join(DIST, rel, 'index.html'), join(DIST, rel + '.html'), join(DIST, rel + '/index.html')];
  return candidates.find((path) => existsSync(path)) ?? null;
}

function idsIn(html) {
  const ids = new Set();
  for (const match of html.matchAll(/\sid="([^"]+)"/gi)) ids.add(match[1]);
  return ids;
}

const htmlFiles = walk(DIST).filter((path) => extname(path) === '.html');
let broken = 0;

for (const file of htmlFiles) {
  const html = readFileSync(file, 'utf8');
  const pageDir = posix.dirname('/' + relative(DIST, file).split('\\').join('/').replace(/index\.html$/, ''));
  const hrefs = [...html.matchAll(/\b(?:href|src)="([^"]+)"/gi)].map((m) => m[1]);
  for (const href of hrefs) {
    if (!href || href.startsWith('mailto:') || href.startsWith('tel:') || href.startsWith('data:') || href.startsWith('javascript:')) continue;
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) continue; // external

    let pathPart = href;
    let hash = '';
    const hashAt = href.indexOf('#');
    if (hashAt === 0) {
      pathPart = '/' + relative(DIST, file).split('\\').join('/').replace(/index\.html$/, '');
      if (!pathPart.endsWith('/')) pathPart += '/';
      if (pathPart === '//') pathPart = '/';
      hash = href;
    } else if (hashAt > 0) {
      pathPart = href.slice(0, hashAt);
      hash = href.slice(hashAt);
    }

    const resolved = pathPart.startsWith('/') ? pathPart : posix.normalize(posix.join(pageDir, pathPart));
    const target = fileForUrlPath(resolved);
    const assetHit = target || existsSync(join(DIST, resolved.replace(/^\//, '')));
    if (!assetHit) {
      broken++;
      console.log(`BROKEN ${relative(DIST, file)} -> ${href} (resolved ${resolved})`);
      continue;
    }
    if (hash && hash !== '#') {
      const id = decodeURIComponent(hash.slice(1));
      const targetHtmlPath = target && extname(target) === '.html' ? target : fileForUrlPath(resolved);
      if (targetHtmlPath && extname(targetHtmlPath) === '.html') {
        const ids = idsIn(readFileSync(targetHtmlPath, 'utf8'));
        if (!ids.has(id)) {
          broken++;
          console.log(`BROKEN ANCHOR ${relative(DIST, file)} -> ${href} (no id="${id}" on ${relative(DIST, targetHtmlPath)})`);
        }
      }
    }
  }
}

if (broken > 0) {
  console.log(`LINK CHECK: FAIL (${broken} broken, ${htmlFiles.length} html files)`);
  process.exit(1);
}
console.log(`LINK CHECK: PASS (${htmlFiles.length} html files, no broken internal links or anchors)`);
