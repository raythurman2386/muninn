// Content collections for the Muninn marketing site.
//
// The docs collection is a glob over the user-facing guides in the repo's
// docs/ folder (not a copy under site/). README-derived pages (getting
// started, keyboard shortcuts) are rendered from README.md in src/lib/docs.ts
// so the site never forks that text either.
import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { resolve } from 'node:path';
import { repoRoot } from './lib/repo-root';
import { DOC_COLLECTION_FILES } from './lib/docs';

const docs = defineCollection({
  loader: glob({
    pattern: `{${DOC_COLLECTION_FILES.join(',')}}`,
    base: resolve(repoRoot(), 'docs'),
  }),
});

export const collections = { docs };
