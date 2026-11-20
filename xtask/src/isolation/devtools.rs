//! A minimal browser-level DevTools protocol client over WebSocket, plus
//! the plain HTTP GET used to find the endpoint.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use super::TIMEOUT;

/// A profile's page on the test site.
pub(super) struct Page {
    pub(super) context: String,
    target: String,
}

/// A browser-level DevTools protocol connection.
pub(super) struct DevTools {
    ws: WebSocket<TcpStream>,
    next_id: u64,
}

impl DevTools {
    pub(super) fn connect(port: u16, url: &str) -> Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        let (ws, _) = tungstenite::client(url, stream).map_err(|e| anyhow!("{e}"))?;
        Ok(Self { ws, next_id: 0 })
    }

    pub(super) fn call(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        self.ws.send(Message::text(message.to_string()))?;
        loop {
            let Message::Text(text) = self.ws.read()? else {
                continue;
            };
            let reply: Value = serde_json::from_str(text.as_str())?;
            if reply["id"] == id {
                if let Some(error) = reply.get("error") {
                    bail!("{method}: {error}");
                }
                return Ok(reply["result"].clone());
            }
        }
    }

    pub(super) fn targets(&mut self) -> Result<Vec<Value>> {
        let result = self.call("Target.getTargets", json!({}), None)?;
        Ok(result["targetInfos"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    /// The two profiles' uBlock Origin Lite store pages, once both are open.
    pub(super) fn store_pages(&mut self) -> Result<Vec<String>> {
        let started = Instant::now();
        loop {
            let store: Vec<String> = self
                .targets()?
                .iter()
                .filter(|t| {
                    t["type"] == "page"
                        && t["url"]
                            .as_str()
                            .is_some_and(|u| u.starts_with("https://chromewebstore.google.com/"))
                })
                .filter_map(|t| t["targetId"].as_str().map(str::to_owned))
                .collect();
            if store.len() == 2 {
                return Ok(store);
            }
            if started.elapsed() > TIMEOUT {
                bail!("{} store pages opened, not 2", store.len());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// One page on the site per profile, once two profiles show it.
    pub(super) fn site_pages(&mut self, origin: &str) -> Result<[Page; 2]> {
        let started = Instant::now();
        loop {
            let mut pages: Vec<Page> = Vec::new();
            for target in self.targets()? {
                let on_site = target["type"] == "page"
                    && target["url"]
                        .as_str()
                        .is_some_and(|u| u.starts_with(origin));
                let context = target["browserContextId"].as_str().unwrap_or_default();
                if on_site && !pages.iter().any(|p| p.context == context) {
                    pages.push(Page {
                        context: context.to_owned(),
                        target: target["targetId"].as_str().unwrap_or_default().to_owned(),
                    });
                }
            }
            if let Ok(pair) = <[Page; 2]>::try_from(pages) {
                return Ok(pair);
            }
            if started.elapsed() > TIMEOUT {
                bail!("two profiles never showed the test site");
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Evaluates `expression` in the page, awaiting a promise result.
    pub(super) fn eval(&mut self, page: &Page, expression: &str) -> Result<Value> {
        let attached = self.call(
            "Target.attachToTarget",
            json!({ "targetId": page.target, "flatten": true }),
            None,
        )?;
        let session = attached["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let result = self.call(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": true, "returnByValue": true }),
            Some(&session),
        );
        let _ = self.call(
            "Target.detachFromTarget",
            json!({ "sessionId": session }),
            None,
        );
        let result = result?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!("page threw: {exception}");
        }
        Ok(result["result"]["value"].clone())
    }

    pub(super) fn eval_text(&mut self, page: &Page, expression: &str) -> Result<String> {
        let value = self.eval(page, expression)?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("expected text, got {value}"))
    }
}

/// A GET to the DevTools HTTP endpoint, which keeps the connection open:
/// read exactly `Content-Length` bytes.
pub(super) fn http_get(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    )?;
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status)?;
    ensure!(status.starts_with("HTTP/1.1 200"), "{}", status.trim());
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse()?;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(String::from_utf8(body)?)
}
