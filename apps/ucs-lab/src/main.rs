#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        ServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer},
    },
};
use uuid::Uuid;

const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_AUTHORITIES_FILE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
struct AuthoritiesFile {
    version: u32,
    authorities: Vec<AuthorityConfig>,
}

#[derive(Debug, Deserialize)]
struct AuthorityConfig {
    authority: String,
    authority_fingerprint: String,
    state_path: PathBuf,
    password_path: PathBuf,
}

#[derive(Debug)]
struct AuthorityRuntime {
    authority_fingerprint: String,
    state_path: PathBuf,
    password_path: PathBuf,
    state: Mutex<LabState>,
}

#[derive(Debug)]
struct LabRuntime {
    authorities: BTreeMap<String, Arc<AuthorityRuntime>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LabState {
    sequence: u64,
    acknowledged_sequence: u64,
    disabled: Vec<String>,
    deleted: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct ProvisioningRequest {
    sequence: u64,
    #[allow(dead_code)]
    event: String,
    username: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    deleted: bool,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    authenticated: bool,
    entry_uuid: String,
    authority_fingerprint: String,
    groups: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Acknowledgement {
    acknowledged: bool,
    sequence: u64,
    cursor: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    authority_fingerprint: String,
    feed: &'static str,
    durable_sequence: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind: SocketAddr = env::var("ERPX_UCS_LAB_BIND")
        .unwrap_or_else(|_| "127.0.0.1:15443".into())
        .parse()?;
    if !bind.ip().is_loopback() {
        return Err("UCS lab must bind to a loopback address".into());
    }
    let cert_path = required_path("ERPX_UCS_LAB_CERT")?;
    let key_path = required_path("ERPX_UCS_LAB_KEY")?;
    let runtime = Arc::new(load_runtime()?);
    let acceptor = TlsAcceptor::from(Arc::new(load_server_config(&cert_path, &key_path)?));
    let listener = TcpListener::bind(bind).await?;
    eprintln!(
        "ERPX UCS lab listening on https://{bind} with {} logical authorities",
        runtime.authorities.len()
    );
    loop {
        let (stream, _) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let runtime = runtime.clone();
        tokio::spawn(async move {
            if let Ok(stream) = acceptor.accept(stream).await {
                let _ = serve(stream, runtime).await;
            }
        });
    }
}

fn load_runtime() -> Result<LabRuntime, Box<dyn std::error::Error + Send + Sync>> {
    let path = required_path("ERPX_UCS_LAB_AUTHORITIES_FILE")?;
    load_authorities_file(path)
}

fn load_authorities_file(
    path: PathBuf,
) -> Result<LabRuntime, Box<dyn std::error::Error + Send + Sync>> {
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_AUTHORITIES_FILE_BYTES
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("UCS lab authorities file must be a private regular file".into());
    }
    let file: AuthoritiesFile = serde_json::from_slice(&fs::read(path)?)?;
    if file.version != 1 {
        return Err("UCS lab authorities file version is unsupported".into());
    }
    runtime_from_configs(file.authorities)
}

fn runtime_from_configs(
    configs: Vec<AuthorityConfig>,
) -> Result<LabRuntime, Box<dyn std::error::Error + Send + Sync>> {
    if configs.is_empty() || configs.len() > 64 {
        return Err("UCS lab authorities count is invalid".into());
    }
    let mut authorities = BTreeMap::new();
    for config in configs {
        if config.authority.is_empty()
            || config.authority.len() > 96
            || !config
                .authority
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || config.authority_fingerprint.len() != 64
            || !config
                .authority_fingerprint
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err("UCS lab authority configuration is invalid".into());
        }
        validate_private_file(&config.password_path, "UCS lab password")?;
        if config.state_path.is_symlink() {
            return Err("UCS lab state path must not be a symlink".into());
        }
        let state = load_state(&config.state_path)?;
        let runtime = Arc::new(AuthorityRuntime {
            authority_fingerprint: config.authority_fingerprint,
            state_path: config.state_path,
            password_path: config.password_path,
            state: Mutex::new(state),
        });
        if authorities.insert(config.authority, runtime).is_some() {
            return Err("UCS lab authority is duplicated".into());
        }
    }
    if !authorities.contains_key("platform") {
        return Err("UCS lab platform authority is required".into());
    }
    Ok(LabRuntime { authorities })
}

fn validate_private_file(
    path: &PathBuf,
    label: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(format!("{label} file must be a private regular file").into());
    }
    Ok(())
}

fn required_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let value = env::var(name).map_err(|_| format!("{name} is required"))?;
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("{name} is invalid").into());
    }
    Ok(PathBuf::from(value))
}

fn load_server_config(
    cert_path: &PathBuf,
    key_path: &PathBuf,
) -> Result<ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let certs = pem_blocks(&fs::read(cert_path)?, "CERTIFICATE")?
        .into_iter()
        .map(CertificateDer::from)
        .collect::<Vec<_>>();
    let key = pem_blocks(&fs::read(key_path)?, "PRIVATE KEY")?
        .into_iter()
        .next()
        .ok_or("UCS lab private key is missing")?;
    let key = PrivateKeyDer::try_from(key).map_err(|_| "UCS lab private key is invalid")?;
    Ok(ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?)
}

fn pem_blocks(
    bytes: &[u8],
    label: &str,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
    let text = std::str::from_utf8(bytes)?;
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&begin) {
        let payload = &rest[start + begin.len()..];
        let stop = payload
            .find(&end)
            .ok_or("UCS lab PEM block is incomplete")?;
        let encoded = payload[..stop].lines().collect::<String>();
        blocks.push(STANDARD.decode(encoded.as_bytes())?);
        rest = &payload[stop + end.len()..];
    }
    Ok(blocks)
}

async fn serve(
    mut stream: tokio_rustls::server::TlsStream<TcpStream>,
    runtime: Arc<LabRuntime>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (method, path, body) = read_request(&mut stream).await?;
    let Some((authority_key, operation)) = routed_operation(&path) else {
        return write_response(stream, "404 Not Found", b"{\"error\":\"not_found\"}").await;
    };
    let Some(authority) = runtime.authorities.get(authority_key).cloned() else {
        return write_response(stream, "404 Not Found", b"{\"error\":\"not_found\"}").await;
    };
    let (status, payload) = match (method.as_str(), operation) {
        ("GET", "/healthz") => {
            let current = authority.state.lock().await;
            (
                "200 OK",
                serde_json::to_vec(&HealthResponse {
                    status: "ok",
                    service: "ucs-compatible-udm-provisioning",
                    authority_fingerprint: authority.authority_fingerprint.clone(),
                    feed: "ucs-provisioning-service",
                    durable_sequence: current.sequence,
                })?,
            )
        }
        ("POST", "/udm/auth") => {
            let input = serde_json::from_slice::<AuthRequest>(&body)?;
            let password = fs::read_to_string(&authority.password_path)?
                .trim()
                .to_owned();
            let current = authority.state.lock().await;
            let disabled = current.disabled.iter().any(|item| item == &input.username);
            let authenticated =
                !disabled && input.password == password && !input.username.trim().is_empty();
            let entry_uuid =
                immutable_entry_uuid(&authority.authority_fingerprint, &input.username);
            (
                if authenticated {
                    "200 OK"
                } else {
                    "401 Unauthorized"
                },
                serde_json::to_vec(&AuthResponse {
                    authenticated,
                    entry_uuid,
                    authority_fingerprint: authority.authority_fingerprint.clone(),
                    groups: if authenticated {
                        vec!["erpx-development-users".into()]
                    } else {
                        Vec::new()
                    },
                })?,
            )
        }
        ("POST", "/udm/provisioning") => {
            let input = serde_json::from_slice::<ProvisioningRequest>(&body)?;
            let mut current = authority.state.lock().await;
            if input.sequence <= current.sequence {
                (
                    "200 OK",
                    serde_json::to_vec(&Acknowledgement {
                        acknowledged: true,
                        sequence: current.sequence,
                        cursor: format!("ucs:{}", current.acknowledged_sequence),
                    })?,
                )
            } else if input.sequence != current.sequence + 1 {
                ("409 Conflict", b"{\"error\":\"cursor_gap\"}".to_vec())
            } else {
                current.sequence = input.sequence;
                current.acknowledged_sequence = input.sequence;
                if input.disabled && !current.disabled.contains(&input.username) {
                    current.disabled.push(input.username.clone());
                }
                if input.deleted && !current.deleted.contains(&input.username) {
                    current.deleted.push(input.username.clone());
                }
                persist_state(&authority.state_path, &current)?;
                (
                    "200 OK",
                    serde_json::to_vec(&Acknowledgement {
                        acknowledged: true,
                        sequence: input.sequence,
                        cursor: format!("ucs:{}", input.sequence),
                    })?,
                )
            }
        }
        _ => ("404 Not Found", b"{\"error\":\"not_found\"}".to_vec()),
    };
    write_response(stream, status, &payload).await
}

fn routed_operation(path: &str) -> Option<(&str, &str)> {
    if matches!(path, "/healthz" | "/udm/auth" | "/udm/provisioning") {
        return Some(("platform", path));
    }
    let rest = path.strip_prefix("/authorities/")?;
    let (authority, suffix) = rest.split_once('/')?;
    if authority.is_empty() {
        return None;
    }
    let operation = match suffix {
        "healthz" => "/healthz",
        "udm/auth" => "/udm/auth",
        "udm/provisioning" => "/udm/provisioning",
        _ => return None,
    };
    Some((authority, operation))
}

async fn write_response(
    mut stream: tokio_rustls::server::TlsStream<TcpStream>,
    status: &str,
    payload: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(payload).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn read_request(
    stream: &mut tokio_rustls::server::TlsStream<TcpStream>,
) -> Result<(String, String, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = Vec::new();
    let (header_end, content_length);
    loop {
        let mut chunk = [0_u8; 2048];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err("UCS lab request ended before headers".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err("UCS lab request is too large".into());
        }
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = position + 4;
            let header = std::str::from_utf8(&buffer[..position])?;
            content_length = header
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                })
                .map(str::trim)
                .map(str::parse)
                .transpose()?
                .unwrap_or(0_usize);
            break;
        }
    }
    while buffer.len() < header_end + content_length {
        let mut chunk = [0_u8; 2048];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err("UCS lab request body is incomplete".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let header = std::str::from_utf8(&buffer[..header_end - 4])?;
    let request_line = header
        .lines()
        .next()
        .ok_or("UCS lab request line is missing")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("UCS lab method is missing")?.to_owned();
    let path = parts.next().ok_or("UCS lab path is missing")?.to_owned();
    Ok((
        method,
        path,
        buffer[header_end..header_end + content_length].to_vec(),
    ))
}

fn immutable_entry_uuid(authority: &str, username: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("ucs:{authority}:{username}").as_bytes(),
    )
    .to_string()
}

fn load_state(path: &PathBuf) -> Result<LabState, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Ok(LabState::default());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn persist_state(
    path: &PathBuf,
    state: &LabState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(state)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_routes_are_explicit_and_platform_root_routes_are_compatible() {
        assert_eq!(routed_operation("/healthz"), Some(("platform", "/healthz")));
        assert_eq!(
            routed_operation("/authorities/demo-tenant/udm/auth"),
            Some(("demo-tenant", "/udm/auth"))
        );
        assert_eq!(
            routed_operation("/authorities/demo-tenant/udm/provisioning"),
            Some(("demo-tenant", "/udm/provisioning"))
        );
        assert_eq!(routed_operation("/authorities/demo-tenant/unknown"), None);
        assert_eq!(routed_operation("/authorities//healthz"), None);
    }

    #[test]
    fn immutable_identity_is_authority_bound() {
        assert_ne!(
            immutable_entry_uuid(&"a".repeat(64), "alice"),
            immutable_entry_uuid(&"b".repeat(64), "alice")
        );
    }
}
