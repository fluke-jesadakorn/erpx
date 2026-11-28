//! R3 native command: sanitized-system-evidence attestation for an attachment set.
//!
//! Owner-local successor of the `POST` branch of the retained public route
//! `/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/sanitization-attestation`.
//! That route resolved a work context, validated `{confirmed:true,note}`, and
//! called the Files owner service, which executed real SQL through the
//! `holdingResources(write)` unit. This module executes those Files-owned
//! statements itself with SQLx: advisory lock, idempotency receipt read,
//! set row lock, evidence-link read, scan/sensitive-material checks, event
//! insert and set touch, then returns the exact `{ data: AttachmentSetDto }`
//! envelope and JSON content type. No TypeScript SQL, no fallback, no stub.
//!
//! Composition owns account/Holding/Company authority and the owning-record
//! write grant; both are fail-closed ports. The module is standalone: a lead
//! mounts it with the exact `pub mod` lines in the worker handoff. It changes
//! no composition, gateway, Cargo, status or ledger file.

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc, time::SystemTime};

use erpx_platform_runtime_server::public::{
    http_support::{
        body::to_bytes, header, routing::post, HeaderMap, HeaderName, IntoResponse, Json, Path,
        Request, Response, Router, State, StatusCode,
    },
    postgres::{
        DatabasePool, PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
    },
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sha2::Digest;
use sqlx::{PgConnection, Row};

// ---------------------------------------------------------------------------
// domain: pure invariants, no IO
// ---------------------------------------------------------------------------

pub const MIN_NOTE_UNITS: usize = 10;
pub const MAX_NOTE_UNITS: usize = 1000;
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_SET_ID_BYTES: usize = 200;

/// JavaScript `String.prototype.trim` whitespace and line terminators.
fn is_js_whitespace(value: char) -> bool {
    matches!(
        value,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// Zod `.trim()` + the service `note.trim()` and UTF-16 length policy.
pub fn normalize_note(raw: &str) -> Result<String, BusinessError> {
    let note = raw.trim_matches(is_js_whitespace);
    let units = note.encode_utf16().count();
    if !(MIN_NOTE_UNITS..=MAX_NOTE_UNITS).contains(&units) {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Record a concise review note of 10 to 1,000 characters.",
        ));
    }
    Ok(note.to_owned())
}

/// Identical to the generic Files command key policy (`Idempotency-Key`).
pub fn valid_command_key(key: &str) -> bool {
    (8..=180).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

pub fn valid_set_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SET_ID_BYTES
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b'?' | b'#'))
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// Byte-identical stable identity: `sha256(company\0user\0key)` first 32 hex.
pub fn attestation_event_id(company_id: &str, user_id: &str, key: &str) -> String {
    let digest = sha2::Sha256::digest(format!("{company_id}\0{user_id}\0{key}").as_bytes());
    format!("attachment-attestation-{}", &hex::encode(digest)[..32])
}

fn lower_separator(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'_' | b'.' | b'-')
}
fn ascii_lower(byte: u8) -> bool {
    byte.is_ascii_lowercase()
}
fn ascii_alnum(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

/// NFKC's ASCII-relevant part (fullwidth ASCII and ideographic space) followed
/// by Unicode lowercase, matching the owner policy's `normalize().toLocaleLowerCase()`.
fn evidence_label(filename: &str, caption: Option<&str>) -> String {
    format!("{filename}\n{}", caption.unwrap_or(""))
        .chars()
        .map(|character| {
            let code = character as u32;
            if (0xff01..=0xff5e).contains(&code) {
                char::from_u32(code - 0xfee0).unwrap_or(character)
            } else if character == '\u{3000}' {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .to_lowercase()
}

fn has_word(label: &[u8], word: &str) -> bool {
    let variants = [word.to_owned(), format!("{word}s")];
    for variant in variants {
        let needle = variant.as_bytes();
        if needle.len() > label.len() {
            continue;
        }
        for start in 0..=label.len() - needle.len() {
            if &label[start..start + needle.len()] != needle {
                continue;
            }
            let before = start.checked_sub(1).map(|i| label[i]);
            let after = label.get(start + needle.len()).copied();
            if before.is_none_or(|b| !ascii_lower(b)) && after.is_none_or(|b| !ascii_lower(b)) {
                return true;
            }
        }
    }
    false
}

fn has_joined(label: &[u8], first: &str, seconds: &[&str]) -> bool {
    let head = first.as_bytes();
    if label.len() < head.len() {
        return false;
    }
    for start in 0..=label.len() - head.len() {
        if &label[start..start + head.len()] != head {
            continue;
        }
        let mut cursor = start + head.len();
        while cursor < label.len() && lower_separator(label[cursor]) {
            cursor += 1;
        }
        for second in seconds {
            let tail = second.as_bytes();
            if label[cursor..].starts_with(tail) {
                return true;
            }
        }
    }
    false
}

fn has_prefixed_id(label: &[u8], suffixes: &[&str]) -> bool {
    for suffix in suffixes {
        let needle = format!("id_{suffix}");
        let needle = needle.as_bytes();
        if label.len() < needle.len() {
            continue;
        }
        for start in 0..=label.len() - needle.len() {
            if &label[start..start + needle.len()] != needle {
                continue;
            }
            let before = start.checked_sub(1).map(|i| label[i]);
            let allowed_before = before.is_none_or(|b| matches!(b, b'/' | b'\\' | b'.' | b'_' | b'-'));
            let after = label.get(start + needle.len()).copied();
            if allowed_before && after.is_none_or(|b| !ascii_lower(b)) {
                return true;
            }
        }
    }
    false
}

fn has_env(label: &[u8]) -> bool {
    let needle = b".env";
    if label.len() < needle.len() {
        return false;
    }
    for start in 0..=label.len() - needle.len() {
        if &label[start..start + needle.len()] == needle
            && label.get(start + needle.len()).is_none_or(|b| !ascii_alnum(*b))
        {
            return true;
        }
    }
    false
}

fn has_extension(label: &[u8], extensions: &[&str]) -> bool {
    for extension in extensions {
        let mut needle = Vec::with_capacity(extension.len() + 1);
        needle.push(b'.');
        needle.extend_from_slice(extension.as_bytes());
        if label.len() < needle.len() {
            continue;
        }
        for start in 0..=label.len() - needle.len() {
            if &label[start..start + needle.len()] == needle
                && label.get(start + needle.len()).is_none_or(|b| !ascii_alnum(*b))
            {
                return true;
            }
        }
    }
    false
}

/// Exact owner policy: passwords, keys, secrets, dumps and configuration
/// exports must never be attested as sanitized review artifacts.
pub fn sensitive_evidence(filename: &str, caption: Option<&str>) -> bool {
    let label = evidence_label(filename, caption);
    let bytes = label.as_bytes();
    has_word(bytes, "password")
        || has_word(bytes, "passwd")
        || has_word(bytes, "secret")
        || has_word(bytes, "credential")
        || has_joined(bytes, "private", &["key"])
        || has_prefixed_id(bytes, &["rsa", "dsa", "ecdsa", "ed25519"])
        || has_env(bytes)
        || has_joined(bytes, "config", &["dump", "export", "backup"])
        || has_joined(bytes, "configuration", &["dump", "export", "backup"])
        || has_joined(bytes, "database", &["dump", "export", "backup"])
        || has_joined(bytes, "db", &["dump", "export", "backup"])
        || has_extension(
            bytes,
            &["pem", "key", "p12", "pfx", "sql", "dump", "bak", "sqlite", "sqlite3", "db"],
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttestationDecision {
    Apply,
    Replay,
}

#[derive(Clone, Debug)]
pub struct EvidenceLink<'a> {
    pub id: &'a str,
    pub source_id: Option<&'a str>,
    pub filename: &'a str,
    pub checksum_sha256: Option<&'a str>,
    pub media_type: &'a str,
    pub bytes: i64,
    pub source_state: Option<&'a str>,
    pub caption: Option<&'a str>,
    pub set_id: &'a str,
}

#[derive(Clone, Debug)]
pub struct PriorAttestation<'a> {
    pub set_id: &'a str,
    pub details_json: &'a str,
}

/// Pure Files-owned attestation policy. Owner/record write authority is a
/// composition port and is checked by the SQL adapter before this decision.
pub fn prepare_attestation(
    target_type: &str,
    confirmed: bool,
    note: &str,
    set_state: &str,
    links: &[EvidenceLink<'_>],
    prior: Option<&PriorAttestation<'_>>,
) -> Result<AttestationDecision, BusinessError> {
    if target_type == "project" {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "NATIVE_PROJECT_FILES_REQUIRED",
            "Project files are served by the native project file service.",
        ));
    }
    if !crate::domain::attachment_read::requires_sanitization_attestation(target_type) {
        return Err(BusinessError::invalid(
            "ATTACHMENT_ATTESTATION_NOT_APPLICABLE",
            "This file purpose does not use the sanitized-system-evidence review.",
        ));
    }
    if !confirmed {
        return Err(BusinessError::invalid(
            "ATTACHMENT_ATTESTATION_REQUIRED",
            "Confirm the sanitized-evidence checklist before continuing.",
        ));
    }
    if normalize_note(note).is_err() {
        return Err(BusinessError::invalid(
            "ATTACHMENT_ATTESTATION_NOTE",
            "Record a concise review note of 10 to 1,000 characters.",
        ));
    }
    if set_state != "draft" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_SET_LOCKED",
            "This file set is already bound or closed.",
        ));
    }
    if links.is_empty() {
        return Err(BusinessError::invalid(
            "ATTACHMENT_SET_EMPTY",
            "Upload at least one sanitized review artifact before attesting.",
        ));
    }
    if links
        .iter()
        .any(|link| link.source_state != Some("available"))
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_SCAN_PENDING",
            "Every file must pass malware scanning before sanitization review.",
        ));
    }
    for link in links {
        if sensitive_evidence(link.filename, link.caption) {
            return Err(BusinessError::invalid(
                "ATTACHMENT_SENSITIVE_SYSTEM_MATERIAL",
                "Passwords, private keys, secrets, configuration exports and database dumps must never be uploaded. Attach only a sanitized review artifact.",
            ));
        }
    }
    let details = details_json(note, links);
    if let Some(prior) = prior {
        if prior.set_id != links[0].set_id {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key was already used for another sanitization attestation.",
            ));
        }
        if prior.details_json != details {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key was already used with a different sanitization attestation payload.",
            ));
        }
        return Ok(AttestationDecision::Replay);
    }
    Ok(AttestationDecision::Apply)
}

/// Exact `safeJson` payload: field order fixed by hand and `<` escaped as
/// `\u003c`, matching the owner writer's stored `details_json` byte for byte.
pub fn details_json(note: &str, links: &[EvidenceLink<'_>]) -> String {
    let escaped = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned());
    let note = escaped(note.trim_matches(is_js_whitespace));
    let mut manifest = String::from("[");
    for (index, link) in links.iter().enumerate() {
        if index > 0 {
            manifest.push(',');
        }
        let source = link.source_id.map(escaped).unwrap_or_else(|| "null".to_owned());
        let checksum = link
            .checksum_sha256
            .map(escaped)
            .unwrap_or_else(|| "null".to_owned());
        manifest.push_str(&format!(
            "{{\"linkId\":{},\"sourceId\":{},\"checksumSha256\":{},\"mediaType\":{},\"bytes\":{}}}",
            escaped(link.id),
            source,
            checksum,
            escaped(link.media_type),
            link.bytes
        ));
    }
    manifest.push(']');
    format!(
        "{{\"note\":{note},\"checklist\":{{\"noPasswords\":true,\"noPrivateKeys\":true,\"noSecrets\":true,\"noConfigurationDump\":true,\"noDatabaseDump\":true,\"sanitizedReviewArtifactOnly\":true}},\"manifest\":{manifest}}}"
    )
    .replace('<', "\\u003c")
}

// ---------------------------------------------------------------------------
// application: ports, no SQL
// ---------------------------------------------------------------------------

pub type AttestationFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Selected work context, resolved by composition from the request's normal
/// sign-in flow and current assignment. No request value mints this identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkActor {
    pub company_id: String,
    pub user_id: String,
}

pub trait AttestationContext: Send + Sync {
    fn work_actor<'a>(
        &'a self,
        host: &'a str,
        holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> AttestationFuture<'a, WorkActor>;
}

/// Exact Files-owned facts presented to the owning-record write authority.
#[derive(Clone, Debug)]
pub struct AttestationTarget<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub set_id: &'a str,
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub purpose_key: &'a str,
    pub component_kind: &'a str,
    pub expected_target_version: i64,
    pub created_by: &'a str,
}

pub trait AttestationAuthority: Send + Sync {
    fn authorize_attestation<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor_id: &'a str,
        target: &'a AttestationTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Bounded, normalized command handed to the SQL executor. `note` is already
/// trimmed and length-checked; `command_key` has passed the exact key regex.
#[derive(Clone, Debug)]
pub struct AttestationCommand<'a> {
    pub company_id: &'a str,
    pub user_id: &'a str,
    pub set_id: &'a str,
    pub note: &'a str,
    pub command_key: &'a str,
    pub now: i64,
}

pub trait AttestationExecutor: Send + Sync {
    fn attest<'a>(
        &'a self,
        command: AttestationCommand<'a>,
    ) -> AttestationFuture<'a, serde_json::Value>;
}

/// One exact registered Holding target. `public_origin` is the exact origin
/// the TS route derived for this host, supplied by composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestationTargetConfig {
    pub host: String,
    pub holding_slug: String,
    pub public_origin: String,
}

struct AttestationState {
    targets: BTreeMap<(String, String), String>,
    context: Arc<dyn AttestationContext>,
    executor: Arc<dyn AttestationExecutor>,
}

// ---------------------------------------------------------------------------
// adapters: SQLx execution, row mapping and axum transport
// ---------------------------------------------------------------------------

const SET_IMAGE_SQL: &str = r#"
SELECT id,target_type,target_id,component_kind,component_ref,purpose_key,
       expected_target_version,policy_version,classification,required_count,max_files,
       state,created_at,updated_at,branch_id,department_id,created_by
FROM attachment_sets WHERE id=$1 AND company_id=$2
"#;
const SET_STATE_LOCK_SQL: &str =
    "SELECT state FROM attachment_sets WHERE id=$1 AND company_id=$2 FOR UPDATE";
const PRIOR_EVENT_SQL: &str = "SELECT set_id,details_json FROM attachment_events WHERE id=$1 AND company_id=$2 AND actor_id=$3 AND action='sanitization.attested'";
const EVIDENCE_LINKS_SQL: &str = r#"
SELECT l.id,a.id source_id,a.original_name,a.checksum_sha256,a.media_type,a.bytes,a.source_state,l.caption
FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
WHERE l.set_id=$1 AND s.company_id=$2 AND l.state='draft' ORDER BY l.created_at,l.id
"#;
const ADVISORY_LOCK_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended($1,0))";
const INSERT_EVENT_SQL: &str = "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)";
const TOUCH_SET_SQL: &str =
    "UPDATE attachment_sets SET updated_at=$1 WHERE id=$2 AND company_id=$3 AND state='draft'";
const ATTESTED_SQL: &str = r#"
SELECT 1 FROM attachment_events attested
WHERE attested.set_id=$1 AND attested.company_id=$2 AND attested.action='sanitization.attested'
  AND attested.created_at>=COALESCE((
    SELECT max(changed.created_at) FROM attachment_events changed
    WHERE changed.set_id=attested.set_id AND changed.action IN ('file.reserved','link.reused','link.withdrawn')
  ),0)
ORDER BY attested.created_at DESC LIMIT 1
"#;
const DTO_LINKS_SQL: &str = r#"
SELECT COALESCE(jsonb_agg(jsonb_build_object(
  'id',links.id,'sourceId',links.attachment_id,
  'filename',COALESCE(links.original_name,links.intent_original_name,'file'),
  'mediaType',COALESCE(links.media_type,links.intent_media_type,'application/octet-stream'),
  'bytes',COALESCE(links.bytes,links.declared_bytes),
  'sourceState',links.source_state,'linkState',links.state,'caption',links.caption,
  'previewAvailable',links.preview_object_key IS NOT NULL,
  'createdAt',links.created_at,'expiresAt',links.expires_at,'boundAt',links.bound_at
) ORDER BY links.created_at,links.id),'[]'::jsonb)
FROM (
  SELECT l.id,l.attachment_id,l.state,l.caption,l.created_at,l.expires_at,l.bound_at,
         a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
         u.original_name intent_original_name,u.media_type intent_media_type,u.declared_bytes
  FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
  LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
  LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
  WHERE l.set_id=$1 AND s.company_id=$2
    AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
    AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
) links
"#;

struct SetImage {
    id: String,
    target_type: String,
    target_id: String,
    component_kind: String,
    component_ref: Option<String>,
    purpose_key: String,
    expected_target_version: i64,
    policy_version: i64,
    classification: String,
    required_count: i64,
    max_files: i64,
    state: String,
    created_at: i64,
    updated_at: i64,
    branch_id: Option<String>,
    department_id: Option<String>,
    created_by: String,
}

impl SetImage {
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            target_type: row.try_get("target_type")?,
            target_id: row.try_get("target_id")?,
            component_kind: row.try_get("component_kind")?,
            component_ref: row.try_get("component_ref")?,
            purpose_key: row.try_get("purpose_key")?,
            expected_target_version: row.try_get("expected_target_version")?,
            policy_version: row.try_get("policy_version")?,
            classification: row.try_get("classification")?,
            required_count: row.try_get("required_count")?,
            max_files: row.try_get("max_files")?,
            state: row.try_get("state")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            created_by: row.try_get("created_by")?,
        })
    }

    fn dto(&self, attested: bool, links: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "targetType": self.target_type,
            "targetId": self.target_id,
            "componentKind": self.component_kind,
            "componentRef": self.component_ref,
            "purposeKey": self.purpose_key,
            "expectedTargetVersion": self.expected_target_version,
            "policyVersion": self.policy_version,
            "classification": self.classification,
            "requiredCount": self.required_count,
            "maxFiles": self.max_files,
            "state": self.state,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at,
            "sanitizationAttested": attested,
            "links": links,
        })
    }
}

fn inaccessible() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTESTATION_UNAVAILABLE",
        "The sanitization attestation could not be recorded safely.",
    )
}

/// Files-owned SQLx executor. Composition supplies the pool and the owning
/// record authority; the executor never begins or commits a transaction itself
/// beyond the unit-of-work and never accepts a request-supplied identity.
#[derive(Clone)]
pub struct PgAttestationExecutor {
    pool: DatabasePool,
    authority: Arc<dyn AttestationAuthority>,
}

impl PgAttestationExecutor {
    pub fn new(pool: DatabasePool, authority: Arc<dyn AttestationAuthority>) -> Self {
        Self { pool, authority }
    }
}

impl AttestationExecutor for PgAttestationExecutor {
    fn attest<'a>(
        &'a self,
        command: AttestationCommand<'a>,
    ) -> AttestationFuture<'a, serde_json::Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let authority = self.authority.clone();
        let company = command.company_id.to_owned();
        let user = command.user_id.to_owned();
        let set_id = command.set_id.to_owned();
        let note = command.note.to_owned();
        let key = command.command_key.to_owned();
        let now = command.now;
        Box::pin(async move {
            let company = &company;
            let user = &user;
            let set_id = &set_id;
            let note = &note;
            let key = &key;
            let result = unit
                .run::<serde_json::Value, BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let company = company.to_owned();
                        let user = user.to_owned();
                        let set_id = set_id.to_owned();
                        let note = note.to_owned();
                        let key = key.to_owned();
                        let authority = authority.clone();
                        Box::pin(async move {
                            let lock_key =
                                format!("attachment-attestation:{company}:{user}:{key}");
                            sqlx::query(ADVISORY_LOCK_SQL)
                                .bind(lock_key)
                                .execute(&mut *connection)
                                .await?;

                            // Authority precedes the Files row lock; a stale or
                            // out-of-scope actor can never reach the write.
                            let before = sqlx::query(SET_IMAGE_SQL)
                                .bind(set_id.as_str())
                                .bind(company.as_str())
                                .fetch_optional(&mut *connection)
                                .await?;
                            let Some(before) = before else {
                                return Err(BusinessError::new(
                                    FailureKind::NotFound,
                                    "ATTACHMENT_SET_NOT_FOUND",
                                    "Attachment set not found.",
                                )
                                .into());
                            };
                            let before = SetImage::from_row(&before).map_err(|_| inaccessible())?;
                            let target = AttestationTarget {
                                company_id: company.as_str(),
                                branch_id: before.branch_id.as_deref(),
                                department_id: before.department_id.as_deref(),
                                set_id: before.id.as_str(),
                                target_type: before.target_type.as_str(),
                                target_id: before.target_id.as_str(),
                                purpose_key: before.purpose_key.as_str(),
                                component_kind: before.component_kind.as_str(),
                                expected_target_version: before.expected_target_version,
                                created_by: before.created_by.as_str(),
                            };
                            authority
                                .authorize_attestation(connection, user.as_str(), &target)
                                .await?;

                            let event_id = attestation_event_id(&company, &user, &key);
                            let prior: Option<(String, String)> =
                                sqlx::query_as(PRIOR_EVENT_SQL)
                                    .bind(event_id.as_str())
                                    .bind(company.as_str())
                                    .bind(user.as_str())
                                    .fetch_optional(&mut *connection)
                                    .await?;
                            let prior = prior.as_ref().map(|(set_id, details)| PriorAttestation {
                                set_id: set_id.as_str(),
                                details_json: details.as_str(),
                            });

                            let locked_state: Option<String> =
                                sqlx::query_scalar(SET_STATE_LOCK_SQL)
                                    .bind(set_id.as_str())
                                    .bind(company.as_str())
                                    .fetch_optional(&mut *connection)
                                    .await?;
                            let Some(locked_state) = locked_state else {
                                return Err(BusinessError::new(
                                    FailureKind::NotFound,
                                    "ATTACHMENT_SET_NOT_FOUND",
                                    "Attachment set not found.",
                                )
                                .into());
                            };

                            let rows: Vec<(
                                String,
                                Option<String>,
                                Option<String>,
                                Option<String>,
                                Option<String>,
                                Option<i64>,
                                Option<String>,
                                Option<String>,
                            )> = sqlx::query_as(EVIDENCE_LINKS_SQL)
                                .bind(set_id.as_str())
                                .bind(company.as_str())
                                .fetch_all(&mut *connection)
                                .await?;
                            let links: Vec<EvidenceLink<'_>> = rows
                                .iter()
                                .map(
                                    |(id, source, filename, checksum, media, bytes, state, caption)| {
                                        EvidenceLink {
                                            id: id.as_str(),
                                            source_id: source.as_deref(),
                                            filename: filename.as_deref().unwrap_or("file"),
                                            checksum_sha256: checksum.as_deref(),
                                            media_type: media
                                                .as_deref()
                                                .unwrap_or("application/octet-stream"),
                                            bytes: bytes.unwrap_or(0),
                                            source_state: state.as_deref(),
                                            caption: caption.as_deref(),
                                            set_id: set_id.as_str(),
                                        }
                                    },
                                )
                                .collect();

                            let decision = prepare_attestation(
                                &before.target_type,
                                true,
                                note.as_str(),
                                &locked_state,
                                &links,
                                prior.as_ref(),
                            )?;
                            match decision {
                                AttestationDecision::Apply => {
                                    let details = details_json(&note, &links);
                                    sqlx::query(INSERT_EVENT_SQL)
                                        .bind(event_id.as_str())
                                        .bind(company.as_str())
                                        .bind(set_id.as_str())
                                        .bind(user.as_str())
                                        .bind("sanitization.attested")
                                        .bind(details)
                                        .bind(now)
                                        .execute(&mut *connection)
                                        .await?;
                                    sqlx::query(TOUCH_SET_SQL)
                                        .bind(now)
                                        .bind(set_id.as_str())
                                        .bind(company.as_str())
                                        .execute(&mut *connection)
                                        .await?;
                                }
                                AttestationDecision::Replay => {}
                            }

                            let row = sqlx::query(SET_IMAGE_SQL)
                                .bind(set_id.as_str())
                                .bind(company.as_str())
                                .fetch_one(&mut *connection)
                                .await?;
                            let current = SetImage::from_row(&row).map_err(|_| inaccessible())?;
                            let attested: bool = sqlx::query_scalar::<_, i32>(ATTESTED_SQL)
                                .bind(set_id.as_str())
                                .bind(company.as_str())
                                .fetch_optional(&mut *connection)
                                .await?
                                .is_some();
                            let links: serde_json::Value = sqlx::query_scalar(DTO_LINKS_SQL)
                                .bind(set_id.as_str())
                                .bind(company.as_str())
                                .fetch_one(&mut *connection)
                                .await?;
                            Ok(current.dto(attested, links))
                        })
                    },
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => inaccessible(),
                })?;
            Ok(result)
        })
    }
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Invalid command request.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid command request."))
        })
        .transpose()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Exact distinct `(host, holding_slug)` targets; no wildcard or suffix match.
/// Composition mounts the returned router; this module never edits composition.
pub fn router(
    executor: Arc<dyn AttestationExecutor>,
    context: Arc<dyn AttestationContext>,
    targets: Vec<AttestationTargetConfig>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.public_origin.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target.public_origin,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FILES_ATTESTATION_ROUTE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "FILES_ATTESTATION_ROUTE_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-sets/{set_id}/sanitization-attestation",
            post(attest),
        )
        .with_state(Arc::new(AttestationState {
            targets: configured,
            context,
            executor,
        })))
}

#[derive(Debug, Deserialize)]
struct AttestationBody {
    confirmed: bool,
    note: String,
}

async fn attest(
    State(state): State<Arc<AttestationState>>,
    Path((slug, set_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let public_origin = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if let Some(origin) = unique_header(&headers, header::ORIGIN)? {
            if origin != public_origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the application origin.",
                ));
            }
        }
        let key = unique_header(&headers, HeaderName::from_static("idempotency-key"))?
            .filter(|value| valid_command_key(value))
            .ok_or_else(|| {
                BusinessError::invalid(
                    "COMMAND_KEY_REQUIRED",
                    "A valid Idempotency-Key is required.",
                )
            })?;
        let actor = state.context.work_actor(host, &slug, &headers).await?;
        if !valid_set_id(&set_id) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_SET_NOT_FOUND",
                "Attachment set not found.",
            ));
        }
        let bytes = to_bytes(request.into_body(), MAX_BODY_BYTES)
            .await
            .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Invalid command request."))?;
        let body: AttestationBody = serde_json::from_slice(&bytes).map_err(|_| {
            BusinessError::invalid("VALIDATION_FAILED", "Confirm the sanitized-evidence review.")
        })?;
        if !body.confirmed {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Confirm the sanitized-evidence review.",
            ));
        }
        let note = normalize_note(&body.note)?;
        let dto = state
            .executor
            .attest(AttestationCommand {
                company_id: &actor.company_id,
                user_id: &actor.user_id,
                set_id: &set_id,
                note: &note,
                command_key: key,
                now: now_millis(),
            })
            .await?;
        Ok(serde_json::json!({ "data": dto }))
    }
    .await;
    match result {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const TARGET: &str = "iam_review";

    fn link<'a>(id: &'a str, state: Option<&'a str>, filename: &'a str) -> EvidenceLink<'a> {
        EvidenceLink {
            id,
            source_id: Some("source-1"),
            filename,
            checksum_sha256: Some("abc123"),
            media_type: "application/pdf",
            bytes: 128,
            source_state: state,
            caption: None,
            set_id: "set-1",
        }
    }

    #[test]
    fn note_trims_and_counts_utf16_units() {
        assert_eq!(normalize_note("  review complete  ").expect("note"), "review complete");
        assert!(normalize_note("          ").is_err());
        let too_long = format!("{}😀", "a".repeat(999));
        assert!(normalize_note(&too_long).is_err());
    }

    #[test]
    fn command_key_and_set_id_are_bounded() {
        assert!(valid_command_key("key_2026-11-28:01"));
        assert!(!valid_command_key("short"));
        assert!(!valid_command_key("bad key!"));
        assert!(valid_set_id("set-1"));
        assert!(!valid_set_id("bad/set"));
    }

    #[test]
    fn event_id_is_stable_and_scoped() {
        assert_eq!(
            attestation_event_id("company", "user", "key"),
            attestation_event_id("company", "user", "key")
        );
        assert_ne!(
            attestation_event_id("company", "user", "key"),
            attestation_event_id("company", "other", "key")
        );
    }

    #[test]
    fn sensitive_evidence_patterns_match_owner_policy() {
        for filename in [
            "passwords.pdf",
            "private-key.pem",
            "id_rsa.txt",
            ".env.local",
            "config_dump.json",
            "database backup.sqlite3",
            "review.bak",
        ] {
            assert!(sensitive_evidence(filename, None), "{filename}");
        }
        assert!(!sensitive_evidence("sanitized-review-artifact.pdf", Some("redacted review")));
    }

    #[test]
    fn details_json_is_stable_and_escapes_material() {
        let links = vec![link("link-1", Some("available"), "review.pdf")];
        let text = details_json("reviewed <all> artifacts", &links);
        assert!(text.starts_with("{\"note\":\"reviewed \\u003call> artifacts\","));
        assert!(text.contains("\"checklist\":{\"noPasswords\":true"));
        assert_eq!(text, details_json("reviewed <all> artifacts", &links));
    }

    #[test]
    fn attestation_decision_enforces_scan_and_idempotency_rules() {
        let clean = vec![link("link-1", Some("available"), "review.pdf")];
        assert!(prepare_attestation("other", true, "reviewed all artifacts", "draft", &clean, None).is_err());
        assert!(prepare_attestation(TARGET, false, "reviewed all artifacts", "draft", &clean, None).is_err());
        assert!(prepare_attestation(TARGET, true, "short", "draft", &clean, None).is_err());
        assert!(prepare_attestation(TARGET, true, "reviewed all artifacts", "bound", &clean, None).is_err());
        assert!(prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &[], None).is_err());
        assert!(prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &[link("l", Some("scanning"), "review.pdf")], None).is_err());
        assert!(prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &[link("l", Some("available"), "id_rsa")], None).is_err());
        assert_eq!(
            prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &clean, None).expect("decision"),
            AttestationDecision::Apply
        );
        let details = details_json("reviewed all artifacts", &clean);
        let prior = PriorAttestation { set_id: "set-1", details_json: &details };
        assert_eq!(
            prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &clean, Some(&prior)).expect("replay"),
            AttestationDecision::Replay
        );
        let other = PriorAttestation { set_id: "set-2", details_json: &details };
        assert!(prepare_attestation(TARGET, true, "reviewed all artifacts", "draft", &clean, Some(&other)).is_err());
    }

    struct Probe {
        commands: Mutex<Vec<String>>,
        replay: bool,
    }
    impl AttestationContext for Probe {
        fn work_actor<'a>(
            &'a self,
            _host: &'a str,
            _slug: &'a str,
            _headers: &'a HeaderMap,
        ) -> AttestationFuture<'a, WorkActor> {
            Box::pin(async {
                Ok(WorkActor {
                    company_id: "company".to_owned(),
                    user_id: "user".to_owned(),
                })
            })
        }
    }
    impl AttestationExecutor for Probe {
        fn attest<'a>(
            &'a self,
            command: AttestationCommand<'a>,
        ) -> AttestationFuture<'a, serde_json::Value> {
            Box::pin(async move {
                self.commands.lock().expect("mutex").push(command.set_id.to_owned());
                Ok(serde_json::json!({
                    "id": command.set_id,
                    "state": if self.replay { "bound" } else { "draft" },
                    "sanitizationAttested": true,
                    "links": [],
                }))
            })
        }
    }
    fn target() -> AttestationTargetConfig {
        AttestationTargetConfig {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
            public_origin: "https://acme.example.test".to_owned(),
        }
    }
    async fn serve(probe: Arc<Probe>) -> (u16, tokio::task::JoinHandle<()>) {
        let context: Arc<dyn AttestationContext> = probe.clone();
        let executor: Arc<dyn AttestationExecutor> = probe;
        let router = router(executor, context, vec![target()]).expect("router");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let port = listener.local_addr().expect("addr").port();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (port, server)
    }
    fn request(host: &str, slug: &str, set: &str, origin: Option<&str>, key: Option<&str>, body: &str) -> String {
        let origin = origin.map(|value| format!("Origin: {value}\r\n")).unwrap_or_default();
        let key = key.map(|value| format!("Idempotency-Key: {value}\r\n")).unwrap_or_default();
        format!(
            "POST /api/v1/holdings/{slug}/attachment-sets/{set}/sanitization-attestation HTTP/1.1\r\n\
             Host: {host}\r\nContent-Type: application/json\r\n{origin}{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream.write_all(raw.as_bytes()).await.expect("write");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        let text = String::from_utf8_lossy(&bytes);
        let status = text.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok()).expect("status");
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or(("", ""));
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null), head.to_owned())
    }

    #[tokio::test]
    async fn serves_exact_data_envelope_and_json_content_type() {
        let probe = Arc::new(Probe { commands: Mutex::new(Vec::new()), replay: false });
        let (port, server) = serve(probe.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let (status, body, head) = http(addr, request(
            "acme.example.test", "acme", "set-1", Some("https://acme.example.test"),
            Some("key_2026-11-28:01"), "{\"confirmed\":true,\"note\":\"reviewed all artifacts\"}",
        )).await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["id"], "set-1");
        assert_eq!(body["data"]["sanitizationAttested"], true);
        assert!(body.get("context").is_none());
        assert!(head.to_ascii_lowercase().contains("content-type: application/json"));
        assert_eq!(*probe.commands.lock().expect("mutex"), vec!["set-1".to_owned()]);
    }

    #[tokio::test]
    async fn fails_closed_on_origin_key_validation_and_target() {
        let probe = Arc::new(Probe { commands: Mutex::new(Vec::new()), replay: false });
        let (port, server) = serve(probe.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let cases = [
            (request("acme.example.test", "acme", "set-1", Some("https://evil.test"), Some("key_2026-11-28:01"), "{\"confirmed\":true,\"note\":\"reviewed all artifacts\"}"), 403, "ORIGIN_FORBIDDEN"),
            (request("acme.example.test", "acme", "set-1", None, None, "{\"confirmed\":true,\"note\":\"reviewed all artifacts\"}"), 422, "COMMAND_KEY_REQUIRED"),
            (request("acme.example.test", "acme", "set-1", None, Some("key_2026-11-28:01"), "{\"confirmed\":false,\"note\":\"reviewed all artifacts\"}"), 422, "VALIDATION_FAILED"),
            (request("acme.example.test", "acme", "set-1", None, Some("key_2026-11-28:01"), "{\"confirmed\":true,\"note\":\"short\"}"), 422, "VALIDATION_FAILED"),
            (request("evil.example.test", "acme", "set-1", None, Some("key_2026-11-28:01"), "{\"confirmed\":true,\"note\":\"reviewed all artifacts\"}"), 404, "NOT_FOUND"),
            (request("acme.example.test", "acme", "bad%5Cset", None, Some("key_2026-11-28:01"), "{\"confirmed\":true,\"note\":\"reviewed all artifacts\"}"), 404, "ATTACHMENT_SET_NOT_FOUND"),
        ];
        for (raw, expected_status, code) in cases {
            let (status, body, _) = http(addr, raw).await;
            assert_eq!(status, expected_status, "{code}");
            assert_eq!(body["error"]["code"], code);
        }
        server.abort();
        assert!(probe.commands.lock().expect("mutex").is_empty());
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            commands: Mutex::new(Vec::new()),
            replay: false,
        });
        let context: Arc<dyn AttestationContext> = probe.clone();
        let executor: Arc<dyn AttestationExecutor> = probe.clone();
        assert!(router(executor.clone(), context.clone(), vec![target()]).is_ok());
        assert!(router(executor.clone(), context.clone(), vec![target(), target()]).is_err());
        assert!(router(executor.clone(), context.clone(), vec![]).is_err());
        assert!(router(
            executor,
            context,
            vec![AttestationTargetConfig {
                host: "a".to_owned(),
                holding_slug: "BAD".to_owned(),
                public_origin: "https://a".to_owned(),
            }]
        )
        .is_err());
    }
}
