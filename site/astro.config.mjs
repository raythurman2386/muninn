// @ts-check
import { defineConfig } from 'astro/config';

// Static build of the Muninn marketing site. Output goes to site/dist/.
export default defineConfig({
  site: 'https://muninn.ravenwoodsoftware.dev',
  output: 'static',
  trailingSlash: 'ignore',
  build: { format: 'directory' },
  devToolbar: { enabled: false },
});
