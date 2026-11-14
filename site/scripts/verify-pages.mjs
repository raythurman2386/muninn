#!/usr/bin/env node
// Browser checks for /privacy/ and the branded 404: the theme toggle label
// matches the theme, clicking it flips the theme, there are no console errors,
// and Lighthouse best-practices is 100. Used after the W3 layout pages landed.
//
//   node scripts/verify-pages.mjs
//   NEW_URL=http://127.0.0.1:8080 node scripts/verify-pages.mjs
//
// Umami is blocked so no pageview is recorded. CHROME_PATH and LIGHTHOUSE
// (default 1) follow verify-w1.mjs.
import { createServer } from 'node:http';
import { createReadStream, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const SITE_DIR = resolve(fileURLToPath(new URL('../package.json', import.meta.url)), '..');
const DIST = resolve(SITE_DIR, 'dist');
const OUT = resolve(process.env.PROOF_DIR || join(SITE_DIR, '.proof-w3'));
const CHROME = process.env.CHROME_PATH || '/usr/bin/google-chrome';
const RUN_LH = (process.env.LIGHTHOUSE ?? '1') !== '0';
const UMAMI = /umami-app\.ravenwoodsoftware\.dev/;
mkdirSync(OUT, { recursive: true });

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

function expectedLabel(theme) {
  // The toggle advertises the theme it will switch to.
  return theme === 'dark' ? 'Light' : 'Dark';
}
function expectedAria(theme) {
  return theme === 'dark' ? 'Use light theme' : 'Use dark theme';
}

let failed = false;
const rows = [];
function record(id, ok, detail) {
  if (!ok) failed = true;
  rows.push({ id, ok, ...detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${id}${detail && detail.note ? ': ' + detail.note : ''}`);
}

const localServer = await serve(DIST);
const localBase = `http://127.0.0.1:${localServer.address().port}/`;
const base = (process.env.NEW_URL || localBase).replace(/\/?$/, '/');
const pages = [
  { id: 'privacy', url: new URL('privacy/', base).href },
  { id: '404', url: new URL('definitely-not-here', base).href, expectStatus: 404 },
];
console.log(`verify-pages against ${base}`);

const browser = await chromium.launch({
  executablePath: CHROME, headless: true,
  args: ['--hide-scrollbars', '--force-color-profile=srgb'],
});

for (const pageDef of pages) {
  for (const start of ['dark', 'light']) {
    const ctx = await browser.newContext({
      viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: start,
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
      // The 404 document itself logs "Failed to load resource ... 404" in Chrome.
      // Ignore that for the page URL only; other failed resources still fail the check.
      if (pageDef.expectStatus === 404 && /status of 404/.test(text)) {
        const loc = m.location()?.url || '';
        if (!loc || loc === pageDef.url || loc.startsWith(pageDef.url) || /definitely-not-here/.test(loc) || /definitely-not-here/.test(text)) return;
      }
      errors.push(text);
    });
    const resp = await page.goto(pageDef.url, { waitUntil: 'load' });
    await page.evaluate(async () => { await document.fonts.ready; });
    const status = resp ? resp.status() : 0;
    if (pageDef.expectStatus && status !== pageDef.expectStatus) {
      record(`${pageDef.id} ${start} status`, false, { note: `got ${status}, want ${pageDef.expectStatus}` });
    }
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
      errors: [...errors],
    });
    record(`${pageDef.id} ${start} no errors before click`, errors.length === 0, { errors: [...errors] });

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
      errors: [...errors],
    });
    record(`${pageDef.id} ${start} no errors after click`, errors.length === 0, { errors: [...errors] });
    await ctx.close();
  }
}

// Lighthouse treats HTTP 404 as a failed page load, so the 404 audit hits
// /404.html on the local dist server (same document, status 200).
const lhPages = [
  { id: 'privacy', url: new URL('privacy/', base).href },
  { id: '404', url: `${localBase}404.html` },
];

let lighthouse = null;
if (RUN_LH) {
  lighthouse = {};
  for (const pageDef of lhPages) {
    const json = join(OUT, `lighthouse-bp-${pageDef.id}.json`);
    const lhArgs = ['--no-install', 'lighthouse', pageDef.url, '--no-enable-error-reporting', '--quiet', '--output=json', `--output-path=${json}`,
      '--only-categories=best-practices',
      '--blocked-url-patterns=*umami-app.ravenwoodsoftware\.dev*',
      '--chrome-flags=--headless=new --no-sandbox --hide-scrollbars'];
    const r = await new Promise((ok) => spawn('npx', lhArgs, { stdio: 'inherit', env: { ...process.env } }).on('close', (status) => ok({ status })));
    if (r.status === 0 && existsSync(json)) {
      const cats = JSON.parse(readFileSync(json, 'utf8')).categories;
      const score = Math.round(cats['best-practices'].score * 100);
      lighthouse[pageDef.id] = score;
      const failedAudits = Object.entries(JSON.parse(readFileSync(json, 'utf8')).audits)
        .filter(([, v]) => v.score !== null && v.score < 1 && v.scoreDisplayMode !== 'informative' && v.scoreDisplayMode !== 'manual' && v.scoreDisplayMode !== 'notApplicable')
        .map(([k]) => k);
      record(`${pageDef.id} lighthouse best-practices`, score === 100, { note: `score=${score}${failedAudits.length ? ' failed: ' + failedAudits.join(', ') : ''}` });
    } else {
      lighthouse[pageDef.id] = `failed (exit ${r.status})`;
      record(`${pageDef.id} lighthouse best-practices`, false, { note: `lighthouse exit ${r.status}` });
    }
  }
}

await browser.close();
localServer.close();
writeFileSync(join(OUT, 'pages-results.json'), JSON.stringify({ rows, lighthouse }, null, 2));
console.log(failed ? 'PAGES VERIFY: FAIL' : 'PAGES VERIFY: PASS');
process.exit(failed ? 1 : 0);
