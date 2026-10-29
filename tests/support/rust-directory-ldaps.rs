use bytes::BytesMut;
use erpx_server_identity_access::public::directory::*;
use erpx_server_shared::public::{Clock, TimestampMillis};
use ldap3_proto::{LdapCodec, control::LdapControl, proto::*};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tokio_util::codec::{Decoder, Encoder};

pub const NOW: i64 = 1_800_000_000_000;
pub const CONNECTION: &str = "11111111-1111-4111-8111-111111111111";
pub const SERVICE_DN: &str = "cn=reader,dc=fixture";
pub const USER_DN: &str = "uid=alice,dc=fixture";
pub const BOB_DN: &str = "uid=bob,dc=fixture";
pub const GROUP_DN: &str = "cn=everyone,dc=fixture";
pub const SERVICE_PASSWORD: &str = "synthetic-directory-service-password";
pub const USER_PASSWORD: &str = "synthetic-directory-user-password";
pub struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

pub struct Material {
    pub ca: String,
    pub wrong_ca: String,
    pub server: Arc<ServerConfig>,
}
pub fn material() -> &'static Material {
    static VALUE: OnceLock<Material> = OnceLock::new();
    VALUE.get_or_init(||{
  let directory=tempfile::tempdir().unwrap();let root=directory.path();
  let config="[req]\nprompt=no\ndistinguished_name=dn\n[dn]\nCN=synthetic LDAPS fixture\n[ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n[server]\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:ldap.fixture.test\n";
  std::fs::write(root.join("extensions.cnf"),config).unwrap();
  run(root,&["req","-x509","-newkey","rsa:2048","-nodes","-keyout","ca.key","-out","ca.pem","-days","2","-sha256","-subj","/CN=synthetic LDAPS fixture CA","-config","extensions.cnf","-extensions","ca"]);
  run(root,&["req","-new","-newkey","rsa:2048","-nodes","-keyout","server.key","-out","server.csr","-subj","/CN=ldap.fixture.test"]);
  run(root,&["x509","-req","-in","server.csr","-CA","ca.pem","-CAkey","ca.key","-CAcreateserial","-out","server.pem","-days","2","-sha256","-extfile","extensions.cnf","-extensions","server"]);
  run(root,&["req","-x509","-newkey","rsa:2048","-nodes","-keyout","other.key","-out","other.pem","-days","2","-sha256","-subj","/CN=other synthetic LDAPS fixture CA","-config","extensions.cnf","-extensions","ca"]);
  let ca=std::fs::read_to_string(root.join("ca.pem")).unwrap();let wrong_ca=std::fs::read_to_string(root.join("other.pem")).unwrap();
  let certificates=CertificateDer::pem_file_iter(root.join("server.pem")).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
  let key=PrivateKeyDer::from_pem_file(root.join("server.key")).unwrap();
  let config=ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().unwrap().with_no_client_auth().with_single_cert(certificates,key).unwrap();
  directory.close().unwrap(); // Synthetic key files never survive preparation.
  Material{ca,wrong_ca,server:Arc::new(config)}
 })
}
fn run(root: &Path, args: &[&str]) {
    let status = Command::new("openssl")
        .args(args)
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("OPENSSL_CONF", root.join("extensions.cnf"))
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "synthetic certificate preparation failed");
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Complete,
    Paged,
    RepeatedCookie,
    MissingCookie,
    PageLimit,
    UcsReferral,
    AdReferral,
    Truncated,
    RowFlood,
    WrongMessage,
    Unsolicited,
    DeclaredBomb,
    Indefinite,
    Deep,
    MalformedPrimitive,
    AdRangeComplete,
    AdRangeMissing,
    AdRangeReferral,
}
#[derive(Default)]
pub struct Stats {
    pub connections: usize,
    pub tls_failed: usize,
    pub sni_ok: usize,
    pub service_binds: usize,
    pub user_binds: usize,
    pub searches: usize,
    pub ranges: usize,
    pub unbinds: usize,
}
pub struct Server {
    pub port: u16,
    pub stats: Arc<Mutex<Stats>>,
    pub started: Arc<Notify>,
    pub resume: Arc<Notify>,
    stop: Arc<Notify>,
    task: JoinHandle<()>,
}
static PORT: AtomicU16 = AtomicU16::new(51_000);
pub async fn listener() -> TcpListener {
    for _ in 0..10_000 {
        let n = PORT.fetch_add(1, Ordering::SeqCst);
        let port = 49_152 + (n % 16_384);
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => return listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(_) => panic!("owned loopback listener unavailable"),
        }
    }
    panic!("owned loopback port budget exhausted")
}
impl Server {
    pub async fn start(scenario: Scenario, pause: bool, referral_port: Option<u16>) -> Self {
        let listener = listener().await;
        let port = listener.local_addr().unwrap().port();
        let stats = Arc::new(Mutex::new(Stats::default()));
        let stop = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        let task = {
            let stats = stats.clone();
            let stop = stop.clone();
            let started = started.clone();
            let resume = resume.clone();
            let acceptor = TlsAcceptor::from(material().server.clone());
            tokio::spawn(async move {
                loop {
                    let accepted = tokio::select! {biased;_ = stop.notified()=>break,value=listener.accept()=>value};
                    let (stream, _) = accepted.unwrap();
                    stats.lock().unwrap().connections += 1;
                    let tls = tokio::select! {_ = stop.notified()=>break,value=timeout(Duration::from_secs(3),acceptor.accept(stream))=>value};
                    let Ok(Ok(tls)) = tls else {
                        stats.lock().unwrap().tls_failed += 1;
                        continue;
                    };
                    if tls.get_ref().1.server_name() == Some("ldap.fixture.test") {
                        stats.lock().unwrap().sni_ok += 1;
                    }
                    let run = serve(
                        tls,
                        scenario,
                        pause,
                        referral_port,
                        stats.clone(),
                        started.clone(),
                        resume.clone(),
                    );
                    tokio::select! {_ = stop.notified()=>break,result=timeout(Duration::from_secs(10),run)=>{assert!(result.is_ok(),"loopback server protocol timeout");}}
                }
            })
        };
        Self {
            port,
            stats,
            started,
            resume,
            stop,
            task,
        }
    }
    pub async fn finish(self) {
        self.stop.notify_one();
        timeout(Duration::from_secs(2), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}
pub struct Fixture {
    pub directory: TempDir,
    pub profiles: Arc<DirectoryProfileStore>,
    pub provider: Arc<LdapDirectoryProvider>,
    pub descriptor: DirectoryConnectionDescriptor,
}
impl Fixture {
    pub async fn new(port: u16, ad: bool, ca: Option<&str>, name: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        write_profile(directory.path(), ca, name, false);
        let profiles = Arc::new(
            DirectoryProfileStore::load(directory.path().join("profiles.json"),Arc::new(erpx_server_identity_access::public::directory::provenance::ProvenanceKeyRing::from_keys("development".into(),vec![("synthetic_provenance_v1".into(),[0x31;32])]).unwrap()))
                .await
                .unwrap(),
        );
        let fingerprint = profiles.fingerprint("fixture", CONNECTION).await.unwrap();
        let provider = Arc::new(LdapDirectoryProvider::new(
            profiles.clone(),
            Arc::new(FixedClock),
        ));
        let descriptor = DirectoryConnectionDescriptor {
            source: None,
            connection_id: CONNECTION.into(),
            holding_id: Some("h001".into()),
            company_id: "c001".into(),
            provider: if ad {
                DirectoryProvider::ActiveDirectory
            } else {
                DirectoryProvider::Univention
            },
            mode: DirectoryMode::Ldaps,
            version: 1,
            tested_version: Some(1),
            config_fingerprint: "synthetic-reviewed-config".into(),
            profile_fingerprint: fingerprint,
            config: DirectoryConnectionConfig {
                mode: DirectoryMode::Ldaps,
                secret_profile: "fixture".into(),
                authority: "synthetic fixture".into(),
                ldap_url: Some(format!("ldaps://127.0.0.1:{port}")),
                base_dn: Some("dc=fixture".into()),
                group_base_dn: None,
                user_filter: None,
                issuer: None,
                tenant_id: None,
                client_id: None,
            },
        };
        Self {
            directory,
            profiles,
            provider,
            descriptor,
        }
    }
}
pub fn write_profile(root: &Path, ca: Option<&str>, name: &str, changed: bool) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut profile = serde_json::json!({"allowedHosts":["127.0.0.1"],"connectionIds":[CONNECTION],"bindDn":SERVICE_DN,"bindPassword":SERVICE_PASSWORD,"tlsServerName":name,"identityAliases":{}});
    if let Some(ca) = ca {
        profile["caPem"] = ca.into();
    }
    if changed {
        profile["identityAliases"] = serde_json::json!({"alice-id":"changed-generation-id"});
    }
    let bytes = serde_json::to_vec(&serde_json::json!({"fixture":profile})).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(root.join("profiles.json"))
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
}
struct Wire {
    stream: TlsStream<TcpStream>,
    codec: LdapCodec,
}
impl Wire {
    async fn read(&mut self) -> Option<LdapMsg> {
        let mut header = [0u8; 6];
        if self.stream.read_exact(&mut header[..2]).await.is_err() {
            return None;
        }
        let mut count = 2;
        let length = match framing::frame_length(&header[..2]).unwrap() {
            Some(length) => length,
            None => {
                let bytes = usize::from(header[1] & 0x7f);
                self.stream
                    .read_exact(&mut header[2..2 + bytes])
                    .await
                    .unwrap();
                count += bytes;
                framing::frame_length(&header[..count]).unwrap().unwrap()
            }
        };
        let mut input = BytesMut::with_capacity(length.total);
        input.extend_from_slice(&header[..count]);
        input.resize(length.total, 0);
        self.stream.read_exact(&mut input[count..]).await.unwrap();
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            self.codec.decode(&mut input)
        })
        .unwrap()
    }
    async fn write_messages(&mut self, messages: Vec<LdapMsg>) -> std::io::Result<()> {
        let mut bytes = BytesMut::new();
        for message in messages {
            tracing::subscriber::with_default(
                tracing::subscriber::NoSubscriber::default(),
                || self.codec.encode(message, &mut bytes),
            )?;
        }
        self.stream.write_all(&bytes).await?;
        self.stream.flush().await
    }
    async fn send(&mut self, messages: Vec<LdapMsg>) {
        self.write_messages(messages).await.unwrap();
    }
    async fn raw(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).await.unwrap();
        self.stream.flush().await.unwrap();
    }
}
fn result(code: LdapResultCode) -> LdapResult {
    LdapResult {
        code,
        matcheddn: String::new(),
        message: String::new(),
        referral: vec![],
    }
}
fn bind_reply(id: i32, code: LdapResultCode) -> LdapMsg {
    LdapMsg {
        msgid: id,
        op: LdapOp::BindResponse(LdapBindResponse {
            res: result(code),
            saslcreds: None,
        }),
        ctrl: vec![],
    }
}
fn done(id: i32, cookie: Option<Vec<u8>>) -> LdapMsg {
    LdapMsg {
        msgid: id,
        op: LdapOp::SearchResultDone(result(LdapResultCode::Success)),
        ctrl: cookie
            .map(|cookie| vec![LdapControl::SimplePagedResults { size: 2, cookie }])
            .unwrap_or_default(),
    }
}
fn attr(name: &str, values: &[&str]) -> LdapPartialAttribute {
    LdapPartialAttribute {
        atype: name.into(),
        vals: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
    }
}
fn immutable(ad: bool, number: u8, id: &str) -> LdapPartialAttribute {
    if ad {
        LdapPartialAttribute {
            atype: "objectGUID".into(),
            vals: vec![vec![number; 16]],
        }
    } else {
        attr("entryUUID", &[id])
    }
}
fn entry(id: i32, dn: &str, attributes: Vec<LdapPartialAttribute>) -> LdapMsg {
    LdapMsg {
        msgid: id,
        op: LdapOp::SearchResultEntry(LdapSearchResultEntry {
            dn: dn.into(),
            attributes,
        }),
        ctrl: vec![],
    }
}
fn user(id: i32, ad: bool, bob: bool) -> LdapMsg {
    let (dn, username, name, number) = if bob {
        (BOB_DN, "bob", "Synthetic Bob", 2)
    } else {
        (USER_DN, "alice", "Synthetic Alice", 1)
    };
    entry(
        id,
        dn,
        vec![
            immutable(ad, number, if bob { "bob-id" } else { "alice-id" }),
            attr(if ad { "sAMAccountName" } else { "uid" }, &[username]),
            attr("cn", &[name]),
        ],
    )
}
fn referral(id: i32, port: Option<u16>) -> LdapMsg {
    LdapMsg {
        msgid: id,
        op: LdapOp::SearchResultReference(LdapSearchResultReference {
            uris: vec![format!("ldaps://127.0.0.1:{}", port.unwrap_or(65_534))],
        }),
        ctrl: vec![],
    }
}
fn deep() -> Vec<u8> {
    let mut data = vec![0x04, 1, b'x'];
    for _ in 0..33 {
        let mut next = vec![0x30, data.len() as u8];
        next.extend(data);
        data = next;
    }
    data
}
async fn serve(
    tls: TlsStream<TcpStream>,
    scenario: Scenario,
    pause: bool,
    referral_port: Option<u16>,
    stats: Arc<Mutex<Stats>>,
    started: Arc<Notify>,
    resume: Arc<Notify>,
) {
    let ad = matches!(
        scenario,
        Scenario::AdReferral
            | Scenario::AdRangeComplete
            | Scenario::AdRangeMissing
            | Scenario::AdRangeReferral
    );
    let mut wire = Wire {
        stream: tls,
        codec: LdapCodec::new(
            Some(framing::MAX_BER_FRAME_BYTES),
            Some(framing::MAX_BER_DEPTH),
        ),
    };
    let mut binds = 0;
    let mut page = 0usize;
    while let Some(message) = wire.read().await {
        let id = message.msgid;
        match message.op {
            LdapOp::BindRequest(request) => {
                binds += 1;
                let LdapBindCred::Simple(password) = request.cred else {
                    panic!("expected synthetic simple bind");
                };
                if binds == 1 {
                    assert!(request.dn == SERVICE_DN);
                    assert!(password == SERVICE_PASSWORD);
                    stats.lock().unwrap().service_binds += 1;
                    match scenario {
                        Scenario::WrongMessage => {
                            wire.send(vec![bind_reply(id + 1, LdapResultCode::Success)])
                                .await;
                            continue;
                        }
                        Scenario::Unsolicited => {
                            wire.send(vec![bind_reply(0, LdapResultCode::Success)])
                                .await;
                            continue;
                        }
                        Scenario::DeclaredBomb => {
                            wire.raw(&[0x30, 0x84, 0x7f, 0xff, 0xff, 0xff]).await;
                            continue;
                        }
                        Scenario::Indefinite => {
                            wire.raw(&[0x30, 0x80]).await;
                            continue;
                        }
                        Scenario::Deep => {
                            wire.raw(&deep()).await;
                            continue;
                        }
                        Scenario::MalformedPrimitive => {
                            wire.raw(&[0x30, 4, 0x01, 2, 0x7f, 0x7f]).await;
                            continue;
                        }
                        _ => {}
                    }
                    wire.send(vec![bind_reply(id, LdapResultCode::Success)])
                        .await;
                } else {
                    assert!(binds == 2 && request.dn == USER_DN);
                    stats.lock().unwrap().user_binds += 1;
                    wire.send(vec![bind_reply(
                        id,
                        if password == USER_PASSWORD {
                            LdapResultCode::Success
                        } else {
                            LdapResultCode::InvalidCredentials
                        },
                    )])
                    .await;
                }
            }
            LdapOp::SearchRequest(request) => {
                assert!(binds == 1);
                let searches = {
                    let mut s = stats.lock().unwrap();
                    s.searches += 1;
                    s.searches
                };
                if pause && searches == 1 {
                    started.notify_one();
                    resume.notified().await;
                }
                let range = request.attrs.iter().any(|a| a.starts_with("member;range="));
                if range {
                    stats.lock().unwrap().ranges += 1;
                    assert!(request.base == GROUP_DN && request.attrs == ["member;range=1-*"]);
                    match scenario {
                        Scenario::AdRangeReferral => {
                            wire.send(vec![referral(id, referral_port), done(id, None)])
                                .await
                        }
                        Scenario::AdRangeMissing => {
                            wire.send(vec![
                                entry(id, GROUP_DN, vec![attr("member", &[BOB_DN])]),
                                done(id, None),
                            ])
                            .await
                        }
                        Scenario::AdRangeComplete => {
                            wire.send(vec![
                                entry(id, GROUP_DN, vec![attr("member;range=1-*", &[BOB_DN])]),
                                done(id, None),
                            ])
                            .await
                        }
                        _ => panic!("unexpected synthetic range request"),
                    }
                    continue;
                }
                if request.sizelimit == 2 {
                    assert!(message.ctrl.is_empty());
                    wire.send(vec![user(id, ad, false), done(id, None)]).await;
                    continue;
                }
                assert!(request.sizelimit == 10001);
                assert!(matches!(
                    message.ctrl.as_slice(),
                    [LdapControl::SimplePagedResults { size: 500, .. }]
                ));
                if request.attrs.iter().any(|a| a == "displayName") {
                    page += 1;
                    match scenario {
                        Scenario::RowFlood => {
                            let mut messages = Vec::new();
                            for number in 0..10_001 {
                                let name = format!("synthetic-{number}");
                                messages.push(entry(
                                    id,
                                    &format!("uid={name},dc=fixture"),
                                    vec![attr("entryUUID", &[&name]), attr("uid", &[&name])],
                                ));
                            }
                            messages.push(done(id, Some(vec![])));
                            if wire.write_messages(messages).await.is_err() {
                                return;
                            }
                        }
                        Scenario::RepeatedCookie => {
                            wire.send(vec![
                                user(id, ad, page > 1),
                                done(id, Some(b"again".to_vec())),
                            ])
                            .await;
                        }
                        Scenario::MissingCookie => {
                            wire.send(vec![user(id, ad, false), done(id, None)]).await;
                        }
                        Scenario::PageLimit => {
                            wire.send(vec![done(id, Some(format!("page-{page}").into_bytes()))])
                                .await;
                        }
                        Scenario::UcsReferral => {
                            wire.send(vec![referral(id, referral_port), done(id, Some(vec![]))])
                                .await;
                        }
                        Scenario::Truncated => {
                            wire.send(vec![user(id, ad, false)]).await;
                            return;
                        }
                        Scenario::Paged if page == 1 => {
                            assert!(
                                matches!(message.ctrl.as_slice(),[LdapControl::SimplePagedResults{cookie,..}]if cookie.is_empty())
                            );
                            wire.send(vec![user(id, ad, false), done(id, Some(b"next".to_vec()))])
                                .await;
                        }
                        Scenario::Paged => {
                            assert!(
                                page == 2
                                    && matches!(message.ctrl.as_slice(),[LdapControl::SimplePagedResults{cookie,..}]if cookie==b"next")
                            );
                            wire.send(vec![user(id, ad, true), done(id, Some(vec![]))])
                                .await;
                        }
                        Scenario::AdReferral => {
                            wire.send(vec![
                                referral(id, referral_port),
                                user(id, ad, false),
                                done(id, Some(vec![])),
                            ])
                            .await
                        }
                        _ => {
                            wire.send(vec![user(id, ad, false), done(id, Some(vec![]))])
                                .await
                        }
                    }
                } else if request.attrs.iter().any(|a| a == "member") {
                    let mut attrs = vec![
                        immutable(ad, 3, "everyone-id"),
                        attr("cn", &["Synthetic Everyone"]),
                    ];
                    if matches!(
                        scenario,
                        Scenario::AdRangeComplete
                            | Scenario::AdRangeMissing
                            | Scenario::AdRangeReferral
                    ) {
                        attrs.push(attr("member;range=0-0", &[USER_DN]));
                    } else {
                        attrs.push(attr("member", &[USER_DN, BOB_DN]));
                    }
                    wire.send(vec![entry(id, GROUP_DN, attrs), done(id, Some(vec![]))])
                        .await;
                } else if request.attrs.iter().any(|a| a == "ou") {
                    wire.send(vec![
                        entry(
                            id,
                            "dc=fixture",
                            vec![
                                immutable(ad, 4, "root-id"),
                                attr("cn", &["Synthetic Root"]),
                                attr("objectClass", &["container"]),
                            ],
                        ),
                        done(id, Some(vec![])),
                    ])
                    .await;
                } else {
                    panic!("unexpected synthetic search shape");
                }
            }
            LdapOp::UnbindRequest => {
                stats.lock().unwrap().unbinds += 1;
                return;
            }
            _ => panic!("unexpected synthetic LDAP operation"),
        }
    }
}
