//! The worker's HTTP transport: synchronous XHR against `api.github.com`.
//!
//! A dedicated worker may make synchronous XHR requests; the API is deprecated
//! only on documents. That is exactly what the blocking engine needs: the
//! engine calls [`HttpTransport::execute`] and gets an answer, with no async
//! rework anywhere in `muninn-sync`. Verified in current Chrome: a sync XHR
//! from a dedicated worker to `https://api.github.com` returns HTTP 200 with a
//! readable body.
//!
//! # Differences from the native transport, on purpose
//!
//! - **Host.** [`ensure_web_allowed`] pins `api.github.com` over https with no
//!   port and no URL credentials. The native allowlist also accepts the OAuth
//!   pages, Google, and Tailnet hosts; a PAT run needs none of them, and the
//!   OAuth endpoints are browser-blocked anyway.
//! - **Timeout.** A synchronous `XMLHttpRequest` cannot carry a `timeout`
//!   attribute (the spec throws `InvalidAccessError` for `async=false`), so
//!   [`HttpRequest::timeout`] is advisory here. The browser's network stack
//!   eventually fails the request; the engine's retry budget still bounds
//!   repeated failures. This is recorded rather than hidden.
//! - **No redirects.** The browser follows redirects for XHR (the native
//!   client refuses them). GitHub's API does not redirect an authenticated API
//!   call, and the host pin means a redirect to another origin would fail CORS
//!   anyway; the transport does not add a follow/no-follow switch.
//!
//! The status→[`SyncError`] mapping is *not* duplicated here: the GitHub
//! client calls the shared `classify_status`, so the browser and desktop
//! classify 401/403/409/5xx identically. This transport only turns JS-level
//! failures (a thrown `NetworkError`, status 0) into [`SyncError::Offline`].

use js_sys::Uint8Array;
use muninn_sync::{HttpRequest, HttpResponse, HttpTransport, SyncError, ensure_web_allowed};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{XmlHttpRequest, XmlHttpRequestResponseType};

use crate::sync_wire::{host_label, parse_response_headers};

/// The browser's [`HttpTransport`].
///
/// It is stateless: each request opens its own `XMLHttpRequest`, sends it
/// synchronously and returns the response. The engine's cancel token is
/// checked between requests by the client, not inside a blocking send; a sync
/// XHR cannot be aborted from the same thread while it blocks.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebTransport;

impl WebTransport {
    /// A transport. It holds no host state, so one instance serves every run
    /// in a worker.
    #[must_use]
    pub fn new() -> WebTransport {
        WebTransport
    }
}

impl HttpTransport for WebTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, SyncError> {
        // Fail before touching the network: a provider URL that is not the
        // pinned API host must never be attempted, even for a read.
        ensure_web_allowed(&request.url)?;
        let xhr = XmlHttpRequest::new().map_err(|_| SyncError::Offline {
            detail: "the browser refused to create an XMLHttpRequest".to_owned(),
        })?;
        xhr.open_with_async(&request.method, &request.url, false)
            .map_err(|error| protocol("the browser refused the request", error))?;
        xhr.set_response_type(XmlHttpRequestResponseType::Arraybuffer);
        for (name, value) in &request.headers {
            xhr.set_request_header(name, value).map_err(|error| {
                protocol(&format!("the browser refused the {name} header"), error)
            })?;
        }
        let sent = if request.body.is_empty() {
            xhr.send()
        } else {
            xhr.send_with_opt_u8_array(Some(&request.body))
        };
        // A synchronous XHR reports a network failure by throwing, not by
        // returning a status. The native client maps a dropped connection to
        // `Offline`; this matches that, including for a mutation.
        sent.map_err(|error| offline(&request.url, error))?;

        let status = xhr.status().map_err(|error| offline(&request.url, error))?;
        if status == 0 {
            return Err(SyncError::Offline {
                detail: format!("could not reach {}", host_label(&request.url)),
            });
        }
        let headers = xhr
            .get_all_response_headers()
            .map(|raw| parse_response_headers(&raw))
            .unwrap_or_default();
        let body = response_bytes(&xhr)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// Bytes from an `arraybuffer` response.
///
/// An empty body is a zero-length buffer in every browser; a non-ArrayBuffer
/// value means the response type was not honoured, which the engine would
/// otherwise see as a mysterious JSON parse failure.
fn response_bytes(xhr: &XmlHttpRequest) -> Result<Vec<u8>, SyncError> {
    let value = xhr
        .response()
        .map_err(|error| offline("the response", error))?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    let buffer = value
        .dyn_into::<js_sys::ArrayBuffer>()
        .map_err(|_| SyncError::Protocol {
            detail: "the browser did not return the response as an array buffer".to_owned(),
        })?;
    let view = Uint8Array::new(&buffer);
    let mut bytes = vec![0; view.length() as usize];
    view.copy_to(&mut bytes);
    Ok(bytes)
}

fn offline(context: &str, error: JsValue) -> SyncError {
    let detail = js_message(&error);
    SyncError::Offline {
        detail: format!("{context}: {detail}"),
    }
}

fn protocol(context: &str, error: JsValue) -> SyncError {
    let detail = js_message(&error);
    SyncError::Protocol {
        detail: format!("{context}: {detail}"),
    }
}

fn js_message(error: &JsValue) -> String {
    if let Some(message) = error.as_string() {
        return message;
    }
    js_sys::Reflect::get(error, &JsValue::from_str("message"))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}
