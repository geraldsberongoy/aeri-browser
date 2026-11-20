//! `cargo xtask isolation`: end-to-end checks that profiles keep website
//! data apart (Phase 5; `docs/architecture/profile-isolation.md`) and keep
//! their own sessions and history (Phase 6).
//!
//! Serves a local test site, runs the bundled app with two profiles that
//! both open it, and drives each profile's page over the DevTools protocol.
//! It then quits, checks what each profile recorded, relaunches without a
//! start address (so each profile restores its session) and checks that
//! each profile kept its own data, and finally closes one profile's window
//! and checks the profile went dormant. No external site is part of any
//! check.
//!
//! Needs `cargo xtask bundle` first, `sqlite3` on the PATH, and a logged-in
//! macOS session: the app opens real windows.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

const APP: &str = "target/bundle/aeri.app/Contents/MacOS/aeri";
const PROFILES: &str = "Isolation A,Isolation B";
const TIMEOUT: Duration = Duration::from_secs(30);

pub fn run() -> Result<()> {
    ensure!(Path::new(APP).exists(), "run `cargo xtask bundle` first");
    let site = Site::start()?;
    let data = std::env::temp_dir().join(format!("aeri-isolation-{}", std::process::id()));
    let log = data.with_extension("log");
    let mut report = Report::default();

    // First run: A and B each sign in and write; neither sees the other's.
    let app = App::launch(&data, &log, Some(&format!("{}/", site.origin)))?;
    let mut cdp = app.devtools()?;
    let [a, b] = cdp.site_pages(&site.origin)?;
    report.check("separate sign-ins", || {
        cdp.eval(&a, "fetch('/login?user=alice').then(r => r.text())")?;
        cdp.eval(&b, "fetch('/login?user=bob').then(r => r.text())")?;
        let (who_a, who_b) = (whoami(&mut cdp, &a)?, whoami(&mut cdp, &b)?);
        ensure!(
            (who_a.as_str(), who_b.as_str()) == ("alice", "bob"),
            "A is {who_a}, B is {who_b}"
        );
        Ok(())
    });
    report.check("cookies", || {
        cdp.eval(&a, "fetch('/set-cookie?v=alpha').then(r => r.text())")?;
        let seen_a = cdp.eval_text(&a, "fetch('/cookies').then(r => r.text())")?;
        let seen_b = cdp.eval_text(&b, "fetch('/cookies').then(r => r.text())")?;
        ensure!(seen_a.contains("who=alpha"), "A sent {seen_a:?}");
        ensure!(!seen_b.contains("who="), "B sent A's cookie: {seen_b:?}");
        Ok(())
    });
    report.check("local storage", || {
        cdp.eval(&a, "localStorage.setItem('owner', 'alpha')")?;
        let in_b = cdp.eval(&b, "localStorage.getItem('owner')")?;
        ensure!(in_b.is_null(), "B read {in_b}");
        Ok(())
    });
    report.check("IndexedDB", || {
        cdp.eval(&a, &idb_put("alpha"))?;
        let in_a = cdp.eval(&a, IDB_GET)?;
        let in_b = cdp.eval(&b, IDB_GET)?;
        ensure!(in_a == json!("alpha"), "A read {in_a}");
        ensure!(in_b.is_null(), "B read {in_b}");
        Ok(())
    });
    report.check("HTTP cache", || {
        let before = site.hits();
        cdp.eval(&a, "fetch('/cached.js').then(r => r.text())")?;
        cdp.eval(&a, "fetch('/cached.js').then(r => r.text())")?;
        let after_a = site.hits() - before;
        cdp.eval(&b, "fetch('/cached.js').then(r => r.text())")?;
        let after_b = site.hits() - before;
        ensure!(
            after_a == 1,
            "A reached the server {after_a} times for 2 fetches"
        );
        ensure!(after_b == 2, "B was served from A's cache");
        Ok(())
    });
    // Site permissions: DevTools can only grant them in contexts it created
    // itself ("Failed to find browser context"), not in a profile's. They
    // are checked with real prompts in Phase 9 (Trust).
    // The uBlock Origin Lite store page each profile offers at its first
    // launch is external: close both. Each is remembered as a closed tab.
    let store = cdp.store_pages()?;
    for target in store {
        cdp.call("Target.closeTarget", json!({ "targetId": target }), None)?;
    }
    // Let the shell see the closes before it is told to quit.
    std::thread::sleep(Duration::from_secs(1));
    drop(cdp);
    report.check("orderly quit", || app.quit());

    report.check("each profile records its own history and session", || {
        let dbs = profile_dbs(&data)?;
        ensure!(dbs.len() == 2, "{} profile.db files", dbs.len());
        for db in &dbs {
            let history = sqlite(db, "SELECT url FROM history")?;
            ensure!(
                history.lines().any(|u| u.starts_with(&site.origin)),
                "{}: history {history:?}",
                db.display()
            );
            let session = sqlite(db, "SELECT url FROM tabs")?;
            ensure!(
                session.lines().any(|u| u.starts_with(&site.origin)),
                "{}: session {session:?}",
                db.display()
            );
            let closed = sqlite(db, "SELECT url FROM closed_tabs")?;
            ensure!(
                closed.contains("chromewebstore.google.com"),
                "{}: closed tabs {closed:?}",
                db.display()
            );
        }
        Ok(())
    });

    // Second run, no start address: each profile restores its session.
    // Everything is still there, and still apart. Contexts get new ids each
    // run, so tell A from B by who is signed in.
    let app = App::launch(&data, &log, None)?;
    let mut cdp = app.devtools()?;
    let pages = cdp.site_pages(&site.origin);
    report.check("sessions restore", || {
        pages.as_ref().map(|_| ()).map_err(|err| anyhow!("{err:#}"))
    });
    let [first, second] = pages?;
    let (a, b) = if whoami(&mut cdp, &first)? == "alice" {
        (first, second)
    } else {
        (second, first)
    };
    report.check("sign-ins survive a restart", || {
        let (who_a, who_b) = (whoami(&mut cdp, &a)?, whoami(&mut cdp, &b)?);
        ensure!(
            (who_a.as_str(), who_b.as_str()) == ("alice", "bob"),
            "A is {who_a}, B is {who_b}"
        );
        Ok(())
    });
    report.check("cookies survive a restart", || {
        let seen_a = cdp.eval_text(&a, "fetch('/cookies').then(r => r.text())")?;
        let seen_b = cdp.eval_text(&b, "fetch('/cookies').then(r => r.text())")?;
        ensure!(seen_a.contains("who=alpha"), "A sent {seen_a:?}");
        ensure!(!seen_b.contains("who="), "B sent {seen_b:?}");
        Ok(())
    });
    report.check("storage survives a restart", || {
        let in_a = cdp.eval(&a, "localStorage.getItem('owner')")?;
        let in_b = cdp.eval(&b, "localStorage.getItem('owner')")?;
        let idb_b = cdp.eval(&b, IDB_GET)?;
        ensure!(in_a == json!("alpha"), "A read {in_a}");
        ensure!(in_b.is_null() && idb_b.is_null(), "B read {in_b} / {idb_b}");
        Ok(())
    });
    report.check("a closed profile goes dormant", || {
        for target in cdp.targets()? {
            if target["browserContextId"] == b.context.as_str() && target["type"] == "page" {
                cdp.call(
                    "Target.closeTarget",
                    json!({ "targetId": target["targetId"] }),
                    None,
                )?;
            }
        }
        let started = Instant::now();
        loop {
            let targets = cdp.targets()?;
            let left: Vec<String> = targets
                .iter()
                .filter(|t| t["browserContextId"] == b.context.as_str())
                .map(|t| format!("{} {}", t["type"], t["url"]))
                .collect();
            let a_alive = targets
                .iter()
                .any(|t| t["browserContextId"] == a.context.as_str());
            if left.is_empty() {
                ensure!(a_alive, "A lost its pages too");
                println!("  (B had no targets left after {:?})", started.elapsed());
                return Ok(());
            }
            if started.elapsed() > TIMEOUT {
                bail!("B still runs {left:?}");
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    drop(cdp);
    report.check("orderly quit", || app.quit());

    let _ = std::fs::remove_dir_all(&data);
    report.finish(&log)
}

/// Each profile's `profile.db` under the data directory.
fn profile_dbs(data: &Path) -> Result<Vec<PathBuf>> {
    let mut dbs = Vec::new();
    for entry in std::fs::read_dir(data.join("Profiles"))? {
        let db = entry?.path().join("profile.db");
        if db.exists() {
            dbs.push(db);
        }
    }
    Ok(dbs)
}

/// Runs `sql` on `db` with the `sqlite3` tool, once the app has quit (a WAL
/// database cannot be opened read-only without its `-shm` file); one row a
/// line.
fn sqlite(db: &Path, sql: &str) -> Result<String> {
    let output = Command::new("sqlite3")
        .arg(db)
        .arg(sql)
        .output()
        .context("running sqlite3")?;
    ensure!(
        output.status.success(),
        "sqlite3: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn whoami(cdp: &mut DevTools, page: &Page) -> Result<String> {
    cdp.eval_text(page, "fetch('/whoami').then(r => r.text())")
}

const IDB_OPEN: &str = "new Promise((ok, fail) => {
    const r = indexedDB.open('aeri-isolation', 1);
    r.onupgradeneeded = () => r.result.createObjectStore('kv');
    r.onsuccess = () => ok(r.result);
    r.onerror = () => fail(r.error);
})";
const IDB_GET: &str = "new Promise((ok, fail) => {
    const r = indexedDB.open('aeri-isolation', 1);
    r.onupgradeneeded = () => r.result.createObjectStore('kv');
    r.onerror = () => fail(r.error);
    r.onsuccess = () => {
        const q = r.result.transaction('kv').objectStore('kv').get('owner');
        q.onsuccess = () => ok(q.result ?? null);
        q.onerror = () => fail(q.error);
    };
})";

fn idb_put(value: &str) -> String {
    format!(
        "{IDB_OPEN}.then(db => new Promise((ok, fail) => {{
            const t = db.transaction('kv', 'readwrite');
            t.objectStore('kv').put('{value}', 'owner');
            t.oncomplete = () => ok(true);
            t.onerror = () => fail(t.error);
        }}))"
    )
}

#[derive(Default)]
struct Report {
    results: Vec<(&'static str, Result<()>)>,
}

impl Report {
    fn check(&mut self, name: &'static str, check: impl FnOnce() -> Result<()>) {
        let result = check();
        match &result {
            Ok(()) => println!("PASS  {name}"),
            Err(err) => println!("FAIL  {name}: {err:#}"),
        }
        self.results.push((name, result));
    }

    fn finish(self, log: &Path) -> Result<()> {
        let failed = self.results.iter().filter(|(_, r)| r.is_err()).count();
        println!(
            "{} checks, {failed} failed (app log: {})",
            self.results.len(),
            log.display()
        );
        ensure!(failed == 0, "isolation checks failed");
        Ok(())
    }
}

/// The bundled app, run on its own data directory with both profiles.
struct App {
    child: std::cell::RefCell<Child>,
    port: u16,
}

impl App {
    /// With `start_url`, each profile opens a window on it; without, each
    /// restores its session.
    fn launch(data: &Path, log: &Path, start_url: Option<&str>) -> Result<Self> {
        let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)?;
        let mut command = Command::new(APP);
        command
            .arg("--use-mock-keychain")
            .env("AERI_DATA_DIR", data)
            .env("AERI_PROFILE", PROFILES)
            .env("AERI_REMOTE_DEBUGGING_PORT", port.to_string())
            .env("AERI_LOG", "info,aeri=debug");
        if let Some(url) = start_url {
            command.env("AERI_START_URL", url);
        }
        let child = command
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()
            .context("launching the app")?;
        Ok(Self {
            child: child.into(),
            port,
        })
    }

    fn devtools(&self) -> Result<DevTools> {
        let started = Instant::now();
        loop {
            match http_get(self.port, "/json/version") {
                Ok(body) => {
                    let version: Value = serde_json::from_str(&body)?;
                    let url = version["webSocketDebuggerUrl"]
                        .as_str()
                        .ok_or_else(|| anyhow!("no browser endpoint"))?;
                    return DevTools::connect(self.port, url);
                }
                Err(_) if started.elapsed() < TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(250))
                }
                Err(err) => return Err(err.context("the app's DevTools never came up")),
            }
        }
    }

    /// SIGTERM, which Aeri handles like the Quit menu; waits for the exit.
    fn quit(&self) -> Result<()> {
        let mut child = self.child.borrow_mut();
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                ensure!(status.success(), "exited with {status}");
                return Ok(());
            }
            if started.elapsed() > TIMEOUT {
                child.kill()?;
                bail!("did not quit within {TIMEOUT:?}");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let mut child = self.child.borrow_mut();
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
    }
}

/// A profile's page on the test site.
struct Page {
    context: String,
    target: String,
}

/// A browser-level DevTools protocol connection.
struct DevTools {
    ws: WebSocket<TcpStream>,
    next_id: u64,
}

impl DevTools {
    fn connect(port: u16, url: &str) -> Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        let (ws, _) = tungstenite::client(url, stream).map_err(|e| anyhow!("{e}"))?;
        Ok(Self { ws, next_id: 0 })
    }

    fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
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

    fn targets(&mut self) -> Result<Vec<Value>> {
        let result = self.call("Target.getTargets", json!({}), None)?;
        Ok(result["targetInfos"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    /// The two profiles' uBlock Origin Lite store pages, once both are open.
    fn store_pages(&mut self) -> Result<Vec<String>> {
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
    fn site_pages(&mut self, origin: &str) -> Result<[Page; 2]> {
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
    fn eval(&mut self, page: &Page, expression: &str) -> Result<Value> {
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

    fn eval_text(&mut self, page: &Page, expression: &str) -> Result<String> {
        let value = self.eval(page, expression)?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("expected text, got {value}"))
    }
}

/// A GET to the DevTools HTTP endpoint, which keeps the connection open:
/// read exactly `Content-Length` bytes.
fn http_get(port: u16, path: &str) -> Result<String> {
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

/// The local test site. Each profile signs in, sets cookies and fetches a
/// cacheable script here; the site counts how often that script is fetched.
struct Site {
    origin: String,
    hits: Arc<AtomicUsize>,
}

impl Site {
    fn start() -> Result<Self> {
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

    fn hits(&self) -> usize {
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
