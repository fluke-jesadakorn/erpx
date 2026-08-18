use std::{
    env,
    error::Error,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
type CdpSocket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

const DEFAULT_BASE_URL: &str = "http://localhost:3004";

struct ChromeGuard {
    child: Child,
    profile: PathBuf,
}

impl Drop for ChromeGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self
            .profile
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("erpx-web-e2e-profile-"))
        {
            let _ = fs::remove_dir_all(&self.profile);
        }
    }
}

struct CdpClient {
    socket: CdpSocket,
    next_id: u64,
    console_errors: Vec<Value>,
    network_failures: Vec<Value>,
    network_http_errors: Vec<Value>,
}

impl CdpClient {
    fn connect(websocket_url: &str) -> TaskResult<Self> {
        let (socket, _) = connect(websocket_url)?;
        Ok(Self {
            socket,
            next_id: 1,
            console_errors: Vec::new(),
            network_failures: Vec::new(),
            network_http_errors: Vec::new(),
        })
    }

    fn command(&mut self, method: &str, params: Value) -> TaskResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.socket.send(Message::Text(
            json!({ "id": id, "method": method, "params": params })
                .to_string()
                .into(),
        ))?;
        loop {
            let message = self.socket.read()?;
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(text.as_ref())?;
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(format!("CDP {method} failed: {error}").into());
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
            self.record_event(value);
        }
    }

    fn record_event(&mut self, value: Value) {
        match value.get("method").and_then(Value::as_str) {
            Some("Runtime.exceptionThrown") => self.console_errors.push(value),
            Some("Runtime.consoleAPICalled")
                if value
                    .pointer("/params/type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| matches!(kind, "error" | "assert")) =>
            {
                self.console_errors.push(value);
            }
            Some("Log.entryAdded")
                if value.pointer("/params/entry/level").and_then(Value::as_str)
                    == Some("error") =>
            {
                self.console_errors.push(value);
            }
            Some("Network.loadingFailed") => self.network_failures.push(value),
            Some("Network.responseReceived") => {
                let url = value
                    .pointer("/params/response/url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let status = value
                    .pointer("/params/response/status")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                if status >= 400.0 && is_hydration_asset_url(url) {
                    self.network_http_errors.push(value);
                }
            }
            _ => {}
        }
    }

    fn evaluate(&mut self, expression: &str) -> TaskResult<Value> {
        let result = self.command(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "awaitPromise": true,
                "returnByValue": true,
            }),
        )?;
        if let Some(details) = result.get("exceptionDetails") {
            return Err(format!("browser evaluation failed: {details}").into());
        }
        Ok(result
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn navigate(&mut self, url: &str) -> TaskResult {
        self.command("Page.navigate", json!({ "url": url }))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.evaluate("document.readyState")?.as_str() == Some("complete") {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!("browser navigation timed out: {url}").into())
    }

    fn screenshot(&mut self, path: &Path) -> TaskResult {
        let result = self.command(
            "Page.captureScreenshot",
            json!({ "format": "png", "captureBeyondViewport": false }),
        )?;
        let encoded = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or("CDP screenshot response omitted data")?;
        fs::write(path, STANDARD.decode(encoded)?)?;
        Ok(())
    }
}

fn is_hydration_asset_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.ends_with(".css") || path.ends_with(".js") || path.ends_with(".wasm")
}

fn chrome_binary() -> TaskResult<PathBuf> {
    if let Some(path) = env::var_os("ERPX_WEB_E2E_CHROMIUM") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err("ERPX_WEB_E2E_CHROMIUM is not a file".into());
    }
    for path in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
    ] {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("Chromium was not found; set ERPX_WEB_E2E_CHROMIUM".into())
}

fn available_port() -> TaskResult<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

async fn launch_chrome() -> TaskResult<(ChromeGuard, String)> {
    let port = available_port()?;
    let profile = env::temp_dir().join(format!(
        "erpx-web-e2e-profile-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    fs::create_dir(&profile)?;
    let child = Command::new(chrome_binary()?)
        .args([
            "--headless=new",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-allow-origins=*",
        ])
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let guard = ChromeGuard { child, profile };
    let endpoint = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if client
            .get(format!("{endpoint}/json/version"))
            .send()
            .await
            .is_ok()
        {
            return Ok((guard, endpoint));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Chromium DevTools endpoint did not become ready".into())
}

async fn page_websocket(endpoint: &str, url: &str) -> TaskResult<String> {
    let response: Value = reqwest::Client::new()
        .put(format!("{endpoint}/json/new?{}", urlencoding(url)))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    response
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Chromium target omitted webSocketDebuggerUrl".into())
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn passkey_round_trip(client: &mut CdpClient) -> TaskResult {
    let result = client.evaluate(
        r#"(async () => {
          const challenge = Uint8Array.from({length: 32}, (_, i) => i + 1);
          const userId = Uint8Array.from({length: 16}, (_, i) => i + 10);
          const created = await navigator.credentials.create({publicKey: {
            challenge, rp: {name: 'ERPX E2E', id: 'localhost'},
            user: {id: userId, name: 'erpx-e2e', displayName: 'ERPX E2E'},
            pubKeyCredParams: [{type: 'public-key', alg: -7}],
            authenticatorSelection: {residentKey: 'required', userVerification: 'required'},
            timeout: 10000, attestation: 'none'
          }});
          const asserted = await navigator.credentials.get({publicKey: {
            challenge: Uint8Array.from({length: 32}, (_, i) => 100 + i),
            rpId: 'localhost', allowCredentials: [{type: 'public-key', id: created.rawId}],
            userVerification: 'required', timeout: 10000
          }});
          return {registered: created.type === 'public-key', authenticated: asserted.id === created.id};
        })()"#,
    )?;
    if result.pointer("/registered").and_then(Value::as_bool) != Some(true)
        || result.pointer("/authenticated").and_then(Value::as_bool) != Some(true)
    {
        return Err("virtual authenticator registration/assertion did not complete".into());
    }
    Ok(())
}

pub async fn run(root: &Path) -> TaskResult {
    let base_url = env::var("ERPX_WEB_E2E_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
    let parsed = reqwest::Url::parse(&base_url)?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some("localhost")
        || parsed.port_or_known_default() != Some(3004)
        || parsed.path() != "/"
        || parsed.query().is_some()
    {
        return Err("web-e2e accepts only the loopback ERPX runtime on port 3004".into());
    }
    reqwest::get(&base_url).await?.error_for_status()?;

    let evidence_dir = root
        .join("audits/web-e2e")
        .join(Utc::now().format("%Y%m%dT%H%M%SZ").to_string());
    fs::create_dir_all(&evidence_dir)?;
    let (_chrome, endpoint) = launch_chrome().await?;
    let websocket = page_websocket(&endpoint, &base_url).await?;
    let mut cdp = CdpClient::connect(&websocket)?;
    for method in [
        "Page.enable",
        "Runtime.enable",
        "Log.enable",
        "Network.enable",
    ] {
        cdp.command(method, json!({}))?;
    }
    cdp.command("WebAuthn.enable", json!({ "enableUI": false }))?;
    cdp.command(
        "WebAuthn.addVirtualAuthenticator",
        json!({ "options": {
            "protocol": "ctap2", "ctap2Version": "ctap2_1", "transport": "internal",
            "hasResidentKey": true, "hasUserVerification": true, "isUserVerified": true,
            "automaticPresenceSimulation": true
        }}),
    )?;
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
    )?;
    cdp.command(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-color-scheme", "value": "light" }] }),
    )?;
    cdp.navigate(&base_url)?;
    passkey_round_trip(&mut cdp)?;
    let focused = cdp.evaluate(
        "(() => { const e=document.querySelector('input,button,a[href]'); if(!e)return false; e.focus(); return document.activeElement===e; })()",
    )?;
    if focused.as_bool() != Some(true) {
        return Err("keyboard focus probe failed".into());
    }
    let light_background = cdp.evaluate(
        "getComputedStyle(document.querySelector('.sign-in-form-region')).backgroundColor",
    )?;
    let locale_time_origin = cdp.evaluate("performance.timeOrigin")?;
    let locale_deadline = Instant::now() + Duration::from_secs(10);
    let mut locale_changed = false;
    while Instant::now() < locale_deadline {
        if cdp
            .evaluate("document.body.innerText.includes('เข้าสู่ระบบพื้นที่ทำงานของคุณ')")?
            .as_bool()
            == Some(true)
        {
            locale_changed = true;
            break;
        }
        cdp.evaluate("(() => { const button=document.querySelector('.ui-locale-toggle'); if (!button) return false; button.click(); return true; })()")?;
        thread::sleep(Duration::from_millis(250));
    }
    if !locale_changed {
        return Err("locale control did not update the hydrated Dioxus UI".into());
    }
    if cdp.evaluate("performance.timeOrigin")? != locale_time_origin {
        return Err("locale control reloaded the document instead of updating hydrated UI".into());
    }
    cdp.screenshot(&evidence_dir.join("desktop-en-light.png"))?;

    cdp.command(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-color-scheme", "value": "dark" }] }),
    )?;
    cdp.command("Emulation.setLocaleOverride", json!({ "locale": "th-TH" }))?;
    let cookie = cdp.command(
        "Network.setCookie",
        json!({
            "name": "erpx_locale",
            "value": "th",
            "domain": "localhost",
            "path": "/",
            "sameSite": "Strict"
        }),
    )?;
    if cookie.get("success").and_then(Value::as_bool) == Some(false) {
        return Err("Chromium rejected the Thai locale cookie".into());
    }
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 390, "height": 844, "deviceScaleFactor": 2, "mobile": true }),
    )?;
    cdp.navigate(&base_url)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if cdp
            .evaluate("document.body.innerText.includes('เข้าสู่ระบบพื้นที่ทำงานของคุณ')")?
            .as_bool()
            == Some(true)
        {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    if cdp
        .evaluate("document.body.innerText.includes('เข้าสู่ระบบพื้นที่ทำงานของคุณ')")?
        .as_bool()
        != Some(true)
    {
        return Err("Thai locale did not render after using the locale control".into());
    }
    let dark_background = cdp.evaluate(
        "getComputedStyle(document.querySelector('.sign-in-form-region')).backgroundColor",
    )?;
    if light_background == dark_background {
        let theme = cdp.evaluate(
            "document.querySelector('[data-erpx-component=provider]').getAttribute('data-theme')",
        )?;
        return Err(format!(
            "light and dark media preferences resolved to the same page colors ({light_background}, theme={theme})"
        )
        .into());
    }
    cdp.screenshot(&evidence_dir.join("mobile-th-dark.png"))?;

    let report = json!({
        "schema_version": 1,
        "captured_at": Utc::now().to_rfc3339(),
        "base_url": base_url,
        "chromium_cdp": true,
        "virtual_authenticator": { "registration": true, "authentication": true },
        "viewports": ["desktop", "390px"],
        "locales": ["en", "th"],
        "themes": ["light", "dark"],
        "keyboard_focus": true,
        "console_errors": cdp.console_errors,
        "network_failures": cdp.network_failures,
        "network_http_errors": cdp.network_http_errors,
        "locale_control_hydrated_without_reload": true,
    });
    fs::write(
        evidence_dir.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if report["console_errors"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty())
        || report["network_failures"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
        || report["network_http_errors"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    {
        return Err(format!(
            "web-e2e captured unexplained browser errors; inspect {}",
            evidence_dir.display()
        )
        .into());
    }
    println!("Web E2E evidence: {}", evidence_dir.display());
    Ok(())
}
