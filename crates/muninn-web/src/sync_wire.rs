//! Pure wire helpers for the browser sync transport (#240 slice 8a).
//!
//! These live outside the wasm-only `sync` module so `cargo test -p
//! muninn-web` exercises them on the host. They are the parts of the sync XHR
//! boundary that do not need a browser: response-header parsing and the safe
//! host label a diagnostic may carry.

/// Parse `XMLHttpRequest.getAllResponseHeaders()`'s CRLF-separated text.
///
/// The browser already hides forbidden headers (notably `Set-Cookie`), so no
/// filtering is needed here. A malformed line is skipped rather than failing a
/// response the engine may still be able to use; an empty body yields an empty
/// list. Header names keep the browser's spelling; `HttpResponse::header`
/// compares case-insensitively.
#[must_use]
pub fn parse_response_headers(raw: &str) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    for line in raw.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }
    headers
}

/// The host of a provider URL, for a diagnostic that must not echo the whole
/// URL (which may carry a signed query).
///
/// The transport calls this only after `ensure_web_allowed` has proved the URL
/// parses and the host is pinned, so the fallback is unreachable in practice.
#[must_use]
pub fn host_label(url: &str) -> &str {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split(['/', '?'])
        .next()
        .unwrap_or("the provider")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_headers_parse_crlf_lines() {
        let parsed = parse_response_headers(
            "Content-Type: application/json\r\nX-RateLimit-Remaining: 4999\r\n\r\n",
        );
        assert_eq!(
            parsed,
            [
                ("Content-Type".to_owned(), "application/json".to_owned()),
                ("X-RateLimit-Remaining".to_owned(), "4999".to_owned()),
            ]
        );
        assert!(parse_response_headers("").is_empty());
        assert!(parse_response_headers("malformed").is_empty());
        assert!(parse_response_headers(": empty-name").is_empty());
    }

    #[test]
    fn a_diagnostic_names_only_the_host() {
        assert_eq!(
            host_label("https://api.github.com/repos/o/r/git/ref/heads/main"),
            "api.github.com"
        );
        assert_eq!(
            host_label("https://api.github.com?token=leak"),
            "api.github.com"
        );
        assert_eq!(host_label("api.github.com"), "api.github.com");
    }
}
