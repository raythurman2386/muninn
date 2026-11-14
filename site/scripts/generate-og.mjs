#!/usr/bin/env node
// Generate the 1200x630 Open Graph images from the official logo and the
// committed Inter path SVGs in scripts/og-text/. Do not hand-edit the PNGs;
// re-run this script instead.
//
//   cd site && npm ci && npm run generate:og
//
// sharp is declared as a devDependency and is available after npm ci.
// If the OG copy changes, first run scripts/generate-og-text.py (needs
// fontTools and a variable Inter TTF; see that script's header).
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const REPO_DIR = dirname(SITE_DIR);
const LOGO = join(REPO_DIR, 'assets', 'logo.svg');
const TEXT_DIR = join(SITE_DIR, 'scripts', 'og-text');
const PUBLIC = join(SITE_DIR, 'public');
const OG_DIR = join(PUBLIC, 'assets', 'og');

const WIDTH = 1200;
const HEIGHT = 630;
const LOGO_SIZE = 256;
const OG_LEFT = 120;
const OG_GAP = 48;
const BACKGROUND = { r: 8, g: 8, b: 8, alpha: 1 };

// Home is written to the W3 default path so Base.astro's ogImage default
// (`/assets/og.png`) picks it up. Every other page uses `/assets/og/<slug>.png`.
const PAGES = [
  { slug: 'home', text: 'home.svg', dest: join(PUBLIC, 'assets', 'og.png') },
  { slug: 'features', text: 'features.svg', dest: join(OG_DIR, 'features.png') },
  { slug: 'download', text: 'download.svg', dest: join(OG_DIR, 'download.png') },
  { slug: 'docs', text: 'docs.svg', dest: join(OG_DIR, 'docs.png') },
  { slug: 'changelog', text: 'changelog.svg', dest: join(OG_DIR, 'changelog.png') },
  { slug: 'privacy', text: 'privacy.svg', dest: join(OG_DIR, 'privacy.png') },
  { slug: 'sponsor', text: 'sponsor.svg', dest: join(OG_DIR, 'sponsor.png') },
  { slug: 'press', text: 'press.svg', dest: join(OG_DIR, 'press.png') },
];

mkdirSync(join(PUBLIC, 'assets'), { recursive: true });
mkdirSync(OG_DIR, { recursive: true });

const logoSvg = readFileSync(LOGO);
const logoOg = await sharp(logoSvg, { density: 384 }).resize(LOGO_SIZE, LOGO_SIZE).png().toBuffer();
const ogLogoTop = Math.round((HEIGHT - LOGO_SIZE) / 2);
const ogTextLeft = OG_LEFT + LOGO_SIZE + OG_GAP;

for (const page of PAGES) {
  const wordmark = await sharp(join(TEXT_DIR, page.text)).png().toBuffer();
  const meta = await sharp(wordmark).metadata();
  const textTop = Math.round((HEIGHT - (meta.height || 130)) / 2);
  const textWidth = meta.width || 0;
  if (ogTextLeft + textWidth > WIDTH - 80) {
    throw new Error(`${page.slug} wordmark is too wide (${textWidth}px)`);
  }
  await sharp({ create: { width: WIDTH, height: HEIGHT, channels: 4, background: BACKGROUND } })
    .composite([
      { input: logoOg, left: OG_LEFT, top: ogLogoTop },
      { input: wordmark, left: ogTextLeft, top: textTop },
    ])
    .png({ compressionLevel: 9 })
    .toFile(page.dest);
  const outMeta = await sharp(page.dest).metadata();
  if (outMeta.width !== WIDTH || outMeta.height !== HEIGHT) {
    throw new Error(`${page.dest} is ${outMeta.width}x${outMeta.height}, expected ${WIDTH}x${HEIGHT}`);
  }
  const bytes = readFileSync(page.dest).length;
  const rel = page.dest.startsWith(PUBLIC + '/') ? page.dest.slice(PUBLIC.length + 1) : page.dest;
  console.log(`wrote public/${rel} (${outMeta.width}x${outMeta.height}, ${bytes} bytes)`);
}
