#!/usr/bin/env node
// Generate the favicon set, apple-touch-icon, web manifest and the default
// 1200x630 OG image from the official assets/logo.svg. Do not hand-edit the
// output files; re-run this script instead.
//
//   cd site && npm ci && npm run generate:icons
//
// sharp is declared as a devDependency and is available after npm ci. The wordmark
// on the OG image is scripts/og-wordmark.svg (Inter, converted to paths).
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const SITE_DIR = dirname(fileURLToPath(new URL('../package.json', import.meta.url)));
const REPO_DIR = dirname(SITE_DIR);
const LOGO = join(REPO_DIR, 'assets', 'logo.svg');
const WORDMARK = join(SITE_DIR, 'scripts', 'og-wordmark.svg');
const PUBLIC = join(SITE_DIR, 'public');

mkdirSync(join(PUBLIC, 'assets'), { recursive: true });

const logoSvg = readFileSync(LOGO);

function png(size) {
  return sharp(logoSvg, { density: Math.max(72, size * 2) }).resize(size, size).png({ compressionLevel: 9 }).toBuffer();
}

// ICO is a short directory of images; modern entries may embed a PNG payload.
function encodeIco(images) {
  const count = images.length;
  const header = 6 + 16 * count;
  let offset = header;
  const entries = images.map((buf, i) => {
    const size = images.sizes[i];
    const entry = { size, buf, offset };
    offset += buf.length;
    return entry;
  });
  const out = Buffer.alloc(offset);
  out.writeUInt16LE(0, 0);
  out.writeUInt16LE(1, 2);
  out.writeUInt16LE(count, 4);
  let p = 6;
  for (const e of entries) {
    out.writeUInt8(e.size >= 256 ? 0 : e.size, p);
    out.writeUInt8(e.size >= 256 ? 0 : e.size, p + 1);
    out.writeUInt8(0, p + 2);
    out.writeUInt8(0, p + 3);
    out.writeUInt16LE(1, p + 4);
    out.writeUInt16LE(32, p + 6);
    out.writeUInt32LE(e.buf.length, p + 8);
    out.writeUInt32LE(e.offset, p + 12);
    p += 16;
    e.buf.copy(out, e.offset);
  }
  return out;
}

const sizes = {
  'favicon-16x16.png': 16,
  'favicon-32x32.png': 32,
  'icon-192x192.png': 192,
  'icon-512x512.png': 512,
  'apple-touch-icon.png': 180,
};

const written = [];
for (const [name, size] of Object.entries(sizes)) {
  const buf = await png(size);
  const dest = join(PUBLIC, name);
  writeFileSync(dest, buf);
  written.push({ name, bytes: buf.length, size });
}

const ico16 = await png(16);
const ico32 = await png(32);
const ico48 = await png(48);
ico16.sizes = undefined;
const icoImages = [ico16, ico32, ico48];
icoImages.sizes = [16, 32, 48];
const ico = encodeIco(icoImages);
writeFileSync(join(PUBLIC, 'favicon.ico'), ico);
written.push({ name: 'favicon.ico', bytes: ico.length, size: '16/32/48' });

copyFileSync(LOGO, join(PUBLIC, 'favicon.svg'));
written.push({ name: 'favicon.svg', bytes: logoSvg.length, size: 'svg' });

const manifest = {
  name: 'Muninn',
  short_name: 'Muninn',
  description: 'A local-first Markdown vault with [[wikilinks]] and a knowledge graph.',
  start_url: '/',
  display: 'browser',
  background_color: '#080808',
  theme_color: '#080808',
  icons: [
    { src: '/favicon.svg', type: 'image/svg+xml', sizes: 'any' },
    { src: '/icon-192x192.png', type: 'image/png', sizes: '192x192' },
    { src: '/icon-512x512.png', type: 'image/png', sizes: '512x512' },
  ],
};
const manifestJson = `${JSON.stringify(manifest, null, 2)}\n`;
writeFileSync(join(PUBLIC, 'site.webmanifest'), manifestJson);
written.push({ name: 'site.webmanifest', bytes: manifestJson.length, size: 'manifest' });

// Default OG image: dark canvas, official logo, Inter wordmark (as paths).
const logoOg = await sharp(logoSvg, { density: 384 }).resize(256, 256).png().toBuffer();
const wordmark = await sharp(WORDMARK).png().toBuffer();
const wordmarkMeta = await sharp(wordmark).metadata();
const ogLeft = 120;
const ogLogoTop = Math.round((630 - 256) / 2);
const ogTextLeft = ogLeft + 256 + 48;
const ogTextTop = Math.round((630 - (wordmarkMeta.height || 130)) / 2);
await sharp({
  create: {
    width: 1200,
    height: 630,
    channels: 4,
    background: { r: 8, g: 8, b: 8, alpha: 1 },
  },
})
  .composite([
    { input: logoOg, left: ogLeft, top: ogLogoTop },
    { input: wordmark, left: ogTextLeft, top: ogTextTop },
  ])
  .png({ compressionLevel: 9 })
  .toFile(join(PUBLIC, 'assets', 'og.png'));
const ogStat = await sharp(join(PUBLIC, 'assets', 'og.png')).metadata();
written.push({ name: 'assets/og.png', bytes: readFileSync(join(PUBLIC, 'assets', 'og.png')).length, size: `${ogStat.width}x${ogStat.height}` });

for (const w of written) {
  console.log(`wrote public/${w.name} (${w.size}, ${w.bytes} bytes)`);
}
