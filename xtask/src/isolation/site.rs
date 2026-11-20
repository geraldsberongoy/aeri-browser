//! The local test web server the isolation checks load in both profiles.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;

/// The local test site. Each profile signs in, sets cookies and fetches a
/// cacheable script here; the site counts how often that script is fetched.
pub(super) struct Site {
    pub(super) origin: String,
    hits: Arc<AtomicUsize>,
}

impl Site {
    pub(super) fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let counter = counter.clone();
                // Browsers open idle sockets; never let one block the rest.
                std::thread::spawn(move || {
                    let _ = serve(stream, &counter);
                });
            }
        });
        Ok(Self { origin, hits })
    }

    pub(super) fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn serve(mut stream: TcpStream, hits: &AtomicUsize) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut cookie = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("cookie")
        {
            cookie = value.trim().to_owned();
        }
    }
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |key: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
            .unwrap_or_default()
            .to_owned()
    };
    let cookie_value = |key: &str| {
        cookie
            .split(';')
            .find_map(|pair| pair.trim().strip_prefix(key)?.strip_prefix('='))
            .unwrap_or_default()
            .to_owned()
    };
    let max_age = "Max-Age=86400; Path=/";
    let (status, headers, body): (&str, Vec<String>, String) = match path {
        "/" => (
            "200 OK",
            vec!["Content-Type: text/html".into()],
            "<!doctype html><title>Aeri isolation</title><p>Profile isolation test site".into(),
        ),
        "/login" => (
            "200 OK",
            vec![format!(
                "Set-Cookie: session={}; {max_age}; HttpOnly",
                param("user")
            )],
            "signed in".into(),
        ),
        "/whoami" => {
            let user = cookie_value("session");
            let user = if user.is_empty() {
                "anonymous".into()
            } else {
                user
            };
            ("200 OK", vec![], user)
        }
        "/set-cookie" => (
            "200 OK",
            vec![format!("Set-Cookie: who={}; {max_age}", param("v"))],
            "set".into(),
        ),
        "/cookies" => ("200 OK", vec![], cookie),
        "/cached.js" => {
            hits.fetch_add(1, Ordering::SeqCst);
            (
                "200 OK",
                vec![
                    "Content-Type: text/javascript".into(),
                    "Cache-Control: max-age=3600".into(),
                ],
                "/* cacheable */".into(),
            )
        }
        _ => ("404 Not Found", vec![], String::new()),
    };
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for header in headers {
        response.push_str(&header);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    response.push_str(&body);
    stream.write_all(response.as_bytes())?;
    Ok(())
}
