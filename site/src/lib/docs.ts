// Curated user-facing docs for the marketing site.
//
// README sections and the named files under docs/ are read from the repo at
// build time (no copied text in site/). Internal specs stay off the site and
// are linked on GitHub instead: ARCHITECTURE.md, docs/sync-architecture.md,
// docs/state-machines.md, docs/i18n.md. Nothing is loaded from an
// Obsidian Kanban tree.
import { posix as posixPath } from 'node:path';
import { readRepoMarkdown, renderMarkdown } from './repo-markdown';

const GITHUB_BLOB = 'https://github.com/raythurman2386/muninn/blob/main/';

export const DOC_COLLECTION_FILES = [
  'sync-setup.md',
  'kanban-format.md',
  'a11y-checklist.md',
  'sync-safety.md',
] as const;

type ReadmeSource = {
  kind: 'readme';
  startHeading: string;
  endHeading: string;
};

type DocsFileSource = {
  kind: 'docs';
  file: (typeof DOC_COLLECTION_FILES)[number];
};

export interface DocPage {
  slug: string;
  title: string;
  description: string;
  source: ReadmeSource | DocsFileSource;
}

// Sidebar and sitemap order, matching the plan: getting started, shortcuts,
// sync setup, kanban format, a11y, privacy of sync.
export const DOC_PAGES: readonly DocPage[] = [
  {
    slug: 'getting-started',
    title: 'Getting started',
    description:
      'Install and run Muninn, manage vaults and settings, and use the session log and reduced-motion options.',
    source: { kind: 'readme', startHeading: '## Quick start', endHeading: '## What it does' },
  },
  {
    slug: 'keyboard-shortcuts',
    title: 'Keyboard shortcuts',
    description: 'The full shortcut map for Write, Read, Graph, Board and Kanban.',
    source: { kind: 'readme', startHeading: '## Keys', endHeading: '## Crates' },
  },
  {
    slug: 'sync-setup',
    title: 'Sync setup',
    description: 'Connect Google Drive, a GitHub repository, or a Tailscale device and sync a vault.',
    source: { kind: 'docs', file: 'sync-setup.md' },
  },
  {
    slug: 'kanban-format',
    title: 'Kanban format',
    description: "How Muninn stores a kanban board in a markdown note, and the syntax you can see in the file.",
    source: { kind: 'docs', file: 'kanban-format.md' },
  },
  {
    slug: 'a11y-checklist',
    title: 'Accessibility',
    description: 'How to run a screen-reader pass over Muninn, and what is already covered.',
    source: { kind: 'docs', file: 'a11y-checklist.md' },
  },
  {
    slug: 'sync-safety',
    title: 'Sync safety',
    description: 'What optional sync will and will not do with your notes, including conflicts and privacy of the remote store.',
    source: { kind: 'docs', file: 'sync-safety.md' },
  },
];

const SITE_ROUTES: Record<string, string> = {
  'CHANGELOG.md': '/changelog/',
  'docs/sync-setup.md': '/docs/sync-setup/',
  'docs/kanban-format.md': '/docs/kanban-format/',
  'docs/a11y-checklist.md': '/docs/a11y-checklist/',
  'docs/sync-safety.md': '/docs/sync-safety/',
};

export function docPage(slug: string | undefined): DocPage | undefined {
  return DOC_PAGES.find((page) => page.slug === slug);
}

export function extractMarkdownSection(source: string, startHeading: string, endHeading: string): string {
  const lines = source.split('\n');
  const start = lines.findIndex((line) => line === startHeading);
  if (start === -1) throw new Error(`heading not found in README.md: ${startHeading}`);
  const end = lines.findIndex((line, index) => index > start && line === endHeading);
  if (end === -1) throw new Error(`end heading not found in README.md: ${endHeading}`);
  return `${lines.slice(start, end).join('\n').trim()}\n`;
}

function splitHash(href: string): [string, string] {
  const hashAt = href.indexOf('#');
  if (hashAt === -1) return [href, ''];
  return [href.slice(0, hashAt), href.slice(hashAt)];
}

export function rewriteHref(href: string, fromRepoFile: string): string {
  const trimmed = href.trim();
  if (!trimmed) return trimmed;
  if (trimmed.startsWith('mailto:') || trimmed.startsWith('tel:')) return trimmed;
  if (trimmed.startsWith('#')) {
    // The Settings section of Getting started points at README's Development
    // "Sync" heading. On the site, send readers to the user-facing setup guide.
    if (trimmed === '#sync' && fromRepoFile === 'README.md') return '/docs/sync-setup/';
    return trimmed;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) return trimmed;

  const [pathPart, hash] = splitHash(trimmed);
  if (pathPart.startsWith('/')) return trimmed;

  const fromDir = posixPath.dirname(fromRepoFile);
  const resolved = posixPath
    .normalize(fromDir === '.' ? pathPart : posixPath.join(fromDir, pathPart))
    .replace(/^\.\//, '');

  const sitePath = SITE_ROUTES[resolved];
  if (sitePath) return `${sitePath}${hash}`;

  return `${GITHUB_BLOB}${resolved}${hash}`;
}

export function rewriteMarkdownLinks(markdown: string, fromRepoFile: string): string {
  return markdown.replace(
    /\[([^\]]+)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/g,
    (_full, text: string, href: string, title?: string) => {
      const next = rewriteHref(href, fromRepoFile);
      return title ? `[${text}](${next} "${title}")` : `[${text}](${next})`;
    },
  );
}

function readmePageMarkdown(page: DocPage & { source: ReadmeSource }): string {
  const section = extractMarkdownSection(
    readRepoMarkdown('README.md'),
    page.source.startHeading,
    page.source.endHeading,
  );
  // Keep the README section heading so H3 subsections stay under an H2
  // (e.g. Getting started → Quick start → Headless checks).
  const withTitle = `# ${page.title}\n\n${section}`;

  return rewriteMarkdownLinks(withTitle, 'README.md');
}

export async function renderDocPage(page: DocPage): Promise<string> {
  if (page.source.kind === 'readme') {
    const markdown = readmePageMarkdown({ ...page, source: page.source });
    const { code } = await renderMarkdown(markdown, 'README.md');
    return code;
  }
  const repoFile = `docs/${page.source.file}`;
  const markdown = rewriteMarkdownLinks(readRepoMarkdown(repoFile), repoFile);
  const { code } = await renderMarkdown(markdown, repoFile);
  return code;
}
