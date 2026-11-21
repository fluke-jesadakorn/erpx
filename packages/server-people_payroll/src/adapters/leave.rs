//! PostgreSQL adapter for native leave requests.
//!
//! The caller owns one serializable Holding transaction and fresh Account/IAM
//! authority. This adapter writes only People-owned leave request,
//! cancellation, evidence-manifest, day and balance facts plus the standard
//! command receipt/audit/outbox ledger. Workflow approvals, Files evidence and
//! Operations staff facts are supplied through the composed ports; their SQL is
//! never reached here.
use crate::application::leave::{
    LeaveCancelWrite, LeaveCancellationInsert, LeaveCancellationWrite, LeaveCommandFuture,
    LeaveCommandRepository, LeaveDecisionWrite, LeaveEmployeeInsert, LeaveInsert, LeaveLedgerEntry,
    LeaveUpdate, StoredLeaveReceipt,
};
use crate::domain::leave::{
    LeaveAccrualDue, LeaveAccrualWrite, LeaveBalance, LeaveBalanceWrite,
    LeaveEvidenceManifestWrite, LeaveEvidencePolicyRule, LeaveRequestRow, idempotency_conflict,
    request_invalid,
};
use crate::domain::workforce::WorkforceCommandActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
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
            "The leave record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgLeaveCommand<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgLeaveCommand<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &WorkforceCommandActor) -> Self {
        Self {
            connection,
            company: actor.company_id.clone(),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json(),
        }
    }

    async fn read_ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_LEAVE_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn ready(&mut self) -> Result<(), Error> {
        self.read_ready().await?;
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PEOPLE_LEAVE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &LeaveLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| request_invalid())?;
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&self.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(result)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("people-leave-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = serde_json::json!({
            "authority": authority,
            "id": entry.record_id,
            "state": entry.state,
            "companyId": self.company.as_str(),
            "version": entry.version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("people-leave-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

fn map_leave_row(row: sqlx::postgres::PgRow) -> Result<LeaveRequestRow, sqlx::Error> {
    Ok(LeaveRequestRow {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        employee_id: row.try_get("employee_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        person_id: row.try_get("person_id")?,
        engagement_id: row.try_get("engagement_id")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
        days: row.try_get("days")?,
        calculated_units: row.try_get("calculated_units")?,
        handover_note: row.try_get("handover_note")?,
        leave_type_id: row.try_get("leave_type_id")?,
        leave_type_name: row.try_get("leave_type_name")?,
        policy_version_id: row.try_get("policy_version_id")?,
        state: row.try_get("state")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        prepared_by: row.try_get("prepared_by")?,
        decided_by: row.try_get("decided_by")?,
        decision_note: row.try_get("decision_note")?,
        cancellation_state: row.try_get("cancellation_state")?,
        evidence_policy_snapshot: row.try_get("evidence_policy_snapshot")?,
        evidence_due_at: row.try_get("evidence_due_at")?,
        evidence_version: i64::from(row.try_get::<i32, _>("evidence_version")?),
        submitted_at: row.try_get("submitted_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl LeaveCommandRepository for PgLeaveCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("people-leave-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> LeaveCommandFuture<'a, String, Self::Error> {
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
    ) -> LeaveCommandFuture<'a, Option<StoredLeaveReceipt>, Self::Error> {
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
                    Error::Business(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_LEAVE_RECEIPT_INVALID",
                        "The leave command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredLeaveReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a LeaveLedgerEntry<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn leave_row<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        user_id: &'a str,
        evidence_reader: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT l.id,l.number,l.employee_id,l.company_id,l.branch_id,l.department_id,l.person_id,\
                 l.engagement_id,l.start_date,l.end_date,l.days,l.calculated_units::float8 calculated_units,\
                 l.handover_note,l.leave_type_id,t.name leave_type_name,l.policy_version_id,l.state,\
                 l.version,l.created_by,l.prepared_by,l.decided_by,l.decision_note,l.cancellation_state,\
                 l.evidence_policy_snapshot,l.evidence_due_at,l.evidence_version,l.submitted_at,\
                 l.created_at,l.updated_at \
                 FROM leave_requests l \
                 LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
                 WHERE l.id=$1 AND l.company_id=$2 \
                   AND (l.created_by=$3 OR l.decided_by=$3 OR $4::boolean) \
                   AND ($5::text IS NULL OR l.branch_id=$5) \
                   AND ($6::text IS NULL OR l.department_id=$6)",
            )
            .bind(id)
            .bind(company.as_str())
            .bind(user_id)
            .bind(evidence_reader)
            .bind(branch_id)
            .bind(department_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_leave_row).transpose().map_err(Error::from)
        })
    }

    fn leave_row_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT l.id,l.number,l.employee_id,l.company_id,l.branch_id,l.department_id,l.person_id,\
                 l.engagement_id,l.start_date,l.end_date,l.days,l.calculated_units::float8 calculated_units,\
                 l.handover_note,l.leave_type_id,t.name leave_type_name,l.policy_version_id,l.state,\
                 l.version,l.created_by,l.prepared_by,l.decided_by,l.decision_note,l.cancellation_state,\
                 l.evidence_policy_snapshot,l.evidence_due_at,l.evidence_version,l.submitted_at,\
                 l.created_at,l.updated_at \
                 FROM leave_requests l \
                 LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
                 WHERE l.id=$1 AND l.company_id=$2 FOR UPDATE OF l",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_leave_row).transpose().map_err(Error::from)
        })
    }

    fn leave_row_for_cancellation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT l.id,l.number,l.employee_id,l.company_id,l.branch_id,l.department_id,l.person_id,\
                 l.engagement_id,l.start_date,l.end_date,l.days,l.calculated_units::float8 calculated_units,\
                 l.handover_note,l.leave_type_id,t.name leave_type_name,l.policy_version_id,l.state,\
                 l.version,l.created_by,l.prepared_by,l.decided_by,l.decision_note,l.cancellation_state,\
                 l.evidence_policy_snapshot,l.evidence_due_at,l.evidence_version,l.submitted_at,\
                 l.created_at,l.updated_at \
                 FROM leave_requests l \
                 LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
                 WHERE l.id=$1 AND l.company_id=$2 AND l.state='approved' \
                   AND l.cancellation_state='pending_review' FOR UPDATE OF l",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_leave_row).transpose().map_err(Error::from)
        })
    }

    fn employee<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM employees WHERE person_id=$1 AND company_id=$2",
            )
            .bind(person_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn create_employee<'a>(
        &'a mut self,
        command: &'a LeaveEmployeeInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,\
                 display_name,status,created_at) VALUES($1,$2,$3,$4,$5,$6,'active',$7) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(command.id)
            .bind(command.person_id)
            .bind(command.company_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.legal_name)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn create_leave<'a>(
        &'a mut self,
        command: &'a LeaveInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,\
                 company_id,branch_id,department_id,start_date,end_date,days,calculated_units,\
                 handover_note,leave_type_id,policy_version_id,evidence_policy_snapshot,\
                 evidence_due_at,state,version,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,CAST($16 AS text)::jsonb,\
                 $17,$18,1,$19,$20,$20)",
            )
            .bind(command.id)
            .bind(command.number)
            .bind(command.employee_id)
            .bind(command.person_id)
            .bind(command.engagement_id)
            .bind(command.company_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.start_date)
            .bind(command.end_date)
            .bind(command.days)
            .bind(command.calculated_units)
            .bind(command.handover_note)
            .bind(command.leave_type_id)
            .bind(command.policy_version_id)
            .bind(command.evidence_policy_snapshot)
            .bind(command.evidence_due_at)
            .bind(command.state)
            .bind(command.created_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn update_leave<'a>(
        &'a mut self,
        command: &'a LeaveUpdate<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE leave_requests SET start_date=$3,end_date=$4,days=$5,\
                 calculated_units=$6,handover_note=$7,leave_type_id=$8,policy_version_id=$9,\
                 evidence_policy_snapshot=CAST($10 AS text)::jsonb,evidence_due_at=$11,state=$12,person_id=$13,\
                 engagement_id=$14,branch_id=$15,department_id=$16,prepared_by=$17,\
                 decided_by=NULL,decision_note=NULL,version=version+1,last_command_id=$18,\
                 updated_at=$19 WHERE id=$1 AND company_id=$2 AND version=$20 \
                 AND state IN ('draft','rejected')",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.start_date)
            .bind(command.end_date)
            .bind(command.days)
            .bind(command.calculated_units)
            .bind(command.handover_note)
            .bind(command.leave_type_id)
            .bind(command.policy_version_id)
            .bind(command.evidence_policy_snapshot)
            .bind(command.evidence_due_at)
            .bind(command.state)
            .bind(command.person_id)
            .bind(command.engagement_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.prepared_by)
            .bind(command.key)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "LEAVE_CHANGED",
                    "This leave request changed. Reload before saving.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn decide_leave<'a>(
        &'a mut self,
        command: &'a LeaveDecisionWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE leave_requests SET state=$3,decision_note=$4,decided_by=$5,\
                 evidence_version=evidence_version+$6,version=version+1,last_command_id=$7,\
                 updated_at=$8 WHERE id=$1 AND company_id=$2 AND version=$9 \
                 AND state='pending_approval'",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.state)
            .bind(command.note)
            .bind(command.decided_by)
            .bind(command.evidence_increment as i32)
            .bind(command.key)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "LEAVE_CHANGED",
                    "This leave request changed. Reload before deciding.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn open_cancellation<'a>(
        &'a mut self,
        command: &'a LeaveCancellationInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO leave_cancellation_reviews(id,request_id,company_id,reason,\
                 requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$6)",
            )
            .bind(command.id)
            .bind(command.request_id)
            .bind(command.company_id)
            .bind(command.reason)
            .bind(command.requested_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn decide_cancellation<'a>(
        &'a mut self,
        command: &'a LeaveCancellationWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE leave_requests SET state=$3,cancellation_state=$4,version=version+1,\
                 last_command_id=$5,updated_at=$6 WHERE id=$1 AND company_id=$2 \
                 AND version=$7 AND state='approved'",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.state)
            .bind(command.cancellation_state)
            .bind(command.key)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "LEAVE_CHANGED",
                    "This cancellation review changed. Reload before deciding.",
                )
                .into());
            }
            if let Some(decided_by) = command.decided_by {
                let review_state = match command.cancellation_state {
                    "approved" => "approved",
                    "rejected" => "rejected",
                    _ => "pending_approval",
                };
                sqlx::query(
                    "UPDATE leave_cancellation_reviews SET state=$2,decided_by=$3,\
                     decision_note=$4,version=version+1,updated_at=$5 \
                     WHERE request_id=$1 AND state='pending_approval'",
                )
                .bind(command.id)
                .bind(review_state)
                .bind(decided_by)
                .bind(command.note)
                .bind(command.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            Ok(())
        })
    }

    fn cancel_leave<'a>(
        &'a mut self,
        command: &'a LeaveCancelWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE leave_requests SET state='cancelled',decision_note=$3,\
                 version=version+1,last_command_id=$4,updated_at=$5 \
                 WHERE id=$1 AND company_id=$2 AND version=$6 \
                 AND state IN ('draft','rejected','pending_approval')",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.note)
            .bind(command.key)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "LEAVE_CHANGED",
                    "This leave request changed. Reload before cancelling.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn record_evidence_manifest<'a>(
        &'a mut self,
        command: &'a LeaveEvidenceManifestWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO leave_evidence_manifests(id,request_id,company_id,\
                 approval_instance_id,decision_command_id,request_version,policy_version_id,\
                 reviewer_user_id,manifest_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,CAST($9 AS text)::jsonb,$10)",
            )
            .bind(command.id)
            .bind(command.request_id)
            .bind(command.company_id)
            .bind(command.approval_instance_id)
            .bind(command.decision_command_id)
            .bind(command.request_version as i32)
            .bind(command.policy_version_id)
            .bind(command.reviewer_user_id)
            .bind(command.manifest_json)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn evidence_policy_rule<'a>(
        &'a mut self,
        policy_version_id: &'a str,
        leave_type_id: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveEvidencePolicyRule>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT evidence_min_units::float8 evidence_min_units,evidence_purpose,evidence_min_count,\
                 evidence_due_stage,evidence_grace_days,evidence_note,evidence_privacy \
                 FROM leave_policy_rules WHERE policy_version_id=$1 AND leave_type_id=$2",
            )
            .bind(policy_version_id)
            .bind(leave_type_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(LeaveEvidencePolicyRule {
                evidence_min_units: row.try_get("evidence_min_units")?,
                evidence_purpose: row.try_get("evidence_purpose")?,
                evidence_min_count: i64::from(row.try_get::<i32, _>("evidence_min_count")?),
                evidence_due_stage: row.try_get("evidence_due_stage")?,
                evidence_grace_days: i64::from(row.try_get::<i32, _>("evidence_grace_days")?),
                evidence_note: row.try_get("evidence_note")?,
                evidence_privacy: row.try_get("evidence_privacy")?,
            }))
        })
    }

    fn balance<'a>(
        &'a mut self,
        company: &'a CompanyId,
        engagement_id: &'a str,
        leave_type_id: &'a str,
        year: i32,
    ) -> LeaveCommandFuture<'a, Option<LeaveBalance>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT coalesce(sum(units),0)::float8 available,\
                 coalesce(-sum(units) FILTER (WHERE kind IN ('reservation','reservation_release')),0)::float8 reserved,\
                 coalesce(-sum(units) FILTER (WHERE kind IN ('usage','usage_reversal')),0)::float8 used,\
                 coalesce(sum(units) FILTER (WHERE kind='accrual'),0)::float8 accrued \
                 FROM leave_balance_entries WHERE company_id=$1 AND engagement_id=$2 \
                 AND leave_type_id=$3 AND entitlement_year=$4",
            )
            .bind(company.as_str())
            .bind(engagement_id)
            .bind(leave_type_id)
            .bind(year)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(Some(LeaveBalance {
                available: row.try_get("available")?,
                reserved: row.try_get("reserved")?,
                used: row.try_get("used")?,
                accrued: row.try_get("accrued")?,
            }))
        })
    }

    fn insert_balance<'a>(
        &'a mut self,
        command: &'a LeaveBalanceWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql: &'static str =
                if command.action == crate::domain::leave::LeaveBalanceAction::Reverse {
                    "INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,\
                 request_id,kind,units,effective_date,entitlement_year,command_key,note,\
                 created_by,created_at) \
                 SELECT $1||':'||entitlement_year,company_id,engagement_id,leave_type_id,\
                 request_id,$2,sum(units)*$3::numeric,entitlement_year||'-01-01',entitlement_year,\
                 $4||':'||entitlement_year,$5,$6,$7 \
                 FROM leave_balance_entries WHERE request_id=$8 AND company_id=$9 \
                 AND kind IN ('usage','usage_reversal') GROUP BY company_id,engagement_id,\
                 leave_type_id,request_id,entitlement_year HAVING sum(units)<0"
                } else {
                    "INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,\
                 request_id,kind,units,effective_date,entitlement_year,command_key,note,\
                 created_by,created_at) \
                 SELECT $1||':'||entitlement_year,company_id,engagement_id,leave_type_id,\
                 request_id,$2,sum(units)*$3::numeric,entitlement_year||'-01-01',entitlement_year,\
                 $4||':'||entitlement_year,$5,$6,$7 \
                 FROM leave_balance_entries WHERE request_id=$8 AND company_id=$9 \
                 AND kind IN ('reservation','reservation_release') GROUP BY company_id,\
                 engagement_id,leave_type_id,request_id,entitlement_year HAVING sum(units)<0"
                };
            Ok(sqlx::query(sql)
                .bind(command.id)
                .bind(command.kind)
                .bind(command.multiplier)
                .bind(command.key)
                .bind(command.note)
                .bind(command.actor)
                .bind(command.now)
                .bind(command.request_id)
                .bind(command.company_id)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?
                .rows_affected())
        })
    }

    fn accrual_due<'a>(
        &'a mut self,
        effective_date: &'a str,
        company_id: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Vec<LeaveAccrualDue>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT a.company_id,a.engagement_id,r.leave_type_id,a.id assignment_id,\
                 p.id policy_version_id,r.grant_kind,r.grant_units::float8 grant_units,\
                 r.accrual_units::float8 accrual_units,r.accrual_period,\
                 r.carryover_limit::float8 carryover_limit,\
                 greatest(a.effective_from,p.effective_from,e.start_date) first_day,\
                 least(coalesce(a.effective_to,$1),coalesce(p.effective_to,$2),\
                   coalesce(e.end_date,$3),$4) last_day \
                 FROM leave_policy_assignments a \
                 JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id \
                 JOIN leave_policy_rules r ON r.policy_version_id=p.id \
                 JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id \
                 JOIN leave_types t ON t.id=r.leave_type_id AND t.company_id=a.company_id \
                 WHERE p.status IN ('active','retired') AND t.tracked=true \
                   AND a.effective_from<=$5 AND ($6::text IS NULL OR a.company_id=$6) \
                   AND (CASE WHEN e.worker_type='contractor' THEN r.contractor_eligible \
                     ELSE r.employee_eligible END) \
                 ORDER BY first_day,a.id,r.leave_type_id",
            )
            .bind(effective_date)
            .bind(effective_date)
            .bind(effective_date)
            .bind(effective_date)
            .bind(effective_date)
            .bind(company_id)
            .bind(company_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(LeaveAccrualDue {
                        company_id: row.try_get("company_id")?,
                        engagement_id: row.try_get("engagement_id")?,
                        leave_type_id: row.try_get("leave_type_id")?,
                        assignment_id: row.try_get("assignment_id")?,
                        policy_version_id: row.try_get("policy_version_id")?,
                        grant_kind: row.try_get("grant_kind")?,
                        grant_units: row.try_get("grant_units")?,
                        accrual_units: row.try_get("accrual_units")?,
                        accrual_period: row.try_get("accrual_period")?,
                        carryover_limit: row.try_get("carryover_limit")?,
                        first_day: row.try_get("first_day")?,
                        last_day: row.try_get("last_day")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Error::from)
        })
    }

    fn insert_accrual<'a>(
        &'a mut self,
        command: &'a LeaveAccrualWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query(
                "INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,\
                 kind,units,effective_date,command_key,note,created_by,created_at) \
                 SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,'system:leave-accrual',$10 \
                 ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.engagement_id)
            .bind(command.leave_type_id)
            .bind(command.kind)
            .bind(command.units)
            .bind(command.effective_date)
            .bind(command.command_key)
            .bind(command.note)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected())
        })
    }
}

/// Fail-closed reader used before a command exists. It requires only an open
/// caller-owned transaction, never a serializable one.
pub struct PgLeaveReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgLeaveReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn leave_row(
        &mut self,
        company: &CompanyId,
        id: &str,
        user_id: &str,
        evidence_reader: bool,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Option<LeaveRequestRow>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_LEAVE_TRANSACTION_REQUIRED").into());
        }
        let row = sqlx::query(
            "SELECT l.id,l.number,l.employee_id,l.company_id,l.branch_id,l.department_id,l.person_id,\
                 l.engagement_id,l.start_date,l.end_date,l.days,l.calculated_units::float8 calculated_units,\
                 l.handover_note,l.leave_type_id,t.name leave_type_name,l.policy_version_id,l.state,\
                 l.version,l.created_by,l.prepared_by,l.decided_by,l.decision_note,l.cancellation_state,\
                 l.evidence_policy_snapshot,l.evidence_due_at,l.evidence_version,l.submitted_at,\
                 l.created_at,l.updated_at \
             FROM leave_requests l \
             LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id \
             WHERE l.id=$1 AND l.company_id=$2 \
               AND (l.created_by=$3 OR l.decided_by=$3 OR $4::boolean) \
               AND ($5::text IS NULL OR l.branch_id=$5) \
               AND ($6::text IS NULL OR l.department_id=$6)",
        )
        .bind(id)
        .bind(company.as_str())
        .bind(user_id)
        .bind(evidence_reader)
        .bind(branch_id)
        .bind(department_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(map_leave_row).transpose().map_err(Error::from)
    }

    pub async fn balance(
        &mut self,
        company: &CompanyId,
        engagement_id: &str,
        leave_type_id: &str,
        year: i32,
    ) -> Result<Option<LeaveBalance>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_LEAVE_TRANSACTION_REQUIRED").into());
        }
        let row = sqlx::query(
            "SELECT coalesce(sum(units),0)::float8 available,\
             coalesce(-sum(units) FILTER (WHERE kind IN ('reservation','reservation_release')),0)::float8 reserved,\
             coalesce(-sum(units) FILTER (WHERE kind IN ('usage','usage_reversal')),0)::float8 used,\
             coalesce(sum(units) FILTER (WHERE kind='accrual'),0)::float8 accrued \
             FROM leave_balance_entries WHERE company_id=$1 AND engagement_id=$2 \
             AND leave_type_id=$3 AND entitlement_year=$4",
        )
        .bind(company.as_str())
        .bind(engagement_id)
        .bind(leave_type_id)
        .bind(year)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(Some(LeaveBalance {
            available: row.try_get("available")?,
            reserved: row.try_get("reserved")?,
            used: row.try_get("used")?,
            accrued: row.try_get("accrued")?,
        }))
    }
}
