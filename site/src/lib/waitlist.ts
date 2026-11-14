// Build-time flags for the Early access form (W9).
//
// PUBLIC_WAITLIST_ENABLED must be the string "true" to include the form and
// its pages. Anything else, including unset, leaves the built site identical
// to a tree without this slice. PUBLIC_WAITLIST_ENDPOINT and
// PUBLIC_TURNSTILE_SITEKEY are only read when the flag is on. No keys are
// committed; the site key is a public build input, never a secret.

export const WAITLIST_ENABLED = import.meta.env.PUBLIC_WAITLIST_ENABLED === 'true';

export const WAITLIST_ENDPOINT = WAITLIST_ENABLED
  ? (import.meta.env.PUBLIC_WAITLIST_ENDPOINT ?? '').trim()
  : '';

export const TURNSTILE_SITEKEY = WAITLIST_ENABLED
  ? (import.meta.env.PUBLIC_TURNSTILE_SITEKEY ?? '').trim()
  : '';
