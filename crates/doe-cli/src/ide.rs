//! `doe --ide`: a small web server for the web IDE.
//!
//! The page is built into the binary (see `build.rs`) and the server has
//! nothing else to give: it answers `GET` and `HEAD` for those files, on the
//! loopback address only, and only to requests that are for it (the `Host` and
//! the `Origin` are checked, which is what stops a page on another site, or a
//! DNS name that was made to point here, from using it). It is written on the
//! standard library: a thread for each connection, which is plenty for a page
//! used by one person.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

include!(concat!(env!("OUT_DIR"), "/ide_assets.rs"));

/// The most connections served at once; more get a 503.
const MAX_CONNECTIONS: usize = 32;
/// The longest a client may take to send its request, or to take the answer.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// The most bytes of a request head that are read.
const MAX_HEAD: usize = 16 * 1024;

/// An answer, before it is written.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: &'static [u8],
    /// For `HEAD`: say how long the body is but do not send it.
    pub(crate) head_only: bool,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn error(status: u16, body: &'static str) -> Response {
    Response {
        status,
        content_type: "text/plain; charset=utf-8",
        body: body.as_bytes(),
        head_only: false,
    }
}

/// Whether `host` (a `Host` header) names this server: the loopback address or
/// `localhost` with the port in use.
fn host_is_ours(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// The answer to the head of a request (the request line and the headers).
pub(crate) fn respond(head: &str, port: u16) -> Response {
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target), Some(version), None) = (
        request_line.next(),
        request_line.next(),
        request_line.next(),
        request_line.next(),
    ) else {
        return error(400, "bad request\n");
    };
    if !version.starts_with("HTTP/1.") {
        return error(400, "bad request\n");
    }

    let (mut host, mut origin) = (None, None);
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("host") {
                if host.replace(value).is_some() {
                    return error(400, "bad request\n");
                }
            } else if name.eq_ignore_ascii_case("origin") {
                origin = Some(value);
            }
        }
    }
    if !host.is_some_and(|host| host_is_ours(host, port)) {
        return error(403, "wrong Host\n");
    }
    // `Origin` is sent by browsers on requests from a page; it must be this
    // server (`null` and any other site are refused).
    if let Some(origin) = origin {
        let ours = origin
            .strip_prefix("http://")
            .is_some_and(|host| host_is_ours(host, port));
        if !ours {
            return error(403, "wrong Origin\n");
        }
    }

    if method != "GET" && method != "HEAD" {
        return error(405, "only GET and HEAD\n");
    }
    let path = target.split(['?', '#']).next().unwrap_or("");
    let name = if path == "/" {
        "index.html"
    } else {
        path.strip_prefix('/').unwrap_or("")
    };
    match ASSETS.iter().find(|(asset, _)| *asset == name) {
        Some((asset, body)) => Response {
            status: 200,
            content_type: content_type(asset),
            body,
            head_only: method == "HEAD",
        },
        None => error(404, "not found\n"),
    }
}

/// What the browser may do with the page: its own files, WebAssembly, and its
/// worker; no other site, no framing.
const POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
style-src 'self' 'unsafe-inline'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; \
frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\
Cross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: {POLICY}\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    if !response.head_only {
        stream.write_all(response.body)?;
    }
    stream.flush()
}

/// Read up to the blank line that ends the head of a request.
fn read_head(stream: &mut TcpStream) -> io::Result<Option<String>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        head.extend_from_slice(chunk.get(..n).unwrap_or(&[]));
    }
    Ok(String::from_utf8(head).ok())
}

fn serve_connection(mut stream: TcpStream, port: u16) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let response = match read_head(&mut stream)? {
        Some(head) => respond(&head, port),
        None => error(400, "bad request\n"),
    };
    write_response(&mut stream, &response)
}

/// Bind to the loopback address; port 0 is any free one.
pub(crate) fn bind(port: u16) -> io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// Answer connections on `listener` for ever.
pub(crate) fn serve(listener: &TcpListener, port: u16) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            // Best effort: the client may be gone already.
            let _ = write_response(&mut stream, &error(503, "busy\n"));
            continue;
        }
        let active = Arc::clone(&active);
        std::thread::spawn(move || {
            if let Err(e) = serve_connection(stream, port) {
                log::debug!("connection ended: {e}");
            }
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Ask the system to open `url` in the browser.
pub(crate) fn open_browser(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(e) = command.arg(url).spawn() {
        log::warn!("could not open the browser: {e}");
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn get(path: &str, headers: &str) -> Response {
        respond(&format!("GET {path} HTTP/1.1\r\n{headers}\r\n\r\n"), 9000)
    }

    #[test]
    fn the_page_is_served_for_the_right_host() {
        for host in ["127.0.0.1:9000", "localhost:9000"] {
            let r = get("/", &format!("Host: {host}"));
            assert_eq!(r.status, 200);
            assert!(r.content_type.starts_with("text/html"));
            assert_ne!(r.body, b"");
        }
        assert_eq!(get("/?limit=5", "Host: 127.0.0.1:9000").status, 200);
    }

    #[test]
    fn a_wrong_or_missing_host_is_refused() {
        for headers in [
            "Host: evil.example:9000",
            "Host: 127.0.0.1:9001",
            "Host: 127.0.0.1",
            "Host: 127.0.0.1:9000.evil.example",
            "",
            "Host: 127.0.0.1:9000\r\nHost: evil.example",
        ] {
            let status = get("/", headers).status;
            assert!(status == 403 || status == 400, "{headers}: {status}");
        }
    }

    #[test]
    fn a_wrong_origin_is_refused_and_ours_is_not() {
        let host = "Host: 127.0.0.1:9000";
        for origin in [
            "http://evil.example",
            "null",
            "https://127.0.0.1:9000",
            "http://127.0.0.1:9001",
        ] {
            assert_eq!(
                get("/", &format!("{host}\r\nOrigin: {origin}")).status,
                403,
                "{origin}"
            );
        }
        assert_eq!(
            get("/", &format!("{host}\r\nOrigin: http://127.0.0.1:9000")).status,
            200
        );
    }

    #[test]
    fn only_the_embedded_files_are_served() {
        let host = "Host: 127.0.0.1:9000";
        for path in [
            "/nope",
            "/../Cargo.toml",
            "/%2e%2e/Cargo.toml",
            "//etc/passwd",
            "/index.html/x",
        ] {
            assert_eq!(get(path, host).status, 404, "{path}");
        }
    }

    #[test]
    fn only_get_and_head_are_answered() {
        let head = |method: &str| {
            respond(
                &format!("{method} / HTTP/1.1\r\nHost: 127.0.0.1:9000\r\n\r\n"),
                9000,
            )
        };
        assert_eq!(head("POST").status, 405);
        assert_eq!(head("PUT").status, 405);
        let r = head("HEAD");
        assert_eq!((r.status, r.head_only), (200, true));
        assert_eq!(respond("garbage\r\n\r\n", 9000).status, 400);
    }

    #[test]
    fn the_server_binds_to_loopback_only_and_answers() {
        let (listener, port) = bind(0).unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
        std::thread::spawn(move || serve(&listener, port));
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(stream, "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("Content-Security-Policy"));
    }
}
