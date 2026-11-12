//! PostgreSQL adapter for native People workforce commands.
//!
//! The caller owns one serializable Holding transaction and fresh Account/IAM
//! authority. This adapter writes only People-owned person, engagement,
//! compensation and legacy employee-mirror rows plus the standard command
//! receipt/audit/outbox ledger. Organization, Identity and Finance effects are
//! supplied through the composed ports; their SQL is never reached here.
use crate::application::workforce::{
    CompensationInsert, CompensationMoneyPort, CompensationMoneyRequest,
    CompensationTransitionWrite, EngagementInsert, EngagementTransitionWrite,
    PersonAccountLinkPort, PersonAccountLinkRequest, PersonInsert, PersonRecordInsert,
    PersonStatusWrite, PersonUpdate, StaffPlacementPort, StaffPlacementRequest,
    StoredWorkforceReceipt, WorkforceCommandFuture, WorkforceCommandRepository,
    WorkforceLedgerEntry,
};
use crate::domain::workforce::{
    CompensationRow, EngagementRow, PersonStatusFacts, WorkforceCommandActor,
    WorkforceCompensationSummary, WorkforcePersonRow, idempotency_conflict,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
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
            "The People record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgWorkforceCommand<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgWorkforceCommand<'c> {
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
            return Err(PersistenceError::new("PEOPLE_WORKFORCE_TRANSACTION_REQUIRED").into());
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
            return Err(PersistenceError::new("PEOPLE_WORKFORCE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &WorkforceLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result)
            .map_err(|_| Error::Business(crate::domain::workforce::request_invalid()))?;
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
        .bind(format!("people-workforce-audit-{}", entry.key))
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
        let payload = json!({
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
        .bind(format!("people-workforce-outbox-{}", entry.key))
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

    async fn select_person(
        &mut self,
        company: &CompanyId,
        person_id: &str,
        engagement_id: Option<&str>,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Option<WorkforcePersonRow>, Error> {
        select_person_row(
            &mut *self.connection,
            company,
            person_id,
            engagement_id,
            branch_id,
            department_id,
        )
        .await
    }

    async fn select_engagement(
        &mut self,
        company: &CompanyId,
        engagement_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Option<EngagementRow>, Error> {
        let row = sqlx::query(
            "SELECT e.id,e.person_id,e.company_id,e.branch_id,e.department_id,e.worker_type,\
             e.employment_type,e.status,e.start_date,e.end_date,e.manager_person_id,e.job_title,\
             e.currency,e.pay_group_id,e.version,e.level_id,e.position_id,p.user_id,p.status AS person_status \
             FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id \
             WHERE e.id=$1 AND e.company_id=$2 \
               AND ($3::text IS NULL OR e.branch_id=$3) \
               AND ($4::text IS NULL OR e.department_id=$4)",
        )
        .bind(engagement_id)
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(EngagementRow {
            id: row.try_get("id")?,
            person_id: row.try_get("person_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            worker_type: row.try_get("worker_type")?,
            employment_type: row.try_get("employment_type")?,
            status: row.try_get("status")?,
            start_date: row.try_get("start_date")?,
            end_date: row.try_get("end_date")?,
            manager_person_id: row.try_get("manager_person_id")?,
            job_title: row.try_get("job_title")?,
            currency: row.try_get("currency")?,
            pay_group_id: row.try_get("pay_group_id")?,
            version: row.try_get("version")?,
            person_user_id: row.try_get("user_id")?,
            person_status: row.try_get("person_status")?,
            level_id: row.try_get("level_id")?,
            position_id: row.try_get("position_id")?,
        }))
    }
}

async fn select_person_row(
    connection: &mut PgConnection,
    company: &CompanyId,
    person_id: &str,
    engagement_id: Option<&str>,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<Option<WorkforcePersonRow>, Error> {
    let row = sqlx::query(
            "SELECT p.id,p.company_id,p.user_id,p.legal_name,p.preferred_name,p.email,p.phone,\
             p.work_email,p.work_phone,p.country_code,p.status,p.version,p.created_by,\
             e.id AS engagement_id,e.version AS engagement_version,e.branch_id,e.department_id,\
             e.job_title,e.worker_type,e.employment_type,e.start_date,e.end_date,e.currency,\
             e.status AS engagement_status,e.manager_person_id,e.pay_group_id,e.position_id,\
             e.level_id,l.name AS level_name,l.level_number \
             FROM hr_people p \
             LEFT JOIN LATERAL (SELECT x.* FROM hr_engagements x \
               WHERE x.person_id=p.id AND x.company_id=p.company_id AND x.worker_type='employee' \
                 AND ($3::text IS NULL OR x.id=$3) \
               ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,\
                 x.start_date DESC,x.id LIMIT 1) e ON true \
             LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id \
             WHERE p.id=$1 AND p.company_id=$2 AND p.merged_into_id IS NULL AND e.id IS NOT NULL \
               AND ($4::text IS NULL OR e.branch_id=$4) \
               AND ($5::text IS NULL OR e.department_id=$5)",
        )
        .bind(person_id)
        .bind(company.as_str())
        .bind(engagement_id)
        .bind(branch_id)
        .bind(department_id)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(WorkforcePersonRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        user_id: row.try_get("user_id")?,
        legal_name: row.try_get("legal_name")?,
        preferred_name: row.try_get("preferred_name")?,
        email: row.try_get("email")?,
        phone: row.try_get("phone")?,
        work_email: row.try_get("work_email")?,
        work_phone: row.try_get("work_phone")?,
        country_code: row.try_get("country_code")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        engagement_id: row.try_get("engagement_id")?,
        engagement_version: row.try_get("engagement_version")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        job_title: row.try_get("job_title")?,
        worker_type: row.try_get("worker_type")?,
        employment_type: row.try_get("employment_type")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
        currency: row.try_get("currency")?,
        engagement_status: row.try_get("engagement_status")?,
        manager_person_id: row.try_get("manager_person_id")?,
        pay_group_id: row.try_get("pay_group_id")?,
        position_id: row.try_get("position_id")?,
        level_id: row.try_get("level_id")?,
        level_name: row.try_get("level_name")?,
        level_number: row.try_get("level_number")?,
    }))
}

impl WorkforceCommandRepository for PgWorkforceCommand<'_> {
    type Error = Error;

    fn lock_command<'b>(&'b mut self, key: &'b str) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("people-workforce-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'b>(
        &'b mut self,
        identity: &'b str,
    ) -> WorkforceCommandFuture<'b, String, Self::Error> {
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
    ) -> WorkforceCommandFuture<'b, Option<StoredWorkforceReceipt>, Self::Error> {
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
                        "PEOPLE_RECEIPT_INVALID",
                        "The People command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredWorkforceReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'b>(
        &'b mut self,
        entry: &'b WorkforceLedgerEntry<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn person<'b>(
        &'b mut self,
        company: &'b CompanyId,
        person_id: &'b str,
        engagement_id: Option<&'b str>,
        branch_id: Option<&'b str>,
        department_id: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<WorkforcePersonRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            self.select_person(company, person_id, engagement_id, branch_id, department_id)
                .await
        })
    }

    fn person_exists<'b>(
        &'b mut self,
        company: &'b CompanyId,
        person_id: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM hr_people WHERE id=$1 AND company_id=$2 \
                 AND status!='archived' AND merged_into_id IS NULL)",
            )
            .bind(person_id)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn person_status_facts<'b>(
        &'b mut self,
        company: &'b CompanyId,
        person_id: &'b str,
    ) -> WorkforceCommandFuture<'b, Option<PersonStatusFacts>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT p.status,p.version,p.user_id,\
                 (SELECT x.status FROM hr_engagements x WHERE x.person_id=p.id \
                   AND x.company_id=p.company_id AND x.worker_type='employee' \
                   ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,\
                     x.start_date DESC,x.id LIMIT 1) AS engagement_status,\
                 EXISTS(SELECT 1 FROM hr_engagements x WHERE x.person_id=p.id \
                   AND x.company_id=p.company_id AND x.status!='ended') AS has_non_ended_engagements,\
                 EXISTS(SELECT 1 FROM people_access_cutoffs c WHERE c.company_id=p.company_id \
                   AND c.person_id=p.id AND c.cancelled_at IS NULL AND c.restored_at IS NULL) \
                   AS has_open_access_cutoff \
                 FROM hr_people p WHERE p.id=$1 AND p.company_id=$2 AND p.merged_into_id IS NULL",
            )
            .bind(person_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(PersonStatusFacts {
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                user_id: row.try_get("user_id")?,
                engagement_status: row.try_get("engagement_status")?,
                has_non_ended_engagements: row.try_get("has_non_ended_engagements")?,
                has_open_access_cutoff: row.try_get("has_open_access_cutoff")?,
            }))
        })
    }

    fn email_available<'b>(
        &'b mut self,
        company: &'b CompanyId,
        email: &'b str,
        person_id: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=$1 \
                 AND lower(email)=lower($2) AND id<>$3 AND merged_into_id IS NULL)",
            )
            .bind(company.as_str())
            .bind(email)
            .bind(person_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn create_person<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b PersonInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO hr_people(id,company_id,legal_name,preferred_name,email,phone,\
                 work_email,work_phone,country_code,user_id,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$12)",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(&command.profile.legal_name)
            .bind(&command.profile.preferred_name)
            .bind(&command.profile.email)
            .bind(&command.profile.phone)
            .bind(
                command
                    .profile
                    .work_email
                    .as_ref()
                    .and_then(|value| value.as_deref()),
            )
            .bind(
                command
                    .profile
                    .work_phone
                    .as_ref()
                    .and_then(|value| value.as_deref()),
            )
            .bind(&command.profile.country_code)
            .bind(command.linked_user_id)
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            sqlx::query(
                "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,\
                 worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,\
                 level_id,position_id,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'onboarding',$8,$9,$10,$11,$12,$13,$14,$15,$15)",
            )
            .bind(command.engagement_id)
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.profile.branch_id.as_str())
            .bind(command.profile.department_id.as_ref().map(|id| id.as_str()))
            .bind(&command.profile.worker_type)
            .bind(&command.profile.employment_type)
            .bind(&command.profile.start_date)
            .bind(command.profile.manager_person_id.as_ref().map(|id| id.as_str()))
            .bind(&command.profile.job_title)
            .bind(&command.profile.currency)
            .bind(command.placement.level_id.as_deref())
            .bind(command.placement.position_id.as_deref())
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            sqlx::query(
                "INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,\
                 created_at,department_id,person_id) VALUES($1,$2,$3,$4,$5,'active',$6,$7,$8)",
            )
            .bind(command.employee_id)
            .bind(command.linked_user_id)
            .bind(company.as_str())
            .bind(command.profile.branch_id.as_str())
            .bind(&command.profile.legal_name)
            .bind(command.now)
            .bind(command.profile.department_id.as_ref().map(|id| id.as_str()))
            .bind(command.id)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn create_person_record<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b PersonRecordInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO hr_people(id,company_id,legal_name,preferred_name,email,phone,\
                 country_code,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(&command.input.legal_name)
            .bind(&command.input.preferred_name)
            .bind(&command.input.email)
            .bind(&command.input.phone)
            .bind(&command.input.country_code)
            .bind(&self.user_id)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn update_person<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b PersonUpdate<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let person = sqlx::query(
                "UPDATE hr_people SET legal_name=$3,preferred_name=$4,email=$5,phone=$6,\
                 work_email=$7,work_phone=$8,country_code=$9,user_id=$10,version=version+1,\
                 updated_at=$11 WHERE id=$1 AND company_id=$2 AND version=$12 \
                 AND merged_into_id IS NULL",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(&command.profile.legal_name)
            .bind(&command.profile.preferred_name)
            .bind(&command.profile.email)
            .bind(&command.profile.phone)
            .bind(command.work_email)
            .bind(command.work_phone)
            .bind(&command.profile.country_code)
            .bind(command.linked_user_id)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if person != 1 {
                return Err(
                    conflict("PERSON_CHANGED", "Reload the person before saving changes.").into(),
                );
            }
            let engagement = sqlx::query(
                "UPDATE hr_engagements SET branch_id=$3,department_id=$4,job_title=$5,\
                 worker_type=$6,employment_type=$7,start_date=$8,currency=$9,\
                 manager_person_id=$10,level_id=$11,position_id=$12,version=version+1,\
                 updated_at=$13 WHERE id=$1 AND company_id=$2 AND version=$14",
            )
            .bind(command.engagement_id)
            .bind(company.as_str())
            .bind(command.profile.branch_id.as_str())
            .bind(command.profile.department_id.as_ref().map(|id| id.as_str()))
            .bind(&command.profile.job_title)
            .bind(&command.profile.worker_type)
            .bind(&command.profile.employment_type)
            .bind(&command.profile.start_date)
            .bind(&command.profile.currency)
            .bind(
                command
                    .profile
                    .manager_person_id
                    .as_ref()
                    .map(|id| id.as_str()),
            )
            .bind(command.placement.level_id.as_deref())
            .bind(command.placement.position_id.as_deref())
            .bind(command.now)
            .bind(command.expected_engagement_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if engagement != 1 {
                return Err(conflict(
                    "ENGAGEMENT_CHANGED",
                    "Reload the engagement before saving changes.",
                )
                .into());
            }
            sqlx::query(
                "UPDATE employees SET display_name=$3,branch_id=$4,department_id=$5,user_id=$6 \
                 WHERE person_id=$1 AND company_id=$2",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(&command.profile.legal_name)
            .bind(command.profile.branch_id.as_str())
            .bind(command.profile.department_id.as_ref().map(|id| id.as_str()))
            .bind(command.linked_user_id)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn set_person_status<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b PersonStatusWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let person = sqlx::query(
                "UPDATE hr_people SET status=$3,version=version+1,updated_at=$4 \
                 WHERE id=$1 AND company_id=$2 AND version=$5 AND merged_into_id IS NULL",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.status)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if person != 1 {
                return Err(conflict(
                    "PERSON_CHANGED",
                    "Reload the person before changing their status.",
                )
                .into());
            }
            sqlx::query(
                "UPDATE hr_engagements SET status=CASE WHEN $3='active' THEN 'active' ELSE 'suspended' END,\
                 version=version+1,updated_at=$4 WHERE person_id=$1 AND company_id=$2 AND status!='ended'",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.status)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            sqlx::query(
                "UPDATE employees SET status=CASE WHEN $3='active' THEN 'active' ELSE 'inactive' END \
                 WHERE person_id=$1 AND company_id=$2",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.status)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn engagement<'b>(
        &'b mut self,
        company: &'b CompanyId,
        engagement_id: &'b str,
        branch_id: Option<&'b str>,
        department_id: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<EngagementRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            self.select_engagement(company, engagement_id, branch_id, department_id)
                .await
        })
    }

    fn create_engagement<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b EngagementInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,\
                 worker_type,employment_type,status,start_date,end_date,manager_person_id,job_title,\
                 currency,pay_group_id,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'onboarding',$8,$9,$10,$11,$12,$13,$14,$15,$15)",
            )
            .bind(command.id)
            .bind(command.input.person_id.as_str())
            .bind(company.as_str())
            .bind(command.input.branch_id.as_ref().map(|id| id.as_str()))
            .bind(command.input.department_id.as_ref().map(|id| id.as_str()))
            .bind(&command.input.worker_type)
            .bind(&command.input.employment_type)
            .bind(&command.input.start_date)
            .bind(&command.input.end_date)
            .bind(command.input.manager_person_id.as_ref().map(|id| id.as_str()))
            .bind(&command.input.job_title)
            .bind(&command.input.currency)
            .bind(command.input.pay_group_id.as_ref().map(|id| id.as_str()))
            .bind(command.created_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn transition_engagement<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b EngagementTransitionWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE hr_engagements SET status=$3,end_date=COALESCE($4,end_date),\
                 version=version+1,updated_at=$5 WHERE id=$1 AND company_id=$2 AND version=$6",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.status)
            .bind(command.end_date)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(conflict(
                    "ENGAGEMENT_CHANGED",
                    "Reload the engagement before saving changes.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn compensation<'b>(
        &'b mut self,
        company: &'b CompanyId,
        compensation_id: &'b str,
        branch_id: Option<&'b str>,
        department_id: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<CompensationRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT c.id,c.engagement_id,c.effective_from,c.effective_to,c.currency,c.status,\
                 c.version,c.created_by,e.branch_id,e.department_id \
                 FROM hr_compensation_revisions c \
                 JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=$1 \
                 WHERE c.id=$2 AND ($3::text IS NULL OR e.branch_id=$3) \
                   AND ($4::text IS NULL OR e.department_id=$4)",
            )
            .bind(company.as_str())
            .bind(compensation_id)
            .bind(branch_id)
            .bind(department_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(CompensationRow {
                id: row.try_get("id")?,
                engagement_id: row.try_get("engagement_id")?,
                effective_from: row.try_get("effective_from")?,
                effective_to: row.try_get("effective_to")?,
                currency: row.try_get("currency")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                created_by: row.try_get("created_by")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
            }))
        })
    }

    fn compensation_evaluation_draft<'b>(
        &'b mut self,
        compensation_id: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts \
                 WHERE compensation_id=$1)",
            )
            .bind(compensation_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn overlapping_compensation<'b>(
        &'b mut self,
        engagement_id: &'b str,
        compensation_id: &'b str,
        effective_from: &'b str,
        effective_to: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM hr_compensation_revisions \
                 WHERE engagement_id=$1 AND id<>$2 AND status IN ('active','approved') \
                   AND effective_from<=COALESCE($4,'9999-12-31') \
                   AND COALESCE(effective_to,'9999-12-31')>=$3)",
            )
            .bind(engagement_id)
            .bind(compensation_id)
            .bind(effective_from)
            .bind(effective_to)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn create_compensation<'b>(
        &'b mut self,
        _company: &'b CompanyId,
        command: &'b CompensationInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,\
                 pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,\
                 standard_daily_hours,expected_hours,currency,components_json,status,\
                 created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'draft',$13,$14,$14)",
            )
            .bind(command.id)
            .bind(command.input.engagement_id.as_str())
            .bind(&command.input.effective_from)
            .bind(&command.input.effective_to)
            .bind(&command.input.pay_frequency)
            .bind(command.input.base_amount_minor)
            .bind(command.input.hourly_rate_minor)
            .bind(command.input.daily_rate_minor)
            .bind(command.input.standard_daily_hours)
            .bind(command.input.expected_hours)
            .bind(&command.input.currency)
            .bind(command.components_json)
            .bind(command.created_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn transition_compensation<'b>(
        &'b mut self,
        company: &'b CompanyId,
        command: &'b CompensationTransitionWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE hr_compensation_revisions SET status=$3,approved_by=$4,\
                 version=version+1,updated_at=$5 WHERE id=$1 AND version=$6 \
                 AND EXISTS(SELECT 1 FROM hr_engagements e \
                   WHERE e.id=hr_compensation_revisions.engagement_id AND e.company_id=$2)",
            )
            .bind(command.id)
            .bind(company.as_str())
            .bind(command.status)
            .bind(command.approved_by)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(conflict(
                    "COMPENSATION_CHANGED",
                    "Reload the compensation revision before changing it.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn person_compensation<'b>(
        &'b mut self,
        engagement_id: &'b str,
    ) -> WorkforceCommandFuture<'b, Option<WorkforceCompensationSummary>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,\
                 pay_frequency,effective_from,effective_to,currency,status,version \
                 FROM hr_compensation_revisions WHERE engagement_id=$1 AND status='approved' \
                   AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                   AND (effective_to IS NULL OR \
                        effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
                 ORDER BY effective_from DESC,created_at DESC LIMIT 1",
            )
            .bind(engagement_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(WorkforceCompensationSummary {
                base_amount_minor: row.try_get("base_amount_minor")?,
                hourly_rate_minor: row.try_get("hourly_rate_minor")?,
                daily_rate_minor: row.try_get("daily_rate_minor")?,
                standard_daily_hours: row.try_get("standard_daily_hours")?,
                pay_frequency: row.try_get("pay_frequency")?,
                effective_from: row.try_get("effective_from")?,
                effective_to: row.try_get("effective_to")?,
                currency: row.try_get("currency")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
            }))
        })
    }

    fn apply_placement<'b, P: StaffPlacementPort>(
        &'b mut self,
        placement: &'b mut P,
        request: &'b StaffPlacementRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            placement.apply(&mut *self.connection, request).await?;
            Ok(())
        })
    }

    fn verify_account_link<'b, A: PersonAccountLinkPort>(
        &'b mut self,
        account_link: &'b mut A,
        request: &'b PersonAccountLinkRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            account_link.verify(&mut *self.connection, request).await?;
            Ok(())
        })
    }

    fn revoke_company_access<'b, A: PersonAccountLinkPort>(
        &'b mut self,
        account_link: &'b mut A,
        company_id: &'b CompanyId,
        user_id: &'b str,
        person_id: &'b str,
        now: i64,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            account_link
                .revoke_company_access(&mut *self.connection, company_id, user_id, person_id, now)
                .await?;
            Ok(())
        })
    }

    fn snapshot_compensation_money<'b, M: CompensationMoneyPort>(
        &'b mut self,
        money: &'b mut M,
        request: &'b CompensationMoneyRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            money.snapshot(&mut *self.connection, request).await?;
            Ok(())
        })
    }
}

/// Fail-closed reader used before a command exists. It requires only an open
/// caller-owned transaction, never a serializable one.
pub struct PgWorkforceReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgWorkforceReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn person(
        &mut self,
        company: &CompanyId,
        person_id: &str,
        engagement_id: Option<&str>,
    ) -> Result<Option<WorkforcePersonRow>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_WORKFORCE_TRANSACTION_REQUIRED").into());
        }
        select_person_row(
            &mut *self.connection,
            company,
            person_id,
            engagement_id,
            None,
            None,
        )
        .await
    }

    pub async fn compensation(
        &mut self,
        company: &CompanyId,
        compensation_id: &str,
    ) -> Result<Option<CompensationRow>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_WORKFORCE_TRANSACTION_REQUIRED").into());
        }
        let Some(row) = sqlx::query(
            "SELECT c.id,c.engagement_id,c.effective_from,c.effective_to,c.currency,c.status,\
             c.version,c.created_by,e.branch_id,e.department_id \
             FROM hr_compensation_revisions c \
             JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=$1 \
             WHERE c.id=$2",
        )
        .bind(company.as_str())
        .bind(compensation_id)
        .fetch_optional(&mut *self.connection)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(CompensationRow {
            id: row.try_get("id")?,
            engagement_id: row.try_get("engagement_id")?,
            effective_from: row.try_get("effective_from")?,
            effective_to: row.try_get("effective_to")?,
            currency: row.try_get("currency")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
        }))
    }
}
