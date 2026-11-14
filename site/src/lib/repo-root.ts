// Locate the muninn repo root (one directory above site/).
//
// Path resolution cannot use import.meta.url: during `astro build` compiled
// modules may live under site/dist/.prerender/. We look from process.cwd()
// instead (site/ or the repo root). Docker copies the markdown sources to
// /src/, next to /src/site, so the same walk works in the image build.
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

export function repoRoot(): string {
  const here = process.cwd();
  const candidates = [resolve(here, '..'), here, resolve(here, '../..')];
  for (const dir of candidates) {
    if (
      existsSync(resolve(dir, 'PRIVACY.md')) &&
      existsSync(resolve(dir, 'CHANGELOG.md')) &&
      existsSync(resolve(dir, 'README.md')) &&
      existsSync(resolve(dir, 'site', 'package.json'))
    ) {
      return dir;
    }
  }
  throw new Error(`cannot locate the muninn repo root from cwd ${here}`);
}
