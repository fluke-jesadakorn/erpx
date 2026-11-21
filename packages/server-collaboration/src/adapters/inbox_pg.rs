//! Production read unit for the workplace Inbox.
//!
//! Notices and Work-projection delivery rows are read directly from the
//! collaboration-owned Inbox tables. Every other record destination is a
//! composition port; when that port is absent the unit fails closed with
//! `INBOX_TARGET_PORT_UNAVAILABLE` rather than inventing a link.
use crate::adapters::command_ledger::{Error, source_changed};
use crate::adapters::inbox_http::{
    InboxCategoryCounts, InboxItemData, InboxPageData, InboxSummaryData,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, FailureKind, UserId};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::future::Future;
use std::pin::Pin;

pub type InboxTargetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<InboxTarget>, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Default)]
pub struct InboxTarget {
    pub href: Option<String>,
    pub source: Option<Value>,
}

/// Cross-owner destination resolver. Composition binds it to the owning
/// read models; the Inbox unit never guesses a URL.
pub trait InboxTargetPort: Send {
    fn target<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> InboxTargetFuture<'a>;
}

/// Fail-closed resolver used when composition has not supplied owner targets.
pub struct UnavailableInboxTargets;

impl InboxTargetPort for UnavailableInboxTargets {
    fn target<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _company_id: &'a str,
        _user_id: &'a str,
        _source_type: &'a str,
        _source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "INBOX_TARGET_PORT_UNAVAILABLE",
                "This Inbox destination requires its owning workspace.",
            ))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboxCursor {
    pub at: i64,
    pub id: String,
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0] as u32,
            *chunk.get(1).unwrap_or(&0) as u32,
            *chunk.get(2).unwrap_or(&0) as u32,
        ];
        let n = (b[0] << 16) | (b[1] << 8) | b[2];
        out.push(BASE64URL[((n >> 18) & 63) as usize] as char);
        out.push(BASE64URL[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL[(n & 63) as usize] as char);
        }
    }
    out
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, ()> {
    fn value(byte: u8) -> Result<u32, ()> {
        match byte {
            b'A'..=b'Z' => Ok((byte - b'A') as u32),
            b'a'..=b'z' => Ok((byte - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((byte - b'0' + 52) as u32),
            b'-' => Ok(62),
            b'_' => Ok(63),
            _ => Err(()),
        }
    }
    let bytes = input.as_bytes();
    if bytes.len() % 4 == 1 {
        return Err(());
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0_u32;
        for (index, byte) in chunk.iter().enumerate() {
            n |= value(*byte)? << (18 - index * 6);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

impl InboxCursor {
    pub fn encode(&self) -> String {
        base64url_encode(
            serde_json::json!({"at": self.at, "id": self.id})
                .to_string()
                .as_bytes(),
        )
    }

    pub fn decode(value: &str) -> Result<Self, BusinessError> {
        if value.is_empty() || value.len() > 512 {
            return Err(cursor_invalid());
        }
        let bytes = base64url_decode(value).map_err(|_| cursor_invalid())?;
        let parsed: Value = serde_json::from_slice(&bytes).map_err(|_| cursor_invalid())?;
        let at = parsed["at"]
            .as_i64()
            .filter(|value| (0..=9_007_199_254_740_991).contains(value))
            .ok_or_else(cursor_invalid)?;
        let id = parsed["id"]
            .as_str()
            .filter(|value| !value.is_empty() && value.len() <= 512)
            .ok_or_else(cursor_invalid)?;
        Ok(Self { at, id: id.into() })
    }
}

fn cursor_invalid() -> BusinessError {
    BusinessError::invalid("INVALID_CURSOR", "Restart the Inbox list.")
}

/// Selected Work facts supplied by composition.
#[derive(Clone, Debug)]
pub struct InboxScope {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
    pub now: i64,
}

impl InboxScope {
    pub fn new(
        company_id: impl Into<String>,
        user_id: impl Into<String>,
        assignment_id: impl Into<String>,
        now: i64,
    ) -> Result<Self, BusinessError> {
        Ok(Self {
            user_id: UserId::new(user_id)?,
            company_id: CompanyId::new(company_id)?,
            assignment_id: AssignmentId::new(assignment_id)?,
            assignment_version: 0,
            now,
        })
    }

    pub fn assignment_version(mut self, version: i64) -> Self {
        self.assignment_version = version;
        self
    }
}

#[derive(Clone, Debug)]
pub struct InboxNotificationQuery {
    pub view: Option<String>,
    pub category: Option<String>,
    pub search: String,
    pub cursor: Option<String>,
    pub limit: usize,
}

pub struct PgInboxUnit<'c> {
    connection: &'c mut PgConnection,
    scope: InboxScope,
    targets: &'c mut dyn InboxTargetPort,
}

impl<'c> PgInboxUnit<'c> {
    /// The caller must already hold its Holding connection transaction and all
    /// owner read locks required by its route.
    pub fn new(
        connection: &'c mut PgConnection,
        scope: InboxScope,
        targets: &'c mut dyn InboxTargetPort,
    ) -> Self {
        Self {
            connection,
            scope,
            targets,
        }
    }

    async fn verify(&mut self) -> Result<(), Error> {
        use sqlx::Connection;
        if !self.connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "INBOX_TRANSACTION_REQUIRED",
                "The Inbox read requires its retained connection transaction.",
            )
            .into());
        }
        let row = sqlx::query(
            "SELECT a.version FROM assignments a \
             WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.revoked_at IS NULL \
             AND a.valid_from<=$4 AND a.valid_until>$4 \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)",
        )
        .bind(self.scope.assignment_id.as_str())
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .bind(self.scope.now)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MEMBERSHIP_REQUIRED",
                "Current Company membership is required.",
            )
            .into());
        };
        let version: i64 = row.try_get(0)?;
        if self.scope.assignment_version > 0 && version != self.scope.assignment_version {
            return Err(source_changed(
                "INBOX_SOURCE_CHANGED",
                "Your selected assignment changed. Refresh and retry.",
            ));
        }
        Ok(())
    }

    pub async fn summary(&mut self) -> Result<InboxSummaryData, Error> {
        self.verify().await?;
        let rows = sqlx::query(
            "SELECT e.category,count(*)::bigint FROM inbox_deliveries d \
             JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id \
             WHERE d.company_id=$1 AND d.user_id=$2 AND d.archived_at IS NULL AND d.read_at IS NULL \
             GROUP BY e.category",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        let mut categories = InboxCategoryCounts::default();
        for row in rows {
            let category: String = row.try_get(0)?;
            let count: i64 = row.try_get(1)?;
            count_category(&mut categories, &category, count);
        }
        let unread = categories.work
            + categories.calendar
            + categories.mention
            + categories.notice
            + categories.system;
        Ok(InboxSummaryData { unread, categories })
    }

    pub async fn notifications(
        &mut self,
        query: InboxNotificationQuery,
    ) -> Result<InboxPageData, Error> {
        self.verify().await?;
        let limit = query.limit.clamp(1, 100);
        let cursor = query
            .cursor
            .as_deref()
            .map(InboxCursor::decode)
            .transpose()?;
        let category = match query.category.as_deref() {
            None | Some("") => None,
            Some(value @ ("work" | "calendar" | "mention" | "notice" | "system")) => {
                Some(value.to_owned())
            }
            Some(_) => {
                return Err(BusinessError::invalid(
                    "INBOX_CATEGORY_INVALID",
                    "Choose a valid Inbox category.",
                )
                .into());
            }
        };
        let all = query.view.as_deref() == Some("all");
        let rows = sqlx::query(
            "SELECT d.id,d.event_id,d.delivered_at,d.read_at,d.archived_at,\
                    e.source_type,e.source_id,e.source_version,e.category,e.preview \
             FROM inbox_deliveries d \
             JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id \
             WHERE d.company_id=$1 AND d.user_id=$2 AND d.archived_at IS NULL \
             AND ($3 OR d.read_at IS NULL) \
             AND ($4::text IS NULL OR e.category=$4) \
             AND ($5='' OR e.preview ILIKE '%'||$5||'%') \
             AND ($6::bigint IS NULL OR (d.delivered_at,d.id)<($6,$7)) \
             ORDER BY d.delivered_at DESC,d.id DESC LIMIT $8",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .bind(all)
        .bind(category.as_deref())
        .bind(&query.search)
        .bind(cursor.as_ref().map(|cursor| cursor.at))
        .bind(cursor.as_ref().map(|cursor| cursor.id.as_str()))
        .bind((limit + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        let has_more = rows.len() > limit;
        let mut items = Vec::with_capacity(rows.len().min(limit));
        let mut next_cursor = None;
        for (index, row) in rows.into_iter().enumerate() {
            if index == limit {
                break;
            }
            let at: i64 = row.try_get(2)?;
            let id: String = row.try_get(0)?;
            next_cursor = Some(InboxCursor { at, id: id.clone() });
            if let Some(item) = self.item(row).await? {
                items.push(item);
            }
        }
        Ok(InboxPageData {
            items,
            next_cursor: has_more.then(|| next_cursor.expect("bounded rows").encode()),
            has_more,
        })
    }

    async fn item(&mut self, row: sqlx::postgres::PgRow) -> Result<Option<InboxItemData>, Error> {
        let id: String = row.try_get(0)?;
        let event_id: String = row.try_get(1)?;
        let delivered_at: i64 = row.try_get(2)?;
        let read_at: Option<i64> = row.try_get(3)?;
        let archived_at: Option<i64> = row.try_get(4)?;
        let source_type: String = row.try_get(5)?;
        let source_id: String = row.try_get(6)?;
        let source_version: i64 = row.try_get(7)?;
        let category: String = row.try_get(8)?;
        let preview: String = row.try_get(9)?;
        if source_type == "field_service_order" {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "NATIVE_FIELD_INBOX_REQUIRED",
                "This Field Inbox operation requires its native owner.",
            )
            .into());
        }
        let target = if source_type == "notice" {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM inbox_notices WHERE id=$1 AND company_id=$2)",
            )
            .bind(&source_id)
            .bind(self.scope.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
            if !exists {
                return Ok(None);
            }
            InboxTarget::default()
        } else {
            let Some(target) = self
                .targets
                .target(
                    &mut *self.connection,
                    self.scope.company_id.as_str(),
                    self.scope.user_id.as_str(),
                    &source_type,
                    &source_id,
                    source_version,
                )
                .await?
            else {
                return Ok(None);
            };
            target
        };
        Ok(Some(InboxItemData {
            id,
            event_id,
            category,
            message: preview,
            href: target.href,
            created_at: delivered_at,
            read_at,
            archived_at,
            source_type,
            source_id,
            source: target.source,
        }))
    }

    /// Active People rows in this Company.
    pub async fn people(&mut self, query: &str) -> Result<Vec<Value>, Error> {
        self.verify().await?;
        let rows = sqlx::query(
            "SELECT p.user_id,COALESCE(NULLIF(p.preferred_name,''),p.legal_name),p.email \
             FROM hr_people p \
             WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL \
             AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id \
               AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$2 \
               AND iam_assignment_active(a.id,$2) AND iam_principal_active(a.user_id,a.company_id,$2)) \
             AND ($3='' OR COALESCE(NULLIF(p.preferred_name,''),p.legal_name) ILIKE '%'||$3||'%' OR p.email ILIKE '%'||$3||'%') \
             ORDER BY COALESCE(NULLIF(p.preferred_name,''),p.legal_name),p.user_id LIMIT 100",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.now)
        .bind(query)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(serde_json::json!({
                    "id": row.try_get::<String, _>(0)?,
                    "name": row.try_get::<String, _>(1)?,
                    "email": row.try_get::<Option<String>, _>(2)?,
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Notices sent by the current user, matching the retained TypeScript read.
    pub async fn notices(&mut self) -> Result<Vec<Value>, Error> {
        self.verify().await?;
        let rows = sqlx::query(
            "SELECT n.id,n.title,n.recipient_count::bigint,n.created_at,\
               (SELECT count(*)::bigint FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id \
                WHERE e.company_id=n.company_id AND e.event_key='notice:'||n.id) delivered_count \
             FROM inbox_notices n WHERE n.company_id=$1 AND n.sender_user_id=$2 \
             ORDER BY n.created_at DESC,n.id DESC LIMIT 50",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(serde_json::json!({
                    "id": row.try_get::<String, _>(0)?,
                    "title": row.try_get::<String, _>(1)?,
                    "accepted_count": row.try_get::<i64, _>(2)?,
                    "created_at": row.try_get::<i64, _>(3)?,
                    "delivered_count": row.try_get::<i64, _>(4)?,
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn notice(&mut self, id: &str) -> Result<Option<Value>, Error> {
        self.verify().await?;
        let row = sqlx::query(
            "SELECT id,title,body,recipient_count::bigint,created_at FROM inbox_notices \
             WHERE id=$1 AND company_id=$2 AND sender_user_id=$3",
        )
        .bind(id)
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let completed: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id \
             WHERE e.company_id=$1 AND e.event_key=$2",
        )
        .bind(self.scope.company_id.as_str())
        .bind(format!("notice:{id}"))
        .fetch_one(&mut *self.connection)
        .await?;
        let recipient_count: i64 = row.try_get(3)?;
        Ok(Some(serde_json::json!({
            "id": row.try_get::<String, _>(0)?,
            "title": row.try_get::<String, _>(1)?,
            "body": row.try_get::<String, _>(2)?,
            "recipient_count": recipient_count,
            "created_at": row.try_get::<i64, _>(4)?,
            "acceptedCount": recipient_count,
            "completedCount": completed,
        })))
    }
}

fn count_category(counts: &mut InboxCategoryCounts, category: &str, count: i64) {
    match category {
        "work" => counts.work += count,
        "calendar" => counts.calendar += count,
        "mention" => counts.mention += count,
        "notice" => counts.notice += count,
        "system" => counts.system += count,
        _ => counts.system += count,
    }
}

/// Convenience for composition: a read-only unit with no cross-owner targets.
pub fn unavailable_targets() -> UnavailableInboxTargets {
    UnavailableInboxTargets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_foreign_data() {
        let cursor = InboxCursor {
            at: 1_700_000_000_000,
            id: "delivery-1".into(),
        };
        assert_eq!(InboxCursor::decode(&cursor.encode()).unwrap(), cursor);
        assert_eq!(
            InboxCursor::decode("not-base64").unwrap_err().code,
            "INVALID_CURSOR"
        );
        assert_eq!(InboxCursor::decode("").unwrap_err().code, "INVALID_CURSOR");
    }

    #[test]
    fn category_counts_fold_unknown_values_into_system() {
        let mut counts = InboxCategoryCounts::default();
        count_category(&mut counts, "mention", 2);
        count_category(&mut counts, "work", 1);
        count_category(&mut counts, "future_category", 3);
        assert_eq!(counts.mention, 2);
        assert_eq!(counts.work, 1);
        assert_eq!(counts.system, 3);
    }

    #[test]
    fn scope_requires_current_identifiers() {
        assert!(InboxScope::new("company", "user", "assignment", 1).is_ok());
        assert!(InboxScope::new("company", "", "assignment", 1).is_err());
    }
}
