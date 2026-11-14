#!/usr/bin/env node
// W8 proof: OG image dimensions, per-page og:image / alt meta, screenshots
// of /sponsor/ and /press/ in both themes, a contact sheet of every OG
// image, and (when LIGHTHOUSE=1, the default) Lighthouse 100s on those pages.
//
//   node scripts/verify-w8.mjs
//   NEW_URL=http://127.0.0.1:8080 node scripts/verify-w8.mjs
//
// Umami is blocked so no pageview is recorded. CHROME_PATH follows verify-w1.
import { createServer } from 'node:http';
import { createReadStream, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import sharp from 'sharp';

const SITE_DIR = resolve(fileURLToPath(new URL('../package.json', import.meta.url)), '..');
const DIST = resolve(SITE_DIR, 'dist');
const PUBLIC = resolve(SITE_DIR, 'public');
const OUT = resolve(process.env.PROOF_DIR || join(SITE_DIR, '.proof-w8'));
const CHROME = process.env.CHROME_PATH || '/usr/bin/google-chrome';
const RUN_LH = (process.env.LIGHTHOUSE ?? '1') !== '0';
const UMAMI = /umami-app\.ravenwoodsoftware\.dev/;
const SITE_ORIGIN = 'https://muninn.ravenwoodsoftware.dev';
// Keep in sync with src/lib/og.ts. Duplicated here so this script can run
// under plain Node without a TypeScript loader.
const OG = {
  home: {
    image: '/assets/og.png',
    alt: 'Muninn logo and the words “Muninn — a local-first Markdown vault with a knowledge graph.”',
  },
  features: {
    image: '/assets/og/features.png',
    alt: 'Muninn logo and the words “Muninn — Features. Notes, Board, Kanban, and the knowledge graph.”',
  },
  download: {
    image: '/assets/og/download.png',
    alt: 'Muninn logo and the words “Muninn — Download. Linux, macOS, and Windows builds.”',
  },
  docs: {
    image: '/assets/og/docs.png',
    alt: 'Muninn logo and the words “Muninn — Docs. Getting started and user guides.”',
  },
  changelog: {
    image: '/assets/og/changelog.png',
    alt: 'Muninn logo and the words “Muninn — Changelog. Release notes for every version.”',
  },
  privacy: {
    image: '/assets/og/privacy.png',
    alt: 'Muninn logo and the words “Muninn — Privacy. How Muninn handles your data.”',
  },
  sponsor: {
    image: '/assets/og/sponsor.png',
    alt: 'Muninn logo and the words “Muninn — Sponsor. Support Muninn’s development.”',
  },
  press: {
    image: '/assets/og/press.png',
    alt: 'Muninn logo and the words “Muninn — Press. Logos, screenshots, and product facts.”',
  },
};
mkdirSync(OUT, { recursive: true });
mkdirSync(join(OUT, 'shots'), { recursive: true });

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript',
  '.svg': 'image/svg+xml', '.webp': 'image/webp', '.png': 'image/png', '.mp4': 'video/mp4',
  '.woff2': 'font/woff2', '.txt': 'text/plain', '.json': 'application/json',
  '.xml': 'application/xml', '.ico': 'image/x-icon', '.webmanifest': 'application/manifest+json',
};

function serve(root) {
  const server = createServer((req, res) => {
    let path;
    try { path = decodeURIComponent(new URL(req.url, 'http://x').pathname); }
    catch { res.writeHead(400).end('bad request'); return; }
    let file = resolve(root, '.' + path);
    const rel = relative(root, file);
    if (rel === '..' || rel.startsWith('..' + sep) || isAbsolute(rel) || path.includes('\0')) {
      res.writeHead(403).end(); return;
    }
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    let status = 200;
    if (!existsSync(file)) {
      file = join(root, '404.html');
      status = 404;
      if (!existsSync(file)) { res.writeHead(404).end('not found'); return; }
    }
    const type = TYPES[extname(file)] || 'application/octet-stream';
    res.writeHead(status, { 'Content-Type': type });
    createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

function attr(html, attrName, valueName) {
  const re = new RegExp(`${attrName}="${valueName}"[^>]*content="([^"]*)"|content="([^"]*)"[^>]*${attrName}="${valueName}"`);
  const m = html.match(re);
  return m ? (m[1] || m[2]) : null;
}

let failed = false;
const rows = [];
function record(id, ok, detail = {}) {
  if (!ok) failed = true;
  rows.push({ id, ok, ...detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${id}${detail.note ? ': ' + detail.note : ''}`);
}

const OG_PAGES = Object.entries(OG);
for (const [slug, meta] of OG_PAGES) {
  const file = join(PUBLIC, meta.image.replace(/^\//, ''));
  if (!existsSync(file)) {
    record(`og ${slug} exists`, false, { note: file });
    continue;
  }
  const info = await sharp(file).metadata();
  const ok = info.width === 1200 && info.height === 630 && info.format === 'png';
  record(`og ${slug} 1200x630`, ok, { note: `${info.width}x${info.height} ${info.format}` });
}

// Pages that exist on this branch, plus any W3–W7 routes already in dist/
// (so a later merge can re-run this script without editing it).
const META_PAGES = [
  { id: 'home', route: '', html: 'index.html', og: OG.home, required: true },
  { id: 'sponsor', route: 'sponsor/', html: 'sponsor/index.html', og: OG.sponsor, required: true },
  { id: 'press', route: 'press/', html: 'press/index.html', og: OG.press, required: true },
  { id: 'features', route: 'features/', html: 'features/index.html', og: OG.features, required: false },
  { id: 'download', route: 'download/', html: 'download/index.html', og: OG.download, required: false },
  { id: 'docs', route: 'docs/', html: 'docs/index.html', og: OG.docs, required: false },
  { id: 'changelog', route: 'changelog/', html: 'changelog/index.html', og: OG.changelog, required: false },
  { id: 'privacy', route: 'privacy/', html: 'privacy/index.html', og: OG.privacy, required: false },
];

for (const page of META_PAGES) {
  const file = join(DIST, page.html);
  if (!existsSync(file)) {
    record(`meta ${page.id}`, page.required ? false : true, { note: page.required ? 'missing HTML' : 'not in this build (ok)' });
    continue;
  }
  const html = readFileSync(file, 'utf8');
  const image = attr(html, 'property', 'og:image');
  const alt = attr(html, 'property', 'og:image:alt');
  const width = attr(html, 'property', 'og:image:width');
  const height = attr(html, 'property', 'og:image:height');
  const twitter = attr(html, 'name', 'twitter:image');
  const card = attr(html, 'name', 'twitter:card');
  const want = `${SITE_ORIGIN}${page.og.image}`;
  const ok = image === want && twitter === want && alt === page.og.alt && width === '1200' && height === '630' && card === 'summary_large_image';
  record(`meta ${page.id}`, ok, { note: `image=${image} alt=${alt ? 'set' : 'missing'} ${width}x${height} card=${card}` });
}

// Contact sheet of every OG image, 2x4 at half size, for the PR proof.
const thumbs = [];
for (const [slug, meta] of OG_PAGES) {
  const file = join(PUBLIC, meta.image.replace(/^\//, ''));
  if (existsSync(file)) thumbs.push({ slug, file });
}
const thumbW = 600, thumbH = 315, labelH = 40, pad = 24, cols = 2;
const rowsN = Math.ceil(thumbs.length / cols);
const sheetW = pad + cols * (thumbW + pad);
const sheetH = pad + rowsN * (thumbH + labelH + pad);
const composites = [];
// Label strip under each thumb.
const labelSvg = (text) => Buffer.from(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${thumbW}" height="${labelH}">` +
  `<rect width="100%" height="100%" fill="#101010"/>` +
  `<text x="0" y="26" font-size="18" font-family="Inter, ui-sans-serif, sans-serif" fill="#fafafa">${text}</text>` +
  `</svg>`
);
for (let i = 0; i < thumbs.length; i++) {
  const r = Math.floor(i / cols);
  const c = i % cols;
  const x = pad + c * (thumbW + pad);
  const y = pad + r * (thumbH + labelH + pad);
  const buf = await sharp(thumbs[i].file).resize(thumbW, thumbH).png().toBuffer();
  composites.push({ input: buf, left: x, top: y });
  composites.push({ input: labelSvg(thumbs[i].slug), left: x, top: y + thumbH });
}
const sheetPath = join(OUT, 'og-contact-sheet.png');
await sharp({ create: { width: sheetW, height: sheetH, channels: 3, background: { r: 16, g: 16, b: 16 } } })
  .composite(composites)
  .png({ compressionLevel: 9 })
  .toFile(sheetPath);
record('og contact sheet', existsSync(sheetPath), { note: sheetPath });

// Also write 300x158 (comment-thumbnail) copies so small-size readability can be inspected.
mkdirSync(join(OUT, 'og-small'), { recursive: true });
for (const thumb of thumbs) {
  await sharp(thumb.file).resize(300, 158).png().toFile(join(OUT, 'og-small', `${thumb.slug}.png`));
}

const localServer = await serve(DIST);
const localBase = `http://127.0.0.1:${localServer.address().port}/`;
const base = (process.env.NEW_URL || localBase).replace(/\/?$/, '/');
console.log(`verify-w8 against ${base}`);

function expectedLabel(theme) { return theme === 'dark' ? 'Light' : 'Dark'; }
function expectedAria(theme) { return theme === 'dark' ? 'Use light theme' : 'Use dark theme'; }

const shotPages = [
  { id: 'sponsor', route: 'sponsor/' },
  { id: 'press', route: 'press/' },
];

const browser = await chromium.launch({
  executablePath: CHROME, headless: true,
  args: ['--hide-scrollbars', '--force-color-profile=srgb'],
});

for (const pageDef of shotPages) {
  for (const start of ['dark', 'light']) {
    for (const viewport of [
      { name: '1440', width: 1440, height: 900 },
      { name: '390', width: 390, height: 844 },
    ]) {
      const ctx = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        deviceScaleFactor: 1,
        colorScheme: start,
      });
      await ctx.addInitScript((t) => { try { localStorage.setItem('muninn-theme', t); } catch (e) {} }, start);
      await ctx.route(UMAMI, (route) => route.abort());
      const errors = [];
      const page = await ctx.newPage();
      page.on('pageerror', (e) => errors.push(String(e)));
      page.on('console', (m) => {
        if (m.type() !== 'error') return;
        const text = m.text();
        if (/umami|ERR_FAILED|net::ERR_BLOCKED_BY_CLIENT/i.test(text)) return;
        errors.push(text);
      });
      const url = new URL(pageDef.route, base).href;
      await page.goto(url, { waitUntil: 'load' });
      await page.evaluate(async () => { await document.fonts.ready; });
      const shot = join(OUT, 'shots', `${pageDef.id}-${start}-${viewport.name}.png`);
      await page.screenshot({ path: shot, fullPage: true });
      if (viewport.name === '1440') {
        const before = await page.evaluate(() => ({
          theme: document.documentElement.getAttribute('data-theme'),
          label: document.querySelector('.theme-toggle')?.getAttribute('aria-label') || null,
          text: document.querySelector('.theme-toggle .toggle-label')?.textContent || null,
        }));
        const labelOk = before.theme === start
          && before.label === expectedAria(start)
          && before.text === expectedLabel(start);
        record(`${pageDef.id} ${start} label`, labelOk, {
          note: `theme=${before.theme} aria="${before.label}" text="${before.text}"`,
        });
        record(`${pageDef.id} ${start} no errors`, errors.length === 0, { errors: [...errors] });
        await page.click('.theme-toggle');
        const flipped = start === 'dark' ? 'light' : 'dark';
        const after = await page.evaluate(() => ({
          theme: document.documentElement.getAttribute('data-theme'),
          label: document.querySelector('.theme-toggle')?.getAttribute('aria-label') || null,
          text: document.querySelector('.theme-toggle .toggle-label')?.textContent || null,
        }));
        const flipOk = after.theme === flipped
          && after.label === expectedAria(flipped)
          && after.text === expectedLabel(flipped);
        record(`${pageDef.id} ${start} click flips to ${flipped}`, flipOk, {
          note: `theme=${after.theme} aria="${after.label}" text="${after.text}"`,
        });
        record(`${pageDef.id} ${start} no errors after click`, errors.length === 0, { errors: [...errors] });
      }
      await ctx.close();
    }
  }
}

let lighthouse = null;
if (RUN_LH) {
  lighthouse = {};
  for (const pageDef of shotPages) {
    lighthouse[pageDef.id] = {};
    const url = new URL(pageDef.route, base).href;
    for (const [name, preset] of [['mobile', null], ['desktop', 'desktop']]) {
      const json = join(OUT, `lighthouse-${pageDef.id}-${name}.json`);
      const lhArgs = ['--no-install', 'lighthouse', url, '--no-enable-error-reporting', '--quiet', '--output=json', `--output-path=${json}`,
        '--only-categories=performance,accessibility,best-practices,seo',
        '--blocked-url-patterns=*umami-app.ravenwoodsoftware.dev*',
        '--chrome-flags=--headless=new --no-sandbox --hide-scrollbars'];
      if (preset) lhArgs.push(`--preset=${preset}`);
      const r = await new Promise((ok) => spawn('npx', lhArgs, { stdio: 'inherit', env: { ...process.env } }).on('close', (status) => ok({ status })));
      if (r.status === 0 && existsSync(json)) {
        const cats = JSON.parse(readFileSync(json, 'utf8')).categories;
        const scores = Object.fromEntries(Object.entries(cats).map(([k, v]) => [k, Math.round(v.score * 100)]));
        lighthouse[pageDef.id][name] = scores;
        const all100 = Object.values(scores).every((s) => s === 100);
        const failedAudits = [];
        if (!all100) {
          const audits = JSON.parse(readFileSync(json, 'utf8')).audits;
          for (const [id, audit] of Object.entries(audits)) {
            if (audit.score !== null && audit.score < 1 && audit.scoreDisplayMode !== 'informative' && audit.scoreDisplayMode !== 'manual' && audit.scoreDisplayMode !== 'notApplicable') {
              failedAudits.push(`${id}=${audit.score}`);
            }
          }
        }
        record(`${pageDef.id} lighthouse ${name}`, all100, {
          note: `${JSON.stringify(scores)}${failedAudits.length ? ' failed: ' + failedAudits.join(', ') : ''}`,
        });
      } else {
        lighthouse[pageDef.id][name] = `failed (exit ${r.status})`;
        record(`${pageDef.id} lighthouse ${name}`, false, { note: `lighthouse exit ${r.status}` });
      }
    }
  }
}

await browser.close();
localServer.close();
writeFileSync(join(OUT, 'results.json'), JSON.stringify({ rows, lighthouse }, null, 2));
console.log(failed ? 'W8 VERIFY: FAIL' : 'W8 VERIFY: PASS');
process.exit(failed ? 1 : 0);
