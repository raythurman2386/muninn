#!/usr/bin/env node
// Generate and check the nginx CSP header from the inline scripts in dist/.
//
//   node scripts/csp.mjs generate   Write nginx/security-headers.conf from the
//                                   template and the current dist/ HTML.
//   node scripts/csp.mjs check      Assert every inline script in dist/ has a
//                                   sha256 in the generated header.
//   node scripts/csp.mjs prove      Run the check, then change one byte of an
//                                   inline script and assert the check fails.
//   node scripts/csp.mjs check --url http://127.0.0.1:8080
//                                   Same assertion against a running server
//                                   (fetches /, /privacy/ and a 404).
//
// Hashes are the CSP SHA-256 of the exact bytes between <script> and
// </script> for every script tag that has no src attribute.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const DIST = process.env.SITE_DIST ? resolveEnvPath(process.env.SITE_DIST) : join(SITE_DIR, 'dist');
const TEMPLATE = join(SITE_DIR, 'nginx', 'security-headers.conf.in');
const OUTPUT = process.env.CSP_OUT ? resolveEnvPath(process.env.CSP_OUT) : join(SITE_DIR, 'nginx', 'security-headers.conf');
const TURNSTILE_ORIGIN = 'https://challenges.cloudflare.com';

function resolveEnvPath(p) {
  return p.startsWith('/') ? p : join(SITE_DIR, p);
}

export function waitlistEnabled(env = process.env) {
  return env.PUBLIC_WAITLIST_ENABLED === 'true';
}

export function endpointOrigin(url = process.env.PUBLIC_WAITLIST_ENDPOINT) {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

// Extra CSP sources used only when PUBLIC_WAITLIST_ENABLED=true. The default
// (flag-off) generate() path leaves the template output byte-identical to main.
export function cspExtras(env = process.env) {
  const empty = { scriptSrc: [], connectSrc: [], formAction: [], frameSrc: [] };
  if (!waitlistEnabled(env)) return empty;
  const scriptSrc = [];
  const connectSrc = [];
  const formAction = [];
  const frameSrc = [];
  const origin = endpointOrigin(env.PUBLIC_WAITLIST_ENDPOINT);
  if (origin) {
    connectSrc.push(origin);
    formAction.push(origin);
  }
  if ((env.PUBLIC_TURNSTILE_SITEKEY || '').trim()) {
    scriptSrc.push(TURNSTILE_ORIGIN);
    connectSrc.push(TURNSTILE_ORIGIN);
    frameSrc.push(TURNSTILE_ORIGIN);
  }
  return { scriptSrc, connectSrc, formAction, frameSrc };
}

function mergeSources(existing, extras) {
  const have = new Set(existing.trim().split(/\s+/).filter(Boolean));
  for (const src of extras) have.add(src);
  const self = [];
  const rest = [];
  for (const src of have) {
    if (src === "'self'") self.push(src);
    else rest.push(src);
  }
  rest.sort();
  return [...self, ...rest].join(' ');
}

export function applyWaitlistCsp(csp, env = process.env) {
  if (!waitlistEnabled(env)) return csp;
  const extras = cspExtras(env);
  const dirs = [
    ['script-src', extras.scriptSrc],
    ['connect-src', extras.connectSrc],
    ['form-action', extras.formAction],
    ['frame-src', extras.frameSrc],
  ];
  let out = csp;
  for (const [name, add] of dirs) {
    if (!add.length) continue;
    const re = new RegExp(`(; |^)${name} ([^;]+)`);
    const m = out.match(re);
    if (m) {
      out = out.replace(re, `${m[1]}${name} ${mergeSources(m[2], add)}`);
    } else if (out.includes('upgrade-insecure-requests')) {
      out = out.replace('upgrade-insecure-requests', `${name} ${mergeSources('', add)}; upgrade-insecure-requests`);
    } else {
      out = `${out}; ${name} ${mergeSources('', add)}`;
    }
  }
  return out;
}

const argv = process.argv.slice(2);
const cmd = argv.find((a) => !a.startsWith('-')) || 'generate';
const urlFlag = argv.indexOf('--url');
const baseUrl = urlFlag >= 0 ? argv[urlFlag + 1]?.replace(/\/$/, '') : null;

export function scriptHash(body) {
  return 'sha256-' + createHash('sha256').update(body, 'utf8').digest('base64');
}

// Script tags with a src are external and are not hashed. The body is the
// exact source text (including leading newlines and indentation).
export function inlineScripts(html) {
  const out = [];
  const re = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(html))) {
    if (/\bsrc\s*=/.test(m[1])) continue;
    out.push(m[2]);
  }
  return out;
}

function htmlFiles(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) htmlFiles(p, acc);
    else if (name.endsWith('.html')) acc.push(p);
  }
  return acc;
}

export function hashesFromHtml(html) {
  return [...new Set(inlineScripts(html).map(scriptHash))].sort();
}

export function hashesFromDist(dist = DIST) {
  if (!existsSync(dist)) throw new Error(`no dist/ at ${dist}; run npm run build first`);
  const found = new Set();
  const files = htmlFiles(dist);
  if (files.length === 0) throw new Error(`no HTML in ${dist}`);
  for (const file of files) {
    for (const h of hashesFromHtml(readFileSync(file, 'utf8'))) found.add(h);
  }
  return [...found].sort();
}

export function parseCspFromNginx(text) {
  const m = text.match(/add_header\s+Content-Security-Policy\s+"([^"]+)"/i);
  if (!m) throw new Error('no Content-Security-Policy in the nginx snippet');
  return m[1];
}

export function hashesInCsp(csp) {
  return [...csp.matchAll(/'sha256-[A-Za-z0-9+/=]+'/g)].map((m) => m[0].slice(1, -1));
}

export function generate(dist = DIST, outFile = OUTPUT) {
  const template = readFileSync(TEMPLATE, 'utf8');
  if (!template.includes('__SCRIPT_HASHES__')) {
    throw new Error(`${TEMPLATE} is missing the __SCRIPT_HASHES__ placeholder`);
  }
  const hashes = hashesFromDist(dist);
  const filler = hashes.length ? hashes.map((h) => `'${h}'`).join(' ') + ' ' : '';
  let text = template.replace('__SCRIPT_HASHES__', filler);
  const policy = parseCspFromNginx(text);
  const next = applyWaitlistCsp(policy);
  if (next !== policy) text = text.replace(policy, next);
  mkdirSync(dirname(outFile), { recursive: true });
  writeFileSync(outFile, text);
  console.log(`csp: wrote ${outFile} with ${hashes.length} inline hash(es): ${hashes.join(' ') || '(none)'}`);
  return { hashes, text };
}

function missingHashes(needed, csp) {
  const have = new Set(hashesInCsp(csp));
  return needed.filter((h) => !have.has(h));
}

export function checkDist(dist = DIST, cspText = null) {
  const csp = cspText ?? parseCspFromNginx(readFileSync(OUTPUT, 'utf8'));
  const needed = hashesFromDist(dist);
  const missing = missingHashes(needed, csp);
  if (missing.length) {
    throw new Error(`CSP is missing hash(es) for inline scripts: ${missing.join(', ')}`);
  }
  if (needed.length === 0) {
    throw new Error('dist/ has no inline scripts to hash; the theme script should be one');
  }
  console.log(`csp check: PASS (${needed.length} inline hash(es) present in the header)`);
  return needed;
}

async function fetchText(url) {
  const res = await fetch(url, { redirect: 'follow' });
  const headers = {};
  res.headers.forEach((v, k) => { headers[k.toLowerCase()] = v; });
  return { status: res.status, headers, body: await res.text() };
}

export async function checkUrl(base) {
  const pages = [base + '/', base + '/privacy/', base + '/definitely-not-here'];
  let csp = null;
  const needed = new Set();
  for (const url of pages) {
    const r = await fetchText(url);
    const header = r.headers['content-security-policy'];
    if (!header) throw new Error(`${url} has no Content-Security-Policy header`);
    if (!csp) csp = header;
    else if (header !== csp) throw new Error(`CSP header differs between pages (${url})`);
    for (const h of hashesFromHtml(r.body)) needed.add(h);
  }
  const missing = missingHashes([...needed].sort(), csp);
  if (missing.length) {
    throw new Error(`served CSP is missing hash(es): ${missing.join(', ')}`);
  }
  if (needed.size === 0) throw new Error('served HTML has no inline scripts to hash');
  console.log(`csp check ${base}: PASS (${needed.size} inline hash(es) present in the header)`);
}

export function prove(dist = DIST) {
  if (!existsSync(OUTPUT)) generate(dist, OUTPUT);
  checkDist(dist);
  const src = join(dist, 'index.html');
  const html = readFileSync(src, 'utf8');
  const scripts = inlineScripts(html);
  if (!scripts.length) throw new Error('no inline script in dist/index.html to mutate');
  const original = scripts[0];
  // One-byte change: insert a space in the first line of the script body.
  const mutatedBody = original.replace(/^(\s*)/, '$1 ');
  if (mutatedBody === original) throw new Error('could not apply a one-byte mutation');
  const mutatedHtml = html.replace(original, mutatedBody);
  const scratch = mkdtempSync(join(existsSync(join(SITE_DIR, '.astro')) ? join(SITE_DIR, '.astro') : tmpdir(), 'csp-prove-'));
  try {
    writeFileSync(join(scratch, 'index.html'), mutatedHtml);
    let failed = false;
    let message = '';
    try {
      checkDist(scratch);
    } catch (e) {
      failed = true;
      message = String(e.message || e);
    }
    if (!failed) {
      throw new Error('csp prove: FAIL (a one-byte change to the inline script was not detected)');
    }
    console.log(`csp prove: PASS (one-byte mutation was rejected: ${message})`);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

const commands = { generate, check: async () => {
  if (baseUrl) await checkUrl(baseUrl);
  else checkDist();
}, prove };

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const fn = commands[cmd];
  if (!fn) {
    console.error(`usage: node scripts/csp.mjs generate|check|prove [--url URL]`);
    process.exit(2);
  }
  try {
    await fn();
  } catch (e) {
    console.error(`csp ${cmd}: FAIL: ${e.message || e}`);
    process.exit(1);
  }
}
