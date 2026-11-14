// Screenshot + pixel-diff harness for the W1 Astro port. Run through scripts/verify-w1.sh.
// Serves the old static site and the new dist/ on 127.0.0.1, captures deterministic full-page
// screenshots in headless Chrome, and compares them with pixelmatch.
import { createServer } from 'node:http';
import { createReadStream, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright-core';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';

const { values: args } = parseArgs({
  options: { old: { type: 'string' }, new: { type: 'string' }, out: { type: 'string' } },
});
const OLD = resolve(args.old);
const NEW = resolve(args.new);
const OUT = resolve(args.out);
const CHROME = process.env.CHROME_PATH || '/usr/bin/google-chrome';
const MAX_DIFF_PIXELS = Number(process.env.MAX_DIFF_PIXELS || 0);
const UMAMI = /umami-app\.ravenwoodsoftware\.dev/;
mkdirSync(join(OUT, 'shots'), { recursive: true });

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript',
  '.svg': 'image/svg+xml', '.webp': 'image/webp', '.png': 'image/png', '.mp4': 'video/mp4',
  '.woff2': 'font/woff2', '.txt': 'text/plain', '.json': 'application/json',
};

// Minimal static server with Range support (video seeking needs it). Loopback only.
// Malformed percent-encoding gets 400; any path that resolves outside root gets 403.
function serve(root) {
  const server = createServer((req, res) => {
    let path;
    try {
      path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    } catch {
      res.writeHead(400).end('bad request');
      return;
    }
    let file = resolve(root, '.' + path);
    const rel = relative(root, file);
    if (rel === '..' || rel.startsWith('..' + sep) || isAbsolute(rel) || path.includes('\0')) { res.writeHead(403).end(); return; }
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) { res.writeHead(404).end('not found'); return; }
    const size = statSync(file).size;
    const type = TYPES[extname(file)] || 'application/octet-stream';
    const range = /bytes=(\d*)-(\d*)/.exec(req.headers.range || '');
    if (range) {
      const start = range[1] ? Number(range[1]) : 0;
      const end = range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
      if (start > end || start >= size) { res.writeHead(416, { 'Content-Range': `bytes */${size}` }).end(); return; }
      res.writeHead(206, { 'Content-Type': type, 'Content-Range': `bytes ${start}-${end}/${size}`, 'Accept-Ranges': 'bytes', 'Content-Length': end - start + 1 });
      createReadStream(file, { start, end }).pipe(res);
    } else {
      res.writeHead(200, { 'Content-Type': type, 'Accept-Ranges': 'bytes', 'Content-Length': size });
      createReadStream(file).pipe(res);
    }
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

const VIEWPORTS = [
  { name: '1440x900', width: 1440, height: 900 },
  { name: '390x844', width: 390, height: 844 },
];
const THEMES = ['dark', 'light'];
const FREEZE_CSS = '*,*::before,*::after{transition:none!important;animation:none!important;caret-color:transparent!important}';

async function capture(browser, url, vp, theme, file, reducedMotion = 'no-preference') {
  const ctx = await browser.newContext({
    viewport: { width: vp.width, height: vp.height }, deviceScaleFactor: 1,
    colorScheme: theme, reducedMotion,
  });
  await ctx.addInitScript((t) => { try { localStorage.setItem('muninn-theme', t); } catch (e) {} }, theme);
  const errors = [];
  let umamiRequests = 0;
  await ctx.route(UMAMI, (route) => { umamiRequests++; return route.abort(); });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error' && !/umami|ERR_FAILED/i.test(m.text())) errors.push(m.text()); });
  await page.goto(url, { waitUntil: 'load' });
  // Adopted stylesheets are applied by the CSSOM, so this freeze still works
  // when the page is served with style-src 'self' (NEW_URL against nginx).
  await page.evaluate((css) => {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(css);
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  }, FREEZE_CSS);
  // Load lazy stills, wait for fonts, and freeze the hero loop on frame 0 so diffs are deterministic.
  const state = await page.evaluate(async () => {
    for (const img of document.images) img.loading = 'eager';
    await Promise.all([...document.images].map((i) => i.decode().catch(() => {})));
    await document.fonts.ready;
    const v = document.querySelector('[data-hero] video');
    if (v) {
      v.pause();
      if (v.readyState < 2) await new Promise((r) => v.addEventListener('loadeddata', r, { once: true }));
      v.pause();
      await new Promise((r) => { v.addEventListener('seeked', r, { once: true }); v.currentTime = 0; });
    }
    return {
      theme: document.documentElement.getAttribute('data-theme'),
      video: v ? v.getAttribute('src') : null,
      pauseHidden: document.querySelector('.motion-toggle').hidden,
      umamiTags: document.querySelectorAll('script[src*="umami-app.ravenwoodsoftware.dev"]').length,
      title: document.title,
      text: document.body.innerText,
    };
  });
  await page.waitForTimeout(150);
  await page.screenshot({ path: file, fullPage: true, animations: 'disabled' });
  await ctx.close();
  return { ...state, errors, umamiRequests };
}

// Exact compare: a pixel differs if any RGBA byte differs, and size mismatches count as
// differing pixels. The gate is MAX_DIFF_PIXELS (default 0: a 1:1 port must be pixel-identical).
// pixelmatch (threshold 0, anti-aliasing included) only paints the visual diff image.
function diff(aFile, bFile, outFile) {
  const a = PNG.sync.read(readFileSync(aFile));
  const b = PNG.sync.read(readFileSync(bFile));
  const w = Math.max(a.width, b.width), h = Math.max(a.height, b.height);
  const pad = (img) => {
    if (img.width === w && img.height === h) return img;
    const p = new PNG({ width: w, height: h }); p.data.fill(0); // transparent black padding: counts as diff
    PNG.bitblt(img, p, 0, 0, img.width, img.height, 0, 0);
    return p;
  };
  const A = pad(a), B = pad(b), D = new PNG({ width: w, height: h });
  let n = 0;
  for (let i = 0; i < A.data.length; i += 4) {
    if (A.data[i] !== B.data[i] || A.data[i + 1] !== B.data[i + 1] || A.data[i + 2] !== B.data[i + 2] || A.data[i + 3] !== B.data[i + 3]) n++;
  }
  pixelmatch(A.data, B.data, D.data, w, h, { threshold: 0, includeAA: true });
  writeFileSync(outFile, PNG.sync.write(D));
  return { pixels: n, total: w * h, pct: (100 * n) / (w * h), oldSize: `${a.width}x${a.height}`, newSize: `${b.width}x${b.height}`, sameSize: a.width === b.width && a.height === b.height };
}
const within = (d) => d.sameSize && d.pixels <= MAX_DIFF_PIXELS;

const oldServer = await serve(OLD);
const newServer = await serve(NEW);
const oldUrl = `http://127.0.0.1:${oldServer.address().port}/`;
const newUrl = process.env.NEW_URL || `http://127.0.0.1:${newServer.address().port}/`;
console.log(`old ${oldUrl} (${OLD})\nnew ${newUrl} (${process.env.NEW_URL ? "running server" : NEW})`);

const browser = await chromium.launch({ executablePath: CHROME, headless: true, args: ['--autoplay-policy=no-user-gesture-required', '--hide-scrollbars', '--force-color-profile=srgb'] });
const rows = [];
let failed = false;
for (const vp of VIEWPORTS) {
  for (const theme of THEMES) {
    const id = `${vp.name}-${theme}`;
    const o = await capture(browser, oldUrl, vp, theme, join(OUT, 'shots', `old-${id}.png`));
    const n = await capture(browser, newUrl, vp, theme, join(OUT, 'shots', `new-${id}.png`));
    const d = diff(join(OUT, 'shots', `old-${id}.png`), join(OUT, 'shots', `new-${id}.png`), join(OUT, 'shots', `diff-${id}.png`));
    const checks = {
      themeApplied: n.theme === theme,
      textIdentical: o.text === n.text,
      titleIdentical: o.title === n.title,
      videoMounted: !!n.video && !n.pauseHidden,
      umamiTagOnce: n.umamiTags === 1,
      noErrors: n.errors.length === 0,
    };
    const ok = within(d) && Object.values(checks).every(Boolean);
    if (!ok) failed = true;
    rows.push({ id, ...d, checks, newErrors: n.errors, ok });
    console.log(`${ok ? 'PASS' : 'FAIL'} ${id}: ${d.pixels} px (${d.pct.toFixed(4)}% of ${d.total}) old ${d.oldSize} new ${d.newSize} ${JSON.stringify(checks)}`);
  }
}

// Reduced motion: the loop must not mount and the still (poster) stays.
for (const site of [['old', oldUrl], ['new', newUrl]]) {
  const r = await capture(browser, site[1], VIEWPORTS[0], 'dark', join(OUT, 'shots', `${site[0]}-1440x900-dark-reduced.png`), 'reduce');
  if (site[0] === 'new') {
    const ok = r.video === null && r.pauseHidden;
    if (!ok) failed = true;
    rows.push({ id: 'reduced-motion (new)', checks: { noVideo: r.video === null, pauseHidden: r.pauseHidden }, ok });
    console.log(`${ok ? 'PASS' : 'FAIL'} reduced-motion: video=${r.video} pauseHidden=${r.pauseHidden}`);
  }
}
const rd = diff(join(OUT, 'shots', 'old-1440x900-dark-reduced.png'), join(OUT, 'shots', 'new-1440x900-dark-reduced.png'), join(OUT, 'shots', 'diff-1440x900-dark-reduced.png'));
rows.push({ id: '1440x900-dark-reduced', ...rd, ok: within(rd) });
if (!within(rd)) failed = true;
console.log(`${within(rd) ? 'PASS' : 'FAIL'} 1440x900-dark-reduced: ${rd.pixels} px (${rd.pct.toFixed(4)}%)`);

// Theme toggle works and persists (new site).
{
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: 'dark' });
  await ctx.route(UMAMI, (r) => r.abort());
  const page = await ctx.newPage();
  await page.goto(newUrl, { waitUntil: 'load' });
  const before = await page.getAttribute('html', 'data-theme');
  await page.click('.theme-toggle');
  const after = await page.getAttribute('html', 'data-theme');
  await page.reload({ waitUntil: 'load' });
  const reloaded = await page.getAttribute('html', 'data-theme');
  const label = await page.getAttribute('.theme-toggle', 'aria-label');
  const ok = before === 'dark' && after === 'light' && reloaded === 'light' && label === 'Use dark theme';
  if (!ok) failed = true;
  rows.push({ id: 'theme-toggle (new)', checks: { before, after, reloaded, label }, ok });
  console.log(`${ok ? 'PASS' : 'FAIL'} theme toggle: ${before} -> ${after}, after reload ${reloaded}, label "${label}"`);
  await ctx.close();
}
await browser.close();

// Optional Lighthouse on the new build (Umami blocked so no pageview is recorded).
// Runs the lighthouse devDependency pinned in package-lock.json (npx --no-install never fetches).
let lighthouse = null;
if (process.env.LIGHTHOUSE === '1') {
  lighthouse = {};
  for (const [name, preset] of [['mobile', null], ['desktop', 'desktop']]) {
    const json = join(OUT, `lighthouse-${name}.json`);
    const lhArgs = ['--no-install', 'lighthouse', newUrl, '--no-enable-error-reporting', '--quiet', '--output=json', `--output-path=${json}`,
      '--only-categories=performance,accessibility,best-practices,seo',
      '--blocked-url-patterns=*umami-app.ravenwoodsoftware.dev*',
      '--chrome-flags=--headless=new --no-sandbox --hide-scrollbars'];
    if (preset) lhArgs.push(`--preset=${preset}`);
    // Async spawn: the static servers live in this process, so the event loop must stay free.
    const r = await new Promise((ok) => spawn('npx', lhArgs, { stdio: 'inherit', env: { ...process.env } }).on('close', (status) => ok({ status })));
    if (r.status === 0 && existsSync(json)) {
      const cats = JSON.parse(readFileSync(json, 'utf8')).categories;
      lighthouse[name] = Object.fromEntries(Object.entries(cats).map(([k, v]) => [k, Math.round(v.score * 100)]));
      console.log(`lighthouse ${name}: ${JSON.stringify(lighthouse[name])}`);
    } else {
      lighthouse[name] = `failed (exit ${r.status})`;
    }
  }
}

oldServer.close();
newServer.close();
writeFileSync(join(OUT, 'results.json'), JSON.stringify({ maxDiffPixels: MAX_DIFF_PIXELS, rows, lighthouse }, null, 2));
console.log(failed ? 'W1 VERIFY: FAIL' : 'W1 VERIFY: PASS');
process.exit(failed ? 1 : 0);
