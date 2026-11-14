// @ts-check
import { defineConfig } from 'astro/config';

// Static build of the Muninn marketing site. Output goes to site/dist/.
// Pages and assets use root-absolute URLs (/assets/..., /_astro/...), so the build assumes it is
// served at the domain root. Set `base` (and update those URLs) before serving it under a sub-path.
export default defineConfig({
  site: 'https://muninn.ravenwoodsoftware.dev',
  output: 'static',
  trailingSlash: 'ignore',
  build: { format: 'directory' },
  devToolbar: { enabled: false },
});
