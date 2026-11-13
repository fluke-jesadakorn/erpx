//! Production Position GPO Directory connector.
//!
//! [`PositionGpoConnector`] implements [`PositionGpoDirectory`] over the
//! installed Directory connector profile: the UCS/Samba or Windows runner for
//! `inventory|apply`, the Univention UDM REST API for object reads and group
//! membership, plus the approved HTTPS/TLS/no-redirect endpoint rules.
//!
//! The profile file is the same private JSON document the Node connector reads
//! from `ERPX_IAM_SECRET_PROFILES`. It is read per call, bounded, and never
//! logged. This adapter never selects a connection or grants authority; it only
//! performs the effect the owner application already decided.
use crate::application::position_gpo::{GpoFuture, PositionGpoDirectory};
use crate::domain::position_gpo::{
    ConnectionRow, GpoAction, Inventory, InventoryObject, inventory_object,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const MAX_PROFILE_FILE: u64 = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 12_000_000;
const MAX_OBJECTS: usize = 10_000;
const MAX_PAGES: usize = 30;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// One bounded connector request. The transport owns TLS, redirects, timeouts
/// and body limits; the adapter owns URL approval and payload semantics.
#[derive(Clone, Debug)]
pub struct ConnectorRequest {
    pub url: String,
    pub method: &'static str,
    pub bearer: Option<String>,
    pub basic: Option<(String, String)>,
    pub body: Option<Value>,
    pub if_match: Option<String>,
    pub ca_pem: Option<String>,
    pub tls_server_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ConnectorReply {
    pub status: u16,
    pub etag: Option<String>,
    pub location: Option<String>,
    pub retry_after: Option<u64>,
    pub body: Value,
}

pub type ConnectorFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Injected HTTP transport. Production uses [`ReqwestConnector`]; owner tests
/// use a bounded in-memory transport so no provider request is made.
pub trait ConnectorTransport: Send + Sync {
    fn send<'a>(&'a self, request: ConnectorRequest) -> ConnectorFuture<'a, ConnectorReply>;
}

/// Approved HTTPS client: explicit trust roots, no redirects, no implicit
/// retries, no ambient proxy and a bounded response body.
pub struct ReqwestConnector;

impl ConnectorTransport for ReqwestConnector {
    fn send<'a>(&'a self, request: ConnectorRequest) -> ConnectorFuture<'a, ConnectorReply> {
        Box::pin(async move { send_https(request).await })
    }
}

fn unavailable_endpoint() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_UNAVAILABLE",
        "The approved directory endpoint is unavailable.",
    )
}

fn response_limit() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_RESPONSE_LIMIT",
        "The directory response exceeded the approved size.",
    )
}

fn response_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_RESPONSE_INVALID",
        "The provider returned an invalid response.",
    )
}

async fn send_https(request: ConnectorRequest) -> Result<ConnectorReply, BusinessError> {
    let mut url = Url::parse(&request.url).map_err(|_| unavailable_endpoint())?;
    let original_host = url.host_str().ok_or_else(unavailable_endpoint)?.to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(unavailable_endpoint)?;
    let mut roots = rustls::RootCertStore::empty();
    match request.ca_pem.as_deref() {
        Some(pem) => {
            for certificate in pem_certificates(pem)? {
                roots.add(certificate).map_err(|_| unavailable_endpoint())?;
            }
        }
        None => {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
    }
    let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| unavailable_endpoint())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut builder = reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .https_only(true)
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(1);
    if let Some(server_name) = request.tls_server_name.as_deref() {
        // Rustls derives SNI and certificate hostname from the URL host. The
        // approved profile names the certificate explicitly, so the request is
        // addressed to that name and pinned to the approved socket.
        if server_name != original_host {
            let address = resolve_socket(&original_host, port).await?;
            url.set_host(Some(server_name))
                .map_err(|_| unavailable_endpoint())?;
            builder = builder.resolve(server_name, address);
        }
    }
    let client = builder.build().map_err(|_| unavailable_endpoint())?;
    let method = match request.method {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PATCH" => reqwest::Method::PATCH,
        "PUT" => reqwest::Method::PUT,
        _ => return Err(unavailable_endpoint()),
    };
    let mut outgoing = client.request(method, url);
    if let Some(token) = request.bearer {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Conflict,
                    "DIRECTORY_RUNNER_REQUIRED",
                    "Install and configure the UCS Samba or Windows runner.",
                )
            })?;
        value.set_sensitive(true);
        outgoing = outgoing.header(reqwest::header::AUTHORIZATION, value);
    }
    if let Some((username, password)) = request.basic {
        outgoing = outgoing.basic_auth(username, Some(password));
    }
    if let Some(body) = request.body {
        outgoing = outgoing.json(&body);
    }
    if let Some(if_match) = request.if_match {
        let value = reqwest::header::HeaderValue::from_str(&if_match)
            .map_err(|_| unavailable_endpoint())?;
        outgoing = outgoing.header(reqwest::header::IF_MATCH, value);
    }
    let response = outgoing.send().await.map_err(|_| unavailable_endpoint())?;
    let status = response.status().as_u16();
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let bytes = response.bytes().await.map_err(|_| unavailable_endpoint())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(response_limit());
    }
    let body = if bytes.is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_slice(&bytes).map_err(|_| response_invalid())?
    };
    Ok(ConnectorReply {
        status,
        etag,
        location,
        retry_after,
        body,
    })
}

async fn resolve_socket(host: &str, port: u16) -> Result<SocketAddr, BusinessError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| unavailable_endpoint())?
        .next()
        .ok_or_else(unavailable_endpoint)
}

/// Parse the bounded PEM bundle used by the approved profile CA.
fn pem_certificates(
    pem: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, BusinessError> {
    use base64::Engine;
    let mut certificates = Vec::new();
    for block in pem.split("-----BEGIN CERTIFICATE-----").skip(1) {
        let body = block
            .split("-----END CERTIFICATE-----")
            .next()
            .ok_or_else(unavailable_endpoint)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(body.split_whitespace().collect::<String>())
            .map_err(|_| unavailable_endpoint())?;
        certificates.push(rustls::pki_types::CertificateDer::from(decoded));
        if certificates.len() > 16 {
            return Err(unavailable_endpoint());
        }
    }
    if certificates.is_empty() {
        return Err(unavailable_endpoint());
    }
    Ok(certificates)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorProfile {
    allowed_hosts: Vec<String>,
    connection_ids: Vec<String>,
    #[serde(default)]
    ca_pem: Option<String>,
    #[serde(default)]
    tls_server_name: Option<String>,
    #[serde(default)]
    udm_url: Option<String>,
    #[serde(default)]
    udm_username: Option<String>,
    #[serde(default)]
    udm_password: Option<String>,
    #[serde(default)]
    samba_runner_url: Option<String>,
    #[serde(default)]
    samba_runner_token: Option<String>,
    #[serde(default)]
    windows_runner_url: Option<String>,
    #[serde(default)]
    windows_runner_token: Option<String>,
    #[serde(default)]
    identity_aliases: BTreeMap<String, String>,
    #[serde(default)]
    write_operations: Vec<String>,
    #[serde(default)]
    write_base_dns: Vec<String>,
}

impl ConnectorProfile {
    fn validate(mut self) -> Result<Self, BusinessError> {
        if self.allowed_hosts.is_empty()
            || self.allowed_hosts.len() > 256
            || self.connection_ids.is_empty()
            || self.connection_ids.len() > 1024
            || self
                .allowed_hosts
                .iter()
                .any(|host| host.is_empty() || host.len() > 1024)
            || self
                .connection_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 200)
            || self
                .ca_pem
                .as_ref()
                .is_some_and(|value| value.len() > 65_536)
            || self
                .tls_server_name
                .as_ref()
                .is_some_and(|value| value.len() > 1024)
            || self.identity_aliases.len() > 20_000
            || self.write_operations.len() > 64
            || self.write_operations.iter().any(|value| {
                value.is_empty()
                    || value.len() > 80
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    })
            })
            || self.write_base_dns.len() > 256
        {
            return Err(secret_unavailable());
        }
        for host in &mut self.allowed_hosts {
            *host = host.trim().to_lowercase();
        }
        Ok(self)
    }

    fn runner(&self) -> Result<(Url, String), BusinessError> {
        let pair = match (
            self.samba_runner_url.as_deref(),
            self.samba_runner_token.as_deref(),
        ) {
            (Some(url), Some(token)) if token.len() >= 32 => Some((url, token)),
            _ => match (
                self.windows_runner_url.as_deref(),
                self.windows_runner_token.as_deref(),
            ) {
                (Some(url), Some(token)) if token.len() >= 32 => Some((url, token)),
                _ => None,
            },
        };
        let (url, token) = pair.ok_or_else(runner_required)?;
        Ok((
            Url::parse(url).map_err(|_| runner_required())?,
            token.to_owned(),
        ))
    }

    fn udm(&self) -> Option<(&str, &str, &str)> {
        Some((
            self.udm_url.as_deref()?,
            self.udm_username.as_deref()?,
            self.udm_password.as_deref()?,
        ))
    }
}

fn secret_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_SECRET_UNAVAILABLE",
        "The approved Directory credential profile is unavailable.",
    )
}

fn profile_scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_PROFILE_SCOPE",
        "This secret profile is not approved for this connection.",
    )
}

fn endpoint_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_ENDPOINT_FORBIDDEN",
        "Use an approved HTTPS directory endpoint.",
    )
}

fn runner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_RUNNER_REQUIRED",
        "Install and configure the UCS Samba or Windows runner.",
    )
}

fn udm_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "UDM_PROFILE_REQUIRED",
        "Install the UDM REST credential profile.",
    )
}

fn udm_endpoint_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "UDM_ENDPOINT_INVALID",
        "The UDM endpoint must end with /univention/udm/.",
    )
}

fn provider_unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROVIDER_UNSUPPORTED",
        "This connector does not support directory preparation.",
    )
}

fn write_not_approved() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_WRITE_NOT_APPROVED",
        "This operation is not allowed by the installed provider profile.",
    )
}

fn write_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_WRITE_SCOPE_REQUIRED",
        "Install approved write DNs in the provider profile.",
    )
}

fn write_outside_scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_WRITE_OUTSIDE_SCOPE",
        "The object is outside the provider's approved write scope.",
    )
}

fn target_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "DIRECTORY_TARGET_INVALID",
        "Choose a current Directory object from this connection.",
    )
}

fn object_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_OBJECT_MISSING",
        "Refresh the directory inventory.",
    )
}

fn version_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_VERSION_CHANGED",
        "The directory object changed. Refresh and preview again.",
    )
}

fn readback_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_READBACK_FAILED",
        "The directory read-back did not match the requested change.",
    )
}

fn identity_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_IDENTITY_CHANGED",
        "The directory changed the permanent identity during the operation.",
    )
}

fn incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_INCOMPLETE",
        "The provider returned an invalid inventory.",
    )
}

fn identity_mapping_unverified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_IDENTITY_MAPPING_UNVERIFIED",
        "Verify the S4 connector mapping before GPO operations.",
    )
}

fn identity_alias_collision() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_IDENTITY_ALIAS_COLLISION",
        "The installed UCS to Samba identity mapping is not one-to-one.",
    )
}

fn limit_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_LIMIT_EXCEEDED",
        "Narrow the directory inventory.",
    )
}

fn request_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_REQUEST_FAILED",
        "The directory returned an unexpected status.",
    )
}

fn validation_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "DIRECTORY_VALIDATION_FAILED",
        "The directory rejected the request. Check names, permissions and supported field values.",
    )
}

fn apply_rejected() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_APPLY_REJECTED",
        "The runner stopped this operation. Read the inventory and preview again.",
    )
}

/// Map a runner-declared Directory code to the owner's static contract. Only
/// reviewed codes cross the adapter boundary; anything else stays generic.
fn runner_failure(code: &str) -> BusinessError {
    match code {
        "DIRECTORY_VERSION_CHANGED" => version_changed(),
        "DIRECTORY_IDENTITY_CHANGED" => identity_changed(),
        "DIRECTORY_OBJECT_MISSING" => object_missing(),
        "DIRECTORY_TARGET_INVALID" => target_invalid(),
        "DIRECTORY_WRITE_OUTSIDE_SCOPE" => write_outside_scope(),
        "DIRECTORY_WRITE_SCOPE_REQUIRED" => write_scope_required(),
        "DIRECTORY_READBACK_FAILED" => readback_failed(),
        "DIRECTORY_PARTIAL_UNCERTAIN" => BusinessError::new(
            FailureKind::Unavailable,
            "DIRECTORY_PARTIAL_UNCERTAIN",
            "The directory move could not be confirmed. Read inventory and preview again.",
        ),
        "DIRECTORY_OBJECT_EXISTS" => BusinessError::new(
            FailureKind::Conflict,
            "DIRECTORY_OBJECT_EXISTS",
            "This name belongs to another directory object.",
        ),
        "DIRECTORY_LINKS_CHANGED" => BusinessError::new(
            FailureKind::Conflict,
            "DIRECTORY_LINKS_CHANGED",
            "Include every current GPO link exactly once.",
        ),
        "DIRECTORY_GROUP_CYCLE" => BusinessError::new(
            FailureKind::Conflict,
            "DIRECTORY_GROUP_CYCLE",
            "Nested groups cannot form a cycle.",
        ),
        "DIRECTORY_GPO_SCOPE_INCOMPLETE" => BusinessError::new(
            FailureKind::Conflict,
            "DIRECTORY_GPO_SCOPE_INCOMPLETE",
            "The GPO has linked targets outside this connector inventory.",
        ),
        _ => apply_rejected(),
    }
}

fn checked(reply: &ConnectorReply) -> Result<(), BusinessError> {
    if reply.status == 412 {
        return Err(version_changed());
    }
    if (400..500).contains(&reply.status) && reply.status != 429 {
        return Err(validation_failed());
    }
    if !(200..300).contains(&reply.status) {
        return Err(request_failed());
    }
    Ok(())
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

/// Approved provider endpoint: HTTPS, allowlisted host, no embedded identity.
fn approved_url(value: &str, profile: &ConnectorProfile) -> Result<Url, BusinessError> {
    let url = Url::parse(value).map_err(|_| endpoint_forbidden())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none_or(|host| {
            !profile
                .allowed_hosts
                .iter()
                .any(|allowed| allowed == &host.to_lowercase())
        })
    {
        return Err(endpoint_forbidden());
    }
    Ok(url)
}

fn join_route(root: &Url, route: &str) -> Result<Url, BusinessError> {
    let base = if root.path().ends_with('/') {
        root.clone()
    } else {
        let mut root = root.clone();
        let path = format!("{}/", root.path());
        root.set_path(&path);
        root
    };
    base.join(route).map_err(|_| endpoint_forbidden())
}

/// Production Directory connector over the installed provider profile.
pub struct PositionGpoConnector<T: ConnectorTransport> {
    profiles: PathBuf,
    transport: T,
}

/// Composition's default connector: the configured profile file plus the
/// approved HTTPS transport.
pub type PgPositionGpoDirectory = PositionGpoConnector<ReqwestConnector>;

impl<T: ConnectorTransport> PositionGpoConnector<T> {
    pub fn new(profiles: impl AsRef<Path>, transport: T) -> Self {
        Self {
            profiles: profiles.as_ref().to_owned(),
            transport,
        }
    }

    async fn profile(&self, connection: &ConnectionRow) -> Result<ConnectorProfile, BusinessError> {
        let profiles = read_profiles(&self.profiles).await?;
        let name = connection
            .config
            .get("secretProfile")
            .and_then(Value::as_str)
            .ok_or_else(secret_unavailable)?;
        let profile = profiles
            .get(name)
            .cloned()
            .ok_or_else(secret_unavailable)?
            .validate()?;
        if !profile.connection_ids.iter().any(|id| id == &connection.id) {
            return Err(profile_scope());
        }
        Ok(profile)
    }

    fn base_dn(connection: &ConnectionRow) -> Option<String> {
        connection
            .config
            .get("baseDn")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    async fn runner_reply(
        &self,
        profile: &ConnectorProfile,
        route: &str,
        payload: Value,
    ) -> Result<ConnectorReply, BusinessError> {
        let (root, token) = profile.runner()?;
        let url = approved_url(root.as_str(), profile)?;
        let url = join_route(&url, route)?;
        let reply = self
            .transport
            .send(ConnectorRequest {
                url: url.to_string(),
                method: "POST",
                bearer: Some(token),
                basic: None,
                body: Some(payload),
                if_match: None,
                ca_pem: profile.ca_pem.clone(),
                tls_server_name: profile.tls_server_name.clone(),
            })
            .await?;
        if reply.status == 409 {
            if let Some(code) = reply.body.get("error").and_then(|value| value.get("code")) {
                if let Some(code) = code.as_str() {
                    if code.starts_with("DIRECTORY_")
                        && code
                            .bytes()
                            .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
                    {
                        return Err(runner_failure(code));
                    }
                }
            }
        }
        checked(&reply)?;
        Ok(reply)
    }

    async fn runner_inventory(
        &self,
        connection: &ConnectionRow,
        profile: &ConnectorProfile,
        aliases: &BTreeMap<String, String>,
    ) -> Result<Inventory, BusinessError> {
        let reply = self
            .runner_reply(
                profile,
                "inventory",
                json!({"connectionId": connection.id, "baseDn": Self::base_dn(connection)}),
            )
            .await?;
        let inventory = parse_runner_inventory(&reply.body)?;
        canonical_inventory(inventory, aliases)
    }

    async fn udm_root(
        &self,
        profile: &ConnectorProfile,
    ) -> Result<(Url, String, String), BusinessError> {
        let (value, username, password) = profile.udm().ok_or_else(udm_required)?;
        let root = approved_url(value, profile)?;
        if !root.path().ends_with("/univention/udm/") {
            return Err(udm_endpoint_invalid());
        }
        Ok((root, username.to_owned(), password.to_owned()))
    }

    async fn udm_call(
        &self,
        profile: &ConnectorProfile,
        root: &Url,
        call: &(String, String),
        method: &'static str,
        route: &str,
        body: Option<Value>,
        if_match: Option<&str>,
    ) -> Result<ConnectorReply, BusinessError> {
        let url = if route.starts_with("https://") || route.starts_with("http://") {
            Url::parse(route).map_err(|_| endpoint_forbidden())?
        } else {
            join_route(root, route)?
        };
        if url.scheme() != root.scheme()
            || url.host_str() != root.host_str()
            || url.port_or_known_default() != root.port_or_known_default()
            || !url.path().starts_with(root.path())
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "DIRECTORY_LINK_FORBIDDEN",
                "A provider link left the approved API.",
            ));
        }
        approved_url(url.as_str(), profile)?;
        self.transport
            .send(ConnectorRequest {
                url: url.to_string(),
                method,
                bearer: None,
                basic: Some(call.clone()),
                body,
                if_match: if_match.map(str::to_owned),
                ca_pem: profile.ca_pem.clone(),
                tls_server_name: profile.tls_server_name.clone(),
            })
            .await
    }

    async fn udm_inventory(
        &self,
        connection: &ConnectionRow,
        profile: &ConnectorProfile,
    ) -> Result<Inventory, BusinessError> {
        let (root, username, password) = self.udm_root(profile).await?;
        let call = (username, password);
        let schema = self
            .udm_call(profile, &root, &call, "GET", "openapi.json", None, None)
            .await?;
        checked(&schema)?;
        let base_dn = Self::base_dn(connection)
            .map(|value| value.to_lowercase())
            .ok_or_else(incomplete)?;
        let mut objects: Vec<InventoryObject> = Vec::new();
        for (module, kind) in [
            ("users/user", "account"),
            ("groups/group", "group"),
            ("container/ou", "ou"),
            ("container/cn", "ou"),
            ("computers/windows", "computer"),
        ] {
            let mut route = format!("{module}/?limit=1000");
            let mut seen = BTreeSet::new();
            loop {
                if !seen.insert(route.clone()) || seen.len() > MAX_PAGES {
                    return Err(incomplete());
                }
                let reply = self
                    .udm_call(profile, &root, &call, "GET", &route, None, None)
                    .await?;
                if reply.status == 404 && kind == "computer" {
                    break;
                }
                checked(&reply)?;
                let embedded = reply
                    .body
                    .get("_embedded")
                    .and_then(|value| value.get("udm:object"));
                let rows = match embedded {
                    Some(Value::Array(rows)) => rows.clone(),
                    None if reply.body.get("results").and_then(Value::as_u64) == Some(0) => {
                        Vec::new()
                    }
                    Some(_) | None => return Err(incomplete()),
                };
                for item in rows {
                    let dn = text(&item["dn"]);
                    if dn.is_empty() {
                        continue;
                    }
                    let lower = dn.to_lowercase();
                    if lower != base_dn && !lower.ends_with(&format!(",{base_dn}")) {
                        continue;
                    }
                    let detail = self
                        .udm_call(
                            profile,
                            &root,
                            &call,
                            "GET",
                            &format!("{module}/{}", encode(&dn)),
                            None,
                            None,
                        )
                        .await?;
                    checked(&detail)?;
                    objects.push(udm_object(&detail, module, kind)?);
                    if objects.len() > MAX_OBJECTS {
                        return Err(limit_exceeded());
                    }
                }
                let next = reply
                    .body
                    .get("_links")
                    .and_then(|value| value.get("next"))
                    .and_then(|value| value.get("href"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                match next {
                    Some(next) if !next.is_empty() => route = next,
                    _ => break,
                }
            }
        }
        let by_dn: BTreeMap<String, String> = objects
            .iter()
            .filter_map(|object| {
                object
                    .properties
                    .get("erpxConnector")
                    .and_then(|value| value.get("dn"))
                    .and_then(Value::as_str)
                    .map(|dn| (dn.to_lowercase(), object.id.clone()))
            })
            .collect();
        for object in &mut objects {
            let dn = object
                .properties
                .get("erpxConnector")
                .and_then(|value| value.get("dn"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let parent = dn
                .split_once(',')
                .map(|(_, parent)| parent.to_lowercase())
                .and_then(|parent| by_dn.get(&parent).cloned());
            let mut members = Vec::new();
            for key in ["users", "hosts", "nestedGroup"] {
                for value in string_list(&object.properties[key]) {
                    if let Some(id) = by_dn.get(&value.to_lowercase()) {
                        members.push(id.clone());
                    }
                }
            }
            object.properties["parentId"] = json!(parent);
            object.member_ids = members;
        }
        if profile.samba_runner_url.is_some() || profile.windows_runner_url.is_some() {
            // A configured runner must be usable before GPO evidence is trusted.
            let _ = profile.runner()?;
            let samba = profile.samba_runner_url.is_some();
            let win = self.runner_inventory_raw(connection, profile).await?;
            merge_runner(&mut objects, win, samba)?;
        }
        Ok(Inventory {
            observed_at: seconds(),
            objects,
        })
    }

    async fn runner_inventory_raw(
        &self,
        connection: &ConnectionRow,
        profile: &ConnectorProfile,
    ) -> Result<Inventory, BusinessError> {
        let reply = self
            .runner_reply(
                profile,
                "inventory",
                json!({"connectionId": connection.id, "baseDn": Self::base_dn(connection)}),
            )
            .await?;
        parse_runner_inventory(&reply.body)
    }

    async fn udm_apply(
        &self,
        _connection: &ConnectionRow,
        profile: &ConnectorProfile,
        operation_id: &str,
        action: &GpoAction,
        before: &Inventory,
    ) -> Result<(), BusinessError> {
        let GpoAction::Members {
            object_id,
            member_ids,
            add,
        } = action
        else {
            return Err(provider_unsupported());
        };
        if profile.write_base_dns.is_empty() {
            return Err(write_scope_required());
        }
        let target = inventory_object(before, object_id, "group").ok_or_else(target_invalid)?;
        let dn = connector_text(target, "dn")?;
        let module = connector_text(target, "module")?;
        if !profile.write_base_dns.iter().any(|base| {
            let base = base.to_lowercase();
            let lower = dn.to_lowercase();
            lower == base || lower.ends_with(&format!(",{base}"))
        }) {
            return Err(write_outside_scope());
        }
        let (root, username, password) = self.udm_root(profile).await?;
        let call = (username, password);
        let current = self
            .udm_call(
                profile,
                &root,
                &call,
                "GET",
                &format!("{module}/{}", encode(&dn)),
                None,
                None,
            )
            .await?;
        checked(&current)?;
        if text(&current.body["uuid"]) != target.id {
            return Err(identity_changed());
        }
        if current.etag.as_deref() != Some(target.version.as_str()) {
            return Err(version_changed());
        }
        let property = "users";
        let existing = string_list(&current.body["properties"][property]);
        let desired: Vec<String> = member_ids
            .iter()
            .map(|id| {
                before
                    .objects
                    .iter()
                    .find(|object| &object.id == id)
                    .map(|object| connector_text(object, "dn"))
                    .transpose()
                    .and_then(|value| value.ok_or_else(target_invalid))
            })
            .collect::<Result<_, _>>()?;
        let next: Vec<String> = if *add {
            let mut values = existing.clone();
            for value in &desired {
                if !values.iter().any(|item| item.eq_ignore_ascii_case(value)) {
                    values.push(value.clone());
                }
            }
            values
        } else {
            existing
                .iter()
                .filter(|item| !desired.iter().any(|value| value.eq_ignore_ascii_case(item)))
                .cloned()
                .collect()
        };
        if next == existing {
            return Ok(());
        }
        let reply = self
            .udm_call(
                profile,
                &root,
                &call,
                "PATCH",
                &format!("{module}/{}", encode(&dn)),
                Some(json!({"properties": {property: next}})),
                current.etag.as_deref(),
            )
            .await?;
        checked(&reply)?;
        let _ = operation_id;
        let after = self
            .udm_call(
                profile,
                &root,
                &call,
                "GET",
                &format!("{module}/{}", encode(&dn)),
                None,
                None,
            )
            .await?;
        checked(&after)?;
        if text(&after.body["uuid"]) != target.id {
            return Err(identity_changed());
        }
        let members = string_list(&after.body["properties"][property]);
        let confirmed = desired
            .iter()
            .all(|value| members.iter().any(|item| item.eq_ignore_ascii_case(value)) == *add);
        if !confirmed {
            return Err(readback_failed());
        }
        Ok(())
    }
}

fn connector_text(object: &InventoryObject, key: &str) -> Result<String, BusinessError> {
    object
        .properties
        .get("erpxConnector")
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(object_missing)
}

fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b',' | b'=') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn udm_object(
    reply: &ConnectorReply,
    module: &str,
    kind: &str,
) -> Result<InventoryObject, BusinessError> {
    let id = text(&reply.body["uuid"]);
    let dn = text(&reply.body["dn"]);
    let version = reply.etag.clone().unwrap_or_default();
    if id.is_empty() || dn.is_empty() || version.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "DIRECTORY_OBJECT_INVALID",
            "An immutable ID, DN and version are required.",
        ));
    }
    let properties = reply.body.get("properties").cloned().unwrap_or_default();
    let mut kept = Map::new();
    for key in [
        "firstname",
        "lastname",
        "displayName",
        "description",
        "mailPrimaryAddress",
        "disabled",
        "username",
        "name",
        "users",
        "hosts",
        "nestedGroup",
        "sambaRID",
        "sambaSID",
        "samba4ObjectGUID",
    ] {
        if let Some(value) = properties.get(key) {
            kept.insert(key.to_owned(), value.clone());
        }
    }
    let name = properties
        .get("username")
        .and_then(Value::as_str)
        .or_else(|| properties.get("name").and_then(Value::as_str))
        .or_else(|| properties.get("displayName").and_then(Value::as_str))
        .unwrap_or(&dn)
        .to_owned();
    kept.insert(
        "erpxConnector".into(),
        json!({"dn": dn, "module": module, "name": name}),
    );
    Ok(InventoryObject {
        id,
        kind: kind.to_owned(),
        version,
        properties: Value::Object(kept),
        member_ids: vec![],
    })
}

fn parse_runner_inventory(body: &Value) -> Result<Inventory, BusinessError> {
    let objects = body
        .get("objects")
        .and_then(Value::as_array)
        .ok_or_else(incomplete)?;
    if objects.len() > MAX_OBJECTS || body.get("complete").and_then(Value::as_bool) != Some(true) {
        return Err(incomplete());
    }
    let observed_at = body
        .get("observedAt")
        .and_then(Value::as_i64)
        .ok_or_else(incomplete)?;
    let mut parsed = Vec::with_capacity(objects.len());
    let mut seen = BTreeSet::new();
    for item in objects {
        let id = text(&item["id"]);
        let dn = text(&item["dn"]);
        let name = text(&item["name"]);
        let module = text(&item["module"]);
        let kind = text(&item["kind"]);
        let version = text(&item["version"]);
        if id.is_empty()
            || version.is_empty()
            || !matches!(
                kind.as_str(),
                "account" | "group" | "ou" | "computer" | "gpo"
            )
            || !seen.insert(id.clone())
        {
            return Err(incomplete());
        }
        let mut properties = item
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if !dn.is_empty() && !properties.contains_key("erpxConnector") {
            properties.insert(
                "erpxConnector".into(),
                json!({"dn": dn, "module": module, "name": name}),
            );
        }
        parsed.push(InventoryObject {
            id,
            kind,
            version,
            properties: Value::Object(properties),
            member_ids: string_list(&item["memberIds"]),
        });
    }
    Ok(Inventory {
        observed_at,
        objects: parsed,
    })
}

fn canonical_inventory(
    inventory: Inventory,
    aliases: &BTreeMap<String, String>,
) -> Result<Inventory, BusinessError> {
    if aliases.is_empty() {
        return Ok(inventory);
    }
    let canonical = |value: &str| -> String {
        aliases
            .get(&value.to_lowercase())
            .cloned()
            .unwrap_or_else(|| value.to_owned())
    };
    let mut remapped = Vec::with_capacity(inventory.objects.len());
    let mut seen = BTreeSet::new();
    for object in inventory.objects {
        let provider_id = object.id.clone();
        let id = canonical(&provider_id);
        if !seen.insert(id.clone()) {
            return Err(identity_alias_collision());
        }
        let mut properties = object.properties;
        let version = object.version.clone();
        if id != provider_id {
            let identity = properties
                .get("erpxConnector")
                .cloned()
                .unwrap_or_else(|| json!({}));
            properties["windowsIdentity"] = json!({
                "id": provider_id,
                "dn": identity.get("dn").cloned().unwrap_or(Value::Null),
                "version": version,
            });
        }
        if let Some(links) = properties
            .get("gpoLinks")
            .and_then(Value::as_array)
            .cloned()
        {
            properties["gpoLinks"] = Value::Array(
                links
                    .into_iter()
                    .map(|link| {
                        let mut link = link.as_object().cloned().unwrap_or_default();
                        if let Some(value) = link.get("gpoId").and_then(Value::as_str) {
                            link.insert("gpoId".into(), json!(canonical(value)));
                        }
                        Value::Object(link)
                    })
                    .collect(),
            );
        }
        for key in ["securityFilterGroupIds"] {
            if let Some(values) = properties.get(key).and_then(Value::as_array).cloned() {
                properties[key] = Value::Array(
                    values
                        .into_iter()
                        .map(|value| {
                            value
                                .as_str()
                                .map(|value| json!(canonical(value)))
                                .unwrap_or(value)
                        })
                        .collect(),
                );
            }
        }
        for key in ["definition", "definitions"] {
            if let Some(value) = properties.get(key).cloned() {
                let remap = |policy: &Value| -> Value {
                    let mut policy = policy.as_object().cloned().unwrap_or_default();
                    for key in ["targetOuId", "filterGroupId", "computerTargetId"] {
                        if let Some(value) = policy.get(key).and_then(Value::as_str) {
                            policy.insert(key.into(), json!(canonical(value)));
                        }
                    }
                    Value::Object(policy)
                };
                let remapped = if key == "definition" {
                    remap(&value)
                } else {
                    Value::Object(
                        value
                            .as_object()
                            .map(|scopes| {
                                scopes
                                    .iter()
                                    .map(|(scope, policy)| (scope.clone(), remap(policy)))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    )
                };
                properties[key] = remapped;
            }
        }
        remapped.push(InventoryObject {
            id,
            kind: object.kind,
            version,
            properties,
            member_ids: object.member_ids.iter().map(|id| canonical(id)).collect(),
        });
    }
    Ok(Inventory {
        observed_at: inventory.observed_at,
        objects: remapped,
    })
}

fn merge_runner(
    objects: &mut Vec<InventoryObject>,
    win: Inventory,
    samba: bool,
) -> Result<(), BusinessError> {
    let mut mapped: BTreeMap<String, String> = BTreeMap::new();
    for remote in &win.objects {
        let remote_dn = text(&remote.properties["dn"]);
        let remote_uuid = remote
            .properties
            .get("udmUuid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let local = objects.iter_mut().find(|local| {
            if local.kind != remote.kind {
                return false;
            }
            if samba {
                !remote_uuid.is_empty() && remote_uuid == local.id
            } else {
                !remote_dn.is_empty()
                    && local
                        .properties
                        .get("erpxConnector")
                        .and_then(|value| value.get("dn"))
                        .and_then(Value::as_str)
                        == Some(remote_dn.as_str())
            }
        });
        if let Some(local) = local {
            if samba {
                let proof = remote
                    .properties
                    .get("udmUuid")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if proof != local.id {
                    return Err(identity_mapping_unverified());
                }
            }
            if let Some(links) = remote.properties.get("gpoLinks") {
                local.properties["gpoLinks"] = links.clone();
            }
            if let Some(value) = remote.properties.get("blockInheritance") {
                local.properties["blockInheritance"] = value.clone();
            }
            let identity = local
                .properties
                .get("erpxConnector")
                .cloned()
                .unwrap_or_else(|| json!({}));
            local.properties["windowsIdentity"] = json!({
                "id": remote.id,
                "dn": identity.get("dn").cloned().unwrap_or(Value::Null),
                "version": remote.version,
            });
            mapped.insert(remote.id.clone(), local.id.clone());
        } else if remote.kind == "gpo" {
            mapped.insert(remote.id.clone(), remote.id.clone());
        }
    }
    for object in objects.iter_mut() {
        if let Some(links) = object
            .properties
            .get("gpoLinks")
            .and_then(Value::as_array)
            .cloned()
        {
            object.properties["gpoLinks"] = Value::Array(
                links
                    .into_iter()
                    .map(|link| {
                        let mut link = link.as_object().cloned().unwrap_or_default();
                        if let Some(value) = link.get("gpoId").and_then(Value::as_str) {
                            link.insert(
                                "gpoId".into(),
                                json!(
                                    mapped
                                        .get(value)
                                        .cloned()
                                        .unwrap_or_else(|| value.to_owned())
                                ),
                            );
                        }
                        Value::Object(link)
                    })
                    .collect(),
            );
        }
    }
    for remote in win.objects {
        if remote.kind != "gpo" {
            continue;
        }
        if remote
            .member_ids
            .iter()
            .any(|id| !mapped.contains_key(id) && !objects.iter().any(|object| &object.id == id))
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "DIRECTORY_GPO_SCOPE_INCOMPLETE",
                "The GPO has linked targets outside this connector inventory.",
            ));
        }
        let remap_policy = |value: &Value| -> Value {
            let mut policy = value.as_object().cloned().unwrap_or_default();
            for key in ["targetOuId", "filterGroupId", "computerTargetId"] {
                if let Some(value) = policy.get(key).and_then(Value::as_str) {
                    policy.insert(
                        key.into(),
                        json!(
                            mapped
                                .get(value)
                                .cloned()
                                .unwrap_or_else(|| value.to_owned())
                        ),
                    );
                }
            }
            Value::Object(policy)
        };
        let mut properties = remote.properties.as_object().cloned().unwrap_or_default();
        if let Some(value) = properties.get("definition").cloned() {
            properties.insert("definition".into(), remap_policy(&value));
        }
        if let Some(value) = properties
            .get("definitions")
            .and_then(Value::as_object)
            .cloned()
        {
            properties.insert(
                "definitions".into(),
                Value::Object(
                    value
                        .iter()
                        .map(|(scope, policy)| (scope.clone(), remap_policy(policy)))
                        .collect(),
                ),
            );
        }
        if let Some(values) = properties
            .get("securityFilterGroupIds")
            .and_then(Value::as_array)
            .cloned()
        {
            properties.insert(
                "securityFilterGroupIds".into(),
                Value::Array(
                    values
                        .into_iter()
                        .map(|value| {
                            value
                                .as_str()
                                .map(|value| {
                                    json!(
                                        mapped
                                            .get(value)
                                            .cloned()
                                            .unwrap_or_else(|| value.to_owned())
                                    )
                                })
                                .unwrap_or(value)
                        })
                        .collect(),
                ),
            );
        }
        objects.push(InventoryObject {
            id: remote.id.clone(),
            kind: remote.kind,
            version: remote.version,
            properties: Value::Object(properties),
            member_ids: remote
                .member_ids
                .iter()
                .map(|id| mapped.get(id).cloned().unwrap_or_else(|| id.clone()))
                .collect(),
        });
    }
    Ok(())
}

fn action_kind(action: &GpoAction) -> &'static str {
    match action {
        GpoAction::Filter { .. } => "gpo.filter",
        GpoAction::Link { .. } => "gpo.link",
        GpoAction::Members { .. } => "group.members",
    }
}

fn action_json(action: &GpoAction) -> Value {
    match action {
        GpoAction::Filter {
            object_id,
            group_ids,
        } => json!({"kind": "gpo.filter", "objectId": object_id, "groupIds": group_ids}),
        GpoAction::Link {
            object_id,
            parent_id,
            enabled,
            enforced,
            remove,
        } => json!({"kind": "gpo.link", "objectId": object_id, "parentId": parent_id,
            "enabled": enabled, "enforced": enforced, "remove": remove}),
        GpoAction::Members {
            object_id,
            member_ids,
            add,
        } => json!({"kind": "group.members", "objectId": object_id, "memberIds": member_ids,
            "action": if *add { "add" } else { "remove" }}),
    }
}

fn validate_target(action: &GpoAction, before: &Inventory) -> Result<(), BusinessError> {
    let find = |id: &str| -> Result<&InventoryObject, BusinessError> {
        before
            .objects
            .iter()
            .find(|object| object.id == id)
            .ok_or_else(object_missing)
    };
    match action {
        GpoAction::Filter {
            object_id,
            group_ids,
        } => {
            if find(object_id)?.kind != "gpo"
                || group_ids
                    .iter()
                    .any(|id| find(id).map(|object| object.kind.as_str()) != Ok("group"))
            {
                return Err(target_invalid());
            }
        }
        GpoAction::Link {
            object_id,
            parent_id,
            ..
        } => {
            if find(object_id)?.kind != "gpo" || find(parent_id)?.kind != "ou" {
                return Err(target_invalid());
            }
        }
        GpoAction::Members {
            object_id,
            member_ids,
            ..
        } => {
            if find(object_id)?.kind != "group" {
                return Err(target_invalid());
            }
            for id in member_ids {
                if !matches!(find(id)?.kind.as_str(), "account" | "group" | "computer") {
                    return Err(target_invalid());
                }
            }
        }
    }
    Ok(())
}

fn before_inventory(before: &Inventory) -> Value {
    json!({
        "objects": before.objects.iter().map(|object| {
            let identity = object.properties.get("erpxConnector").cloned().unwrap_or_else(|| json!({}));
            json!({
                "id": object.id,
                "dn": identity.get("dn").cloned().unwrap_or_else(|| json!(object.id)),
                "name": identity.get("name").cloned().unwrap_or_else(|| json!(object.id)),
                "kind": object.kind,
                "parentId": object.properties.get("parentId").cloned().unwrap_or(Value::Null),
                "version": object.version,
                "module": identity.get("module").cloned().unwrap_or_else(|| json!("")),
                "properties": object.properties,
                "memberIds": object.member_ids,
            })
        }).collect::<Vec<_>>(),
        "fields": [],
        "providerVersion": "erpx-native",
        "observedAt": before.observed_at,
        "complete": true,
    })
}

impl<T: ConnectorTransport> PositionGpoDirectory for PositionGpoConnector<T> {
    fn inventory<'a>(
        &'a mut self,
        connection: &'a ConnectionRow,
    ) -> GpoFuture<'a, Inventory, BusinessError> {
        Box::pin(async move {
            let profile = self.profile(connection).await?;
            match connection.provider.as_str() {
                "active_directory" => {
                    self.runner_inventory(connection, &profile, &profile.identity_aliases)
                        .await
                }
                "univention" => self.udm_inventory(connection, &profile).await,
                _ => Err(provider_unsupported()),
            }
        })
    }

    fn apply<'a>(
        &'a mut self,
        connection: &'a ConnectionRow,
        operation_id: &'a str,
        action: &'a GpoAction,
        before: &'a Inventory,
    ) -> GpoFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let profile = self.profile(connection).await?;
            let kind = action_kind(action);
            if !profile.write_operations.iter().any(|value| value == kind) {
                return Err(write_not_approved());
            }
            validate_target(action, before)?;
            let runner_action = matches!(action, GpoAction::Filter { .. } | GpoAction::Link { .. });
            if connection.provider == "active_directory" || runner_action {
                self.runner_reply(
                    &profile,
                    "apply",
                    json!({
                        "connectionId": connection.id,
                        "operationId": operation_id,
                        "action": action_json(action),
                        "before": before_inventory(before),
                        "registry": [],
                        "preferences": [],
                    }),
                )
                .await?;
                return Ok(());
            }
            self.udm_apply(connection, &profile, operation_id, action, before)
                .await
        })
    }
}

async fn read_profiles(path: &Path) -> Result<BTreeMap<String, ConnectorProfile>, BusinessError> {
    if !path.is_absolute() {
        return Err(secret_unavailable());
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| secret_unavailable())?;
    if bytes.len() as u64 > MAX_PROFILE_FILE {
        return Err(secret_unavailable());
    }
    let profiles: BTreeMap<String, ConnectorProfile> =
        serde_json::from_slice(&bytes).map_err(|_| secret_unavailable())?;
    if profiles.is_empty() || profiles.len() > 256 {
        return Err(secret_unavailable());
    }
    Ok(profiles)
}
