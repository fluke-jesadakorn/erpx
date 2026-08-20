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
const DEVELOPMENT_CONTROLLER_PRINCIPAL: &str = "local:dev-controller";
const SIGN_IN_FORM_SELECTOR: &str = "[data-testid='sign-in-form']";
const SIGN_IN_ERROR_SELECTOR: &str = "[data-testid='sign-in-error'][role='alert']";
const ROLE_OPTION_SELECTOR: &str = "[data-testid='role-option'][data-selection-id][data-role-key='platform_holding_administrator']";
const IDENTITY_ROLE_OPTION_SELECTOR: &str =
    "[data-testid='role-option'][data-selection-id][data-role-key='customer_security_head']";
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
    resource_type == "Document" && (canceled || error == "net::ERR_ABORTED")
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
        cdp.wait_for_selector(".empty-state")?;
        let layout_safe = cdp.evaluate(
            "document.querySelectorAll('main').length === 1 && document.body.scrollWidth <= innerWidth && document.querySelectorAll('.record-row').length === 0 && document.querySelector('.workspace-route')?.dataset.freshness === 'not-connected' && !document.body.innerText.includes('ข้อมูลตัวอย่าง')",
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
            "location.pathname === {expected_path} && document.querySelectorAll('.record-row').length === 0 && document.querySelectorAll('.empty-state').length === 1 && document.querySelector('.workspace-route')?.dataset.freshness === 'not-connected' && document.querySelectorAll('main').length === 1 && document.body.scrollWidth <= innerWidth && !document.body.innerText.includes('ข้อมูลตัวอย่าง')"
        ))?;
        if valid.as_bool() != Some(true) {
            return Err(format!("workspace route did not render successfully: {route}").into());
        }
        cdp.wait_for_selector(".console-workspace-menu__trigger[data-dioxus-id]")?;
        let workspace_menu_opened = cdp.evaluate(
            "(() => { const trigger=document.querySelector('.console-workspace-menu__trigger'); if (!trigger) return false; trigger.click(); return true; })()",
        )?;
        if workspace_menu_opened.as_bool() != Some(true) {
            return Err(format!("workspace menu trigger was unavailable: {route}").into());
        }
        cdp.wait_for(
            &format!(
                "Boolean(document.querySelector(`a[aria-current='page'][href={}]`))",
                serde_json::to_string(route)?
            ),
            &format!("active workspace navigation for {route}"),
        )?;
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
