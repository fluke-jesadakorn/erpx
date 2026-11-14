//! PostgreSQL adapter for the bounded native Files attachment metadata read.
//!
//! Only Files-owned relations are read (`attachment_sets`, `attachment_links`,
//! `attachments`, `upload_intents`, `attachment_scan_attempts`,
//! `attachment_events`, `attachment_access_events`). The owning record's
//! authority is supplied by the composition port; this adapter never reads
//! another owner's table and never opens, commits or retries a transaction.
use crate::adapters::attachment_read_use_cases::{AttachmentReadPorts, AttachmentTargetAuthority};
use crate::domain::attachment_read::{
    AttachmentListFilter, AttachmentReader, AttachmentSetFacts, AttachmentStatusFacts,
    AttachmentTarget, AttachmentTargetScope, MAX_SET_RESPONSE_BYTES, MAX_SETS,
    MAX_STATUS_RESPONSE_BYTES,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const SETS_SQL: &str = r#"
SELECT CASE WHEN octet_length(image::text) <= $11 THEN image END
FROM (
  SELECT COALESCE(jsonb_agg(row_json ORDER BY created_at DESC, id DESC), '[]'::jsonb) AS image
  FROM (
    SELECT s.created_at, s.id, jsonb_build_object(
      'companyId', s.company_id,
      'id', s.id,
      'targetType', s.target_type,
      'targetId', s.target_id,
      'componentKind', s.component_kind,
      'componentRef', s.component_ref,
      'purposeKey', s.purpose_key,
      'expectedTargetVersion', s.expected_target_version,
      'policyVersion', s.policy_version,
      'classification', s.classification,
      'requiredCount', s.required_count,
      'maxFiles', s.max_files,
      'state', s.state,
      'createdAt', s.created_at,
      'updatedAt', s.updated_at,
      'sanitizationAttested', CASE WHEN s.target_type IN ('iam_review','iam_migration','company_release_acceptance','people_provision_operation','company_provisioning')
        THEN EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id AND e.action='sanitization.attested')
        ELSE NULL END,
      'links', COALESCE((
        SELECT jsonb_agg(jsonb_build_object(
          'id', l.id,
          'sourceId', l.attachment_id,
          'filename', COALESCE(a.original_name, u.original_name, 'file'),
          'mediaType', COALESCE(a.media_type, u.media_type, 'application/octet-stream'),
          'bytes', COALESCE(a.bytes, u.declared_bytes),
          'sourceState', a.source_state,
          'linkState', l.state,
          'caption', l.caption,
          'previewAvailable', a.preview_object_key IS NOT NULL,
          'createdAt', l.created_at,
          'expiresAt', l.expires_at,
          'boundAt', l.bound_at
        ) ORDER BY l.created_at, l.id)
        FROM attachment_links l
        LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
        LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
        WHERE l.set_id=s.id
      ), '[]'::jsonb)
    ) AS row_json
    FROM attachment_sets s
    WHERE s.company_id=$1
      AND s.target_type=$2
      AND s.target_id=$3
      AND ($4::text IS NULL OR s.purpose_key=$4)
      AND ($5::text IS NULL OR s.component_kind=$5)
      AND ($6::bigint IS NULL OR s.expected_target_version=$6)
      AND (CASE WHEN $7::boolean THEN s.state='bound'
                WHEN $8::boolean THEN s.state IN ('draft','bound')
                ELSE (s.state='bound' OR (s.state='draft' AND s.created_by=$9)) END)
    ORDER BY s.created_at DESC, s.id DESC
    LIMIT $10
  ) bounded
) sized
"#;

const STATUS_SQL: &str = r#"
SELECT CASE WHEN octet_length(image::text) <= $3 THEN image END
FROM (
  SELECT jsonb_build_object(
    'companyId', s.company_id,
    'linkId', l.id,
    'setId', l.set_id,
    'creatorId', l.created_by,
    'targetType', s.target_type,
    'targetId', s.target_id,
    'componentKind', s.component_kind,
    'purposeKey', s.purpose_key,
    'expectedTargetVersion', s.expected_target_version,
    'sourceId', a.id,
    'sourceState', a.source_state,
    'linkState', l.state,
    'previewAvailable', a.preview_object_key IS NOT NULL,
    'expiresAt', l.expires_at,
    'boundAt', l.bound_at,
    'withdrawnAt', l.withdrawn_at,
    'scan', jsonb_build_object(
      'attempts', COALESCE(j.attempt, 0),
      'lastResult', j.state,
      'lastError', j.error_code,
      'signature', j.scanner_signature,
      'definitionUpdatedAt', j.scanner_definition_at,
      'scannedAt', j.completed_at
    )
  ) AS image
  FROM attachment_links l
  JOIN attachment_sets s ON s.id=l.set_id
  LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
  LEFT JOIN LATERAL (
    SELECT attempt,state,error_code,scanner_signature,scanner_definition_at,completed_at
    FROM attachment_scan_attempts
    WHERE attachment_id=a.id
    ORDER BY attempt DESC
    LIMIT 1
  ) j ON true
  WHERE l.id=$1
    AND s.company_id=$2
    AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
) sized
"#;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_READ_CHANGED",
        "The file metadata changed while it was being read.",
    )
    .into()
}

fn invalid_projection() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_READ_INVALID",
        "The file metadata could not be read safely.",
    )
    .into()
}

async fn serializable_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Files ledger classification only; this grants no authority to read a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentOwnerKind {
    FieldVisit,
    Other,
}

pub struct AttachmentOwnershipWitness {
    company: erpx_server_shared::public::CompanyId,
    unit: (String, i32, String),
    link_id: String,
    kind: AttachmentOwnerKind,
    digest: Vec<u8>,
}

impl AttachmentOwnershipWitness {
    pub fn kind(&self) -> AttachmentOwnerKind {
        self.kind
    }
}

/// Persisted Files target facts, not a determination that a Count exists,
/// belongs to Service, or is readable by the selected actor.
#[derive(Debug, PartialEq, Eq)]
pub struct StockCountAttachmentTarget {
    pub target_id: String,
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
}

/// Persisted Files target metadata only, never Finance ownership or authority.
#[derive(Debug, PartialEq, Eq)]
pub struct FinanceAttachmentTarget {
    pub target_type: String,
    pub target_id: String,
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
}

/// Read only the two Finance target types from this already-retained Files
/// source. Every target-specific Finance and selected-actor check remains a
/// separate obligation of the owning coordinator.
pub async fn finance_attachment_target(
    c: &mut PgConnection,
    witness: &AttachmentOwnershipWitness,
) -> Result<Option<FinanceAttachmentTarget>, Error> {
    revalidate_attachment_ownership(c, witness).await?;
    let row:Option<(String,String,String,String,Option<String>)>=sqlx::query_as("SELECT s.target_type,s.target_id,s.purpose_key,s.component_kind,s.component_ref FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 AND s.target_type IN ('finance_journal','finance_payment') AND (l.attachment_id IS NULL OR a.id IS NOT NULL)")
        .bind(&witness.link_id).bind(witness.company.as_str()).fetch_optional(c).await?;
    Ok(row.map(|(target_type,target_id,purpose_key,component_kind,component_ref)| FinanceAttachmentTarget{target_type,target_id,purpose_key,component_kind,component_ref}))
}

/// Read exact `stock_count` target fields only from the witness's locked unit.
/// Other target types, including `inventory_stock_count`, remain separate.
pub async fn stock_count_attachment_target(
    c: &mut PgConnection,
    witness: &AttachmentOwnershipWitness,
) -> Result<Option<StockCountAttachmentTarget>, Error> {
    revalidate_attachment_ownership(c, witness).await?;
    let row: Option<(String, String, String, Option<String>)> = sqlx::query_as("SELECT s.target_id,s.purpose_key,s.component_kind,s.component_ref FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 AND s.target_type='stock_count' AND (l.attachment_id IS NULL OR a.id IS NOT NULL)")
        .bind(&witness.link_id).bind(witness.company.as_str()).fetch_optional(c).await?;
    Ok(
        row.map(|(target_id, purpose_key, component_kind, component_ref)| {
            StockCountAttachmentTarget {
                target_id,
                purpose_key,
                component_kind,
                component_ref,
            }
        }),
    )
}

/// Acquire before the first Holding snapshot, together with every target-owner
/// lock. The caller must establish a compatible global lock order.
pub async fn lock_attachment_ownership_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachment_sets, attachment_links, attachments IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

async fn ownership_unit(c: &mut PgConnection) -> Result<(String, i32, String), Error> {
    serializable_unit(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachment_sets'::regclass,'attachment_links'::regclass,'attachments'::regclass)")
        .fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("FILES_OWNERSHIP_LOCKS_REQUIRED").into());
    }
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text")
            .fetch_one(c)
            .await?,
    )
}

async fn ownership_facts(
    c: &mut PgConnection,
    company: &erpx_server_shared::public::CompanyId,
    link: &str,
) -> Result<Option<(AttachmentOwnerKind, Vec<u8>)>, Error> {
    let row: Option<(bool, Vec<u8>)> = sqlx::query_as("SELECT s.target_type='field_service_visit', sha256(convert_to(jsonb_build_array(s.company_id,l.id,s.id,s.target_type,s.target_id,s.component_kind,s.component_ref,s.purpose_key,l.attachment_id,a.id)::text,'UTF8')) FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 AND (l.attachment_id IS NULL OR a.id IS NOT NULL)")
        .bind(link).bind(company.as_str()).fetch_optional(c).await?;
    Ok(row.map(|(field, digest)| {
        (
            if field {
                AttachmentOwnerKind::FieldVisit
            } else {
                AttachmentOwnerKind::Other
            },
            digest,
        )
    }))
}

/// Bind only after all owner source locks are held. Lock presence does not prove
/// that the caller acquired them before its first snapshot.
pub async fn inspect_attachment_ownership(
    c: &mut PgConnection,
    company: &erpx_server_shared::public::CompanyId,
    link: &str,
) -> Result<Option<AttachmentOwnershipWitness>, Error> {
    let unit = ownership_unit(c).await?;
    Ok(ownership_facts(c, company, link)
        .await?
        .map(|(kind, digest)| AttachmentOwnershipWitness {
            company: company.clone(),
            unit,
            link_id: link.to_owned(),
            kind,
            digest,
        }))
}

pub async fn revalidate_attachment_ownership(
    c: &mut PgConnection,
    witness: &AttachmentOwnershipWitness,
) -> Result<(), Error> {
    if ownership_unit(c).await? != witness.unit
        || ownership_facts(c, &witness.company, &witness.link_id)
            .await?
            .as_ref()
            != Some(&(witness.kind, witness.digest.clone()))
    {
        return Err(changed());
    }
    Ok(())
}

fn parse_sets(value: Option<serde_json::Value>) -> Result<Vec<AttachmentSetFacts>, Error> {
    let value = value.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "ATTACHMENT_LIST_TOO_LARGE",
            "This record has too many files to show at once.",
        )
    })?;
    serde_json::from_value::<Vec<AttachmentSetFacts>>(value).map_err(|_| invalid_projection())
}

fn parse_status(value: Option<serde_json::Value>) -> Result<Option<AttachmentStatusFacts>, Error> {
    match value {
        Some(value) => serde_json::from_value::<AttachmentStatusFacts>(value)
            .map(Some)
            .map_err(|_| invalid_projection()),
        None => Ok(None),
    }
}

/// Files-owned PostgreSQL ledger and status audit implementation. The generic
/// authority is the composition root's current owner-record read port.
#[derive(Clone)]
pub struct PostgresAttachmentRead<A> {
    authority: A,
}
impl<A> PostgresAttachmentRead<A> {
    pub fn new(authority: A) -> Self {
        Self { authority }
    }
    pub fn authority(&self) -> &A {
        &self.authority
    }
}
impl<A: AttachmentTargetAuthority> AttachmentReadPorts for PostgresAttachmentRead<A> {
    fn authorize_target<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        target: &'a AttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, AttachmentTargetScope, BusinessError> {
        self.authority.authorize_read(c, reader, target)
    }

    fn list_sets<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        filter: &'a AttachmentListFilter,
        read_only: bool,
    ) -> PgWorkFuture<'a, Vec<AttachmentSetFacts>, BusinessError> {
        Box::pin(async move {
            serializable_unit(c).await?;
            let leave_review = filter.leave_review();
            let value: Option<serde_json::Value> = sqlx::query_scalar(SETS_SQL)
                .bind(reader.company_id().as_str())
                .bind(filter.target_type())
                .bind(filter.target_id())
                .bind(filter.purpose_key())
                .bind(filter.component_kind())
                .bind(filter.expected_target_version())
                .bind(read_only)
                .bind(leave_review)
                .bind(reader.user_id())
                .bind((MAX_SETS + 1) as i64)
                .bind(MAX_SET_RESPONSE_BYTES as i64)
                .fetch_one(&mut *c)
                .await?;
            parse_sets(value)
        })
    }

    fn read_status<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        link_id: &'a str,
        read_only: bool,
    ) -> PgWorkFuture<'a, Option<AttachmentStatusFacts>, BusinessError> {
        Box::pin(async move {
            serializable_unit(c).await?;
            let value: Option<serde_json::Value> = sqlx::query_scalar(STATUS_SQL)
                .bind(link_id)
                .bind(reader.company_id().as_str())
                .bind(MAX_STATUS_RESPONSE_BYTES as i64)
                .fetch_optional(&mut *c)
                .await?
                .flatten();
            // `None` covers both an absent row and an oversized row; the latter
            // must never be mistaken for a missing link.
            let present: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 AND (l.attachment_id IS NULL OR a.id IS NOT NULL))",
            )
            .bind(link_id)
            .bind(reader.company_id().as_str())
            .fetch_one(&mut *c)
            .await?;
            if present && value.is_none() {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "ATTACHMENT_STATUS_TOO_LARGE",
                    "The file status could not be read safely.",
                )
                .into());
            }
            let facts = parse_status(value)?;
            if let Some(facts) = &facts
                && read_only
                && (facts.link_state != "bound"
                    || facts.source_state.as_deref() != Some("available"))
            {
                return Ok(None);
            }
            Ok(facts)
        })
    }

    fn record_status_access<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        link_id: &'a str,
        attachment_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            serializable_unit(c).await?;
            let id = format!("attachment-access-{}", uuid::Uuid::new_v4());
            let written = sqlx::query(
                "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,'status','allowed',$6)",
            )
            .bind(id)
            .bind(reader.company_id().as_str())
            .bind(attachment_id)
            .bind(link_id)
            .bind(reader.user_id())
            .bind(now)
            .execute(&mut *c)
            .await?;
            if written.rows_affected() != 1 {
                return Err(changed());
            }
            Ok(())
        })
    }
}
