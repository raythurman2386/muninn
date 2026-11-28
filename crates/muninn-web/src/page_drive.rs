//! Google Identity Services on the page (#240 slice 9).
//!
//! This is the browser glue the brief calls for: no relay, no redirect page of
//! ours. [`connect`] loads Google's GIS client (`https://accounts.google.com/gsi/client`),
//! initialises the OAuth **token client** with the public Web-application
//! client id, and asks it to open Google's own popup. GIS performs the PKCE
//! exchange itself; the page only receives the result.
//!
//! # The token handoff
//!
//! The popup answers on the main thread, so the callback hands the tokens to
//! [`muninn_ui::web_sync::drive_authorized`], which stores them in the page's
//! state home and queues a token-free [`WebSyncEvent`] for the form. The run
//! later takes them from that state home and puts them in the worker's wire
//! request, exactly as the PAT travels. Nothing here logs a token, sends one
//! anywhere, or puts one in an event.
//!
//! GIS's token flow may grant no refresh token at all. An absent refresh token
//! is honest: the engine reports `auth` when the access token expires and the
//! surface says Connect again. When Google does grant one, the worker's public
//! `refresh_public` path uses it against `oauth2.googleapis.com`.
//!
//! # Script loading and CSP
//!
//! The script is loaded on the first Connect, not at boot, so a GitHub-only
//! session never fetches anything from Google. The serving image's CSP admits
//! `https://accounts.google.com` in `script-src` and Google's Drive hosts in
//! `connect-src`; `web/README.md` records why the OAuth popup also makes COOP
//! `same-origin-allow-popups` the right setting for the image.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Function, Object, Promise, Reflect};
use muninn_ui::web_sync::{self, WebDriveToken};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use zeroize::Zeroizing;

use crate::google_auth::{DRIVE_APP_DATA_SCOPE, parse_token_response};

/// The GIS client script. Loaded lazily on the first Drive Connect.
const GIS_SCRIPT: &str = "https://accounts.google.com/gsi/client";

thread_local! {
    /// The live token client and its callbacks. GIS invokes them from the
    /// popup's message loop, so they must outlive [`connect`]'s stack frame.
    static GIS: RefCell<Option<Gis>> = const { RefCell::new(None) };
}

struct Gis {
    _client: JsValue,
    _callback: Closure<dyn FnMut(JsValue)>,
    _error: Closure<dyn FnMut(JsValue)>,
}

/// Open Google's sign-in popup for Drive's app-data scope.
///
/// Called by the shared UI's Connect (through [`muninn_ui::web_sync::WebSyncHooks`]).
/// A missing client id is a bug the UI already guards against; the defensive
/// path reports it as a failed sign-in rather than opening a popup Google would
/// reject.
pub fn connect() {
    let Some(client_id) = crate::google_auth::web_client_id() else {
        web_sync::drive_sign_in_failed();
        return;
    };
    spawn_local(async move {
        if ensure_gis().await.is_err() {
            web_sync::drive_sign_in_failed();
            return;
        }
        if let Err(error) = request_token(client_id) {
            console_warn(&format!("Google sign-in could not start: {error}"));
            web_sync::drive_sign_in_failed();
        }
    });
}

/// Resolve when `google.accounts.oauth2.initTokenClient` exists, injecting the
/// GIS script if this page has not loaded it yet.
async fn ensure_gis() -> Result<(), ()> {
    if init_token_client().is_some() {
        return Ok(());
    }
    // `Promise::new`'s executor is `FnMut`, so the settle functions live in
    // slots the one-shot script handlers can take. A `move` into those
    // handlers from a fresh clone keeps the executor callable.
    let resolve: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let reject: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let resolve_slot = Rc::clone(&resolve);
    let reject_slot = Rc::clone(&reject);
    let promise = Promise::new(&mut |resolve_now, reject_now| {
        *resolve_slot.borrow_mut() = Some(resolve_now);
        *reject_slot.borrow_mut() = Some(reject_now);
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            settle(&reject, Err("no document"));
            return;
        };
        let Ok(script) = document.create_element("script") else {
            settle(&reject, Err("no script element"));
            return;
        };
        let _ = script.set_attribute("src", GIS_SCRIPT);
        let _ = script.set_attribute("async", "true");
        // One-shot handlers: `once_into_js` frees them when they fire, so a
        // reconnect does not pile closures up.
        let resolve_load = Rc::clone(&resolve);
        let onload = Closure::<dyn FnMut()>::once_into_js(move || {
            settle(&resolve_load, Ok(()));
        });
        let reject_load = Rc::clone(&reject);
        let onerror = Closure::<dyn FnMut()>::once_into_js(move || {
            settle(&reject_load, Err("the GIS script failed to load"));
        });
        let target: &JsValue = script.unchecked_ref();
        let _ = Reflect::set(target, &JsValue::from_str("onload"), &onload);
        let _ = Reflect::set(target, &JsValue::from_str("onerror"), &onerror);
        if let Some(head) = document.head() {
            let _ = head.append_child(&script);
        }
    });
    JsFuture::from(promise).await.map(|_| ()).map_err(|_| ())
}

/// Settle a slot once. Later calls are no-ops, which is what the two script
/// handlers need if a browser fires both.
fn settle(slot: &RefCell<Option<Function>>, outcome: Result<(), &str>) {
    let Some(function) = slot.borrow_mut().take() else {
        return;
    };
    match outcome {
        Ok(()) => {
            let _ = function.call0(&JsValue::UNDEFINED);
        }
        Err(detail) => {
            let _ = function.call1(&JsValue::UNDEFINED, &JsValue::from_str(detail));
        }
    }
}

/// `google.accounts.oauth2.initTokenClient`, when the GIS script is loaded.
fn init_token_client() -> Option<Function> {
    Reflect::get(&js_sys::global(), &JsValue::from_str("google"))
        .ok()
        .and_then(|google| Reflect::get(&google, &JsValue::from_str("accounts")).ok())
        .and_then(|accounts| Reflect::get(&accounts, &JsValue::from_str("oauth2")).ok())
        .and_then(|oauth2| Reflect::get(&oauth2, &JsValue::from_str("initTokenClient")).ok())
        .and_then(|init| init.dyn_into::<Function>().ok())
}

/// Build the token client, keep its callbacks alive, and ask for a token.
fn request_token(client_id: &'static str) -> Result<(), String> {
    let init = init_token_client().ok_or("the GIS client is not loaded")?;
    let config = Object::new();
    set(&config, "client_id", &JsValue::from_str(client_id))?;
    set(&config, "scope", &JsValue::from_str(DRIVE_APP_DATA_SCOPE))?;
    // An explicit Connect asks for consent, so the popup always appears rather
    // than silently reusing a grant the person may have forgotten.
    set(&config, "prompt", &JsValue::from_str("consent"))?;
    // A web client can ask for offline access; whether Google grants a refresh
    // token depends on the client and the flow, and both outcomes are handled.
    set(&config, "access_type", &JsValue::from_str("offline"))?;

    let callback =
        Closure::<dyn FnMut(JsValue)>::new(|response: JsValue| {
            match token_from_response(&response) {
                Ok(token) => web_sync::drive_authorized(token, ""),
                Err(reason) => {
                    console_warn(&format!("Google sign-in did not return a token: {reason}"));
                    web_sync::drive_sign_in_failed();
                }
            }
        });
    let error = Closure::<dyn FnMut(JsValue)>::new(|error: JsValue| {
        let detail = error
            .dyn_ref::<Object>()
            .and_then(|object| {
                let object: &JsValue = object.as_ref();
                Reflect::get(object, &JsValue::from_str("error")).ok()
            })
            .and_then(|value| value.as_string())
            .or_else(|| error.as_string())
            .unwrap_or_else(|| "the popup was closed".to_owned());
        console_warn(&format!("Google sign-in stopped: {detail}"));
        web_sync::drive_sign_in_failed();
    });
    set(
        &config,
        "callback",
        callback.as_ref().unchecked_ref::<JsValue>(),
    )?;
    set(
        &config,
        "error_callback",
        error.as_ref().unchecked_ref::<JsValue>(),
    )?;

    let client = init
        .call1(&JsValue::UNDEFINED, &config)
        .map_err(|error| format!("initTokenClient failed: {}", js_message(&error)))?;
    let request = Reflect::get(&client, &JsValue::from_str("requestAccessToken"))
        .ok()
        .and_then(|function| function.dyn_into::<Function>().ok())
        .ok_or("the token client has no requestAccessToken")?;
    let options = Object::new();
    set(&options, "prompt", &JsValue::from_str("consent"))?;
    request
        .call1(&client, &options)
        .map_err(|error| format!("requestAccessToken failed: {}", js_message(&error)))?;
    GIS.with(|slot| {
        *slot.borrow_mut() = Some(Gis {
            _client: client,
            _callback: callback,
            _error: error,
        })
    });
    Ok(())
}

/// Turn the GIS callback payload into the UI's token set.
fn token_from_response(response: &JsValue) -> Result<WebDriveToken, String> {
    let json = js_sys::JSON::stringify(response)
        .map_err(|_| "the Google response was not serializable".to_owned())?;
    let text = json
        .as_string()
        .ok_or_else(|| "the Google response was not a string".to_owned())?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "the Google response was not JSON".to_owned())?;
    let now = js_sys::Date::now() as u64;
    let token = parse_token_response(&value, now)?;
    Ok(WebDriveToken {
        access: Zeroizing::new(token.access_token),
        refresh: token.refresh_token.map(Zeroizing::new),
        expires_unix_ms: token.expires_unix_ms,
    })
}

fn set(object: &Object, key: &str, value: &JsValue) -> Result<(), String> {
    let target: &JsValue = object.as_ref();
    Reflect::set(target, &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|_| format!("could not set {key}"))
}

/// A console warning. It never carries a token: callers pass GIS's error code
/// or a parser reason, not a response body.
fn console_warn(message: &str) {
    web_sys::console::warn_1(&JsValue::from_str(message));
}

fn js_message(error: &JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(error, &JsValue::from_str("message"))
                .ok()
                .and_then(|value| value.as_string())
        })
        .unwrap_or_else(|| "unknown error".to_owned())
}
