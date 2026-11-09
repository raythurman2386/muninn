//! Sign-in flows against loopback fixtures. No browser is opened: a fake
//! opener plays the person and follows the redirect.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::auth::{
    BrowserOpener, LoopbackRedirect, SignInEvents, browser_command, github_device_sign_in,
    google_sign_in,
};
use crate::drive::{self, DriveClient};
use crate::error::SyncError;
use crate::github::GitHubClient;
use crate::http::{CancelToken, RecordingSleeper, ReqwestTransport};
use crate::testkit::{ScriptedServer, json_reply};

#[derive(Default)]
struct Events {
    codes: Vec<(String, String)>,
    waiting: Vec<String>,
    failed: Vec<String>,
}

impl SignInEvents for Arc<Mutex<Events>> {
    fn device_code(&mut self, user_code: &str, verification_uri: &str) {
        self.lock()
            .unwrap()
            .codes
            .push((user_code.to_owned(), verification_uri.to_owned()));
    }
    fn waiting_for_browser(&mut self, url: &str) {
        self.lock().unwrap().waiting.push(url.to_owned());
    }
    fn browser_failed(&mut self, url: &str, _error: &SyncError) {
        self.lock().unwrap().failed.push(url.to_owned());
    }
}

/// Records the URL and, for Google, answers the redirect like a browser
/// would after consent.
struct FakeBrowser {
    opened: Mutex<Vec<String>>,
    redirect: Option<fn(&str, &str) -> String>,
}

impl BrowserOpener for FakeBrowser {
    fn open(&self, url: &str) -> Result<(), SyncError> {
        self.opened.lock().unwrap().push(url.to_owned());
        if let Some(answer) = self.redirect {
            let parsed = url::Url::parse(url).unwrap();
            let get = |name: &str| {
                parsed
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.into_owned())
                    .unwrap()
            };
            let target = answer(&get("redirect_uri"), &get("state"));
            std::thread::spawn(move || {
                let _ = http_get(&target);
            });
        }
        Ok(())
    }
}

fn http_get(url: &str) -> std::io::Result<String> {
    let parsed = url::Url::parse(url).unwrap();
    let mut stream =
        std::net::TcpStream::connect((parsed.host_str().unwrap(), parsed.port().unwrap()))?;
    let target = match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_owned(),
    };
    write!(stream, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n")?;
    let mut out = String::new();
    stream.read_to_string(&mut out)?;
    Ok(out)
}

#[test]
fn browser_opens_only_https_urls_without_a_shell() {
    let (program, args) = browser_command("https://github.com/login/device").unwrap();
    assert_eq!(args.last().unwrap(), "https://github.com/login/device");
    if cfg!(target_os = "macos") {
        assert_eq!(program, "open");
    } else if cfg!(windows) {
        assert_eq!(program, "rundll32");
        assert_eq!(args[0], "url.dll,FileProtocolHandler");
    } else {
        assert_eq!(program, "xdg-open");
    }
    for bad in [
        "http://example.com",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "--help",
        "",
        "https://",
        "ms-settings:privacy",
    ] {
        assert!(browser_command(bad).is_err(), "{bad} must be refused");
    }
}

#[test]
fn loopback_redirect_checks_state_and_ignores_other_paths() {
    let redirect = LoopbackRedirect::bind().unwrap();
    let uri = redirect.redirect_uri();
    assert!(uri.starts_with("http://127.0.0.1:") && uri.ends_with("/callback"));
    let base = uri.trim_end_matches("/callback").to_owned();
    let favicon = format!("{base}/favicon.ico");
    let good = format!("{uri}?code=the-code&state=s1");
    let worker = std::thread::spawn(move || {
        let missing = http_get(&favicon).unwrap();
        assert!(missing.starts_with("HTTP/1.1 404"));
        http_get(&good).unwrap()
    });
    let answer = redirect
        .wait("s1", &CancelToken::new(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(answer.code, "the-code");
    assert!(worker.join().unwrap().contains("You can close this tab"));

    for (query, state) in [
        ("code=x&state=forged", "s2"),
        ("error=access_denied&state=s3", "s3"),
    ] {
        let redirect = LoopbackRedirect::bind().unwrap();
        let url = format!("{}?{query}", redirect.redirect_uri());
        std::thread::spawn(move || http_get(&url));
        let error = redirect
            .wait(state, &CancelToken::new(), Duration::from_secs(5))
            .unwrap_err();
        assert!(matches!(error, SyncError::Auth { .. }), "{error}");
    }

    let redirect = LoopbackRedirect::bind().unwrap();
    let error = redirect
        .wait("s4", &CancelToken::new(), Duration::from_millis(100))
        .unwrap_err();
    assert!(matches!(error, SyncError::Auth { .. }), "{error}");
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = redirect
        .wait("s4", &cancel, Duration::from_secs(5))
        .unwrap_err();
    assert!(matches!(error, SyncError::Cancelled), "{error}");
}

#[test]
fn google_sign_in_uses_pkce_the_consent_host_and_a_loopback_redirect() {
    let server = ScriptedServer::spawn(|request| {
        if request.method == "POST" && request.path == "/token" {
            let body = request.body_str();
            let pairs: std::collections::BTreeMap<String, String> =
                form_urlencoded::parse(body.as_bytes()).into_owned().collect();
            if pairs.get("grant_type").map(String::as_str) != Some("authorization_code")
                || pairs.get("code").map(String::as_str) != Some("consented")
                || pairs.get("code_verifier").is_none_or(|v| v.len() < 43)
                || !pairs
                    .get("redirect_uri")
                    .is_some_and(|uri| uri.starts_with("http://127.0.0.1:"))
            {
                return json_reply(400, json!({"error": "invalid_grant"}));
            }
            return json_reply(
                200,
                json!({"access_token": "ya29.test", "refresh_token": "1//refresh", "expires_in": 3599, "scope": drive::DRIVE_SCOPE}),
            );
        }
        json_reply(404, json!({}))
    })
    .unwrap();
    let sleeper = RecordingSleeper::default();
    let client = DriveClient {
        api_base: server.base(),
        upload_base: server.base(),
        oauth_base: server.base(),
        auth_base: "https://accounts.google.com".to_owned(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel: CancelToken::new(),
        sleeper: &sleeper,
        request_timeout: Duration::from_secs(5),
    };
    let browser = FakeBrowser {
        opened: Mutex::new(Vec::new()),
        redirect: Some(|redirect_uri, state| {
            format!(
                "{redirect_uri}?state={state}&code=consented&scope={}",
                drive::DRIVE_SCOPE
            )
        }),
    };
    let events = Arc::new(Mutex::new(Events::default()));
    let mut sink = Arc::clone(&events);
    let token = google_sign_in(
        &client,
        "client.apps.googleusercontent.com",
        &browser,
        &mut sink,
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(token.access.expose(), "ya29.test");
    assert_eq!(token.refresh.as_ref().unwrap().expose(), "1//refresh");
    assert!(token.expires_unix_ms.unwrap() > crate::secret::now_unix_ms());
    let opened = browser.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    let url = url::Url::parse(&opened[0]).unwrap();
    assert_eq!(url.host_str(), Some("accounts.google.com"));
    assert_eq!(url.path(), "/o/oauth2/v2/auth");
    let query: std::collections::BTreeMap<String, String> =
        url.query_pairs().into_owned().collect();
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["scope"], drive::DRIVE_SCOPE);
    assert!(query["redirect_uri"].starts_with("http://127.0.0.1:"));
    assert_eq!(events.lock().unwrap().waiting, opened);
}

#[test]
fn github_device_sign_in_polls_at_the_server_interval_and_records_the_login() {
    let polls = Arc::new(Mutex::new(0u32));
    let polls_in_server = Arc::clone(&polls);
    let server = ScriptedServer::spawn(move |request| {
        if request.path.contains("/login/device/code") {
            return json_reply(
                200,
                json!({"device_code": "dev", "user_code": "WDJB-MJHT", "verification_uri": "https://github.com/login/device", "expires_in": 900, "interval": 5}),
            );
        }
        if request.path.contains("/login/oauth/access_token") {
            let mut polls = polls_in_server.lock().unwrap();
            *polls += 1;
            return match *polls {
                1 => json_reply(200, json!({"error": "authorization_pending"})),
                2 => json_reply(200, json!({"error": "slow_down", "interval": 10})),
                _ => json_reply(
                    200,
                    json!({"access_token": "ghu_x", "refresh_token": "ghr_y", "expires_in": 28800, "token_type": "bearer"}),
                ),
            };
        }
        if request.path == "/user" {
            return json_reply(200, json!({"login": "raymond"}));
        }
        json_reply(404, json!({}))
    })
    .unwrap();
    let sleeper = RecordingSleeper::default();
    let client = GitHubClient {
        api_base: server.base(),
        oauth_base: server.base(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel: CancelToken::new(),
        sleeper: &sleeper,
        request_timeout: Duration::from_secs(5),
    };
    let browser = FakeBrowser {
        opened: Mutex::new(Vec::new()),
        redirect: None,
    };
    let events = Arc::new(Mutex::new(Events::default()));
    let mut sink = Arc::clone(&events);
    let token = github_device_sign_in(&client, "Iv1.client", &browser, &mut sink).unwrap();
    assert_eq!(token.account, "raymond");
    assert_eq!(token.refresh.as_ref().unwrap().expose(), "ghr_y");
    assert_eq!(
        events.lock().unwrap().codes,
        vec![(
            "WDJB-MJHT".to_owned(),
            "https://github.com/login/device".to_owned()
        )]
    );
    assert_eq!(
        *browser.opened.lock().unwrap(),
        vec!["https://github.com/login/device".to_owned()]
    );
    let delays = sleeper.delays.lock().unwrap().clone();
    assert_eq!(
        delays,
        vec![
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(10)
        ]
    );
}

#[test]
fn github_device_code_that_expires_stops_polling() {
    let server = ScriptedServer::spawn(|request| {
        if request.path.contains("/login/device/code") {
            return json_reply(
                200,
                json!({"device_code": "dev", "user_code": "AAAA-BBBB", "verification_uri": "https://github.com/login/device", "expires_in": 20, "interval": 5}),
            );
        }
        json_reply(200, json!({"error": "authorization_pending"}))
    })
    .unwrap();
    let sleeper = RecordingSleeper::default();
    let client = GitHubClient {
        api_base: server.base(),
        oauth_base: server.base(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel: CancelToken::new(),
        sleeper: &sleeper,
        request_timeout: Duration::from_secs(5),
    };
    let browser = FakeBrowser {
        opened: Mutex::new(Vec::new()),
        redirect: None,
    };
    let mut sink = Arc::new(Mutex::new(Events::default()));
    let error = github_device_sign_in(&client, "Iv1.client", &browser, &mut sink).unwrap_err();
    assert!(matches!(error, SyncError::Auth { .. }), "{error}");
    assert_eq!(
        sleeper.delays.lock().unwrap().len(),
        4,
        "20 s at 5 s per poll"
    );
}
