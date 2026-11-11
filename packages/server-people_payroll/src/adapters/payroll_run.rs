//! PostgreSQL adapter for native payroll run commands.
//!
//! The caller owns one serializable Holding transaction and fresh Account/IAM
//! authority. This adapter writes only People-owned payroll rows plus the
//! standard command receipt/audit/outbox ledger. Finance accounting rows are
//! written exclusively by the composed Finance journal port.
use crate::application::payroll_run::{
    CancelRunCommand, CommitPostCommand, CreateRunCommand, PayrollCommandFuture,
    PayrollCommandRepository, PayrollJournalPort, SaveCalculationCommand, StoredPayrollReceipt,
};
use crate::domain::payroll_run::{
    CountryPackFact, CreatePayrollRunInput, PayGroupFact, PayrollAdvanceRepayment,
    PayrollCalculationContext, PayrollCalculationInput, PayrollCompensation, PayrollCostAssignment,
    PayrollEngagement, PayrollHandoff, PayrollMappingFact, PayrollPayee, PayrollPostingBasis,
    PayrollPostingItem, PayrollRun, ThaiPayrollProfile, ThaiPayrollRules, ThaiPriorItem,
    ThaiStatutoryEvidence, idempotency_conflict, not_found, readback_invalid,
};
use crate::domain::payroll_run::{PayrollJournalRequest, PayrollPreparedJournal};
use erpx_server_benefits::public::payroll_source::{
    BenefitsPayrollQuery, BenefitsPayrollScope, PgBenefitsPayrollSource, payroll_benefit_inputs,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Column, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn source_invalid() -> BusinessError {
    unavailable(
        "PAYROLL_SOURCE_INVALID",
        "The current payroll source is invalid.",
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
            "The payroll record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgPayrollCommand<'a> {
    connection: &'a mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'a> PgPayrollCommand<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        actor: &crate::domain::payroll_run::PayrollCommandActor,
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
            return Err(PersistenceError::new("PAYROLL_COMMAND_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PAYROLL_COMMAND_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_receipt(
        &mut self,
        key: &str,
        operation: &str,
        request_hash: &str,
        record_id: &str,
        result: &Value,
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
        .bind(serde_json::to_string(result).unwrap_or_else(|_| "null".into()))
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
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,'hr_payroll_run',$6,$7,$8,$9,$10)",
        )
        .bind(format!("payroll-run-audit-{key}"))
        .bind(key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(operation)
        .bind(now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn insert_outbox(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        state: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = json!({
            "authority": authority,
            "id": record_id,
            "state": state,
            "companyId": self.company.as_str(),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'hr_payroll_run',$4,$5,$6,$7)",
        )
        .bind(format!("payroll-run-outbox-{key}"))
        .bind(key)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn select_run(
        &mut self,
        company: &CompanyId,
        run_id: &str,
    ) -> Result<Option<PayrollRun>, Error> {
        let row = sqlx::query(
            "SELECT r.id,r.company_id,r.pay_group_id,g.currency AS pay_group_currency,\
             g.status AS pay_group_status,g.overtime_multiplier_bps,g.proration_method,\
             r.country_pack_id,r.period_start,r.period_end,r.pay_date,r.currency,r.mode,\
             r.status,r.revision,r.totals_json,r.input_digest,r.version,r.created_by,\
             r.submitted_by,r.approved_by,r.posted_by,r.adjustment_of_run_id,\
             r.scope_engagement_id \
             FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id \
             WHERE r.id=$1 AND r.company_id=$2",
        )
        .bind(run_id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(PayrollRun {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            pay_group_id: row.try_get("pay_group_id")?,
            pay_group_currency: row.try_get("pay_group_currency")?,
            pay_group_status: row.try_get("pay_group_status")?,
            overtime_multiplier_bps: row.try_get("overtime_multiplier_bps")?,
            proration_method: row.try_get("proration_method")?,
            country_pack_id: row.try_get("country_pack_id")?,
            period_start: row.try_get("period_start")?,
            period_end: row.try_get("period_end")?,
            pay_date: row.try_get("pay_date")?,
            currency: row.try_get("currency")?,
            mode: row.try_get("mode")?,
            status: row.try_get("status")?,
            revision: row.try_get("revision")?,
            totals_json: row.try_get("totals_json")?,
            input_digest: row.try_get("input_digest")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            submitted_by: row.try_get("submitted_by")?,
            approved_by: row.try_get("approved_by")?,
            posted_by: row.try_get("posted_by")?,
            adjustment_of_run_id: row.try_get("adjustment_of_run_id")?,
            scope_engagement_id: row.try_get("scope_engagement_id")?,
        }))
    }

    async fn pack(
        &mut self,
        company: &CompanyId,
        pack_id: &str,
    ) -> Result<Option<CountryPackFact>, Error> {
        let row = sqlx::query(
            "SELECT id,country_code,version,status,rules_json,adapter_id,source_digest,\
             record_version,effective_from,effective_to FROM hr_country_packs \
             WHERE id=$1 AND status='active'",
        )
        .bind(pack_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let _ = company;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(CountryPackFact {
            id: row.try_get("id")?,
            country_code: row.try_get("country_code")?,
            version: row.try_get("version")?,
            status: row.try_get("status")?,
            rules_json: row.try_get("rules_json")?,
            adapter_id: row.try_get("adapter_id")?,
            source_digest: row.try_get("source_digest")?,
            record_version: row.try_get("record_version")?,
            effective_from: row.try_get("effective_from")?,
            effective_to: row.try_get("effective_to")?,
        }))
    }

    async fn thai_evidence(
        &mut self,
        run: &PayrollRun,
        engagement_id: &str,
    ) -> Result<Option<ThaiStatutoryEvidence>, Error> {
        if !run.requires_statutory() {
            return Ok(None);
        }
        let pack_id = run.country_pack_id.as_deref().ok_or_else(|| {
            Error::Business(conflict(
                "PAYROLL_COUNTRY_PACK_REQUIRED",
                "Select an active Thai rule pack and THB payroll currency.",
            ))
        })?;
        let company = self.company.clone();
        let pack = self.pack(&company, pack_id).await?.ok_or_else(|| {
            Error::Business(conflict(
                "PAYROLL_COUNTRY_PACK_INACTIVE",
                "The selected payroll rule pack is not active.",
            ))
        })?;
        let year: i64 = run.pay_date[..4]
            .parse()
            .map_err(|_| Error::Business(source_invalid()))?;
        let profile_row = sqlx::query(
            "SELECT id,version,tax_year,effective_from,effective_to,annual_deductions_minor,\
             scheduled_pays,social_security_eligible,social_security_exemption_evidence,\
             opening_through_date,opening_taxable_minor,opening_irregular_minor,\
             opening_withheld_minor,prior_income_complete,evidence_ref,status \
             FROM hr_thai_payroll_profiles WHERE company_id=$1 AND engagement_id=$2 AND tax_year=$3 \
             AND status='approved' AND effective_from<=$4 AND effective_to>=$4 LIMIT 2",
        )
        .bind(self.company.as_str())
        .bind(engagement_id)
        .bind(year)
        .bind(&run.pay_date)
        .fetch_all(&mut *self.connection)
        .await?;
        if profile_row.len() != 1 {
            return Err(Error::Business(
                crate::domain::payroll_run::statutory_evidence_required(),
            ));
        }
        let profile_row = profile_row.into_iter().next().unwrap();
        let profile = ThaiPayrollProfile {
            id: profile_row.try_get("id")?,
            version: profile_row.try_get("version")?,
            tax_year: profile_row.try_get("tax_year")?,
            effective_from: profile_row.try_get("effective_from")?,
            effective_to: profile_row.try_get("effective_to")?,
            annual_deductions_minor: profile_row.try_get("annual_deductions_minor")?,
            scheduled_pays: profile_row.try_get("scheduled_pays")?,
            social_security_eligible: profile_row.try_get("social_security_eligible")?,
            social_security_exemption_evidence: profile_row
                .try_get("social_security_exemption_evidence")?,
            opening_through_date: profile_row.try_get("opening_through_date")?,
            opening_taxable_minor: profile_row.try_get("opening_taxable_minor")?,
            opening_irregular_minor: profile_row.try_get("opening_irregular_minor")?,
            opening_withheld_minor: profile_row.try_get("opening_withheld_minor")?,
            prior_income_complete: profile_row.try_get("prior_income_complete")?,
            evidence_ref: profile_row.try_get("evidence_ref")?,
            status: profile_row.try_get("status")?,
        };
        if !profile.prior_income_complete {
            return Err(Error::Business(
                crate::domain::payroll_run::statutory_evidence_required(),
            ));
        }
        let year_start = format!("{year:04}-01-01");
        let prior_rows = sqlx::query(
            "SELECT i.id,i.run_id,r.version run_version,r.input_digest,i.result_json,r.pay_date \
             FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id \
             WHERE r.company_id=$1 AND i.engagement_id=$2 AND r.status='posted' \
             AND r.currency='THB' AND r.pay_date>$3 AND r.pay_date<=$4 AND r.id<>$5 \
             AND r.pay_date>=$6 ORDER BY r.pay_date,i.id",
        )
        .bind(self.company.as_str())
        .bind(engagement_id)
        .bind(&profile.opening_through_date)
        .bind(&run.pay_date)
        .bind(&run.id)
        .bind(&year_start)
        .fetch_all(&mut *self.connection)
        .await?;
        let prior_items: Vec<ThaiPriorItem> = prior_rows
            .iter()
            .map(|row| {
                Ok(ThaiPriorItem {
                    id: row.try_get("id")?,
                    run_id: row.try_get("run_id")?,
                    run_version: row.try_get("run_version")?,
                    input_digest: row.try_get("input_digest")?,
                    result_json: row.try_get("result_json")?,
                    pay_date: row.try_get("pay_date")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let rules: ThaiPayrollRules = serde_json::from_str(&pack.rules_json).map_err(|_| {
            Error::Business(conflict(
                "PAYROLL_COUNTRY_PACK_INVALID",
                "The Thai payroll rule pack is invalid.",
            ))
        })?;
        let source_digest = pack.source_digest.clone().ok_or_else(|| {
            Error::Business(conflict(
                "PAYROLL_COUNTRY_PACK_NOT_EFFECTIVE",
                "The adapter and immutable rule evidence must be effective on the pay date.",
            ))
        })?;
        Ok(Some(ThaiStatutoryEvidence {
            pack_id: pack.id,
            record_version: pack.record_version,
            source_digest,
            rules,
            profile,
            prior_items,
        }))
    }
}

impl<'a> PayrollCommandRepository for PgPayrollCommand<'a> {
    type Error = Error;

    fn lock_command<'b>(&'b mut self, key: &'b str) -> PayrollCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("payroll-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'b>(
        &'b mut self,
        identity: &'b str,
    ) -> PayrollCommandFuture<'b, String, Self::Error> {
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

    fn receipt<'b>(
        &'b mut self,
        key: &'b str,
    ) -> PayrollCommandFuture<'b, Option<StoredPayrollReceipt>, Self::Error> {
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
                Some(value) => Some(serde_json::from_str(&value).map_err(|_| {
                    Error::Business(unavailable(
                        "PAYROLL_RECEIPT_INVALID",
                        "The Payroll command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredPayrollReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn run<'b>(
        &'b mut self,
        company: &'b CompanyId,
        run_id: &'b str,
    ) -> PayrollCommandFuture<'b, Option<PayrollRun>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.select_run(company, run_id).await
        })
    }

    fn pay_group_fact<'b>(
        &'b mut self,
        company: &'b CompanyId,
        pay_group_id: &'b str,
    ) -> PayrollCommandFuture<'b, Option<PayGroupFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,currency,country_code,frequency,status FROM hr_pay_groups \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(pay_group_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(PayGroupFact {
                id: row.try_get("id")?,
                currency: row.try_get("currency")?,
                country_code: row.try_get("country_code")?,
                frequency: row.try_get("frequency")?,
                status: row.try_get("status")?,
            }))
        })
    }

    fn duplicate_period<'b>(
        &'b mut self,
        run: &'b CreatePayrollRunInput,
        company: &'b CompanyId,
    ) -> PayrollCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=$1 \
                 AND pay_group_id=$2 AND mode!='final_pay' AND adjustment_of_run_id IS NULL \
                 AND status!='cancelled' AND period_start<=$3 AND period_end>=$4)",
            )
            .bind(company.as_str())
            .bind(&run.pay_group_id)
            .bind(&run.period_end)
            .bind(&run.period_start)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn country_pack<'b>(
        &'b mut self,
        company: &'b CompanyId,
        pack_id: &'b str,
    ) -> PayrollCommandFuture<'b, Option<CountryPackFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.pack(company, pack_id).await
        })
    }

    fn create_run<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b CreateRunCommand<'b>,
    ) -> PayrollCommandFuture<'b, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let mode = command.input.mode.clone();
            let result = json!({
                "id": command.id,
                "status": "calculating",
                "mode": mode,
                "version": 1,
            });
            self.insert_receipt(
                command.key,
                crate::domain::payroll_run::CREATE_OPERATION,
                command.request_hash,
                command.id,
                &result,
                command.now,
            )
            .await?;
            self.insert_audit(
                command.key,
                crate::domain::payroll_run::CREATE_OPERATION,
                command.id,
                1,
                command.now,
            )
            .await?;
            self.insert_outbox(
                command.key,
                crate::domain::payroll_run::CREATE_OPERATION,
                command.id,
                1,
                "calculating",
                command.now,
            )
            .await?;
            sqlx::query(
                "INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,\
                 period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'calculating',$10,$11,$12)",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(&command.input.pay_group_id)
            .bind(command.input.country_pack_id.as_deref())
            .bind(&command.input.period_start)
            .bind(&command.input.period_end)
            .bind(&command.input.pay_date)
            .bind(&command.input.currency)
            .bind(&command.input.mode)
            .bind(&self.user_id)
            .bind(command.now)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let row = sqlx::query(
                "SELECT id,status,mode,version FROM hr_payroll_runs WHERE id=$1 AND company_id=$2",
            )
            .bind(command.id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            let readback = json!({
                "id": row.try_get::<String, _>("id")?,
                "status": row.try_get::<String, _>("status")?,
                "mode": row.try_get::<String, _>("mode")?,
                "version": row.try_get::<i64, _>("version")?,
            });
            if readback != result {
                return Err(readback_invalid().into());
            }
            Ok(result)
        })
    }

    fn calculation_context<'b>(
        &'b mut self,
        run: &'b PayrollRun,
    ) -> PayrollCommandFuture<'b, PayrollCalculationContext, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let company = self.company.clone();
            let rows = sqlx::query(
                "SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,\
                 e.end_date,e.job_title,e.branch_id,e.department_id \
                 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id \
                 WHERE e.company_id=$1 AND e.pay_group_id=$2 AND e.currency=$3 \
                 AND e.worker_type='employee' AND e.status IN ('active','ended') \
                 AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=$4 \
                 AND (e.end_date IS NULL OR e.end_date>=$5) \
                 AND ($6::text IS NULL OR e.id=$6) ORDER BY e.id",
            )
            .bind(company.as_str())
            .bind(&run.pay_group_id)
            .bind(&run.currency)
            .bind(&run.period_end)
            .bind(&run.period_start)
            .bind(run.scope_engagement_id.as_deref())
            .fetch_all(&mut *self.connection)
            .await?;
            let mut inputs = Vec::with_capacity(rows.len());
            for row in rows {
                let engagement = PayrollEngagement {
                    id: row.try_get("id")?,
                    version: row.try_get("version")?,
                    person_id: row.try_get("person_id")?,
                    worker_type: row.try_get("worker_type")?,
                    employment_type: row.try_get("employment_type")?,
                    start_date: row.try_get("start_date")?,
                    end_date: row.try_get("end_date")?,
                    job_title: row.try_get("job_title")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                };
                let payee = sqlx::query(
                    "SELECT id,version,legal_name,preferred_name FROM hr_people \
                     WHERE id=$1 AND company_id=$2 AND status='active' AND merged_into_id IS NULL",
                )
                .bind(&engagement.person_id)
                .bind(company.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .map(|row| {
                    Ok::<_, sqlx::Error>(PayrollPayee {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        legal_name: row.try_get("legal_name")?,
                        preferred_name: row.try_get("preferred_name")?,
                    })
                })
                .transpose()?
                .ok_or_else(|| Error::Business(invalid_payee()))?;
                let compensation = sqlx::query(
                    "SELECT id,version,base_amount_minor,hourly_rate_minor,daily_rate_minor,\
                     standard_daily_hours,currency,pay_frequency,effective_from,effective_to,\
                     status,components_json FROM hr_compensation_revisions \
                     WHERE engagement_id=$1 AND status IN ('active','approved') \
                     AND effective_from<=$2 AND (effective_to IS NULL OR effective_to>=$3) \
                     ORDER BY effective_from DESC LIMIT 1",
                )
                .bind(&engagement.id)
                .bind(&run.period_end)
                .bind(&run.period_start)
                .fetch_optional(&mut *self.connection)
                .await?
                .map(|row| {
                    Ok::<_, sqlx::Error>(PayrollCompensation {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        base_amount_minor: row.try_get("base_amount_minor")?,
                        hourly_rate_minor: row.try_get("hourly_rate_minor")?,
                        daily_rate_minor: row.try_get("daily_rate_minor")?,
                        standard_daily_hours: row.try_get("standard_daily_hours")?,
                        currency: row.try_get("currency")?,
                        pay_frequency: row.try_get("pay_frequency")?,
                        effective_from: row.try_get("effective_from")?,
                        effective_to: row.try_get("effective_to")?,
                        status: row.try_get("status")?,
                        components_json: row.try_get("components_json")?,
                    })
                })
                .transpose()?;
                let time_entries = sqlx::query(
                    "SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,\
                     cost_branch_id,cost_department_id FROM hr_time_entries \
                     WHERE engagement_id=$1 AND work_date BETWEEN $2 AND $3 AND status='approved' \
                     ORDER BY id",
                )
                .bind(&engagement.id)
                .bind(&run.period_start)
                .bind(&run.period_end)
                .fetch_all(&mut *self.connection)
                .await?
                .iter()
                .map(|row| {
                    Ok(PayrollTimeEntryRow {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        work_date: row.try_get("work_date")?,
                        regular_hours: row.try_get("regular_hours")?,
                        overtime_hours: row.try_get("overtime_hours")?,
                        cost_assignment_id: row.try_get("cost_assignment_id")?,
                        cost_branch_id: row.try_get("cost_branch_id")?,
                        cost_department_id: row.try_get("cost_department_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?
                .into_iter()
                .map(|row| crate::domain::payroll_run::PayrollTimeEntry {
                    id: row.id,
                    version: row.version,
                    work_date: row.work_date,
                    regular_hours: row.regular_hours,
                    overtime_hours: row.overtime_hours,
                    cost_assignment_id: row.cost_assignment_id,
                    cost_branch_id: row.cost_branch_id,
                    cost_department_id: row.cost_department_id,
                })
                .collect();
                let handoffs = sqlx::query(
                    "SELECT id,request_id,request_version,minutes,work_date \
                     FROM overtime_payroll_handoffs WHERE engagement_id=$1 \
                     AND work_date BETWEEN $2 AND $3 ORDER BY id",
                )
                .bind(&engagement.id)
                .bind(&run.period_start)
                .bind(&run.period_end)
                .fetch_all(&mut *self.connection)
                .await?
                .iter()
                .map(|row| {
                    Ok(PayrollHandoff {
                        id: row.try_get("id")?,
                        request_id: row.try_get("request_id")?,
                        request_version: row.try_get("request_version")?,
                        minutes: row.try_get("minutes")?,
                        work_date: row.try_get("work_date")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
                let advances = sqlx::query(
                    "SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason \
                     FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id \
                     WHERE r.company_id=$1 AND a.engagement_id=$2 AND a.status='approved' \
                     AND r.status IN ('open','reserved') AND r.due_period_start<=$3 \
                     AND r.due_period_end>=$4 AND (r.payroll_run_id IS NULL OR r.payroll_run_id=$5) \
                     ORDER BY r.sequence",
                )
                .bind(company.as_str())
                .bind(&engagement.id)
                .bind(&run.period_end)
                .bind(&run.period_start)
                .bind(&run.id)
                .fetch_all(&mut *self.connection)
                .await?
                .iter()
                .map(|row| {
                    Ok(PayrollAdvanceRepayment {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        amount_minor: row.try_get("amount_minor")?,
                        due_period_start: row.try_get("due_period_start")?,
                        due_period_end: row.try_get("due_period_end")?,
                        reason: row.try_get("reason")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
                let active_start = if engagement.start_date > run.period_start {
                    engagement.start_date.clone()
                } else {
                    run.period_start.clone()
                };
                let active_end = match engagement.end_date.as_deref() {
                    Some(value) if value < run.period_end.as_str() => value.to_owned(),
                    _ => run.period_end.clone(),
                };
                let cost_assignments = sqlx::query(
                    "SELECT id,version,effective_from,effective_to,branch_id,department_id,evidence_status \
                     FROM hr_payroll_cost_assignments WHERE engagement_id=$1 AND company_id=$2 \
                     AND status='active' AND effective_from<=$3 \
                     AND (effective_to IS NULL OR effective_to>=$4) ORDER BY effective_from,id",
                )
                .bind(&engagement.id)
                .bind(company.as_str())
                .bind(&active_end)
                .bind(&active_start)
                .fetch_all(&mut *self.connection)
                .await?
                .iter()
                .map(|row| {
                    Ok(PayrollCostAssignment {
                        id: row.try_get("id")?,
                        version: row.try_get("version")?,
                        effective_from: row.try_get("effective_from")?,
                        effective_to: row.try_get("effective_to")?,
                        branch_id: row.try_get("branch_id")?,
                        department_id: row.try_get("department_id")?,
                        evidence_status: row.try_get("evidence_status")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
                let benefits = {
                    let mut source = PgBenefitsPayrollSource::new(&mut *self.connection);
                    payroll_benefit_inputs(
                        &mut source,
                        &BenefitsPayrollScope::new(company.clone()),
                        &BenefitsPayrollQuery {
                            engagement_id: engagement.id.clone(),
                            currency: run.currency.clone(),
                            period_start: run.period_start.clone(),
                            period_end: run.period_end.clone(),
                        },
                    )
                    .await?
                };
                let thai_statutory = self.thai_evidence(run, &engagement.id).await?;
                inputs.push(PayrollCalculationInput {
                    engagement,
                    payee,
                    compensation,
                    time_entries,
                    handoffs,
                    advances,
                    cost_assignments,
                    benefits,
                    thai_statutory,
                });
            }
            Ok(PayrollCalculationContext { inputs })
        })
    }

    fn save_calculation<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b SaveCalculationCommand<'b>,
    ) -> PayrollCommandFuture<'b, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let totals = json!({
                "gross": command.totals.gross_minor,
                "deductions": command.totals.deductions_minor,
                "employer": command.totals.employer_cost_minor,
                "net": command.totals.net_minor,
                "headcount": command.totals.headcount,
            });
            let version = self.next_version(command.run_id).await?;
            let result = json!({
                "id": command.run_id,
                "status": "preview",
                "version": version,
                "totals": totals,
            });
            self.insert_receipt(
                command.key,
                crate::domain::payroll_run::CALCULATE_OPERATION,
                command.request_hash,
                command.run_id,
                &result,
                command.now,
            )
            .await?;
            self.insert_audit(
                command.key,
                crate::domain::payroll_run::CALCULATE_OPERATION,
                command.run_id,
                result["version"].as_i64().unwrap_or(1),
                command.now,
            )
            .await?;
            self.insert_outbox(
                command.key,
                crate::domain::payroll_run::CALCULATE_OPERATION,
                command.run_id,
                result["version"].as_i64().unwrap_or(1),
                "preview",
                command.now,
            )
            .await?;
            sqlx::query("DELETE FROM hr_payroll_items WHERE run_id=$1")
                .bind(command.run_id)
                .execute(&mut *self.connection)
                .await?;
            let changed = sqlx::query(
                "UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=$1,\
                 input_digest=$2,version=version+1,updated_at=$3,error_message=NULL \
                 WHERE id=$4 AND company_id=$5 AND status IN ('draft','calculating','preview','failed')",
            )
            .bind(command.totals.to_json())
            .bind(command.digest)
            .bind(command.now)
            .bind(command.run_id)
            .bind(company.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "PAYROLL_LOCKED",
                    "Only an unapproved payroll preview can be recalculated.",
                )
                .into());
            }
            for item in command.items {
                sqlx::query(
                    "INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,\
                     input_snapshot_json,result_json,gross_minor,deductions_minor,\
                     employer_cost_minor,net_minor,currency,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
                )
                .bind(&item.id)
                .bind(command.run_id)
                .bind(&item.engagement_id)
                .bind(&item.worker_type)
                .bind(&item.input_snapshot_json)
                .bind(&item.result_json)
                .bind(item.gross_minor)
                .bind(item.deductions_minor)
                .bind(item.employer_cost_minor)
                .bind(item.net_minor)
                .bind(&item.currency)
                .bind(command.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let row = sqlx::query(
                "SELECT id,status,version,totals_json,input_digest FROM hr_payroll_runs \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(command.run_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            let readback = json!({
                "id": row.try_get::<String, _>("id")?,
                "status": row.try_get::<String, _>("status")?,
                "version": row.try_get::<i64, _>("version")?,
                "totals": serde_json::from_str::<Value>(&row.try_get::<String, _>("totals_json")?)
                    .unwrap_or(Value::Null),
            });
            if readback["status"] != "preview"
                || readback["version"] != result["version"]
                || row.try_get::<Option<String>, _>("input_digest")?.as_deref()
                    != Some(command.digest)
            {
                return Err(readback_invalid().into());
            }
            Ok(result)
        })
    }

    fn cancel_run<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b CancelRunCommand<'b>,
    ) -> PayrollCommandFuture<'b, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let result = json!({
                "id": command.run.id,
                "status": "cancelled",
                "version": command.input.expected_version + 1,
                "cancellationId": command.evidence_id,
            });
            self.insert_receipt(
                command.key,
                crate::domain::payroll_run::CANCEL_OPERATION,
                command.request_hash,
                &command.run.id,
                &result,
                command.now,
            )
            .await?;
            let snapshot = sqlx::query_scalar::<_, String>(
                "SELECT to_jsonb(r)::text FROM hr_payroll_runs r WHERE r.id=$1 AND r.company_id=$2",
            )
            .bind(&command.run.id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(not_found)?;
            let changed = sqlx::query(
                "UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND status=$4 AND version=$5",
            )
            .bind(command.now)
            .bind(&command.run.id)
            .bind(company.as_str())
            .bind(&command.run.status)
            .bind(command.input.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "PAYROLL_RUN_CHANGED",
                    "Reload the payroll run before cancelling it.",
                )
                .into());
            }
            sqlx::query(
                "INSERT INTO hr_payroll_run_cancellations(id,company_id,run_id,command_id,\
                 cancelled_by,reason,prior_status,prior_version,run_revision,source_snapshot_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            )
            .bind(command.evidence_id)
            .bind(company.as_str())
            .bind(&command.run.id)
            .bind(command.key)
            .bind(&self.user_id)
            .bind(&command.input.reason)
            .bind(&command.run.status)
            .bind(command.run.version)
            .bind(command.run.revision)
            .bind(snapshot)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_audit(
                command.key,
                crate::domain::payroll_run::CANCEL_OPERATION,
                &command.run.id,
                command.input.expected_version + 1,
                command.now,
            )
            .await?;
            self.insert_outbox(
                command.key,
                crate::domain::payroll_run::CANCEL_OPERATION,
                &command.run.id,
                command.input.expected_version + 1,
                "cancelled",
                command.now,
            )
            .await?;
            let readback = sqlx::query(
                "SELECT id,status,version FROM hr_payroll_runs WHERE id=$1 AND company_id=$2",
            )
            .bind(&command.run.id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            if readback.try_get::<String, _>("status")? != "cancelled"
                || readback.try_get::<i64, _>("version")? != command.input.expected_version + 1
            {
                return Err(readback_invalid().into());
            }
            Ok(result)
        })
    }

    fn posting_basis<'b>(
        &'b mut self,
        company: &'b CompanyId,
        run_id: &'b str,
        mapping_id: Option<&'b str>,
    ) -> PayrollCommandFuture<'b, Option<PayrollPostingBasis>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let Some(run) = self.select_run(company, run_id).await? else {
                return Ok(None);
            };
            let items = sqlx::query(
                "SELECT id,worker_type,status,gross_minor,deductions_minor,employer_cost_minor,\
                 net_minor,input_snapshot_json,result_json FROM hr_payroll_items \
                 WHERE run_id=$1 ORDER BY id",
            )
            .bind(run_id)
            .fetch_all(&mut *self.connection)
            .await?
            .iter()
            .map(|row| {
                Ok(PayrollPostingItem {
                    id: row.try_get("id")?,
                    worker_type: row.try_get("worker_type")?,
                    status: row.try_get("status")?,
                    gross_minor: row.try_get("gross_minor")?,
                    deductions_minor: row.try_get("deductions_minor")?,
                    employer_cost_minor: row.try_get("employer_cost_minor")?,
                    net_minor: row.try_get("net_minor")?,
                    input_snapshot_json: row.try_get("input_snapshot_json")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let mapping_sql = if mapping_id.is_some() {
                "SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings \
                 WHERE company_id=$1 AND pay_group_id=$2 AND status='approved' AND id=$3 \
                 ORDER BY created_at DESC LIMIT 1"
            } else {
                "SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings \
                 WHERE company_id=$1 AND pay_group_id=$2 AND status='approved' \
                 ORDER BY created_at DESC LIMIT 1"
            };
            let mut query = sqlx::query(mapping_sql)
                .bind(company.as_str())
                .bind(&run.pay_group_id);
            if let Some(mapping_id) = mapping_id {
                query = query.bind(mapping_id);
            }
            let mapping = query
                .fetch_optional(&mut *self.connection)
                .await?
                .map(|row| {
                    Ok::<_, sqlx::Error>(PayrollMappingFact {
                        id: row.try_get("id")?,
                        chart_id: row.try_get("chart_id")?,
                        accounts_json: row.try_get("accounts_json")?,
                        version: row.try_get("version")?,
                    })
                })
                .transpose()?;
            Ok(Some(PayrollPostingBasis {
                run,
                items,
                mapping,
            }))
        })
    }

    fn commit_post<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b CommitPostCommand<'b>,
    ) -> PayrollCommandFuture<'b, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let result = json!({
                "id": command.run.id,
                "status": "posted",
                "version": command.run.version + 1,
                "journalId": command.journal.id,
                "netPayableMinor": command.accrual.totals.net_minor,
                "payslipIds": [],
            });
            self.insert_receipt(
                command.command_key,
                crate::domain::payroll_run::POST_OPERATION,
                command.request_hash,
                &command.run.id,
                &result,
                command.now,
            )
            .await?;
            let changed = sqlx::query(
                "UPDATE hr_payroll_runs SET status='posted',posted_by=$1,version=version+1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND status='approved' AND version=$5",
            )
            .bind(&self.user_id)
            .bind(command.now)
            .bind(&command.run.id)
            .bind(company.as_str())
            .bind(command.run.version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "VERSION_CONFLICT",
                    "The payroll run changed. Refresh before posting.",
                )
                .into());
            }
            sqlx::query(
                "UPDATE hr_payroll_items SET status='posted' WHERE run_id=$1 \
                 AND status IN ('calculated','approved')",
            )
            .bind(&command.run.id)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO hr_payroll_postings(id,run_id,journal_id,account_mapping_id,\
                 account_mapping_version,created_at) VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(format!("payroll-posting-{}", command.command_key))
            .bind(&command.run.id)
            .bind(&command.journal.id)
            .bind(command.resolved.mapping_id.as_deref())
            .bind(command.resolved.mapping_version)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_audit(
                command.command_key,
                crate::domain::payroll_run::POST_OPERATION,
                &command.run.id,
                command.run.version + 1,
                command.now,
            )
            .await?;
            self.insert_outbox(
                command.command_key,
                crate::domain::payroll_run::POST_OPERATION,
                &command.run.id,
                command.run.version + 1,
                "posted",
                command.now,
            )
            .await?;
            let readback = sqlx::query(
                "SELECT r.status,r.version,p.journal_id FROM hr_payroll_runs r \
                 JOIN hr_payroll_postings p ON p.run_id=r.id \
                 WHERE r.id=$1 AND r.company_id=$2",
            )
            .bind(&command.run.id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            if readback.try_get::<String, _>("status")? != "posted"
                || readback.try_get::<i64, _>("version")? != command.run.version + 1
                || readback.try_get::<String, _>("journal_id")? != command.journal.id
            {
                return Err(readback_invalid().into());
            }
            Ok(result)
        })
    }

    fn detail<'b>(
        &'b mut self,
        company: &'b CompanyId,
        run_id: &'b str,
        payment_only: bool,
    ) -> PayrollCommandFuture<'b, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let Some(run) = self.select_run(company, run_id).await? else {
                return Ok(None);
            };
            if payment_only {
                let items: Vec<Value> = Vec::new();
                let exports: Vec<Value> = Vec::new();
                let corrections: Vec<Value> = Vec::new();
                let payment_batch = payment_batch(&mut *self.connection, company, run_id).await?;
                let payment_items = payment_items(&mut *self.connection, &payment_batch).await?;
                return Ok(Some(json!({
                    "run": {
                        "id": run.id,
                        "company_id": run.company_id,
                        "pay_group_id": run.pay_group_id,
                        "period_start": run.period_start,
                        "period_end": run.period_end,
                        "pay_date": run.pay_date,
                        "currency": run.currency,
                        "status": run.status,
                        "mode": run.mode,
                        "revision": run.revision,
                        "version": run.version,
                        "totals_json": "{}",
                        "input_digest": Value::Null,
                        "error_message": Value::Null,
                    },
                    "items": items,
                    "exports": exports,
                    "corrections": corrections,
                    "paymentBatch": payment_batch,
                    "paymentItems": payment_items,
                })));
            }
            let items = sqlx::query(
                "SELECT i.id,i.run_id,i.engagement_id,p.legal_name,p.id AS person_id,i.worker_type,\
                 i.input_snapshot_json,i.result_json,i.gross_minor,i.deductions_minor,\
                 i.employer_cost_minor,i.net_minor,i.currency,i.status,i.created_at \
                 FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id \
                 JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=$1 ORDER BY p.legal_name",
            )
            .bind(run_id)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(row_to_json)
            .collect::<Vec<_>>();
            let exports = sqlx::query(
                "SELECT id,run_revision,checksum_sha256,created_at FROM hr_payroll_review_exports \
                 WHERE run_id=$1 ORDER BY run_revision DESC",
            )
            .bind(run_id)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(row_to_json)
            .collect::<Vec<_>>();
            let corrections = sqlx::query(
                "SELECT * FROM hr_payroll_corrections WHERE run_id=$1 AND company_id=$2 \
                 ORDER BY created_at DESC,id DESC",
            )
            .bind(run_id)
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(row_to_json)
            .collect::<Vec<_>>();
            let payment_batch = payment_batch(&mut *self.connection, company, run_id).await?;
            let payment_items = payment_items(&mut *self.connection, &payment_batch).await?;
            Ok(Some(json!({
                "run": {
                    "id": run.id,
                    "company_id": run.company_id,
                    "pay_group_id": run.pay_group_id,
                    "period_start": run.period_start,
                    "period_end": run.period_end,
                    "pay_date": run.pay_date,
                    "currency": run.currency,
                    "status": run.status,
                    "mode": run.mode,
                    "revision": run.revision,
                    "version": run.version,
                    "totals_json": run.totals_json,
                    "input_digest": run.input_digest,
                    "error_message": Value::Null,
                    "overtime_multiplier_bps": run.overtime_multiplier_bps,
                    "proration_method": run.proration_method,
                },
                "items": items,
                "exports": exports,
                "corrections": corrections,
                "paymentBatch": payment_batch,
                "paymentItems": payment_items,
            })))
        })
    }

    fn prepare_journal<'b, J: PayrollJournalPort>(
        &'b mut self,
        journal: &'b mut J,
        request: &'b PayrollJournalRequest,
    ) -> PayrollCommandFuture<'b, PayrollPreparedJournal, Error> {
        Box::pin(async move {
            self.ready().await?;
            let prepared = journal.prepare(&mut *self.connection, request).await?;
            Ok(prepared)
        })
    }
}

impl PgPayrollCommand<'_> {
    async fn next_version(&mut self, run_id: &str) -> Result<i64, Error> {
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT version+1 FROM hr_payroll_runs WHERE id=$1")
                .bind(run_id)
                .fetch_one(&mut *self.connection)
                .await?,
        )
    }
}

struct PayrollTimeEntryRow {
    id: String,
    version: i64,
    work_date: String,
    regular_hours: f64,
    overtime_hours: f64,
    cost_assignment_id: Option<String>,
    cost_branch_id: Option<String>,
    cost_department_id: Option<String>,
}

fn invalid_payee() -> BusinessError {
    BusinessError::invalid(
        "PAYROLL_PAYEE_INVALID",
        "Verify the employee identity before calculating payroll.",
    )
}

fn row_to_json(row: sqlx::postgres::PgRow) -> Value {
    let mut object = serde_json::Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        let value = match row.try_get::<Option<String>, _>(index) {
            Ok(Some(value)) => Value::String(value),
            Ok(None) => Value::Null,
            Err(_) => match row.try_get::<i64, _>(index) {
                Ok(value) => json!(value),
                Err(_) => match row.try_get::<f64, _>(index) {
                    Ok(value) => json!(value),
                    Err(_) => Value::Null,
                },
            },
        };
        object.insert(column.name().to_owned(), value);
    }
    Value::Object(object)
}

async fn payment_batch(
    connection: &mut PgConnection,
    company: &CompanyId,
    run_id: &str,
) -> Result<Option<Value>, Error> {
    Ok(sqlx::query(
        "SELECT * FROM hr_payment_batches WHERE company_id=$1 AND source_type='payroll' \
         AND source_id=$2 AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1",
    )
    .bind(company.as_str())
    .bind(run_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(row_to_json))
}

async fn payment_items(
    connection: &mut PgConnection,
    batch: &Option<Value>,
) -> Result<Vec<Value>, Error> {
    let Some(batch_id) = batch
        .as_ref()
        .and_then(|batch| batch.get("id"))
        .and_then(Value::as_str)
    else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query("SELECT * FROM hr_payment_batch_items WHERE batch_id=$1 ORDER BY reference")
            .bind(batch_id)
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(row_to_json)
            .collect(),
    )
}
