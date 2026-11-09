//! Interactive sign-in: GitHub device flow and Google OAuth with PKCE and a
//! loopback redirect.
//!
//! Both flows open the system browser with a fixed program and the URL as a
//! single argument (no shell), and only for `https` URLs. Tokens come back to
//! the caller, which stores them in the OS credential store; nothing here
//! writes a token anywhere.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use crate::drive::{self, DriveClient, OAuthRedirect};
use crate::error::SyncError;
use crate::github::{DeviceChallenge, DevicePoll, GitHubClient};
use crate::http::{CancelToken, HttpTransport};
use crate::secret::TokenSet;

/// Opens a URL for the person signing in.
pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), SyncError>;
}

/// The desktop's default browser.
pub struct SystemBrowser;

impl BrowserOpener for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), SyncError> {
        let (program, args) = browser_command(url)?;
        let status = std::process::Command::new(program)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| SyncError::Config {
                detail: format!("could not open the browser ({error}); open {url} yourself"),
            })?;
        // The opener exits at once; reap it in the background.
        std::thread::spawn(move || {
            let mut child = status;
            let _ = child.wait();
        });
        Ok(())
    }
}

/// The program and arguments that open `url` on this OS. Only absolute
/// `https` URLs are opened, so a hostile value cannot name a file or a
/// custom scheme handler.
pub fn browser_command(url: &str) -> Result<(&'static str, Vec<String>), SyncError> {
    let parsed = url::Url::parse(url).map_err(|_| SyncError::Config {
        detail: "sign-in URL is not a URL".to_owned(),
    })?;
    if parsed.scheme() != "https" || parsed.host_str().is_none_or(str::is_empty) {
        return Err(SyncError::Config {
            detail: "sign-in URL must be https".to_owned(),
        });
    }
    let url = parsed.as_str().to_owned();
    if cfg!(target_os = "macos") {
        Ok(("open", vec![url]))
    } else if cfg!(windows) {
        Ok((
            "rundll32",
            vec!["url.dll,FileProtocolHandler".to_owned(), url],
        ))
    } else {
        Ok(("xdg-open", vec![url]))
    }
}

/// How a sign-in reports progress to the UI.
pub trait SignInEvents: Send {
    /// GitHub: show this code; the browser is opening `verification_uri`.
    fn device_code(&mut self, user_code: &str, verification_uri: &str);
    /// Google: the browser is opening this URL; waiting for the redirect.
    fn waiting_for_browser(&mut self, url: &str);
    /// The browser could not be opened; the person can open `url` manually.
    fn browser_failed(&mut self, url: &str, error: &SyncError);
}

/// GitHub device flow: request a code, open the verification page, poll at
/// the server's interval (honoring `slow_down`) until approval, denial, the
/// code's expiry, or cancel. The returned token carries the login.
pub fn github_device_sign_in<T: HttpTransport>(
    client: &GitHubClient<'_, T>,
    client_id: &str,
    browser: &dyn BrowserOpener,
    events: &mut dyn SignInEvents,
) -> Result<TokenSet, SyncError> {
    let challenge: DeviceChallenge = client.device_start(client_id)?;
    events.device_code(&challenge.user_code, &challenge.verification_uri);
    if let Err(error) = browser.open(&challenge.verification_uri) {
        events.browser_failed(&challenge.verification_uri, &error);
    }
    let deadline = Instant::now() + Duration::from_secs(challenge.expires_in.max(1));
    let mut interval = challenge.interval.max(1);
    // Waited time counts too, so a code is never polled past its lifetime
    // even when polls themselves are instant.
    let mut waited = 0u64;
    loop {
        client.cancel.check()?;
        if Instant::now() >= deadline || waited.saturating_add(interval) > challenge.expires_in {
            return Err(SyncError::Auth {
                detail: "the device code expired before it was approved".to_owned(),
            });
        }
        client
            .sleeper
            .sleep(Duration::from_secs(interval), &client.cancel)?;
        waited = waited.saturating_add(interval);
        match client.device_poll(client_id, &challenge.device_code)? {
            DevicePoll::Pending { .. } => {}
            DevicePoll::SlowDown { interval: next } => interval = next.max(interval + 5),
            DevicePoll::Approved(mut token) => {
                token.account = client.current_login(&token.access)?;
                return Ok(token);
            }
        }
    }
}

/// Google OAuth for an installed app: PKCE (S256), a random `state`, and a
/// one-shot listener on `127.0.0.1` for the redirect.
pub fn google_sign_in<T: HttpTransport>(
    client: &DriveClient<'_, T>,
    client_id: &str,
    browser: &dyn BrowserOpener,
    events: &mut dyn SignInEvents,
    timeout: Duration,
) -> Result<TokenSet, SyncError> {
    let redirect = LoopbackRedirect::bind()?;
    let pkce = drive::new_pkce(&redirect.redirect_uri());
    let url = client.authorization_url(client_id, &pkce)?;
    events.waiting_for_browser(&url);
    if let Err(error) = browser.open(&url) {
        events.browser_failed(&url, &error);
    }
    let answer = redirect.wait(&pkce.state, &client.cancel, timeout)?;
    client.exchange_code(client_id, &pkce.redirect_uri, &answer.code, &pkce.verifier)
}

/// A listener on `127.0.0.1:<ephemeral>` that accepts exactly the OAuth
/// redirect carrying the expected `state`.
pub struct LoopbackRedirect {
    listener: TcpListener,
    addr: SocketAddr,
}

impl LoopbackRedirect {
    pub fn bind() -> Result<Self, SyncError> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(SyncError::io)?;
        listener.set_nonblocking(true).map_err(SyncError::io)?;
        let addr = listener.local_addr().map_err(SyncError::io)?;
        Ok(Self { listener, addr })
    }

    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.addr.port())
    }

    /// Serve requests until one to `/` or `/callback` carries an OAuth
    /// answer. Other paths (a favicon) get a 404 and waiting continues. A
    /// wrong `state` or an `error=` answer ends the wait with `Auth`.
    pub fn wait(
        &self,
        expected_state: &str,
        cancel: &CancelToken,
        timeout: Duration,
    ) -> Result<OAuthRedirect, SyncError> {
        let deadline = Instant::now() + timeout;
        loop {
            cancel.check()?;
            if Instant::now() >= deadline {
                return Err(SyncError::Auth {
                    detail: "timed out waiting for the browser to finish signing in".to_owned(),
                });
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = answer(stream, expected_state) {
                        return result;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(SyncError::io(error)),
            }
        }
    }
}

fn answer(mut stream: TcpStream, expected_state: &str) -> Option<Result<OAuthRedirect, SyncError>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut line).is_err() {
            return None;
        }
        // Drain headers so the browser sees a clean response.
        let mut header = String::new();
        while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
            header.clear();
        }
    }
    let mut parts = line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        let _ = respond(&mut stream, 405, "Method not allowed.");
        return None;
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/" && path != "/callback" {
        let _ = respond(&mut stream, 404, "Not found.");
        return None;
    }
    if query.is_empty() {
        let _ = respond(&mut stream, 400, "Missing sign-in answer.");
        return None;
    }
    // Anything on this machine can reach the loopback port. A request that
    // does not carry this attempt's state is not the browser's answer: refuse
    // it and keep waiting, so it cannot cancel the real sign-in.
    let field = |name: &str| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    if field("state").as_deref() != Some(expected_state) {
        let _ = respond(
            &mut stream,
            400,
            "This sign-in answer is not for Muninn's current attempt.",
        );
        return None;
    }
    if field("code").is_none() && field("error").is_none() {
        let _ = respond(&mut stream, 400, "Missing sign-in answer.");
        return None;
    }
    let result = drive::parse_oauth_redirect(query, expected_state);
    let message = if result.is_ok() {
        "Muninn is signed in to Google Drive. You can close this tab."
    } else {
        "Muninn could not finish signing in. Return to Muninn for details."
    };
    let _ = respond(&mut stream, 200, message);
    Some(result)
}

fn respond(stream: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let body =
        format!("<!doctype html><meta charset=\"utf-8\"><title>Muninn</title><p>{message}</p>");
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
