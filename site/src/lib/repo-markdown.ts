// Read a markdown file from the muninn repo root and render it with Astro's
// markdown pipeline. The site never copies the text into site/.
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
    // Keep source punctuation so rendered pages stay faithful to the repo files.
    features: { smartPunctuation: false, attributes: true },
  });
  return renderer.render(content, { fileURL: new URL(repoFile, `file://${repoRoot()}/`) });
}

export async function renderRepoMarkdown(name: string) {
  return renderMarkdown(readRepoMarkdown(name), name);
}
