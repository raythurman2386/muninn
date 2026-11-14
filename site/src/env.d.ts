/// <reference types="astro/client" />

interface ImportMetaEnv {
  readonly PUBLIC_WAITLIST_ENABLED?: string;
  readonly PUBLIC_WAITLIST_ENDPOINT?: string;
  readonly PUBLIC_TURNSTILE_SITEKEY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
