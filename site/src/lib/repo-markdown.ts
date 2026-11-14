// Read a markdown file from the muninn repo root (one directory above site/)
// and render it with Astro's markdown pipeline. The site never copies the text.
//
// Path resolution cannot use import.meta.url: during `astro build` this module
// is compiled into site/dist/.prerender/, which would point at the wrong place.
// We look for the repo root from process.cwd() instead (site/ or the repo root).
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createSatteriMarkdownProcessor } from '@astrojs/markdown-satteri';
import type { MarkdownRenderer } from '@astrojs/internal-helpers/markdown';

export function repoRoot(): string {
  const here = process.cwd();
  const candidates = [resolve(here, '..'), here, resolve(here, '../..')];
  for (const dir of candidates) {
    if (existsSync(resolve(dir, 'PRIVACY.md')) && existsSync(resolve(dir, 'site', 'package.json'))) {
      return dir;
    }
  }
  throw new Error(`cannot locate the muninn repo root from cwd ${here}`);
}

export function readRepoMarkdown(name: string): string {
  return readFileSync(resolve(repoRoot(), name), 'utf8');
}

let renderer: MarkdownRenderer | undefined;

export async function renderRepoMarkdown(name: string) {
  renderer ??= await createSatteriMarkdownProcessor({
    syntaxHighlight: false,
    // Keep the source punctuation so /privacy/ can be diffed against PRIVACY.md.
    features: { smartPunctuation: false },
  });
  const content = readRepoMarkdown(name);
  return renderer.render(content, { fileURL: new URL(name, `file://${repoRoot()}/`) });
}
