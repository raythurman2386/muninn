// Progressive enhancement for the Early access form.
// Without this script the form still POSTs to the proxy (JSON or form body).
// With it, the page stays put and an aria-live status region reports the result.
// Matches muninn-waitlist: POST {email, website, cf-turnstile-response} (turnstile_token accepted too).
// 200 {ok:true} and 409 (already exists) are both treated as success so the
// page never reveals whether an address is already on the list.

const MESSAGES = {
  success: 'Check your email for a confirmation link.',
  invalid_email: 'That email address does not look valid.',
  captcha: 'The check could not be completed. Please try again.',
  rate_limited: 'Too many attempts. Please wait and try again.',
  disabled: 'Signups are not open right now.',
  unavailable: 'The sign-up service is unavailable. Please try again later.',
  bad_request: 'The request could not be sent. Please try again.',
  origin: 'This page is not allowed to send sign-ups.',
  method: 'The request could not be sent. Please try again.',
  pending: 'Sending…',
  fallback: 'Something went wrong. Please try again.',
};

function setStatus(el, kind, text) {
  if (!el) return;
  el.hidden = false;
  el.dataset.state = kind;
  el.textContent = text;
}

function tokenFrom(form) {
  const data = new FormData(form);
  return (data.get('cf-turnstile-response') || data.get('turnstile_token') || '').toString();
}

function resetTurnstile() {
  const w = globalThis.turnstile;
  if (w && typeof w.reset === 'function') {
    try { w.reset(); } catch { /* widget stays as-is */ }
  }
}

async function submit(form, status) {
  const data = new FormData(form);
  const body = {
    email: (data.get('email') || '').toString(),
    website: (data.get('website') || '').toString(),
    'cf-turnstile-response': tokenFrom(form),
  };
  const res = await fetch(form.action, {
    method: 'POST',
    headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  let payload = {};
  try { payload = await res.json(); } catch { payload = {}; }

  // 409 from the proxy (or Listmonk, if it is ever forwarded) means the
  // address already exists and is treated as success, same as 200 {ok:true}.
  const ok = res.status === 409 || (res.ok && payload && payload.ok === true);
  if (ok) {
    setStatus(status, 'success', MESSAGES.success);
    form.reset();
    resetTurnstile();
    return;
  }
  const err = payload && typeof payload.error === 'string' ? payload.error : '';
  setStatus(status, 'error', MESSAGES[err] || MESSAGES.fallback);
  resetTurnstile();
}

function enhance(form) {
  const status = form.querySelector('[data-waitlist-status]');
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    if (!form.action) {
      setStatus(status, 'error', MESSAGES.unavailable);
      return;
    }
    setStatus(status, 'pending', MESSAGES.pending);
    const submitBtn = form.querySelector('[type="submit"]');
    if (submitBtn) submitBtn.disabled = true;
    submit(form, status)
      .catch(() => setStatus(status, 'error', MESSAGES.fallback))
      .finally(() => { if (submitBtn) submitBtn.disabled = false; });
  });
}

for (const form of document.querySelectorAll('[data-waitlist-form]')) enhance(form);
