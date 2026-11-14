// @ts-check
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

// Static build of the Muninn marketing site. Output goes to site/dist/.
// Pages and assets use root-absolute URLs (/assets/..., /_astro/...), so the build assumes it is
// served at the domain root. Set `base` (and update those URLs) before serving it under a sub-path.
const waitlistEnabled = process.env.PUBLIC_WAITLIST_ENABLED === 'true';

/** Inject /waitlist/* only when the build flag is on, so a flag-off build
 *  does not bundle the form's CSS or JS into dist/_astro/. */
function waitlistRoutes() {
  return {
    name: 'muninn-waitlist-routes',
    hooks: {
      'astro:config:setup': (/** @type {{ injectRoute: (route: { pattern: string, entrypoint: string }) => void }} */ { injectRoute }) => {
        if (!waitlistEnabled) return;
        injectRoute({
          pattern: '/waitlist/[...slug]',
          entrypoint: './src/waitlist/page.astro',
        });
      },
    },
  };
}

export default defineConfig({
  site: 'https://muninn.ravenwoodsoftware.dev',
  output: 'static',
  trailingSlash: 'ignore',
  build: { format: 'directory' },
  devToolbar: { enabled: false },
  // Keep processed scripts as files under /_astro/ so the CSP can use 'self'
  // plus a single hash for the no-flash theme script, with no 'unsafe-inline'.
  vite: {
    build: { assetsInlineLimit: 0 },
  },
  integrations: [
    sitemap({
      // 404 is omitted by the integration; keep any utility paths out of the index.
      filter: (page) => !page.includes('/404'),
    }),
    waitlistRoutes(),
  ],
});
