//! Legal-owned retained destruction source and queued identity input.
//!
//! This port does not authorize deletion. The caller must revalidate the
//! appropriate IAM binding, Legal visibility, independent review, hold and
//! Commercial purchase rights before any journal, file or source mutation.
use erpx_server_identity_access::public::effective_authority::QueuedWorkBinding;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_RETENTION_SOURCE_CHANGED",
        "Review the exact retained destruction source through its existing retry workflow.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

/// The two historical formats are distinct; neither is current authority.
/// Legacy snapshots must go through the existing WorkContext revalidator; they
/// must never be promoted into native bindings by guessing missing versions.
pub enum LegalQueuedIdentity {
    LegacyWorkContext(Value),
    Native(QueuedWorkBinding),
}

fn parse_identity(
    raw: &str,
    actor: &str,
    assignment: &str,
    company: &str,
) -> Result<LegalQueuedIdentity, Error> {
    if raw.len() > 65_536 {
        return Err(changed());
    }
    let value: Value = serde_json::from_str(raw).map_err(|_| changed())?;
    let object = value.as_object().ok_or_else(changed)?;
    if object.get("userId").and_then(Value::as_str) != Some(actor)
        || object.get("assignmentId").and_then(Value::as_str) != Some(assignment)
        || object.get("companyId").and_then(Value::as_str) != Some(company)
        || actor.is_empty()
        || assignment.is_empty()
        || company.is_empty()
    {
        return Err(changed());
    }
    if object.contains_key("version") {
        let binding = QueuedWorkBinding::parse(raw).map_err(|_| changed())?;
        if binding.version != 1 {
            return Err(changed());
        }
        Ok(LegalQueuedIdentity::Native(binding))
    } else {
        // Preserve the full original snapshot. Legacy IAM must independently
        // validate its shape and authority instead of trusting this parser.
        if object
            .get("holdingId")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
            || object
                .get("role")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            || !object.contains_key("scope")
        {
            return Err(changed());
        }
        Ok(LegalQueuedIdentity::LegacyWorkContext(value))
    }
}

#[derive(Clone, Eq, PartialEq)]
struct Image {
    item: String,
    company: String,
    item_version: i64,
    requester: String,
    approver: String,
    command_id: String,
    actor: String,
    assignment: String,
    authority_json: String,
    source_hash: String,
}

// This JSONB text is deliberately byte-identical to the image hashed by the
// existing Legal retention-execution.ts producer; do not reorder its contents
// or silently reserialize outside PostgreSQL.
async fn capture(
    c: &mut PgConnection,
    company: &str,
    destruction: &str,
) -> Result<Option<Image>, Error> {
    let source = sqlx::query("SELECT x.item_id,i.company_id,i.version::bigint item_version,x.requested_by,x.approved_by,encode(sha256(convert_to(jsonb_build_object('request',x.id,'item',i.id,'company',i.company_id,'branch',i.branch_id,'department',i.department_id,'kind',i.kind,'classification',i.classification,'state',i.state,'version',i.version,'requester',x.requested_by,'approver',x.approved_by,'manifest',x.manifest,'policyId',x.policy_id,'policyVersion',x.policy_version,'policy',to_jsonb(p))::text,'UTF8')),'hex') current_hash FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id JOIN legal_retention_policies p ON p.id=x.policy_id AND p.company_id=i.company_id WHERE x.id=$1 AND i.company_id=$2 FOR SHARE OF x,i,p")
        .bind(destruction).bind(company).fetch_optional(&mut *c).await?;
    let Some(source) = source else {
        return Ok(None);
    };
    let authorization = sqlx::query("SELECT command_id,item_id,company_id,actor_id,assignment_id,authority_json,source_hash,item_version::bigint FROM legal_retention_execution_authorizations WHERE destruction_id=$1 ORDER BY item_version DESC,created_at DESC,command_id DESC LIMIT 1 FOR SHARE")
        .bind(destruction).fetch_optional(&mut *c).await?.ok_or_else(changed)?;
    let item: String = source.try_get("item_id")?;
    let source_company: String = source.try_get("company_id")?;
    let item_version: i64 = source.try_get("item_version")?;
    let source_hash: String = source.try_get("current_hash")?;
    let approver: Option<String> = source.try_get("approved_by")?;
    if authorization.try_get::<String, _>("item_id")? != item
        || authorization.try_get::<String, _>("company_id")? != source_company
        || authorization.try_get::<i64, _>("item_version")? != item_version
        || authorization.try_get::<String, _>("source_hash")? != source_hash
        || approver.as_deref().is_none_or(str::is_empty)
    {
        return Err(changed());
    }
    let actor: String = authorization.try_get("actor_id")?;
    let assignment: String = authorization.try_get("assignment_id")?;
    let authority_json: String = authorization.try_get("authority_json")?;
    parse_identity(&authority_json, &actor, &assignment, &source_company)?;
    Ok(Some(Image {
        item,
        company: source_company,
        item_version,
        requester: source.try_get("requested_by")?,
        approver: approver.unwrap_or_default(),
        command_id: authorization.try_get("command_id")?,
        actor,
        assignment,
        authority_json,
        source_hash,
    }))
}

/// Opaque immutable source/queue witness, valid only in its originating
/// serializable transaction; it is not an IAM or purchase grant.
pub struct CurrentLegalQueuedAuthority {
    destruction: String,
    unit: Unit,
    image: Image,
}
impl CurrentLegalQueuedAuthority {
    pub fn item_id(&self) -> &str {
        &self.image.item
    }
    pub fn actor_id(&self) -> &str {
        &self.image.actor
    }
    pub fn assignment_id(&self) -> &str {
        &self.image.assignment
    }
    pub fn requested_by(&self) -> &str {
        &self.image.requester
    }
    pub fn approved_by(&self) -> &str {
        &self.image.approver
    }
    pub fn identity(&self) -> Result<LegalQueuedIdentity, Error> {
        parse_identity(
            &self.image.authority_json,
            &self.image.actor,
            &self.image.assignment,
            &self.image.company,
        )
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.image.company, &self.destruction)
                .await?
                .as_ref()
                != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Caller first acquires the documented Legal/Identity/Files locks and supplies
/// current Holding/Company context. Missing or stale rows fail closed.
pub async fn read_legal_queued_authority(
    c: &mut PgConnection,
    company: &str,
    destruction: &str,
) -> Result<CurrentLegalQueuedAuthority, Error> {
    if company.is_empty() || destruction.is_empty() {
        return Err(changed());
    }
    let unit = unit(c).await?;
    let image = capture(c, company, destruction)
        .await?
        .ok_or_else(changed)?;
    Ok(CurrentLegalQueuedAuthority {
        destruction: destruction.into(),
        unit,
        image,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_identity_is_not_promoted_or_used_as_authority() {
        let raw = json!({"holdingId":"h","companyId":"c","userId":"u","assignmentId":"a","role":"manager","scope":{"kind":"holding"},"iam":{"permissions":["legal.destroy"]}}).to_string();
        assert!(matches!(
            parse_identity(&raw, "u", "a", "c"),
            Ok(LegalQueuedIdentity::LegacyWorkContext(_))
        ));
        assert!(parse_identity(&raw, "other", "a", "c").is_err());
        assert!(parse_identity(&raw, "u", "other", "c").is_err());
        assert!(parse_identity(&raw, "u", "a", "other").is_err());
    }
    #[test]
    fn malformed_or_unknown_native_version_never_downgrades_to_legacy() {
        let mut raw = json!({"version":2,"holdingId":"h","companyId":"c","userId":"u","assignmentId":"a","role":"manager","scope":{}});
        assert!(parse_identity(&raw.to_string(), "u", "a", "c").is_err());
        raw["version"] = json!(1);
        assert!(parse_identity(&raw.to_string(), "u", "a", "c").is_err());
    }

    #[test]
    fn native_binding_preserves_all_versioned_fields_without_legacy_conversion() {
        let raw = json!({"version":1,"holdingId":"h","companyId":"c","userId":"u",
            "branchId":null,"departmentId":null,"branchSlug":null,"departmentSlug":null,
            "assignmentId":"a","assignmentVersion":7,"role":"manager","sessionId":"s",
            "selectionRevision":9,"identityVersion":2,"roleRevision":3,
            "hrAppointmentVersion":null,"emergencyUntil":null,"expiresAt":1700000000000_i64});
        let parsed = parse_identity(&raw.to_string(), "u", "a", "c").unwrap();
        let LegalQueuedIdentity::Native(binding) = parsed else {
            panic!("native binding was downgraded")
        };
        assert_eq!(binding.assignment_version, 7);
        assert_eq!(binding.selection_revision, 9);
        assert_eq!(binding.expires_at, 1700000000000_i64);
    }
}
