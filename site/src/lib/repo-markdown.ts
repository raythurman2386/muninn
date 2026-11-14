// Read a markdown file from the muninn repo root (one directory above site/)
// and render it with Astro's markdown pipeline. The site never copies the text.
//
// Path resolution cannot use import.meta.url: during `astro build` this module
// is compiled into site/dist/.prerender/, which would point at the wrong place.
// We look for the repo root from process.cwd() instead (site/ or the repo root).
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createSatteriMarkdownProcessor } from '@astrojs/markdown-satteri';
import type { MarkdownRenderer } from '@astrojs/internal-helpers/markdown';
import { repoRoot } from './repo-root';

export { repoRoot };

export function readRepoMarkdown(name: string): string {
  return readFileSync(resolve(repoRoot(), name), 'utf8');
}

let renderer: MarkdownRenderer | undefined;

export async function renderMarkdown(content: string, repoFile: string) {
  renderer ??= await createSatteriMarkdownProcessor({
    syntaxHighlight: false,
    // Keep source punctuation so rendered pages stay faithful to the repo files
    // (including /privacy/ vs PRIVACY.md). attributes:true is required for the
    // changelog's `{#0.8.2}` heading ids.
    features: { smartPunctuation: false, attributes: true },
  });
  return renderer.render(content, { fileURL: new URL(repoFile, `file://${repoRoot()}/`) });
}

export async function renderRepoMarkdown(name: string) {
  return renderMarkdown(readRepoMarkdown(name), name);
}
