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
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
type CdpSocket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

const DEFAULT_BASE_URL: &str = "http://localhost:3004";
const DEVELOPMENT_CONTROLLER_PRINCIPAL: &str = "local:dev-controller";
const NATIVE_DEMO_PRINCIPAL: &str = "local:native-demo";
const DEVICE_REVOCATION_TARGET_LABEL: &str = "ERPX E2E revoke target";
const SIGN_IN_FORM_SELECTOR: &str = "[data-testid='sign-in-form']";
const SIGN_IN_ERROR_SELECTOR: &str = "[data-testid='sign-in-error'][role='alert']";
const ROLE_OPTION_SELECTOR: &str = "[data-testid='role-option'][data-selection-id][data-role-key='platform_holding_administrator']";
const IDENTITY_ROLE_OPTION_SELECTOR: &str =
    "[data-testid='role-option'][data-selection-id][data-role-key='customer_security_head']";
const FINANCE_ROLE_OPTION_SELECTOR: &str =
    "[data-testid='role-option'][data-selection-id][data-role-key='finance_viewer']";
const TOP_LEVEL_DATA_ROUTES: [&str; 3] = ["/work", "/processes", "/insights"];
const WORKSPACE_ROUTES: [&str; 21] = [
    "/operations",
    "/finance",
    "/sales",
    "/procurement",
    "/inventory",
    "/asset",
    "/maintenance",
    "/quality",
    "/marketing",
    "/people",
    "/pos",
    "/legal",
    "/manufacturing",
    "/planning",
    "/project",
    "/commerce",
    "/talent",
    "/learning",
    "/benefits",
    "/field-service",
    "/ai",
];

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
            Some("Network.loadingFailed") if !is_expected_navigation_cancellation(&value) => {
                self.network_failures.push(value);
            }
            Some("Network.responseReceived") => {
                let url = value
                    .pointer("/params/response/url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let status = value
                    .pointer("/params/response/status")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                if status >= 400.0 && (url.starts_with("http://") || url.starts_with("https://")) {
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
        let previous = self.evaluate(
            "({href: location.href, timeOrigin: performance.timeOrigin, ready: document.readyState})",
        )?;
        let result = self.command("Page.navigate", json!({ "url": url }))?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            return Err(format!("browser navigation failed for {url}: {error}").into());
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let state = self.evaluate(
                "({href: location.href, timeOrigin: performance.timeOrigin, ready: document.readyState})",
            )?;
            let changed = state.get("href") != previous.get("href")
                || state.get("timeOrigin") != previous.get("timeOrigin");
            if changed && state.get("ready").and_then(Value::as_str) == Some("complete") {
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

    fn wait_for(&mut self, expression: &str, description: &str) -> TaskResult<Value> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut last = Value::Null;
        while Instant::now() < deadline {
            last = self.evaluate(expression)?;
            if last.as_bool() == Some(true) {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!("browser timed out waiting for {description}; last value: {last}").into())
    }

    fn wait_for_selector(&mut self, selector: &str) -> TaskResult {
        let selector = serde_json::to_string(selector)?;
        self.wait_for(
            &format!("Boolean(document.querySelector({selector}))"),
            &format!("selector {selector}"),
        )?;
        Ok(())
    }
}

fn is_expected_navigation_cancellation(event: &Value) -> bool {
    let canceled = event
        .pointer("/params/canceled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let error = event
        .pointer("/params/errorText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let resource_type = event
        .pointer("/params/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    (resource_type == "Document" && (canceled || error == "net::ERR_ABORTED"))
        || (resource_type == "Stylesheet" && canceled && error == "net::ERR_ABORTED")
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

fn development_controller_password(root: &Path) -> TaskResult<String> {
    let path = root.join(".runtime/erpx/secrets/dev-controller.password");
    if !path.is_file() {
        return Err(
            "development controller credential is unavailable; run `cargo xtask dev-access init`"
                .into(),
        );
    }
    let password = fs::read_to_string(path)?;
    if password.is_empty() || password.chars().any(char::is_control) {
        return Err("development controller credential is empty or invalid".into());
    }
    Ok(password)
}

fn native_demo_password(root: &Path) -> TaskResult<String> {
    let path = root.join(".runtime/erpx/secrets/native-demo-password.key");
    if !path.is_file() {
        return Err("native demo credential is unavailable; run `cargo xtask dev`".into());
    }
    let component = fs::read_to_string(path)?;
    let component = component.trim();
    if component.is_empty() || component.chars().any(char::is_control) {
        return Err("native demo credential is empty or invalid".into());
    }
    Ok(format!("ERPX-NATIVE-{component}"))
}

fn decode_totp_secret(encoded: &str) -> TaskResult<Vec<u8>> {
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    let mut decoded = Vec::new();
    for byte in encoded.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return Err("TOTP enrollment returned invalid base32 material".into()),
        };
        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            decoded.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    if decoded.len() < 10 {
        return Err("TOTP enrollment returned an undersized secret".into());
    }
    Ok(decoded)
}

fn totp_code(secret: &[u8], step: i64) -> TaskResult<String> {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret)
        .map_err(|_| "TOTP enrollment returned invalid key material")?;
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[31] & 0x0f);
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | u32::from(digest[offset + 1]) << 16
        | u32::from(digest[offset + 2]) << 8
        | u32::from(digest[offset + 3]);
    Ok(format!("{:06}", binary % 1_000_000))
}

fn current_totp_code(secret: &[u8]) -> TaskResult<(String, i64)> {
    let step = Utc::now().timestamp().div_euclid(30);
    Ok((totp_code(secret, step)?, step))
}

fn totp_code_after(secret: &[u8], previous_step: i64) -> TaskResult<String> {
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        let step = Utc::now().timestamp().div_euclid(30);
        if step > previous_step {
            thread::sleep(Duration::from_millis(300));
            return totp_code(secret, step);
        }
        if Instant::now() >= deadline {
            return Err("TOTP period did not advance within its bounded wait".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn set_hydrated_input(client: &mut CdpClient, selector: &str, value: &str) -> TaskResult {
    let selector = serde_json::to_string(selector)?;
    let value = serde_json::to_string(value)?;
    let changed = client.evaluate(&format!(
        "(() => {{ const input=document.querySelector({selector}); if(!input)return false; const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')?.set; setter?.call(input,{value}); input.dispatchEvent(new Event('input',{{bubbles:true}})); return true; }})()"
    ))?;
    if changed.as_bool() != Some(true) {
        return Err(format!("hydrated input was unavailable: {selector}").into());
    }
    Ok(())
}

fn remove_totp_in_browser(
    client: &mut CdpClient,
    base_url: &str,
    secret: &[u8],
    verified_step: i64,
) -> TaskResult<bool> {
    client.navigate(&format!("{base_url}/auth/security"))?;
    client.wait_for_selector(".security-factor-page")?;
    let enabled = client
        .evaluate("document.querySelector('.security-factor-card[data-enabled=true]') !== null")?;
    if enabled.as_bool() != Some(true) {
        return Ok(false);
    }
    client.wait_for_selector("[data-testid=totp-remove][data-dioxus-id]")?;
    client.evaluate("document.querySelector('[data-testid=totp-remove]').click()")?;
    client.wait_for_selector("[data-testid=totp-remove-confirm][data-dioxus-id]")?;
    let code = totp_code_after(secret, verified_step)?;
    set_hydrated_input(client, "[data-testid=totp-remove-code]", &code)?;
    client.wait_for(
        "!document.querySelector('[data-testid=totp-remove-confirm]').disabled",
        "valid TOTP removal proof",
    )?;
    client.evaluate("document.querySelector('[data-testid=totp-remove-confirm]').click()")?;
    client.wait_for(
        "document.querySelector('.identity-command-feedback')?.dataset.state === 'success' && document.querySelector('.security-factor-card[data-enabled=false]') !== null",
        "authoritative TOTP removal confirmation",
    )?;
    Ok(true)
}

async fn create_device_revocation_target(password: &str) -> TaskResult {
    let response = reqwest::Client::new()
        .post("http://127.0.0.1:50052/auth/login")
        .json(&json!({
            "principal_ref": NATIVE_DEMO_PRINCIPAL,
            "password": password,
            "device_label": DEVICE_REVOCATION_TARGET_LABEL,
        }))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!(
            "identity owner rejected the disposable device target with status {}",
            response.status()
        )
        .into());
    }
    let session: Value = response.json().await?;
    if session.get("session_id").and_then(Value::as_str).is_none()
        || session
            .get("access_token")
            .and_then(Value::as_str)
            .is_none()
    {
        return Err("identity owner omitted the disposable device session receipt".into());
    }
    Ok(())
}

fn submit_password_form(client: &mut CdpClient, principal: &str, password: &str) -> TaskResult {
    let form = serde_json::to_string(SIGN_IN_FORM_SELECTOR)?;
    let principal = serde_json::to_string(principal)?;
    let password = serde_json::to_string(password)?;
    let submitted = client.evaluate(&format!(
        r#"(() => {{
          const form = document.querySelector({form});
          const principal = form?.querySelector('[name=principal_ref]');
          const password = form?.querySelector('[name=password]');
          if (!form || !principal || !password) return false;
          principal.value = {principal};
          password.value = {password};
          form.submit();
          return true;
        }})()"#
    ))?;
    if submitted.as_bool() != Some(true) {
        return Err("sign-in form was unavailable for submission".into());
    }
    Ok(())
}

fn prepare_identity_principal_command(
    client: &mut CdpClient,
    action: &str,
    purpose: &str,
) -> TaskResult<Value> {
    let action = serde_json::to_string(action)?;
    client.wait_for_selector(&format!("[data-action={action}][data-dioxus-id]"))?;
    client.wait_for_selector("[data-testid='identity-command-purpose'][data-dioxus-id]")?;
    client.wait_for_selector("[data-testid='identity-command-mfa'][data-dioxus-id]")?;
    client.wait_for_selector("[data-testid='identity-command-confirm'][data-dioxus-id]")?;
    if client
        .evaluate(
            "(() => { const input=document.querySelector('[data-testid=identity-command-purpose]'); if(!input)return false; input.focus(); return document.activeElement===input; })()",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("IAM lifecycle purpose input was unavailable".into());
    }
    client.command("Input.insertText", json!({ "text": purpose }))?;
    if client
        .evaluate(
            "(() => { const input=document.querySelector('[data-testid=identity-command-mfa]'); if(!input)return false; input.focus(); return document.activeElement===input; })()",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("IAM lifecycle MFA input was unavailable".into());
    }
    client.command(
        "Input.insertText",
        json!({ "text": "development-mfa-proof" }),
    )?;
    client.evaluate(
        "(() => { const input=document.querySelector('[data-testid=identity-command-confirm]'); if(!input)return false; if(!input.checked)input.click(); return input.checked; })()",
    )?;
    let expression = format!(
        "(() => {{ const button=document.querySelector(`[data-action=${{{action}}}]`); return Boolean(button && !button.disabled && button.dataset.purposeReady === 'true' && button.dataset.mfaReady === 'true' && button.dataset.confirmed === 'true'); }})()"
    );
    client.wait_for(&expression, "hydrated IAM lifecycle command form")?;
    client.evaluate(&format!(
        "(() => {{ const button=document.querySelector(`[data-action=${{{action}}}]`); return button && {{disabled:button.disabled,purposeReady:button.dataset.purposeReady,mfaReady:button.dataset.mfaReady,confirmed:button.dataset.confirmed}}; }})()"
    ))
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
    cdp.wait_for_selector(SIGN_IN_FORM_SELECTOR)?;
    passkey_round_trip(&mut cdp)?;
    let focused = cdp.evaluate(
        "(() => { const e=document.querySelector('[data-testid=sign-in-account]'); if(!e)return false; e.focus(); return document.activeElement===e; })()",
    )?;
    if focused.as_bool() != Some(true) {
        return Err("keyboard focus probe failed".into());
    }

    let sign_in_url = format!("{base_url}/auth/sign-in?return_to=%2Fwork");
    cdp.navigate(&sign_in_url)?;
    cdp.wait_for_selector(SIGN_IN_FORM_SELECTOR)?;
    submit_password_form(&mut cdp, DEVELOPMENT_CONTROLLER_PRINCIPAL, "")?;
    cdp.wait_for(
        "location.search.includes('auth_error=invalid_request')",
        "server-side sign-in validation redirect",
    )?;
    cdp.wait_for_selector(SIGN_IN_ERROR_SELECTOR)?;
    cdp.screenshot(&evidence_dir.join("sign-in-invalid.png"))?;

    let password = development_controller_password(root)?;
    cdp.navigate(&sign_in_url)?;
    cdp.wait_for_selector(SIGN_IN_FORM_SELECTOR)?;
    submit_password_form(&mut cdp, DEVELOPMENT_CONTROLLER_PRINCIPAL, &password)?;
    cdp.wait_for(
        "location.pathname === '/auth/accounts' || location.pathname === '/work'",
        "authenticated account or work route",
    )?;
    let role_selection_required = cdp.evaluate("location.pathname === '/auth/accounts'")?;
    if role_selection_required.as_bool() == Some(true) {
        cdp.wait_for_selector(ROLE_OPTION_SELECTOR)?;
        let selector = serde_json::to_string(ROLE_OPTION_SELECTOR)?;
        let clicked = cdp.evaluate(&format!(
            "(() => {{ const option=document.querySelector({selector}); if (!option) return false; option.click(); return true; }})()"
        ))?;
        if clicked.as_bool() != Some(true) {
            return Err("authorized role option was unavailable".into());
        }
        cdp.wait_for(
            "location.pathname === '/work'",
            "role activation return path",
        )?;
    }
    cdp.wait_for_selector(".workspace-route")?;
    if cdp
        .evaluate("Boolean(document.querySelector('[data-testid=sign-in-form]'))")?
        .as_bool()
        != Some(false)
    {
        return Err("authenticated workspace still rendered the sign-in form".into());
    }

    let light_background =
        cdp.evaluate("getComputedStyle(document.querySelector('.console-shell')).backgroundColor")?;
    cdp.screenshot(&evidence_dir.join("desktop-en-light.png"))?;
    let locale_time_origin = cdp.evaluate("performance.timeOrigin")?;
    cdp.wait_for_selector(".ui-locale-toggle[data-dioxus-id]")?;
    let locale_clicked = cdp.evaluate(
        "(() => { const button=document.querySelector('.ui-locale-toggle'); if (!button) return false; button.click(); return true; })()",
    )?;
    if locale_clicked.as_bool() != Some(true) {
        return Err("locale control was unavailable".into());
    }
    cdp.wait_for(
        "document.querySelector('[data-erpx-component=provider]')?.getAttribute('data-locale') === 'th'",
        "Thai locale update",
    )?;
    if cdp.evaluate("performance.timeOrigin")? != locale_time_origin {
        return Err("locale control reloaded the document instead of updating hydrated UI".into());
    }

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
    cdp.navigate(&format!("{base_url}/modules"))?;
    cdp.wait_for(
        "document.body.innerText.includes('โมดูลและพื้นที่ทำงาน')",
        "Thai module catalog",
    )?;
    let dark_background =
        cdp.evaluate("getComputedStyle(document.querySelector('.console-shell')).backgroundColor")?;
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
    let mobile_layout_safe = cdp.evaluate(
        "document.body.scrollWidth <= innerWidth && [...document.querySelectorAll('a,button,input,textarea,select')].every((element) => { const rect=element.getBoundingClientRect(); return rect.width === 0 || rect.height === 0 || (rect.width >= 40 && rect.height >= 40); })",
    )?;
    if mobile_layout_safe.as_bool() != Some(true) {
        return Err("mobile module catalog overflowed or exposed a control below 40px".into());
    }

    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
    )?;
    cdp.command(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-color-scheme", "value": "light" }] }),
    )?;
    cdp.navigate(&format!("{base_url}/auth/accounts?return_to=%2Ffinance"))?;
    cdp.wait_for_selector(FINANCE_ROLE_OPTION_SELECTOR)?;
    let finance_role_selector = serde_json::to_string(FINANCE_ROLE_OPTION_SELECTOR)?;
    if cdp
        .evaluate(&format!(
            "(() => {{ const option=document.querySelector({finance_role_selector}); if (!option || option.disabled) return false; option.click(); return true; }})()"
        ))?
        .as_bool()
        != Some(true)
    {
        return Err("Finance viewer role was unavailable for workspace E2E".into());
    }
    cdp.wait_for(
        "location.pathname === '/finance'",
        "Finance viewer role activation",
    )?;
    cdp.wait_for_selector(".workspace-route")?;
    if cdp
        .evaluate(
            "['fresh','stale'].includes(document.querySelector('.workspace-route')?.dataset.freshness) && !document.querySelector('.route-error') && !document.body.innerText.includes('This route is temporarily unavailable')",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("Finance workspace did not render from its authorized projection".into());
    }
    cdp.navigate(&format!("{base_url}/modules"))?;
    cdp.wait_for_selector(".module-catalog__search input[data-dioxus-id]")?;
    let authorized_workspace_routes = serde_json::from_value::<Vec<String>>(cdp.evaluate(
        "[...document.querySelectorAll('.module-card')].map((card) => new URL(card.href).pathname)",
    )?)?;
    if authorized_workspace_routes.iter().any(|route| {
        !WORKSPACE_ROUTES.contains(&route.as_str())
            || authorized_workspace_routes
                .iter()
                .filter(|candidate| *candidate == route)
                .count()
                != 1
    }) {
        return Err("authorized module catalog contains an unknown or duplicate route".into());
    }
    if !authorized_workspace_routes
        .iter()
        .any(|route| route == "/finance")
    {
        return Err("Finance viewer module catalog omitted the Finance workspace".into());
    }
    let module_search_focused = cdp.evaluate(
        "(() => { const input=document.querySelector('.module-catalog__search input'); if (!input) return false; input.focus(); return document.activeElement === input; })()",
    )?;
    if module_search_focused.as_bool() != Some(true) {
        return Err("module search input could not receive keyboard focus".into());
    }
    let search_probe = cdp
        .evaluate("document.querySelector('.module-card strong')?.innerText || '__no_authorized_workspace__'")?
        .as_str()
        .unwrap_or("__no_authorized_workspace__")
        .to_owned();
    cdp.command("Input.insertText", json!({ "text": search_probe }))?;
    let expected_search_results = usize::from(!authorized_workspace_routes.is_empty());
    cdp.wait_for(
        &format!("document.querySelectorAll('.module-card').length === {expected_search_results}"),
        "module search result",
    )?;
    cdp.navigate(&format!("{base_url}/modules"))?;
    cdp.wait_for(
        &format!(
            "document.querySelectorAll('.module-card').length === {}",
            authorized_workspace_routes.len()
        ),
        "cleared module search",
    )?;

    let mut top_level_data_routes = Vec::with_capacity(TOP_LEVEL_DATA_ROUTES.len());
    for route in TOP_LEVEL_DATA_ROUTES {
        cdp.navigate(&format!("{base_url}{route}"))?;
        cdp.wait_for_selector(".workspace-route")?;
        let layout_safe = cdp.evaluate(
            "!document.querySelector('.route-error') && !document.body.innerText.includes('This route is temporarily unavailable')",
        )?;
        if layout_safe.as_bool() != Some(true) {
            return Err(
                format!("top-level empty route failed its layout/data probe: {route}").into(),
            );
        }
        top_level_data_routes.push(route);
    }

    let mut workspace_routes = Vec::with_capacity(authorized_workspace_routes.len());
    for route in &authorized_workspace_routes {
        cdp.navigate(&format!("{base_url}{route}"))?;
        cdp.wait_for_selector(".workspace-route")?;
        let expected_path = serde_json::to_string(route)?;
        let valid = cdp.evaluate(&format!(
            "(() => {{ const root=document.querySelector('.workspace-route'); const freshness=root?.dataset.freshness; const rows=document.querySelectorAll('.record-row').length; const empty=document.querySelectorAll('.empty-state').length; const boundedEmpty=rows === 0 ? empty === 1 : empty === 0; const unavailableSafe=freshness !== 'unavailable' || rows === 0; return location.pathname === {expected_path} && ['fresh','stale','unavailable'].includes(freshness) && boundedEmpty && unavailableSafe && !document.querySelector('.route-error') && !document.body.innerText.includes('This route is temporarily unavailable') && document.querySelectorAll('main').length === 1 && document.body.scrollWidth <= innerWidth && !document.body.innerText.includes('ข้อมูลตัวอย่าง'); }})()"
        ))?;
        if valid.as_bool() != Some(true) {
            return Err(format!("workspace route did not render successfully: {route}").into());
        }
        if route == "/finance"
            && cdp
                .evaluate(
                    "['fresh','stale'].includes(document.querySelector('.workspace-route')?.dataset.freshness)",
                )?
                .as_bool()
                != Some(true)
        {
            return Err("Finance workspace did not reach its authorized projection".into());
        }
        if route == "/finance" {
            cdp.wait_for_selector(".console-finance-menu__trigger[data-dioxus-id]")?;
            let finance_menu_opened = cdp.evaluate(
                "(() => { const trigger=document.querySelector('.console-finance-menu__trigger'); if (!trigger) return false; if (trigger.getAttribute('aria-expanded') !== 'true') trigger.click(); return true; })()",
            )?;
            if finance_menu_opened.as_bool() != Some(true) {
                return Err(format!("finance menu trigger was unavailable: {route}").into());
            }
            cdp.wait_for(
                &format!(
                    "Boolean(document.querySelector(`.console-finance-menu a[aria-current='page'][href={}]`))",
                    serde_json::to_string(route)?
                ),
                &format!("active finance navigation for {route}"),
            )?;
        } else {
            cdp.wait_for(
                &format!(
                    "Boolean(document.querySelector(`a[aria-current='page'][href={}]`))",
                    serde_json::to_string(route)?
                ),
                &format!("active workspace navigation for {route}"),
            )?;
        }
        workspace_routes.push(route.clone());
    }

    cdp.navigate(&format!(
        "{base_url}/auth/accounts?return_to=%2Fidentity%2Fprincipal"
    ))?;
    cdp.wait_for_selector(IDENTITY_ROLE_OPTION_SELECTOR)?;
    let identity_role_selector = serde_json::to_string(IDENTITY_ROLE_OPTION_SELECTOR)?;
    if cdp
        .evaluate(&format!(
            "(() => {{ const option=document.querySelector({identity_role_selector}); if (!option || option.disabled) return false; option.click(); return true; }})()"
        ))?
        .as_bool()
        != Some(true)
    {
        return Err("customer security role was unavailable for IAM E2E".into());
    }
    cdp.wait_for(
        "location.pathname === '/identity/principal'",
        "customer security role activation",
    )?;
    cdp.wait_for_selector(".identity-access-surface .record-row")?;
    if cdp
        .evaluate(
            "(() => { const row=[...document.querySelectorAll('.record-row')].find((candidate) => candidate.innerText.includes('Suspended Auditor')); const link=row?.querySelector('a'); if(!link)return false; link.click(); return true; })()",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("suspended IAM fixture principal was unavailable".into());
    }
    cdp.wait_for(
        "location.pathname.startsWith('/identity/principal/') && document.querySelector('.route-meta')?.innerText.includes('suspended')",
        "suspended principal detail",
    )?;
    let baseline_version = cdp
        .evaluate(
            "Number(document.querySelector('.route-meta')?.innerText.match(/\\bv(\\d+)\\b/)?.[1])",
        )?
        .as_u64()
        .filter(|version| *version >= 3)
        .ok_or("suspended IAM fixture version was unavailable")?;
    let reactivated_version = baseline_version.saturating_add(1);
    let resuspended_version = baseline_version.saturating_add(2);
    cdp.wait_for_selector("[data-action='ReactivatePrincipal']")?;
    if cdp
        .evaluate(
            r#"(() => {
              const original = window.fetch.bind(window);
              window.__erpxOriginalFetch = original;
              window.__erpxMutationCapture = null;
              window.__erpxFirstMutationResponse = null;
              window.fetch = async (input, init) => {
                const request = input instanceof Request ? input.clone() : new Request(input, init);
                const isMutation = request.url.includes('/identity/principal/command');
                if (isMutation) {
                  const copy = request.clone();
                  window.__erpxMutationCapture = {
                    url: copy.url,
                    method: copy.method,
                    headers: [...copy.headers.entries()],
                    credentials: copy.credentials,
                    body: Array.from(new Uint8Array(await copy.arrayBuffer()))
                  };
                }
                const response = await original(input, init);
                if (isMutation) {
                  const responseCopy = response.clone();
                  window.__erpxFirstMutationResponse = {
                    status: response.status,
                    replayed: response.headers.get('x-erpx-idempotency-replayed'),
                    body: await responseCopy.text()
                  };
                }
                return response;
              };
              return true;
            })()"#,
        )?
        .as_bool()
        != Some(true)
    {
        return Err("IAM mutation capture could not be installed".into());
    }
    let reactivate_state = prepare_identity_principal_command(
        &mut cdp,
        "ReactivatePrincipal",
        "Restore the suspended E2E auditor for a lifecycle verification",
    )?;
    if reactivate_state
        .pointer("/disabled")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            format!("principal reactivate command was not ready: {reactivate_state}").into(),
        );
    }
    cdp.evaluate("document.querySelector('[data-action=ReactivatePrincipal]').click()")?;
    cdp.wait_for(
        "['success','error'].includes(document.querySelector('.identity-command-feedback')?.dataset.state)",
        "terminal principal reactivation result",
    )?;
    let first_mutation = cdp.evaluate("window.__erpxFirstMutationResponse")?;
    let reactivate_result = cdp.evaluate(
        "(() => { const feedback=document.querySelector('.identity-command-feedback'); return {state:feedback?.dataset.state,message:feedback?.innerText,meta:document.querySelector('.route-meta')?.innerText}; })()",
    )?;
    if reactivate_result.pointer("/state").and_then(Value::as_str) != Some("success")
        || !reactivate_result
            .pointer("/meta")
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.contains("active") && value.contains(&format!("v{reactivated_version}"))
            })
    {
        let request_meta = cdp.evaluate(
            "(() => { const capture=window.__erpxMutationCapture; return capture && {url:capture.url,headers:capture.headers,documentOrigin:location.origin}; })()",
        )?;
        return Err(format!(
            "principal reactivation failed: result={reactivate_result}, request={request_meta}, response={first_mutation}, console_errors={:?}, network_http_errors={:?}",
            cdp.console_errors, cdp.network_http_errors
        )
        .into());
    }
    if first_mutation.pointer("/status").and_then(Value::as_u64) != Some(200)
        || first_mutation.pointer("/replayed").and_then(Value::as_str) != Some("false")
    {
        return Err(format!("first IAM mutation receipt was invalid: {first_mutation}").into());
    }
    let replay = cdp.evaluate(
        r#"(async () => {
          const captured=window.__erpxMutationCapture;
          if(!captured || !window.__erpxOriginalFetch)return null;
          const response=await window.__erpxOriginalFetch(captured.url, {
            method: captured.method,
            headers: new Headers(captured.headers),
            credentials: captured.credentials,
            body: new Uint8Array(captured.body)
          });
          return {
            status: response.status,
            replayed: response.headers.get('x-erpx-idempotency-replayed'),
            bytes: (await response.arrayBuffer()).byteLength
          };
        })()"#,
    )?;
    if replay.pointer("/status").and_then(Value::as_u64) != Some(200)
        || replay.pointer("/replayed").and_then(Value::as_str) != Some("true")
        || replay.pointer("/bytes").and_then(Value::as_u64) == Some(0)
    {
        return Err(format!("duplicate IAM mutation did not replay: {replay}").into());
    }
    let current_url = cdp.evaluate("location.href")?;
    let current_url = current_url
        .as_str()
        .ok_or("IAM detail URL was unavailable after replay")?
        .to_owned();
    cdp.navigate(&current_url)?;
    cdp.wait_for(
        &format!(
            "document.querySelector('.route-meta')?.innerText.includes('active') && document.querySelector('.route-meta')?.innerText.includes('v{reactivated_version}')"
        ),
        "duplicate IAM mutation retained aggregate version",
    )?;

    let stale_url = cdp.evaluate("location.href")?;
    let stale_url = stale_url.as_str().ok_or("IAM detail URL was unavailable")?;
    let stale_websocket = page_websocket(&endpoint, stale_url).await?;
    let mut stale_cdp = CdpClient::connect(&stale_websocket)?;
    for method in [
        "Page.enable",
        "Runtime.enable",
        "Log.enable",
        "Network.enable",
    ] {
        stale_cdp.command(method, json!({}))?;
    }
    stale_cdp.navigate(stale_url)?;
    stale_cdp.wait_for(
        &format!(
            "document.querySelector('.route-meta')?.innerText.includes('active') && document.querySelector('.route-meta')?.innerText.includes('v{reactivated_version}')"
        ),
        "stale principal detail tab",
    )?;

    let suspend_state = prepare_identity_principal_command(
        &mut cdp,
        "SuspendPrincipal",
        "Suspend the E2E auditor after the successful reactivation verification",
    )?;
    if suspend_state.pointer("/disabled").and_then(Value::as_bool) != Some(false) {
        return Err(format!("principal suspend command was not ready: {suspend_state}").into());
    }
    cdp.evaluate("document.querySelector('[data-action=SuspendPrincipal]').click()")?;
    cdp.wait_for(
        &format!(
            "document.querySelector('.identity-command-feedback')?.dataset.state === 'success' && document.querySelector('.route-meta')?.innerText.includes('suspended') && document.querySelector('.route-meta')?.innerText.includes('v{resuspended_version}')"
        ),
        "suspended principal projection version",
    )?;

    let disable_state = prepare_identity_principal_command(
        &mut stale_cdp,
        "DisablePrincipal",
        "Attempt a stale disable to verify optimistic concurrency",
    )?;
    if disable_state.pointer("/disabled").and_then(Value::as_bool) != Some(false) {
        return Err(
            format!("stale principal disable command was not ready: {disable_state}").into(),
        );
    }
    stale_cdp.evaluate("document.querySelector('[data-action=DisablePrincipal]').click()")?;
    stale_cdp.wait_for(
        &format!(
            "document.querySelector('.identity-command-feedback')?.dataset.state === 'error' && document.querySelector('.route-meta')?.innerText.includes('suspended') && document.querySelector('.route-meta')?.innerText.includes('v{resuspended_version}')"
        ),
        "stale principal conflict recovery",
    )?;
    stale_cdp.screenshot(&evidence_dir.join("identity-principal-conflict.png"))?;
    cdp.console_errors.append(&mut stale_cdp.console_errors);
    cdp.network_failures.append(&mut stale_cdp.network_failures);
    cdp.network_http_errors
        .append(&mut stale_cdp.network_http_errors);

    let public_id = "8f9f3a8f-24f0-4c9a-a7e8-6bd0d9d4e4d2";
    cdp.navigate(&format!("{base_url}/public/marketing/form/{public_id}"))?;
    cdp.wait_for_selector(".public-marketing-form")?;
    if cdp
        .evaluate(
            "document.querySelectorAll('.public-marketing-form [required]').length === 4 && document.querySelector('.public-marketing-form')?.method === 'post' && document.querySelector('.public-marketing-form textarea')?.value === '' && !location.search.includes('name=') && !location.search.includes('email=') && !location.search.includes('message=') && document.body.scrollWidth <= innerWidth",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("public marketing form requirements or layout regressed".into());
    }
    cdp.navigate(&format!("{base_url}/public/marketing/preview/{public_id}"))?;
    cdp.wait_for(
        "document.querySelectorAll('.public-preview-highlights').length === 0",
        "unpublished public preview fails closed without generated highlights",
    )?;

    let native_password = native_demo_password(root)?;
    let native_password_json = serde_json::to_string(&native_password)?;
    let native_login = cdp.evaluate(&format!(
        r#"(async () => {{
          const response = await fetch('/api/auth/password', {{
            method: 'POST', credentials: 'same-origin',
            headers: {{'content-type': 'application/json', 'x-erpx-csrf': 'console'}},
            body: JSON.stringify({{
              principal_ref: 'local:native-demo',
              password: {native_password_json},
              device_label: 'ERPX E2E current device'
            }})
          }});
          return {{status: response.status, ok: response.ok}};
        }})()"#
    ))?;
    if native_login.pointer("/ok").and_then(Value::as_bool) != Some(true) {
        return Err(format!("native demo BFF login failed: {native_login}").into());
    }
    create_device_revocation_target(&native_password).await?;

    cdp.command("Emulation.setLocaleOverride", json!({ "locale": "en-US" }))?;
    let english_cookie = cdp.command(
        "Network.setCookie",
        json!({
            "name": "erpx_locale", "value": "en", "domain": "localhost",
            "path": "/", "sameSite": "Strict"
        }),
    )?;
    if english_cookie.get("success").and_then(Value::as_bool) == Some(false) {
        return Err("Chromium rejected the English locale cookie".into());
    }
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
    )?;

    let mut enrolled_totp_secret = None::<Vec<u8>>;
    let mut enrolled_totp_step = None::<i64>;
    let security_lifecycle = (|| -> TaskResult {
        cdp.navigate(&format!("{base_url}/auth/security"))?;
        cdp.wait_for_selector(".security-factor-page")?;
        cdp.wait_for_selector("[data-testid=totp-enroll][data-dioxus-id]")?;
        if cdp
            .evaluate(
                "document.body.innerText.includes('Authoritative factor state') && document.querySelector('.security-factor-card[data-enabled=false]') !== null && !document.querySelector('[data-testid=recovery-codes]')",
            )?
            .as_bool()
            != Some(true)
        {
            return Err("security factor page was not authoritative and secret-free".into());
        }
        cdp.evaluate("document.querySelector('[data-testid=totp-enroll]').click()")?;
        cdp.wait_for_selector("[role=alertdialog] [data-testid=totp-generate]")?;
        if cdp
            .evaluate(
                "document.querySelector('[role=alertdialog]')?.innerText.includes('shown once') && !document.querySelector('[data-testid=totp-secret]')",
            )?
            .as_bool()
            != Some(true)
        {
            return Err("TOTP enrollment confirmation omitted its one-time boundary".into());
        }
        cdp.screenshot(&evidence_dir.join("totp-enrollment-confirmation.png"))?;
        cdp.evaluate("document.querySelector('[data-testid=totp-generate]').click()")?;
        cdp.wait_for_selector("[data-testid=totp-secret]")?;
        let encoded = cdp.evaluate(
            "document.querySelector('[data-testid=totp-secret]')?.innerText.trim() || ''",
        )?;
        let encoded = encoded
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or("TOTP enrollment omitted one-time secret material")?;
        let secret = decode_totp_secret(encoded)?;
        let encoded_json = serde_json::to_string(encoded)?;
        if cdp
            .evaluate(&format!(
                "!location.href.includes({encoded_json}) && !JSON.stringify(history.state ?? {{}}).includes({encoded_json})"
            ))?
            .as_bool()
            != Some(true)
        {
            return Err("TOTP one-time material leaked into URL or browser history".into());
        }
        enrolled_totp_secret = Some(secret.clone());
        let (code, verified_step) = current_totp_code(&secret)?;
        enrolled_totp_step = Some(verified_step);
        set_hydrated_input(&mut cdp, "[data-testid=totp-code]", &code)?;
        cdp.wait_for(
            "!document.querySelector('[data-testid=totp-verify]').disabled",
            "valid TOTP enrollment proof",
        )?;
        cdp.evaluate("document.querySelector('[data-testid=totp-verify]').click()")?;
        cdp.wait_for(
            "document.querySelector('.identity-command-feedback')?.dataset.state === 'success' && document.querySelector('.security-factor-card[data-enabled=true]') !== null && !document.querySelector('[data-testid=totp-secret]')",
            "authoritative TOTP enrollment confirmation",
        )?;

        cdp.navigate(&format!("{base_url}/auth/recovery"))?;
        cdp.wait_for_selector("[data-testid=recovery-rotate][data-dioxus-id]")?;
        if cdp
            .evaluate("!document.querySelector('[data-testid=recovery-codes]')")?
            .as_bool()
            != Some(true)
        {
            return Err("recovery page disclosed codes before a confirmed rotation".into());
        }
        cdp.evaluate("document.querySelector('[data-testid=recovery-rotate]').click()")?;
        cdp.wait_for_selector("[role=alertdialog] [data-testid=recovery-rotate-confirm]")?;
        if cdp
            .evaluate(
                "document.querySelector('[role=alertdialog]')?.innerText.includes('displayed once') && !document.querySelector('[data-testid=recovery-codes]')",
            )?
            .as_bool()
            != Some(true)
        {
            return Err("recovery rotation confirmation omitted its one-time boundary".into());
        }
        cdp.screenshot(&evidence_dir.join("recovery-rotation-confirmation.png"))?;
        cdp.evaluate("document.querySelector('[data-testid=recovery-rotate-cancel]').click()")?;
        cdp.wait_for(
            "!document.querySelector('[data-testid=recovery-rotate-confirm]')",
            "recovery rotation cancellation",
        )?;

        if !remove_totp_in_browser(&mut cdp, &base_url, &secret, verified_step)? {
            return Err("newly enrolled TOTP factor disappeared before cleanup".into());
        }
        enrolled_totp_secret = None;
        enrolled_totp_step = None;
        Ok(())
    })();
    if let Err(error) = security_lifecycle {
        let cleanup = match (&enrolled_totp_secret, enrolled_totp_step) {
            (Some(secret), Some(step)) => remove_totp_in_browser(&mut cdp, &base_url, secret, step)
                .map(|removed| format!("cleanup attempted; factor removed={removed}"))
                .unwrap_or_else(|cleanup_error| format!("cleanup failed: {cleanup_error}")),
            _ => "cleanup not required because enrollment was not verified".to_owned(),
        };
        return Err(format!("security lifecycle failed: {error}; {cleanup}").into());
    }

    let mut device_url = format!("{base_url}/auth/devices");
    let mut target_found = false;
    for _ in 0..10 {
        cdp.navigate(&device_url)?;
        cdp.wait_for_selector(".security-device-list")?;
        let probe = cdp.evaluate(&format!(
            "(() => {{ const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => candidate.querySelector('h3')?.innerText === {} && candidate.dataset.revoked === 'false'); return {{found:Boolean(card), currentFirst:document.querySelector('.security-device-card')?.dataset.current === 'true', next:document.querySelector('a[href^=\"/auth/devices?cursor=\"]')?.href || null}}; }})()",
            serde_json::to_string(DEVICE_REVOCATION_TARGET_LABEL)?
        ))?;
        if device_url.ends_with("/auth/devices")
            && probe.pointer("/currentFirst").and_then(Value::as_bool) != Some(true)
        {
            return Err(
                "the authoritative device page did not keep the current device first".into(),
            );
        }
        if probe.pointer("/found").and_then(Value::as_bool) == Some(true) {
            target_found = true;
            break;
        }
        let Some(next) = probe.get("next").and_then(Value::as_str) else {
            break;
        };
        device_url = next.to_owned();
    }
    if !target_found {
        return Err("the disposable device target was absent from bounded pagination".into());
    }
    let target_label_json = serde_json::to_string(DEVICE_REVOCATION_TARGET_LABEL)?;
    cdp.wait_for(
        &format!(
            "(() => {{ const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => candidate.querySelector('h3')?.innerText === {target_label_json} && candidate.dataset.revoked === 'false'); return Boolean(card?.querySelector('[data-testid=device-revoke][data-dioxus-id]')); }})()"
        ),
        "hydrated disposable device revoke action",
    )?;
    if cdp
        .evaluate(&format!(
            "(() => {{ const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => candidate.querySelector('h3')?.innerText === {target_label_json} && candidate.dataset.revoked === 'false'); const button=card?.querySelector('[data-testid=device-revoke][data-dioxus-id]'); if(!button)return false; button.click(); return true; }})()"
        ))?
        .as_bool()
        != Some(true)
    {
        return Err("the disposable device target had no revoke action".into());
    }
    cdp.wait_for_selector("[role=alertdialog] [data-testid=device-revoke-confirm]")?;
    let confirmation_safe = cdp.evaluate(
        "document.querySelector('[role=alertdialog]')?.innerText.includes('does not delete ERPX business data') && !document.querySelector('.identity-command-feedback')",
    )?;
    if confirmation_safe.as_bool() != Some(true) {
        return Err("device revocation confirmation omitted its data-safety boundary".into());
    }
    cdp.screenshot(&evidence_dir.join("device-revoke-confirmation.png"))?;
    cdp.evaluate("document.querySelector('[data-testid=device-revoke-confirm]').click()")?;
    cdp.wait_for(
        &format!(
            "(() => {{ const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => candidate.querySelector('h3')?.innerText === {target_label_json}); return card?.dataset.revoked === 'true' && document.querySelector('.identity-command-feedback')?.dataset.state === 'success'; }})()"
        ),
        "authoritative device revocation confirmation",
    )?;

    let cleanup_labels =
        serde_json::to_string(&[DEVICE_REVOCATION_TARGET_LABEL, "ERPX E2E current device"])?;
    for _ in 0..20 {
        let mut cleanup_url = format!("{base_url}/auth/devices");
        let mut cleanup_found = false;
        for _ in 0..10 {
            cdp.navigate(&cleanup_url)?;
            cdp.wait_for_selector(".security-device-list")?;
            let probe = cdp.evaluate(&format!(
                "(() => {{ const labels={cleanup_labels}; const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => labels.includes(candidate.querySelector('h3')?.innerText) && candidate.dataset.current !== 'true' && candidate.dataset.revoked === 'false'); return {{found:Boolean(card), next:document.querySelector('a[href^=\"/auth/devices?cursor=\"]')?.href || null}}; }})()"
            ))?;
            if probe.pointer("/found").and_then(Value::as_bool) == Some(true) {
                cleanup_found = true;
                break;
            }
            let Some(next) = probe.get("next").and_then(Value::as_str) else {
                break;
            };
            cleanup_url = next.to_owned();
        }
        if !cleanup_found {
            break;
        }
        cdp.wait_for(
            &format!(
                "(() => {{ const labels={cleanup_labels}; const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => labels.includes(candidate.querySelector('h3')?.innerText) && candidate.dataset.current !== 'true' && candidate.dataset.revoked === 'false'); return Boolean(card?.querySelector('[data-testid=device-revoke][data-dioxus-id]')); }})()"
            ),
            "hydrated disposable device cleanup action",
        )?;
        if cdp
            .evaluate(&format!(
                "(() => {{ const labels={cleanup_labels}; const card=[...document.querySelectorAll('.security-device-card')].find((candidate) => labels.includes(candidate.querySelector('h3')?.innerText) && candidate.dataset.current !== 'true' && candidate.dataset.revoked === 'false'); const button=card?.querySelector('[data-testid=device-revoke][data-dioxus-id]'); if(!button)return false; button.click(); return true; }})()"
            ))?
            .as_bool()
            != Some(true)
        {
            return Err("a disposable device cleanup action was unavailable".into());
        }
        cdp.wait_for_selector("[role=alertdialog] [data-testid=device-revoke-confirm]")?;
        cdp.evaluate("document.querySelector('[data-testid=device-revoke-confirm]').click()")?;
        cdp.wait_for(
            "document.querySelector('.identity-command-feedback')?.dataset.state === 'success'",
            "disposable device cleanup confirmation",
        )?;
    }

    cdp.command("Emulation.setLocaleOverride", json!({ "locale": "th-TH" }))?;
    let thai_cookie = cdp.command(
        "Network.setCookie",
        json!({
            "name": "erpx_locale", "value": "th", "domain": "localhost",
            "path": "/", "sameSite": "Strict"
        }),
    )?;
    if thai_cookie.get("success").and_then(Value::as_bool) == Some(false) {
        return Err("Chromium rejected the Thai locale cookie".into());
    }
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 390, "height": 844, "deviceScaleFactor": 2, "mobile": true }),
    )?;
    cdp.navigate(&format!("{base_url}/auth/security"))?;
    cdp.wait_for(
        "document.body.innerText.includes('ปัจจัยความปลอดภัย') && document.querySelector('.security-factor-card[data-enabled=false]') !== null",
        "Thai mobile security page after TOTP cleanup",
    )?;
    let mobile_security_safe = cdp.evaluate(
        "document.body.scrollWidth <= innerWidth && [...document.querySelectorAll('.security-factor-page a,.security-factor-page button')].every((element) => { const rect=element.getBoundingClientRect(); return rect.width === 0 || rect.height === 0 || (rect.width >= 40 && rect.height >= 40); })",
    )?;
    if mobile_security_safe.as_bool() != Some(true) {
        return Err("mobile security factors overflowed or exposed a control below 40px".into());
    }
    cdp.screenshot(&evidence_dir.join("mobile-th-security.png"))?;
    cdp.navigate(&format!("{base_url}/auth/recovery"))?;
    cdp.wait_for(
        "document.body.innerText.includes('รหัสกู้คืน') && document.body.innerText.includes('ต้องใช้ TOTP') && !document.querySelector('[data-testid=recovery-codes]')",
        "Thai mobile recovery page fails closed without TOTP",
    )?;
    let mobile_recovery_safe = cdp.evaluate(
        "document.body.scrollWidth <= innerWidth && [...document.querySelectorAll('.security-factor-page a,.security-factor-page button')].every((element) => { const rect=element.getBoundingClientRect(); return rect.width === 0 || rect.height === 0 || (rect.width >= 40 && rect.height >= 40); })",
    )?;
    if mobile_recovery_safe.as_bool() != Some(true) {
        return Err("mobile recovery page overflowed or exposed a control below 40px".into());
    }
    cdp.screenshot(&evidence_dir.join("mobile-th-recovery.png"))?;
    cdp.navigate(&format!("{base_url}/auth/devices"))?;
    cdp.wait_for(
        "document.body.innerText.includes('อุปกรณ์และเซสชัน') && document.querySelector('.security-device-card')?.dataset.current === 'true'",
        "Thai mobile device page with current device first",
    )?;
    let mobile_devices_safe = cdp.evaluate(
        "document.body.scrollWidth <= innerWidth && [...document.querySelectorAll('.device-management-page a,.device-management-page button')].every((element) => { const rect=element.getBoundingClientRect(); return rect.width === 0 || rect.height === 0 || (rect.width >= 40 && rect.height >= 40); })",
    )?;
    if mobile_devices_safe.as_bool() != Some(true) {
        let diagnostic = cdp.evaluate(
            "({bodyWidth:document.body.scrollWidth,innerWidth,controls:[...document.querySelectorAll('.device-management-page a,.device-management-page button')].map((element) => { const rect=element.getBoundingClientRect(); return {tag:element.tagName,text:element.innerText,width:rect.width,height:rect.height,display:getComputedStyle(element).display}; }).filter((control) => control.width !== 0 && control.height !== 0 && (control.width < 40 || control.height < 40))})",
        )?;
        return Err(format!(
            "mobile device management overflowed or exposed a control below 40px: {diagnostic}"
        )
        .into());
    }
    cdp.screenshot(&evidence_dir.join("mobile-th-devices.png"))?;

    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
    )?;
    cdp.navigate(&format!("{base_url}/auth/accounts"))?;
    cdp.wait_for_selector(".account-quick-row[data-state=selected] [data-testid=account-remove]")?;
    if cdp
        .evaluate(
            "(() => { const row=document.querySelector('.account-quick-row[data-state=selected]'); if(!row?.innerText.includes('local:native-demo'))return false; row.querySelector('[data-testid=account-remove]')?.click(); return true; })()",
        )?
        .as_bool()
        != Some(true)
    {
        return Err("current native demo account was unavailable for sign-out confirmation".into());
    }
    cdp.wait_for_selector("[data-testid=account-remove-confirm]")?;
    if cdp
        .evaluate("location.pathname === '/auth/accounts'")?
        .as_bool()
        != Some(true)
    {
        return Err("account removal executed before explicit confirmation".into());
    }
    cdp.screenshot(&evidence_dir.join("account-sign-out-confirmation.png"))?;
    cdp.evaluate("document.querySelector('[data-testid=account-remove-confirm]').click()")?;
    cdp.wait_for(
        "document.querySelector('.account-quick-row[data-state=selected]')?.innerText.includes('local:dev-controller') && ![...document.querySelectorAll('.account-quick-row')].some((row) => row.innerText.includes('local:native-demo'))",
        "confirmed sign-out and account session cleanup",
    )?;

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
        "password_sign_in": true,
        "invalid_sign_in_feedback": true,
        "role_selection": role_selection_required.as_bool() == Some(true),
        "module_search": true,
        "mobile_layout_safe": true,
        "top_level_data_routes": top_level_data_routes,
        "workspace_routes": workspace_routes,
        "workspace_detail_routes": [],
        "identity_principal_lifecycle": {
            "reactivate_read_your_writes": true,
            "duplicate_replayed": true,
            "duplicate_version_stable": true,
            "suspend_read_your_writes": true,
            "stale_version_conflict_recovered": true
        },
        "device_management": {
            "authoritative_current_first": true,
            "bounded_pagination": true,
            "explicit_revoke_confirmation": true,
            "revoke_read_your_writes": true,
            "mobile_thai_layout_safe": true,
            "account_sign_out_confirmation": true,
            "disposable_sessions_cleaned_up": true
        },
        "security_management": {
            "authoritative_factor_state": true,
            "totp_enrollment_confirmation": true,
            "totp_enrollment_read_your_writes": true,
            "totp_secret_absent_from_url_and_history": true,
            "recovery_rotation_confirmation": true,
            "recovery_codes_not_disclosed_before_commit": true,
            "totp_removal_read_your_writes": true,
            "mobile_thai_layout_safe": true,
            "test_factor_cleaned_up": true
        },
        "public_marketing_form": true,
        "public_marketing_preview_fail_closed": true,
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
