#!/usr/bin/env node
// W9 proof.
//
//   MAIN_DIST=../path/to/main/site/dist node scripts/verify-w9.mjs
//
// Flag-off (the default `npm run build`): every HTML page in MAIN_DIST is
// byte-identical, CSP is byte-identical, and /waitlist/ is not emitted.
// Flag-on (proof only, never deployed): a local mock of the muninn-waitlist
// API is used to exercise success, 409, validation and error paths, plus
// the honeypot, the status region, and Lighthouse 100s on /waitlist/.
//
// Umami is blocked so no pageview is recorded.
import { createServer } from 'node:http';
import {
  createReadStream, existsSync, mkdirSync, readFileSync, readdirSync,
  statSync, writeFileSync,
} from 'node:fs';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { applyWaitlistCsp, parseCspFromNginx } from './csp.mjs';

const SITE_DIR = resolve(fileURLToPath(new URL('../package.json', import.meta.url)), '..');
const DIST_OFF = resolve(process.env.DIST_OFF || join(SITE_DIR, 'dist'));
const DIST_ON = resolve(process.env.DIST_ON || join(SITE_DIR, 'dist-on'));
const MAIN_DIST = process.env.MAIN_DIST ? resolve(process.env.MAIN_DIST) : null;
const MAIN_CSP = process.env.MAIN_CSP ? resolve(process.env.MAIN_CSP) : (MAIN_DIST ? resolve(MAIN_DIST, '../nginx/security-headers.conf') : null);
const OUT = resolve(process.env.PROOF_DIR || join(SITE_DIR, '.proof-w9'));
const CHROME = process.env.CHROME_PATH || '/usr/bin/google-chrome';
const RUN_LH = (process.env.LIGHTHOUSE ?? '1') !== '0';
const UMAMI = /umami-app\.ravenwoodsoftware\.dev/;
const DUMMY_SITEKEY = '00000000-0000-0000-0000-000000000000';
mkdirSync(OUT, { recursive: true });

let failed = false;
const rows = [];
function record(id, ok, detail = {}) {
  if (!ok) failed = true;
  rows.push({ id, ok, ...detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${id}${detail.note ? ': ' + detail.note : ''}`);
}

function listFiles(root, acc = [], prefix = '') {
  for (const name of readdirSync(root)) {
    const rel = prefix ? `${prefix}/${name}` : name;
    const p = join(root, name);
    if (statSync(p).isDirectory()) listFiles(p, acc, rel);
    else acc.push(rel);
  }
  return acc;
}

function byteIdentical(a, b) {
  return existsSync(a) && existsSync(b) && readFileSync(a).equals(readFileSync(b));
}

// --- Flag-off identity vs main -------------------------------------------
if (MAIN_DIST) {
  if (!existsSync(DIST_OFF)) throw new Error(`no flag-off dist at ${DIST_OFF}`);
  const oldFiles = listFiles(MAIN_DIST).sort();
  const newFiles = listFiles(DIST_OFF).sort();
  record('flag-off file list', oldFiles.join('\n') === newFiles.join('\n'), {
    note: oldFiles.join('\n') === newFiles.join('\n') ? `${oldFiles.length} files` : `old=${oldFiles.length} new=${newFiles.length}`,
  });
  let htmlDiffs = 0;
  let otherDiffs = 0;
  for (const rel of oldFiles) {
    const same = byteIdentical(join(MAIN_DIST, rel), join(DIST_OFF, rel));
    if (same) continue;
    if (rel.endsWith('.html')) htmlDiffs++;
    else otherDiffs++;
    record(`flag-off identical ${rel}`, false);
  }
  if (htmlDiffs === 0 && otherDiffs === 0) {
    record('flag-off every page byte-identical', true, { note: `${oldFiles.filter((f) => f.endsWith('.html')).length} html files` });
  }
  record('flag-off has no /waitlist/', !existsSync(join(DIST_OFF, 'waitlist')), {});
  const offCsp = join(SITE_DIR, 'nginx', 'security-headers.conf');
  if (MAIN_CSP && existsSync(MAIN_CSP) && existsSync(offCsp)) {
    record('flag-off CSP byte-identical', byteIdentical(MAIN_CSP, offCsp));
  } else {
    record('flag-off CSP present', existsSync(offCsp));
  }
} else {
  console.log('skip flag-off vs main (set MAIN_DIST)');
}

// --- CSP unit checks (no keys, no network) --------------------------------
{
  const template = readFileSync(join(SITE_DIR, 'nginx', 'security-headers.conf.in'), 'utf8');
  const policy = 'default-src \'self\'; script-src \'self\' \'sha256-AAA\' https://umami-app.ravenwoodsoftware.dev; style-src \'self\'; img-src \'self\'; font-src \'self\'; media-src \'self\'; connect-src \'self\' https://umami-app.ravenwoodsoftware.dev; manifest-src \'self\'; base-uri \'self\'; form-action \'self\'; frame-ancestors \'self\'; object-src \'none\'; upgrade-insecure-requests';
  const off = applyWaitlistCsp(policy, {});
  record('CSP unchanged when flag off', off === policy);
  const on = applyWaitlistCsp(policy, {
    PUBLIC_WAITLIST_ENABLED: 'true',
    PUBLIC_WAITLIST_ENDPOINT: 'http://127.0.0.1:8765/api/waitlist',
  });
  record('CSP flag-on adds endpoint to connect-src and form-action',
    on.includes('connect-src') && on.includes('http://127.0.0.1:8765')
    && on.includes('form-action \'self\' http://127.0.0.1:8765')
    && !on.includes('challenges.cloudflare.com'));
  const ts = applyWaitlistCsp(policy, {
    PUBLIC_WAITLIST_ENABLED: 'true',
    PUBLIC_WAITLIST_ENDPOINT: 'https://waitlist.example.test/api/waitlist',
    PUBLIC_TURNSTILE_SITEKEY: DUMMY_SITEKEY,
  });
  record('CSP flag-on with site key adds Turnstile origins',
    ts.includes('https://challenges.cloudflare.com')
    && ts.includes('frame-src https://challenges.cloudflare.com')
    && ts.includes('https://waitlist.example.test'));
  record('CSP template is unchanged on disk',
    template.includes('form-action \'self\'') && !template.includes('challenges.cloudflare.com') && !template.includes('PUBLIC_WAITLIST'));
}

// --- Mock proxy + flag-on browser tests ----------------------------------
const TYPES = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript',
  '.svg': 'image/svg+xml', '.webp': 'image/webp', '.png': 'image/png', '.mp4': 'video/mp4',
  '.woff2': 'font/woff2', '.txt': 'text/plain', '.json': 'application/json',
  '.xml': 'application/xml', '.ico': 'image/x-icon', '.webmanifest': 'application/manifest+json',
};

function serveDist(root) {
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
    if (!existsSync(file)) {
      const fb = join(root, '404.html');
      if (existsSync(fb)) { res.writeHead(404, { 'Content-Type': 'text/html; charset=utf-8' }); createReadStream(fb).pipe(res); return; }
      res.writeHead(404).end('not found'); return;
    }
    res.writeHead(200, { 'Content-Type': TYPES[extname(file)] || 'application/octet-stream' });
    createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

function startMock() {
  const server = createServer((req, res) => {
    const origin = req.headers.origin;
    if (origin) {
      res.setHeader('Access-Control-Allow-Origin', origin);
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
      res.setHeader('Vary', 'Origin');
    }
    if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }
    if (req.url !== '/api/waitlist' || req.method !== 'POST') {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: 'bad_request' }));
      return;
    }
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      let body = {};
      try { body = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}'); } catch { body = {}; }
      const write = (status, payload) => {
        res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
        res.end(JSON.stringify(payload) + '\n');
      };
      if (body.website) return write(200, { ok: true });
      const email = (body.email || '').toString();
      if (email.startsWith('bad@')) return write(400, { ok: false, error: 'invalid_email' });
      if (email.startsWith('rate@')) return write(429, { ok: false, error: 'rate_limited' });
      if (email.startsWith('off@')) return write(503, { ok: false, error: 'disabled' });
      if (email.startsWith('down@')) return write(502, { ok: false, error: 'unavailable' });
      if (email.startsWith('captcha@')) return write(400, { ok: false, error: 'captcha' });
      if (email.startsWith('exists@')) return write(409, { ok: true });
      if (!email.includes('@')) return write(400, { ok: false, error: 'invalid_email' });
      return write(200, { ok: true });
    });
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

async function buildFlagOn(endpoint, outDir, sitekey = '') {
  mkdirSync(outDir, { recursive: true });
  const env = {
    ...process.env,
    ASTRO_TELEMETRY_DISABLED: '1',
    PUBLIC_WAITLIST_ENABLED: 'true',
    PUBLIC_WAITLIST_ENDPOINT: endpoint,
    SITE_DIST: outDir,
    CSP_OUT: join(outDir, 'security-headers.conf'),
  };
  if (sitekey) env.PUBLIC_TURNSTILE_SITEKEY = sitekey;
  else delete env.PUBLIC_TURNSTILE_SITEKEY;
  const astro = await new Promise((ok) => spawn('npx', ['--no-install', 'astro', 'build', '--outDir', outDir], {
    cwd: SITE_DIR, env, stdio: 'inherit',
  }).on('close', (status) => ok(status)));
  if (astro !== 0) throw new Error(`flag-on astro build exited ${astro}`);
  const csp = await new Promise((ok) => spawn('node', ['scripts/csp.mjs', 'generate'], {
    cwd: SITE_DIR, env, stdio: 'inherit',
  }).on('close', (status) => ok(status)));
  if (csp !== 0) throw new Error(`flag-on csp generate exited ${csp}`);
}

if (existsSync(DIST_ON) || process.env.VERIFY_W9_ON !== '0') {
  const mock = await startMock();
  const endpoint = `http://127.0.0.1:${mock.address().port}/api/waitlist`;
  console.log(`mock waitlist at ${endpoint}`);
  await buildFlagOn(endpoint, DIST_ON);

  record('flag-on emits /waitlist/', existsSync(join(DIST_ON, 'waitlist', 'index.html')));
  record('flag-on emits /waitlist/check-email/', existsSync(join(DIST_ON, 'waitlist', 'check-email', 'index.html')));
  record('flag-on emits /waitlist/confirmed/', existsSync(join(DIST_ON, 'waitlist', 'confirmed', 'index.html')));

  const formHtml = readFileSync(join(DIST_ON, 'waitlist', 'index.html'), 'utf8');
  record('flag-on form is email-only',
    /name="email"/.test(formHtml) && /type="email"/.test(formHtml) && !/name="name"/.test(formHtml));
  record('flag-on form has honeypot website', /name="website"/.test(formHtml));
  record('flag-on form posts to the proxy', formHtml.includes(`action="${endpoint}"`));
  record('flag-on form has an accessible status region',
    /role="status"/.test(formHtml) && /aria-live="polite"/.test(formHtml));
  record('flag-on page says Early access', /Early access/.test(formHtml));
  record('flag-on has no Turnstile widget without a site key',
    !/challenges\.cloudflare\.com/.test(formHtml) && !/cf-turnstile/.test(formHtml));
  record('flag-on Early access nav is present', formHtml.includes('href="/waitlist/"'));

  const onCsp = parseCspFromNginx(readFileSync(join(DIST_ON, 'security-headers.conf'), 'utf8'));
  const origin = new URL(endpoint).origin;
  record('flag-on generated CSP lists the endpoint',
    onCsp.includes(`connect-src 'self' ${origin}`) || onCsp.includes(origin));
  record('flag-on generated CSP form-action lists the endpoint', onCsp.includes(origin) && onCsp.includes('form-action'));
  record('flag-on generated CSP has no Turnstile origin without a site key',
    !onCsp.includes('challenges.cloudflare.com'));

  const distServer = await serveDist(DIST_ON);
  const base = `http://127.0.0.1:${distServer.address().port}/`;
  const browser = await chromium.launch({
    executablePath: CHROME, headless: true,
    args: ['--hide-scrollbars', '--force-color-profile=srgb'],
  });

  async function openForm() {
    const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: 'dark' });
    await ctx.route(UMAMI, (route) => route.abort());
    const errors = [];
    const page = await ctx.newPage();
    page.on('pageerror', (e) => errors.push(String(e)));
    page.on('console', (m) => {
      if (m.type() !== 'error') return;
      if (/umami|ERR_FAILED|net::ERR_BLOCKED_BY_CLIENT/i.test(m.text())) return;
      errors.push(m.text());
    });
    await page.goto(new URL('waitlist/', base).href, { waitUntil: 'load' });
    await page.evaluate(async () => { await document.fonts.ready; });
    return { ctx, page, errors };
  }

  async function submitEmail(page, email, { honeypot = '' } = {}) {
    if (honeypot) await page.fill('input[name="website"]', honeypot);
    await page.fill('#waitlist-email', email);
    await page.click('button[type="submit"]');
    await page.waitForFunction(() => {
      const el = document.querySelector('[data-waitlist-status]');
      return el && !el.hidden && el.textContent && el.textContent !== 'Sending…';
    }, null, { timeout: 5000 });
    return page.evaluate(() => {
      const el = document.querySelector('[data-waitlist-status]');
      return { text: el?.textContent || '', state: el?.dataset.state || '', hidden: el?.hidden ?? true };
    });
  }

  {
    const { ctx, page, errors } = await openForm();
    const shots = join(OUT, 'shots');
    mkdirSync(shots, { recursive: true });
    await page.screenshot({ path: join(shots, 'waitlist-1440-dark.png'), fullPage: true, animations: 'disabled' });
    const status = await page.evaluate(() => {
      const el = document.querySelector('[data-waitlist-status]');
      return {
        role: el?.getAttribute('role'),
        live: el?.getAttribute('aria-live'),
        name: document.querySelector('input[name="name"]') ? true : false,
        emailType: document.querySelector('#waitlist-email')?.getAttribute('type'),
        heading: document.querySelector('h1')?.textContent,
      };
    });
    record('flag-on heading is Early access', status.heading === 'Early access');
    record('flag-on no name field in the DOM', status.name === false);
    record('flag-on email input type', status.emailType === 'email');
    record('flag-on status is role=status aria-live=polite', status.role === 'status' && status.live === 'polite');
    const ok = await submitEmail(page, 'ok@example.com');
    record('flag-on success status', ok.state === 'success' && /Check your email/.test(ok.text), { note: ok.text });
    record('flag-on success has no page errors', errors.length === 0, { errors });
    await ctx.close();
  }

  const cases = [
    ['exists@example.com', 'success', /Check your email/, '409 already-exists is success'],
    ['bad@example.com', 'error', /does not look valid/, 'invalid_email'],
    ['rate@example.com', 'error', /Too many attempts/, 'rate_limited'],
    ['off@example.com', 'error', /not open right now/, 'disabled'],
    ['down@example.com', 'error', /unavailable/, 'unavailable'],
    ['captcha@example.com', 'error', /check could not be completed/, 'captcha'],
  ];
  for (const [email, state, pattern, id] of cases) {
    const { ctx, page, errors } = await openForm();
    const got = await submitEmail(page, email);
    record(`flag-on ${id}`, got.state === state && pattern.test(got.text), { note: got.text, errors });
    await ctx.close();
  }
  {
    const { ctx, page } = await openForm();
    const got = await submitEmail(page, 'bot@example.com', { honeypot: 'http://spam.example' });
    record('flag-on honeypot looks like success', got.state === 'success' && /Check your email/.test(got.text), { note: got.text });
    await ctx.close();
  }

  if (RUN_LH) {
    const url = new URL('waitlist/', base).href;
    for (const [name, preset] of [['mobile', null], ['desktop', 'desktop']]) {
      const json = join(OUT, `lighthouse-waitlist-${name}.json`);
      const lhArgs = ['--no-install', 'lighthouse', url, '--no-enable-error-reporting', '--quiet', '--output=json', `--output-path=${json}`,
        '--only-categories=performance,accessibility,best-practices,seo',
        '--blocked-url-patterns=*umami-app.ravenwoodsoftware.dev*',
        '--chrome-flags=--headless=new --no-sandbox --hide-scrollbars'];
      if (preset) lhArgs.push(`--preset=${preset}`);
      const r = await new Promise((ok) => spawn('npx', lhArgs, { stdio: 'inherit', env: { ...process.env }, cwd: SITE_DIR }).on('close', (status) => ok(status)));
      if (r === 0 && existsSync(json)) {
        const cats = JSON.parse(readFileSync(json, 'utf8')).categories;
        const scores = Object.fromEntries(Object.entries(cats).map(([k, v]) => [k, Math.round(v.score * 100)]));
        const all100 = Object.values(scores).every((s) => s === 100);
        record(`flag-on lighthouse /waitlist/ ${name}`, all100, { note: JSON.stringify(scores) });
      } else {
        record(`flag-on lighthouse /waitlist/ ${name}`, false, { note: `exit ${r}` });
      }
    }
  }

  await browser.close();
  distServer.close();
  mock.close();

  // Second flag-on build with a dummy (non-real) site key, to prove the widget
  // and CSP additions are emitted only when the key is provided at build time.
  const distTs = resolve(process.env.DIST_ON_TS || join(SITE_DIR, 'dist-on-turnstile'));
  await buildFlagOn('https://waitlist.example.test/api/waitlist', distTs, DUMMY_SITEKEY);
  const tsHtml = readFileSync(join(distTs, 'waitlist', 'index.html'), 'utf8');
  record('flag-on with site key emits the Turnstile script and widget',
    tsHtml.includes('https://challenges.cloudflare.com/turnstile/v0/api.js')
    && tsHtml.includes('data-sitekey="' + DUMMY_SITEKEY + '"'));
  const tsCsp = parseCspFromNginx(readFileSync(join(distTs, 'security-headers.conf'), 'utf8'));
  record('flag-on with site key CSP lists challenges.cloudflare.com',
    tsCsp.includes('https://challenges.cloudflare.com') && /frame-src/.test(tsCsp));
}

writeFileSync(join(OUT, 'w9-results.json'), JSON.stringify({ rows, failed }, null, 2));
console.log(failed ? 'W9 VERIFY: FAIL' : 'W9 VERIFY: PASS');
process.exit(failed ? 1 : 0);
