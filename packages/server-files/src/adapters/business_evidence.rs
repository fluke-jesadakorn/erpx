//! Files-owned implementation of the shared Records business-evidence port.
//!
//! The caller supplies the exact Company/branch/department scope and the
//! registered owner policy facts. Files validates current set targets, links,
//! scanned attachment sources and context, then binds the exact draft links and
//! sets inside the caller's serializable owner unit. It never commits.
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceBinding, BusinessEvidenceFilesPort, EvidenceContext, EvidencePolicyFact,
    EvidenceSetFact,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_BINDING_CHANGED",
        "The original file set changed. Reload the files and try again.",
    )
    .into()
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::InvalidInput, code, message).into()
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "File set not found.",
    )
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedBusinessEvidence {
    pub set_ids: Vec<String>,
    pub link_ids: Vec<String>,
    pub target_id: String,
    pub expected_target_version: i64,
    pub binding_type: String,
    pub binding_id: String,
    pub command_id: String,
}

pub struct PgBusinessEvidenceFiles<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    actor_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    policies: Vec<EvidencePolicyFact>,
    now: TimestampMillis,
}

impl<'c> PgBusinessEvidenceFiles<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        company: CompanyId,
        actor_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        policies: Vec<EvidencePolicyFact>,
        now: TimestampMillis,
    ) -> Self {
        Self {
            connection,
            company,
            actor_id,
            branch_id,
            department_id,
            policies,
            now,
        }
    }

    async fn unit(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
        }
        let level: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if level != "serializable" {
            return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}

impl BusinessEvidenceFilesPort for PgBusinessEvidenceFiles<'_> {
    type Error = Error;
    type Prepared = PreparedBusinessEvidence;

    async fn read_set_targets(
        &mut self,
        ids: &[String],
    ) -> Result<Vec<EvidenceSetFact>, Self::Error> {
        self.unit().await?;
        let rows = sqlx::query(
            "SELECT id,target_type FROM attachment_sets \
             WHERE id = ANY($1) AND company_id=$2 AND state='draft' \
             AND (branch_id IS NULL OR branch_id=$3) \
             AND (department_id IS NULL OR department_id=$4)",
        )
        .bind(ids)
        .bind(self.company.as_str())
        .bind(self.branch_id.as_deref())
        .bind(self.department_id.as_deref())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok::<_, Error>(EvidenceSetFact {
                    id: row.try_get("id")?,
                    target_type: row.try_get("target_type")?,
                })
            })
            .collect()
    }

    fn policy_facts(&self) -> &[EvidencePolicyFact] {
        &self.policies
    }

    async fn prepare_binding(
        &mut self,
        input: BusinessEvidenceBinding,
    ) -> Result<Self::Prepared, Self::Error> {
        self.unit().await?;
        if input.target_id.is_empty()
            || input.expected_target_version < 1
            || input.command_id.is_empty()
            || input.binding_type.is_empty()
            || input.binding_id.is_empty()
            || input.selection.set_ids.is_empty()
        {
            return Err(invalid(
                "ATTACHMENT_BINDING_CONTEXT",
                "Choose current files for this exact owner action.",
            ));
        }
        let rows = sqlx::query(
            "SELECT id,target_type,target_id,expected_target_version,component_kind,\
             component_ref,purpose_key,required_count,max_files \
             FROM attachment_sets WHERE id = ANY($1) AND company_id=$2 AND state='draft' \
             AND (branch_id IS NULL OR branch_id=$3) \
             AND (department_id IS NULL OR department_id=$4) FOR UPDATE",
        )
        .bind(&input.selection.set_ids)
        .bind(self.company.as_str())
        .bind(self.branch_id.as_deref())
        .bind(self.department_id.as_deref())
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() != input.selection.set_ids.len() {
            return Err(missing());
        }
        let mut link_ids = Vec::new();
        for row in &rows {
            let id: String = row.try_get("id")?;
            let target_type: String = row.try_get("target_type")?;
            let target_id: String = row.try_get("target_id")?;
            let version: i64 = row.try_get("expected_target_version")?;
            let component_kind: String = row.try_get("component_kind")?;
            let component_ref: Option<String> = row.try_get("component_ref")?;
            let purpose: String = row.try_get("purpose_key")?;
            let required: i64 = row.try_get("required_count")?;
            let maximum: i64 = row.try_get("max_files")?;
            if target_type != input.selection.target_type
                || target_id != input.target_id
                || version != input.expected_target_version
                || !input.selection.contexts.iter().any(|context| {
                    context.purpose_key == purpose
                        && context.component_kind == component_kind
                        && (context.component_ref.is_none()
                            || context.component_ref == component_ref)
                })
            {
                return Err(changed());
            }
            let links = sqlx::query(
                "SELECT l.id,l.state,l.attachment_id,a.source_state \
                 FROM attachment_links l \
                 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=$2 \
                 WHERE l.set_id=$1 ORDER BY l.created_at,l.id",
            )
            .bind(&id)
            .bind(self.company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if (links.len() as i64) > maximum
                || (links.len() as i64) < required
                || links.iter().any(|link| {
                    link.try_get::<String, _>("state")
                        .map(|state| state != "draft")
                        .unwrap_or(true)
                        || link
                            .try_get::<Option<String>, _>("attachment_id")
                            .ok()
                            .flatten()
                            .is_none()
                        || link
                            .try_get::<Option<String>, _>("source_state")
                            .ok()
                            .flatten()
                            .as_deref()
                            != Some("available")
                })
            {
                return Err(changed());
            }
            for link in &links {
                let link_id: String = link.try_get("id")?;
                let updated = sqlx::query(
                    "UPDATE attachment_links SET state='bound',bound_at=$1,\
                     binding_type=$2,binding_id=$3 \
                     WHERE id=$4 AND set_id=$5 AND state='draft'",
                )
                .bind(self.now.get())
                .bind(&input.binding_type)
                .bind(&input.binding_id)
                .bind(&link_id)
                .bind(&id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if updated != 1 {
                    return Err(changed());
                }
                link_ids.push(link_id);
            }
            let updated = sqlx::query(
                "UPDATE attachment_sets SET state='bound',bound_at=$1,\
                 bound_command_id=$2,updated_at=$1 \
                 WHERE id=$3 AND company_id=$4 AND state='draft'",
            )
            .bind(self.now.get())
            .bind(&input.command_id)
            .bind(&id)
            .bind(self.company.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            let event = format!("attachment-bind-{}-{}", input.command_id, id);
            sqlx::query(
                "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,\
                 details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
            )
            .bind(event)
            .bind(self.company.as_str())
            .bind(&id)
            .bind(&self.actor_id)
            .bind(
                serde_json::json!({
                    "bindingType": input.binding_type,
                    "bindingId": input.binding_id,
                    "targetId": input.target_id,
                    "expectedTargetVersion": input.expected_target_version,
                })
                .to_string(),
            )
            .bind(self.now.get())
            .execute(&mut *self.connection)
            .await?;
        }
        // Re-read the exact bound state within the same unit.
        let bound: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_sets WHERE id=ANY($1) AND company_id=$2 \
             AND state='bound' AND bound_command_id=$3 AND bound_at=$4",
        )
        .bind(&input.selection.set_ids)
        .bind(self.company.as_str())
        .bind(&input.command_id)
        .bind(self.now.get())
        .fetch_one(&mut *self.connection)
        .await?;
        if bound != input.selection.set_ids.len() as i64 {
            return Err(changed());
        }
        Ok(PreparedBusinessEvidence {
            set_ids: input.selection.set_ids,
            link_ids,
            target_id: input.target_id,
            expected_target_version: input.expected_target_version,
            binding_type: input.binding_type,
            binding_id: input.binding_id,
            command_id: input.command_id,
        })
    }
}

/// Owner policy facts are a caller-owned static registry slice. Files does not
/// grant a context that the owner has not registered.
pub fn policy_fact(
    target_type: &str,
    purpose_key: &str,
    component_kinds: &[&str],
) -> EvidencePolicyFact {
    EvidencePolicyFact {
        target_type: target_type.to_string(),
        purpose_key: purpose_key.to_string(),
        component_kinds: component_kinds
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

pub fn evidence_context(
    purpose_key: &str,
    component_kind: &str,
    component_ref: Option<&str>,
) -> EvidenceContext {
    EvidenceContext {
        purpose_key: purpose_key.to_string(),
        component_kind: component_kind.to_string(),
        component_ref: component_ref.map(str::to_owned),
    }
}
