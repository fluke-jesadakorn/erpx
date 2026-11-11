//! Read-only owner source facts for Maintenance plans and work orders, fenced
//! to the caller's serializable unit.
//!
//! This module reads only `maintenance_preventive_plans` and
//! `maintenance_work_orders`. It never writes, opens an independent
//! transaction, or fabricates a row for a missing or foreign company source.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The owning source changed.",
    )
    .into()
}

fn unsupported() -> Error {
    BusinessError::new(
        FailureKind::InvalidInput,
        "SOURCE_RECORD_INVALID",
        "This owner cannot read that source kind.",
    )
    .into()
}

fn sql(kind: SourceKind) -> Result<&'static str, Error> {
    match kind {
        SourceKind::MaintenancePlan => Ok(PLAN_SQL),
        SourceKind::MaintenanceWork => Ok(WORK_SQL),
        _ => Err(unsupported()),
    }
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("RECORD_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    kind: SourceKind,
    id: &str,
) -> Result<Option<(SourceRecord, String)>, Error> {
    let row = sqlx::query(sql(kind)?)
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(c)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let raw: Option<String> = row.try_get("payload")?;
    let source = serde_json::from_str(&raw.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "RECORD_SOURCE_LIMIT",
            "The source exceeds its bounded read.",
        )
    })?)
    .map_err(|_| changed())?;
    Ok(Some((source, row.try_get("digest")?)))
}

pub struct CurrentRecordSource {
    company: CompanyId,
    kind: SourceKind,
    id: String,
    unit: Unit,
    image: (SourceRecord, String),
}

impl CurrentRecordSource {
    pub fn data(&self) -> &SourceRecord {
        &self.image.0
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, self.kind, &self.id)
                .await?
                .as_ref()
                != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}

pub async fn read_record_source(
    c: &mut PgConnection,
    company: &CompanyId,
    kind: SourceKind,
    id: &str,
) -> Result<Option<CurrentRecordSource>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, kind, id)
        .await?
        .map(|image| CurrentRecordSource {
            company: company.clone(),
            kind,
            id: id.into(),
            unit,
            image,
        }))
}

pub async fn lock_record_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RECORD_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE maintenance_preventive_plans,maintenance_work_orders IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

const PLAN_SQL: &str = "WITH s AS MATERIALIZED(SELECT * FROM maintenance_preventive_plans WHERE id=$1 AND company_id=$2 FOR SHARE),payload AS(SELECT jsonb_build_object('kind','maintenance_plan','id',s.id,'company_id',s.company_id,'branch_id',s.branch_id,'department_id',s.department_id,'version',s.version,'title',s.title,'status',s.status)::text value,to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(value)<=2097152 AND octet_length(image)<=2097152 THEN value END payload,encode(sha256(convert_to(image,'UTF8')),'hex') digest FROM payload";

const WORK_SQL: &str = "WITH s AS MATERIALIZED(SELECT * FROM maintenance_work_orders WHERE id=$1 AND company_id=$2 FOR SHARE),payload AS(SELECT jsonb_build_object('kind','maintenance_work','id',s.id,'company_id',s.company_id,'branch_id',s.branch_id,'department_id',s.department_id,'version',s.version,'title',s.title,'status',s.status)::text value,to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(value)<=2097152 AND octet_length(image)<=2097152 THEN value END payload,encode(sha256(convert_to(image,'UTF8')),'hex') digest FROM payload";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn supported_kinds_read_only_their_owner_tables_with_the_portable_shape() {
        let plan = sql(SourceKind::MaintenancePlan).unwrap();
        assert!(plan.contains("FROM maintenance_preventive_plans"));
        assert!(plan.contains("'maintenance_plan'"));
        for key in [
            "'id'",
            "'company_id'",
            "'branch_id'",
            "'department_id'",
            "'version'",
            "'title'",
            "'status'",
        ] {
            assert!(plan.contains(key), "missing payload key {key}");
        }
        assert!(plan.contains("octet_length(value)<=2097152"));
        assert!(plan.contains("octet_length(image)<=2097152"));
        assert!(plan.contains("sha256"));

        let work = sql(SourceKind::MaintenanceWork).unwrap();
        assert!(work.contains("FROM maintenance_work_orders"));
        assert!(work.contains("'maintenance_work'"));
        assert!(work.contains("'status',s.status"));
    }

    #[test]
    fn non_maintenance_kinds_are_rejected() {
        for kind in [
            SourceKind::CommerceOrder,
            SourceKind::SupportTicket,
            SourceKind::FieldServiceOrder,
            SourceKind::StaffTask,
            SourceKind::PosSale,
            SourceKind::PmsRecord,
            SourceKind::Asset,
        ] {
            assert!(matches!(sql(kind), Err(PgTransactionError::Business(_))));
        }
    }

    #[test]
    fn captured_payload_deserializes_to_the_portable_record() {
        for (kind, wire_kind) in [
            (SourceKind::MaintenancePlan, "maintenance_plan"),
            (SourceKind::MaintenanceWork, "maintenance_work"),
        ] {
            let payload = json!({
                "kind": wire_kind,
                "id": "source-1",
                "company_id": "company",
                "branch_id": "branch",
                "department_id": null,
                "version": 3,
                "title": "Synthetic source",
                "status": "active",
            });
            let source: SourceRecord = serde_json::from_value(payload).unwrap();
            assert_eq!(source.kind, kind);
            assert_eq!(source.version, 3);
        }
    }
}
