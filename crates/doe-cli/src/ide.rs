//! `doe --ide`: a small web server for the web IDE.
//!
//! The page is built into the binary (see `build.rs`) and the server has
//! nothing else to give: it answers `GET` and `HEAD` for those files, on the
//! loopback address only, and only to requests that are for it (the `Host` and
//! the `Origin` are checked, which is what stops a page on another site, or a
//! DNS name that was made to point here, from using it). It is written on the
//! standard library: a thread for each connection, which is plenty for a page
//! used by one person.

use std::borrow::Cow;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::ide_api::Local;

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
    pub(crate) body: Cow<'static, [u8]>,
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
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
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
        "yaml" | "yml" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

pub(crate) fn error(status: u16, body: &'static str) -> Response {
    Response {
        status,
        content_type: "text/plain; charset=utf-8",
        body: Cow::Borrowed(body.as_bytes()),
        head_only: false,
    }
}

/// Whether `host` (a `Host` header) names this server: the loopback address or
/// `localhost` with the port in use.
fn host_is_ours(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// The answer to a request: its head (the request line and the headers) and
/// its body.
pub(crate) fn respond(head: &str, body: &[u8], port: u16, local: &Local) -> Response {
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

    let (mut host, mut origin, mut json) = (None, None, false);
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("host") {
                if host.replace(value).is_some() {
                    return error(400, "bad request\n");
                }
            } else if name.eq_ignore_ascii_case("origin") {
                origin = Some(value);
            } else if name.eq_ignore_ascii_case("content-type") {
                json = value
                    .split(';')
                    .next()
                    .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"));
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

    let (path, query) = target
        .split('#')
        .next()
        .unwrap_or("")
        .split_once('?')
        .unwrap_or((target, ""));
    if let Some(api) = path.strip_prefix("/api/") {
        // JSON only for what is posted: a form on another site cannot send it
        // without the browser asking this server first (and the Origin is refused).
        return match (method, json) {
            ("GET" | "HEAD", _) | ("POST", true) => local.handle(method, api, query, body),
            ("POST", false) => error(415, "send application/json\n"),
            _ => error(405, "only GET, HEAD and POST\n"),
        };
    }
    if method != "GET" && method != "HEAD" {
        return error(405, "only GET and HEAD\n");
    }
    let name = if path == "/" {
        "index.html"
    } else {
        path.strip_prefix('/').unwrap_or("")
    };
    match ASSETS.iter().find(|(asset, _)| *asset == name) {
        Some((asset, body)) => Response {
            status: 200,
            content_type: content_type(asset),
            body: Cow::Borrowed(body),
            head_only: method == "HEAD",
        },
        None => error(404, "not found\n"),
    }
}

/// What the browser may do with the page: its own files, WebAssembly, and its
/// worker; no other site, no framing.
const POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
style-src 'self' 'unsafe-inline'; worker-src 'self'; connect-src 'self' https://raw.githubusercontent.com; img-src 'self' data:; \
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
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

/// The most bytes of a request body that are read.
const MAX_BODY: usize = 4 * 1024 * 1024;

/// Read a request: its head (up to the blank line), and its body when the head
/// says there is one. `None` when it is not a request.
fn read_request(stream: &mut TcpStream) -> io::Result<Result<(String, Vec<u8>), Response>> {
    let mut data = Vec::new();
    let mut chunk = [0_u8; 4096];
    let end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if data.len() > MAX_HEAD {
            return Ok(Err(error(400, "bad request\n")));
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(Err(error(400, "bad request\n")));
        }
        data.extend_from_slice(chunk.get(..n).unwrap_or(&[]));
    };
    let Ok(head) = std::str::from_utf8(data.get(..end).unwrap_or(&[])) else {
        return Ok(Err(error(400, "bad request\n")));
    };
    let head = head.to_owned();
    let declared = head
        .split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>());
    let length = match declared {
        None => 0,
        Some(Ok(length)) if length <= MAX_BODY => length,
        Some(Ok(_)) => return Ok(Err(error(413, "the body is too large\n"))),
        Some(Err(_)) => return Ok(Err(error(400, "bad request\n"))),
    };
    let mut body = data.get(end..).unwrap_or(&[]).to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(Err(error(400, "bad request\n")));
        }
        body.extend_from_slice(chunk.get(..n).unwrap_or(&[]));
    }
    body.truncate(length);
    Ok(Ok((head, body)))
}

fn serve_connection(mut stream: TcpStream, port: u16, local: &Local) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let response = match read_request(&mut stream)? {
        Ok((head, body)) => respond(&head, &body, port, local),
        Err(response) => response,
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
pub(crate) fn serve(listener: &TcpListener, port: u16, local: Local) {
    let local = Arc::new(local);
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
        let local = Arc::clone(&local);
        std::thread::spawn(move || {
            if let Err(e) = serve_connection(stream, port, &local) {
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

    #[test]
    fn a_type_of_the_library_is_served_as_text() {
        assert_eq!(
            content_type("library/tar.yaml"),
            "text/plain; charset=utf-8"
        );
        assert_eq!(content_type("gallery.json"), "application/json");
    }

    fn local() -> Local {
        Local::new(&[], None).unwrap()
    }

    fn get(path: &str, headers: &str) -> Response {
        respond(
            &format!("GET {path} HTTP/1.1\r\n{headers}\r\n\r\n"),
            b"",
            9000,
            &local(),
        )
    }

    #[test]
    fn the_page_is_served_for_the_right_host() {
        for host in ["127.0.0.1:9000", "localhost:9000"] {
            let r = get("/", &format!("Host: {host}"));
            assert_eq!(r.status, 200);
            assert!(r.content_type.starts_with("text/html"));
            assert_ne!(&r.body[..], &b""[..]);
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
                b"",
                9000,
                &local(),
            )
        };
        assert_eq!(head("POST").status, 405);
        assert_eq!(head("PUT").status, 405);
        let r = head("HEAD");
        assert_eq!((r.status, r.head_only), (200, true));
        assert_eq!(respond("garbage\r\n\r\n", b"", 9000, &local()).status, 400);
    }

    #[test]
    fn the_server_binds_to_loopback_only_and_answers() {
        let (listener, port) = bind(0).unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
        std::thread::spawn(move || serve(&listener, port, local()));
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(stream, "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("Content-Security-Policy"));
    }
}
