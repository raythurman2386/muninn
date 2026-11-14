// Astro content-layer data loader for the latest GitHub Release.
// Fetches at build time; a committed snapshot keeps the build green if the
// API is down or rate-limited. GITHUB_TOKEN / GH_TOKEN is used only if set.

import type { Loader } from 'astro/loaders';
import { loadLatestRelease, type LatestRelease } from './releases';

function toData(release: LatestRelease): Record<string, unknown> {
  // The content store needs a plain JSON-serializable object.
  return JSON.parse(JSON.stringify(release)) as Record<string, unknown>;
}

export const githubReleasesLoader: Loader = {
  name: 'github-releases-latest',
  async load({ store, logger, parseData, generateDigest }) {
    const release = await loadLatestRelease({
      info: (message) => logger.info(message),
      warn: (message) => logger.warn(message),
    });
    const data = await parseData({ id: 'latest', data: toData(release) });
    store.clear();
    store.set({
      id: 'latest',
      data,
      digest: generateDigest(data),
    });
  },
};
