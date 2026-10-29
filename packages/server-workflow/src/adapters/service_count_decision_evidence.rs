//! Workflow-owned original Stock Count review decision evidence.
use crate::{application::commands::CommandActor, domain::command_json::CommandJson};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_COUNT_DECISION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The Count decision evidence changed.",
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
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_service_count_decision_evidence(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE decision_evidence IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// The arrays are the ordered pre-action Inventory projection, after retaining
/// the corresponding Commerce master witness. Canonicalization is ECMAScript
/// JSON.stringify, including number and Unicode semantics.
pub fn service_count_decision_digest(
    reason: &str,
    lines: &str,
    lot_lines: &str,
) -> Result<String, BusinessError> {
    let reason = CommandJson::from_serializable(&reason)?;
    let body = CommandJson::parse(&format!(
        "{{\"reason\":{},\"lines\":{},\"lotLines\":{}}}",
        reason.as_str(),
        lines,
        lot_lines
    ))?;
    Ok(hex::encode(Sha256::digest(body.as_str().as_bytes())))
}
#[derive(Clone, Debug)]
pub struct ServiceCountDecisionFacts {
    pub id: String,
    pub command_id: String,
    pub record_id: String,
    pub action: String,
    pub reviewed_version: i64,
    pub input_digest: String,
    pub created_at: i64,
}
pub struct PlannedServiceCountDecision {
    unit: Unit,
    facts: ServiceCountDecisionFacts,
    actor: String,
    assignment: String,
    authority: String,
}
impl PlannedServiceCountDecision {
    pub async fn prepare(
        c: &mut PgConnection,
        actor: &CommandActor,
        facts: ServiceCountDecisionFacts,
    ) -> Result<Self, Error> {
        if facts.id.is_empty()
            || facts.command_id.is_empty()
            || facts.record_id.is_empty()
            || !matches!(
                facts.action.as_str(),
                "inventory.count.approve" | "inventory.count.post" | "inventory.count.reject"
            )
            || facts.reviewed_version <= 0
            || facts.input_digest.len() != 64
            || !facts
                .input_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let unit = unit(c).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM decision_evidence WHERE id=$1 OR command_id=$2)",
        )
        .bind(&facts.id)
        .bind(&facts.command_id)
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(changed());
        }
        Ok(Self {
            unit,
            facts,
            actor: actor.user_id.as_str().into(),
            assignment: actor.assignment_id.as_str().into(),
            authority: actor.receipt_authority()?.as_str().into(),
        })
    }
    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let f = &self.facts;
        let receipt:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND record_id=$2 AND operation=$3 AND actor_id=$4 AND authority_json=$5)").bind(&f.command_id).bind(&f.record_id).bind(&f.action).bind(&self.actor).bind(&self.authority).fetch_one(&mut *c).await?;
        if !receipt {
            return Err(changed());
        }
        sqlx::query("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(&f.id).bind(&f.command_id).bind(&f.record_id).bind(&f.action).bind(&self.actor).bind(&self.assignment).bind(f.reviewed_version).bind(&f.input_digest).bind(f.created_at).execute(c).await?;
        Ok(())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let f = &self.facts;
        let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM decision_evidence WHERE id=$1 AND command_id=$2 AND record_id=$3 AND action=$4 AND actor_id=$5 AND assignment_id=$6 AND reviewed_version=$7 AND input_digest=$8 AND created_at=$9 AND authority_json=$10)").bind(&f.id).bind(&f.command_id).bind(&f.record_id).bind(&f.action).bind(&self.actor).bind(&self.assignment).bind(f.reviewed_version).bind(&f.input_digest).bind(f.created_at).bind(&self.authority).fetch_one(c).await?;
        if !exact {
            return Err(changed());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_matches_pinned_bun_json_stringify_and_sha256() {
        assert_eq!(
            service_count_decision_digest(
                "Review ✓",
                r#"[{"id":"l","sku":"ABC","current_quantity_milli":null,"current_version":0}]"#,
                r#"[{"id":"lot","counted_quantity_milli":1234}]"#
            )
            .unwrap(),
            "78feaed6bbacb41de8f25a1cfdd16125756b41cac18cde8b9651c36ad093b548"
        );
        assert_eq!(service_count_decision_digest("Review", r#"[{"id":"detail-line","count_id":"detail-count","product_id":"product","product_version":1,"description":"Product","unit":"piece","snapshot_quantity_milli":0,"snapshot_value_minor":0,"snapshot_version":0,"counted_quantity_milli":1000,"counted_value_minor":500,"unit_cost_minor":500,"ordinal":0,"sku":"SKU","tracking_mode":"none","current_quantity_milli":5000,"current_value_minor":1000,"current_version":1}]"#, "[]").unwrap(),"ea2c0274ccc48034cd17ea517edc617d2f7434a9f8be26a0f8144cfd155f5c0e");
    }
}
