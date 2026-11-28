//! PostgreSQL adapter for the bounded native Files attachment content read.
//!
//! Only Files-owned relations are read (`attachment_links`, `attachment_sets`,
//! `attachments`) and only the Files-owned access audit is written. The owning
//! record authority is a composition port; this adapter never grants access and
//! never opens, commits or retries a transaction.
use crate::adapters::attachment_content_use_cases::AttachmentContentPorts;
use crate::domain::attachment_content::ContentFacts;
use erpx_platform_runtime_server::public::postgres::PgWorkFuture;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

const CONTENT_SQL: &str = r#"
SELECT l.id,l.set_id,s.company_id,s.target_type,s.target_id,s.purpose_key,s.component_kind,
       l.created_by,l.state link_state,l.attachment_id source_id,a.source_state,
       a.scanned_at,a.scanner_definition_at,a.revoked_at,a.destroyed_at,
       a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.object_key,
       a.preview_object_key,a.preview_bytes,a.preview_checksum_sha256,a.preview_media_type
FROM attachment_links l
JOIN attachment_sets s ON s.id=l.set_id
LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
WHERE l.id=$1 AND s.company_id=$2
"#;

const ACCESS_EVENT_SQL: &str = r#"
INSERT INTO attachment_access_events(
  id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at
) VALUES($1,$2,$3,$4,$5,$6,'allowed',$7)
"#;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_CONTENT_UNAVAILABLE",
        "The native file reader is unavailable.",
    )
}

/// Composition supplies the current owning-record authority. It receives only
/// Files-owned facts and returns no request-derived grant.
pub trait AttachmentContentAuthority: Send + Sync {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        facts: &'a ContentFacts,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

pub struct PgContentPorts<A> {
    authority: A,
}

impl<A: AttachmentContentAuthority> PgContentPorts<A> {
    pub fn new(authority: A) -> Self {
        Self { authority }
    }
}

impl<A: AttachmentContentAuthority> AttachmentContentPorts<PgConnection> for PgContentPorts<A> {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        facts: &'a ContentFacts,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        self.authority.authorize(c, facts)
    }

    fn load<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        link_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ContentFacts>, BusinessError> {
        Box::pin(async move {
            let row = sqlx::query(CONTENT_SQL)
                .bind(link_id)
                .bind(company_id)
                .fetch_optional(&mut *c)
                .await
                .map_err(|_| unavailable())?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(ContentFacts {
                link_id: row.get("id"),
                set_id: row.get("set_id"),
                company_id: row.get("company_id"),
                target_type: row.get("target_type"),
                target_id: row.get("target_id"),
                purpose_key: row.get("purpose_key"),
                component_kind: row.get("component_kind"),
                created_by: row.get("created_by"),
                link_state: row.get("link_state"),
                source_id: row.get("source_id"),
                source_state: row.get("source_state"),
                scanned_at: row.get("scanned_at"),
                scanner_definition_at: row.get("scanner_definition_at"),
                revoked_at: row.get("revoked_at"),
                destroyed_at: row.get("destroyed_at"),
                original_name: row.get("original_name"),
                media_type: row.get("media_type"),
                bytes: row.get("bytes"),
                checksum_sha256: row.get("checksum_sha256"),
                object_key: row.get("object_key"),
                preview_object_key: row.get("preview_object_key"),
                preview_bytes: row.get("preview_bytes"),
                preview_checksum_sha256: row.get("preview_checksum_sha256"),
                preview_media_type: row.get("preview_media_type"),
            }))
        })
    }

    fn record_access<'a>(
        &'a self,
        c: &'a mut PgConnection,
        facts: &'a ContentFacts,
        variant: &'static str,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let Some(attachment_id) = facts.source_id.as_deref() else {
                return Ok(());
            };
            sqlx::query(ACCESS_EVENT_SQL)
                .bind(format!("attachment-access-{}", uuid::Uuid::new_v4()))
                .bind(facts.company_id.as_str())
                .bind(attachment_id)
                .bind(facts.link_id.as_str())
                .bind(facts.created_by.as_str())
                .bind(variant)
                .bind(now)
                .execute(&mut *c)
                .await
                .map_err(|_| unavailable())?;
            Ok(())
        })
    }
}
