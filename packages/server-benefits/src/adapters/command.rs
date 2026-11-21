//! PostgreSQL write adapter for native Benefits owner commands.
//!
//! The caller owns one serializable Holding transaction and current Account/IAM
//! authority. This adapter writes only Benefits rows, immutable Benefits
//! events, the standard command receipt/audit/outbox ledger and the Benefits
//! payroll-input handoff row. It never reads another owner's business tables
//! and never opens its own transaction.
use crate::application::command::{BenefitsCommandFuture, BenefitsCommandRepository};
use crate::domain::command::{
    BenefitCommandResult, BenefitRecordKind, BenefitRecordState, ChangeBenefit, CreateDependent,
    CreateElection, CreatePlan, CreateWindow, StoredBenefitReceipt, idempotency_conflict,
};
use crate::domain::workspace::{BenefitPlanRow, BenefitWindowRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn readback_invalid() -> BusinessError {
    unavailable(
        "BENEFITS_READBACK_INVALID",
        "The Benefits command did not persist its expected owner row.",
    )
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        conflict(
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgBenefitsCommand<'a> {
    connection: &'a mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'a> PgBenefitsCommand<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        actor: &crate::domain::command::BenefitsCommandActor,
    ) -> Self {
        Self {
            connection,
            company: actor.company_id.clone(),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("BENEFITS_COMMAND_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("BENEFITS_COMMAND_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_receipt(
        &mut self,
        key: &str,
        operation: &str,
        request_hash: &str,
        record_id: &str,
        result: &BenefitCommandResult,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(&self.user_id)
        .bind(request_hash)
        .bind(record_id)
        .bind(result.to_json())
        .bind(now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    async fn insert_audit(
        &mut self,
        kind: BenefitRecordKind,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(format!("benefit-{}-audit-{key}", kind.as_str()))
        .bind(key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(operation)
        .bind(kind.record_type())
        .bind(record_id)
        .bind(version)
        .bind(operation)
        .bind(now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_outbox(
        &mut self,
        kind: BenefitRecordKind,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        status: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = json!({
            "authority": authority,
            "id": record_id,
            "state": status,
            "companyId": self.company.as_str(),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("benefit-{}-outbox-{key}", kind.as_str()))
        .bind(key)
        .bind(operation)
        .bind(kind.record_type())
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn readback(
        &mut self,
        kind: BenefitRecordKind,
        id: &str,
        expected: &BenefitCommandResult,
    ) -> Result<BenefitCommandResult, Error> {
        let sql = format!(
            "SELECT id,status,version FROM {} WHERE id=$1 AND company_id=$2",
            kind.table()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(self.company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
        let result = BenefitCommandResult::new(
            row.try_get::<String, _>("id")?,
            row.try_get::<String, _>("status")?,
            row.try_get::<i64, _>("version")?,
        );
        result.validate()?;
        if result != *expected {
            return Err(readback_invalid().into());
        }
        Ok(result)
    }

    async fn commit(
        &mut self,
        kind: BenefitRecordKind,
        key: &str,
        operation: &str,
        request_hash: &str,
        result: BenefitCommandResult,
        now: i64,
    ) -> Result<BenefitCommandResult, Error> {
        self.insert_receipt(key, operation, request_hash, &result.id, &result, now)
            .await?;
        self.insert_audit(kind, key, operation, &result.id, result.version, now)
            .await?;
        self.insert_outbox(
            kind,
            key,
            operation,
            &result.id,
            result.version,
            &result.status,
            now,
        )
        .await?;
        self.readback(kind, &result.id, &result).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_event(
        &mut self,
        kind: BenefitRecordKind,
        key: &str,
        record_id: &str,
        version: i64,
        action: &str,
        reason: &str,
        status: &str,
        now: i64,
    ) -> Result<(), Error> {
        let sql = format!(
            "INSERT INTO {}(id,{},version,action,actor_id,reason,status,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            kind.event_table(),
            kind.id_column()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(format!("benefit-{}-event-{key}", kind.as_str()))
            .bind(record_id)
            .bind(version)
            .bind(action)
            .bind(&self.user_id)
            .bind(reason)
            .bind(status)
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
        Ok(())
    }
}

fn map_plan(row: &sqlx::postgres::PgRow) -> Result<BenefitPlanRow, sqlx::Error> {
    Ok(BenefitPlanRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        name: row.try_get("name")?,
        provider: row.try_get("provider")?,
        coverage_type: row.try_get("coverage_type")?,
        currency: row.try_get("currency")?,
        employee_deduction_minor: row.try_get("employee_deduction_minor")?,
        employer_cost_minor: row.try_get("employer_cost_minor")?,
        effective_from: row.try_get("effective_from")?,
        effective_to: row.try_get("effective_to")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        required_eligibility_code: row.try_get("required_eligibility_code")?,
    })
}

fn map_window(row: &sqlx::postgres::PgRow) -> Result<BenefitWindowRow, sqlx::Error> {
    Ok(BenefitWindowRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        name: row.try_get("name")?,
        opens_on: row.try_get("opens_on")?,
        closes_on: row.try_get("closes_on")?,
        effective_on: row.try_get("effective_on")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl BenefitsCommandRepository for PgBenefitsCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> BenefitsCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("benefits-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> BenefitsCommandFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<StoredBenefitReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                None => None,
                Some(value) => {
                    let result: BenefitCommandResult =
                        serde_json::from_str(&value).map_err(|_| {
                            unavailable(
                                "BENEFITS_RECEIPT_INVALID",
                                "The Benefits command receipt is invalid.",
                            )
                        })?;
                    result.validate()?;
                    Some(result)
                }
            };
            Ok(Some(StoredBenefitReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn record_state<'a>(
        &'a mut self,
        kind: BenefitRecordKind,
        id: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<BenefitRecordState>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = match kind {
                BenefitRecordKind::Plan | BenefitRecordKind::Window => format!(
                    "SELECT id,status,version,created_by,NULL::text acknowledged_by,NULL::text person_id,NULL::text effective_from \
                     FROM {} WHERE id=$1 AND company_id=$2",
                    kind.table()
                ),
                BenefitRecordKind::Dependent => format!(
                    "SELECT id,status,version,created_by,NULL::text acknowledged_by,person_id,NULL::text effective_from \
                     FROM {} WHERE id=$1 AND company_id=$2",
                    kind.table()
                ),
                BenefitRecordKind::Election => format!(
                    "SELECT id,status,version,created_by,acknowledged_by,person_id,effective_from \
                     FROM {} WHERE id=$1 AND company_id=$2",
                    kind.table()
                ),
            };
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(self.company.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(BenefitRecordState {
                    id: row.try_get("id")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    acknowledged_by: row.try_get("acknowledged_by")?,
                    person_id: row.try_get("person_id")?,
                    effective_from: row.try_get("effective_from")?,
                })
            })
            .transpose()
        })
    }

    fn plan_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitPlanRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,name,provider,coverage_type,currency,\
                 employee_deduction_minor,employer_cost_minor,effective_from,effective_to,\
                 status,version,created_by,reviewed_by,created_at,updated_at,\
                 required_eligibility_code FROM benefit_plans \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='published'",
            )
            .bind(id)
            .bind(self.company.as_str())
            .bind(version)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(map_plan).transpose().map_err(Into::into)
        })
    }

    fn window_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitWindowRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,name,opens_on,closes_on,effective_on,status,version,\
                 created_by,reviewed_by,created_at,updated_at FROM benefit_enrollment_windows \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='open'",
            )
            .bind(id)
            .bind(self.company.as_str())
            .bind(version)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(map_window).transpose().map_err(Into::into)
        })
    }

    fn verified_dependents<'a>(
        &'a mut self,
        person_id: &'a str,
    ) -> BenefitsCommandFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT id FROM benefit_dependents WHERE company_id=$1 AND person_id=$2 \
                 AND status='verified' ORDER BY id",
            )
            .bind(self.company.as_str())
            .bind(person_id)
            .fetch_all(&mut *self.connection)
            .await?)
        })
    }

    fn learning_requirement<'a>(
        &'a mut self,
        id: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT p.required_eligibility_code FROM benefit_elections e \
                 JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id \
                 WHERE e.id=$1 AND e.company_id=$2",
            )
            .bind(id)
            .bind(self.company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row
                .map(|row| row.try_get::<Option<String>, _>("required_eligibility_code"))
                .transpose()?
                .flatten())
        })
    }

    fn create_plan<'a>(
        &'a mut self,
        command: &'a CreatePlan<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let input = command.input;
            sqlx::query(
                "INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,\
                 employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,\
                 created_by,reviewed_by,version,created_at,updated_at,required_eligibility_code) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'draft',$11,NULL,1,$12,$12,$13)",
            )
            .bind(command.id)
            .bind(self.company.as_str())
            .bind(&input.name)
            .bind(&input.provider)
            .bind(&input.coverage_type)
            .bind(&input.currency)
            .bind(input.employee_deduction_minor)
            .bind(input.employer_cost_minor)
            .bind(&input.effective_from)
            .bind(&input.effective_to)
            .bind(&self.user_id)
            .bind(command.now)
            .bind(&input.required_eligibility_code)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_event(
                BenefitRecordKind::Plan,
                command.key,
                command.id,
                1,
                "create",
                "Plan drafted for independent review",
                "draft",
                command.now,
            )
            .await?;
            let result = BenefitCommandResult::new(command.id, "draft", 1);
            self.commit(
                BenefitRecordKind::Plan,
                command.key,
                command.operation,
                command.request_hash,
                result,
                command.now,
            )
            .await
        })
    }

    fn create_window<'a>(
        &'a mut self,
        command: &'a CreateWindow<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let input = command.input;
            sqlx::query(
                "INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,\
                 effective_on,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'draft',$7,NULL,1,$8,$8)",
            )
            .bind(command.id)
            .bind(self.company.as_str())
            .bind(&input.name)
            .bind(&input.opens_on)
            .bind(&input.closes_on)
            .bind(&input.effective_on)
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_event(
                BenefitRecordKind::Window,
                command.key,
                command.id,
                1,
                "create",
                "Window drafted for independent review",
                "draft",
                command.now,
            )
            .await?;
            let result = BenefitCommandResult::new(command.id, "draft", 1);
            self.commit(
                BenefitRecordKind::Window,
                command.key,
                command.operation,
                command.request_hash,
                result,
                command.now,
            )
            .await
        })
    }

    fn create_dependent<'a>(
        &'a mut self,
        command: &'a CreateDependent<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let input = command.input;
            sqlx::query(
                "INSERT INTO benefit_dependents(id,company_id,person_id,name,relationship,\
                 birth_date,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'pending',$7,NULL,1,$8,$8)",
            )
            .bind(command.id)
            .bind(self.company.as_str())
            .bind(command.person_id)
            .bind(&input.name)
            .bind(&input.relationship)
            .bind(&input.birth_date)
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_event(
                BenefitRecordKind::Dependent,
                command.key,
                command.id,
                1,
                "create",
                "Dependent submitted for verification",
                "pending",
                command.now,
            )
            .await?;
            let result = BenefitCommandResult::new(command.id, "pending", 1);
            self.commit(
                BenefitRecordKind::Dependent,
                command.key,
                command.operation,
                command.request_hash,
                result,
                command.now,
            )
            .await
        })
    }

    fn create_election<'a>(
        &'a mut self,
        command: &'a CreateElection<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let input = command.input;
            let plan = command.plan;
            let window = command.window;
            let dependent_ids = serde_json::to_string(&input.dependent_ids).map_err(|_| {
                unavailable(
                    "BENEFITS_SOURCE_INVALID",
                    "The Benefits dependent selection is invalid.",
                )
            })?;
            sqlx::query(
                "INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,\
                 plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,\
                 employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,\
                 created_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'elected',$16,1,$17,$17)",
            )
            .bind(command.id)
            .bind(self.company.as_str())
            .bind(&command.worker.person_id)
            .bind(&command.worker.engagement_id)
            .bind(&input.plan_id)
            .bind(input.plan_version)
            .bind(&input.window_id)
            .bind(input.window_version)
            .bind(&plan.coverage_type)
            .bind(dependent_ids)
            .bind(&plan.currency)
            .bind(plan.employee_deduction_minor)
            .bind(plan.employer_cost_minor)
            .bind(&window.effective_on)
            .bind(&plan.effective_to)
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_event(
                BenefitRecordKind::Election,
                command.key,
                command.id,
                1,
                "elect",
                "Employee elected reviewed coverage",
                "elected",
                command.now,
            )
            .await?;
            let result = BenefitCommandResult::new(command.id, "elected", 1);
            self.commit(
                BenefitRecordKind::Election,
                command.key,
                command.operation,
                command.request_hash,
                result,
                command.now,
            )
            .await
        })
    }

    fn change<'a>(
        &'a mut self,
        command: &'a ChangeBenefit<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let version = command.input.expected_version + 1;
            let changed = match command.kind {
                BenefitRecordKind::Plan
                | BenefitRecordKind::Window
                | BenefitRecordKind::Dependent => {
                    let sql = format!(
                        "UPDATE {} SET status=$3,version=version+1,updated_at=$4,\
                         reviewed_by=COALESCE($5,reviewed_by) \
                         WHERE id=$1 AND company_id=$2 AND version=$6 AND status=$7",
                        command.kind.table()
                    );
                    sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(command.id)
                        .bind(self.company.as_str())
                        .bind(command.to)
                        .bind(command.now)
                        .bind(command.changes.reviewed_by.as_deref())
                        .bind(command.input.expected_version)
                        .bind(command.from)
                        .execute(&mut *self.connection)
                        .await?
                        .rows_affected()
                }
                BenefitRecordKind::Election => sqlx::query(
                    "UPDATE benefit_elections SET status=$3,version=version+1,updated_at=$4,\
                         submitted_by=COALESCE($5,submitted_by),\
                         provider_reference=COALESCE($6,provider_reference),\
                         acknowledged_by=COALESCE($7,acknowledged_by),\
                         released_by=COALESCE($8,released_by) \
                         WHERE id=$1 AND company_id=$2 AND version=$9 AND status=$10",
                )
                .bind(command.id)
                .bind(self.company.as_str())
                .bind(command.to)
                .bind(command.now)
                .bind(command.changes.submitted_by.as_deref())
                .bind(command.changes.provider_reference.as_deref())
                .bind(command.changes.acknowledged_by.as_deref())
                .bind(command.changes.released_by.as_deref())
                .bind(command.input.expected_version)
                .bind(command.from)
                .execute(&mut *self.connection)
                .await?
                .rows_affected(),
            };
            if changed != 1 {
                return Err(conflict("CONFLICT", "The record changed. Reload and retry.").into());
            }
            self.insert_event(
                command.kind,
                command.key,
                command.id,
                version,
                &command.input.action,
                &command.input.reason,
                command.to,
                command.now,
            )
            .await?;
            if command.release_payroll {
                sqlx::query(
                    "INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,\
                     engagement_id,currency,employee_deduction_minor,employer_cost_minor,\
                     effective_from,effective_to,source_election_version,status,released_by,created_at) \
                     SELECT $1,id,company_id,person_id,engagement_id,currency,\
                     employee_deduction_minor,employer_cost_minor,effective_from,effective_to,\
                     $2,'released',$3,$4 FROM benefit_elections WHERE id=$5 AND company_id=$6",
                )
                .bind(format!("benefit-payroll-{}", command.key))
                .bind(version)
                .bind(&self.user_id)
                .bind(command.now)
                .bind(command.id)
                .bind(self.company.as_str())
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let result = BenefitCommandResult::new(command.id, command.to, version);
            self.commit(
                command.kind,
                command.key,
                command.operation,
                command.request_hash,
                result,
                command.now,
            )
            .await
        })
    }
}
