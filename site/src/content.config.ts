// Content collections for the Muninn marketing site.
//
// The docs collection is a glob over the user-facing guides in the repo's
// docs/ folder (not a copy under site/). README-derived pages (getting
// started, keyboard shortcuts) are rendered from README.md in src/lib/docs.ts
// so the site never forks that text either.
import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { resolve } from 'node:path';
import { z } from 'astro/zod';
import { repoRoot } from './lib/repo-root';
import { DOC_COLLECTION_FILES } from './lib/docs';
import { githubReleasesLoader } from './lib/releases-loader';

const docs = defineCollection({
  loader: glob({
    pattern: `{${DOC_COLLECTION_FILES.join(',')}}`,
    base: resolve(repoRoot(), 'docs'),
  }),
});

const releaseAsset = z.object({
  name: z.string(),
  size: z.number(),
  url: z.string(),
  sha256: z.string().nullable(),
  kind: z.enum(['archive', 'deb', 'checksums', 'sigstore', 'other']),
  cardId: z.string().nullable(),
});

const osCard = z.object({
  id: z.string(),
  os: z.enum(['linux', 'macos', 'windows', 'other']),
  arch: z.string(),
  title: z.string(),
  archLabel: z.string(),
  notes: z.array(z.string()),
  assets: z.array(releaseAsset),
});

const releases = defineCollection({
  loader: githubReleasesLoader,
  schema: z.object({
    source: z.enum(['api', 'snapshot']),
    tag: z.string(),
    name: z.string(),
    htmlUrl: z.string(),
    publishedAt: z.string(),
    body: z.string(),
    assets: z.array(releaseAsset),
    checksumsUrl: z.string().nullable(),
    sigstoreUrl: z.string().nullable(),
    checksumsTxt: z.string(),
    cards: z.array(osCard),
    cosignOidcIssuer: z.string(),
    cosignCertificateIdentity: z.string(),
    cosignCertificateIdentityRegexp: z.string(),
  }),
});

export const collections = { docs, releases };
