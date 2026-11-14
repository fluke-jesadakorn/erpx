//! PostgreSQL adapter for the Manage ERPX company-onboarding command.
//!
//! Two caller-owned serializable transactions are required: the control plane
//! (`beta_slots`, `beta_waitlist`, principals/bindings/billing) and the selected
//! Holding write database (company provisioning, the bootstrap graph, the
//! command receipt, audit and outbox). The deployment commit order is control
//! reservation, Holding bootstrap, control activation; each phase uses the
//! narrow adapter so the Holding pool is resolved from the claimed slot. The
//! adapter never opens, commits, rolls back or retries a transaction.
//!
//! The bootstrap graph SQL is the native replacement for the retained
//! `onboard()` effect batch (`packages/server-*/src/adapters/beta-onboarding-
//! effects.ts`). Extract owner ports when those producers are migrated; until
//! then this is the single native writer for the bootstrap path.
use crate::application::onboarding_command::{OnboardingBootstrapPort, OnboardingControlPort};
use crate::domain::onboarding_command::{
    OnboardingActivationPlan, OnboardingBootstrapOutcome, OnboardingBootstrapPlan,
    OnboardingClaimOutcome, OnboardingCommandInput, OnboardingCommandReceipt, OnboardingSlotFacts,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
}

async fn ready(connection: &mut PgConnection, scope: &'static str) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new(scope).into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new(scope).into());
    }
    Ok(())
}

fn slot_row(row: &sqlx::postgres::PgRow) -> Result<OnboardingSlotFacts, BusinessError> {
    OnboardingSlotFacts::from_row(
        row.try_get("holding_id").map_err(|_| source_invalid())?,
        row.try_get("slug").map_err(|_| source_invalid())?,
        row.try_get("company_name").map_err(|_| source_invalid())?,
        row.try_get("branch_name").map_err(|_| source_invalid())?,
        row.try_get("state").map_err(|_| source_invalid())?,
    )
}

fn source_invalid() -> BusinessError {
    unavailable(
        "ONBOARDING_SOURCE_INVALID",
        "The reserved company slot is unavailable. Retry the current onboarding state.",
    )
}

const SELECT_SLOT: &str =
    "SELECT holding_id,slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=$1";
const SELECT_ANY_SLOT: &str =
    "SELECT holding_id,slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=$1";
const CLAIM_SLOT: &str = "UPDATE beta_slots SET owner_user_id=$1,slug=$2,company_name=$3,\
branch_name=$4,state='provisioning',updated_at=$5 \
WHERE holding_id=(SELECT holding_id FROM beta_slots WHERE state='available' \
ORDER BY holding_id FOR UPDATE SKIP LOCKED LIMIT 1) \
AND state='available' AND NOT EXISTS(SELECT 1 FROM beta_slots WHERE owner_user_id=$6) \
RETURNING holding_id,slug,company_name,branch_name,state";

pub struct PgOnboardingControl<'a> {
    control: &'a mut PgConnection,
}

impl<'a> PgOnboardingControl<'a> {
    pub fn new(control: &'a mut PgConnection) -> Self {
        Self { control }
    }
}

pub struct PgOnboardingBootstrap<'a> {
    holding: &'a mut PgConnection,
}

impl<'a> PgOnboardingBootstrap<'a> {
    pub fn new(holding: &'a mut PgConnection) -> Self {
        Self { holding }
    }

    async fn provisioning_status(
        &mut self,
        company: &str,
        holding: &str,
    ) -> Result<Option<String>, Error> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT status FROM company_provisioning WHERE company_id=$1 AND holding_id=$2",
        )
        .bind(company)
        .bind(holding)
        .fetch_optional(&mut *self.holding)
        .await?)
    }

    async fn graph(&mut self, plan: &OnboardingBootstrapPlan) -> Result<(), Error> {
        let holding = plan.holding_id.as_str();
        let company = plan.company_id.as_str();
        let branch = plan.branch_id.as_str();
        let department = plan.department_id.as_str();
        let owner = plan.owner_user_id.as_str();
        let user = &mut *self.holding;

        // Retained bootstrap-owner assertion: no other bootstrap may exist and
        // an existing bootstrap must be this Holding/owner. The CHECK(ok=1)
        // makes the insert itself the fence.
        sqlx::query(
            "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'bootstrap-owner',\
CASE WHEN (NOT EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id<>$2 OR owner_user_id<>$3) \
AND (EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id=$4 AND owner_user_id=$5) \
OR NOT EXISTS(SELECT 1 FROM organization_nodes))) THEN 1 ELSE 0 END)",
        )
        .bind(&plan.bootstrap_key)
        .bind(holding)
        .bind(owner)
        .bind(holding)
        .bind(owner)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO company_provisioning(company_id,holding_id,owner_user_id,status,version,\
requested_at,updated_at) VALUES($1,$2,$3,'provisioning',1,$4,$5) \
ON CONFLICT(company_id) DO NOTHING",
        )
        .bind(company)
        .bind(holding)
        .bind(owner)
        .bind(plan.requested_at)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,\
actor_id,detail_json,created_at) VALUES($1,$2,$3,'started',1,$4,$5,$6) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.started_event_id)
        .bind(company)
        .bind(holding)
        .bind(owner)
        .bind(
            serde_json::json!({
                "companyName": plan.company_name,
                "branchName": plan.branch_name,
                "slug": plan.slug,
            })
            .to_string(),
        )
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) \
VALUES(1,$1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(holding)
        .bind(owner)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO beta_members(user_id,invitation_id,joined_at) VALUES($1,NULL,$2) \
ON CONFLICT DO NOTHING",
        )
        .bind(owner)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,\
storage_bytes,created_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
        )
        .bind(company)
        .bind(plan.limits.members)
        .bind(plan.limits.branches)
        .bind(plan.limits.ai)
        .bind(plan.limits.storage)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        // Organization graph: company, branch, department.
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) \
VALUES($1,'company',$2,$3,$4) ON CONFLICT DO NOTHING",
        )
        .bind(company)
        .bind(&plan.slug)
        .bind(&plan.company_name)
        .bind(company)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,\
branch_id) VALUES($1,'branch','main',$2,$3,$4,$5) ON CONFLICT DO NOTHING",
        )
        .bind(branch)
        .bind(&plan.branch_name)
        .bind(company)
        .bind(company)
        .bind(branch)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,\
branch_id) VALUES($1,'department','team','Team',$2,$3,$4) ON CONFLICT DO NOTHING",
        )
        .bind(department)
        .bind(branch)
        .bind(company)
        .bind(branch)
        .execute(&mut *user)
        .await?;

        // Staff levels: one member level and the numbered organization levels.
        sqlx::query(
            "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,\
updated_at) VALUES($1,$2,'member','Team member',1,NULL,$3,$4) ON CONFLICT DO NOTHING",
        )
        .bind(&plan.member_level_id)
        .bind(company)
        .bind(plan.requested_at)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;
        const LEVELS: [(&str, &str, i64); 6] = [
            ("ORG_L1", "CEO", -1),
            ("ORG_L2", "Executive / Director", -2),
            ("ORG_L3", "Branch / Department Manager", -3),
            ("ORG_L4", "Supervisor", -4),
            ("ORG_L5", "Senior Staff", -5),
            ("ORG_L6", "Staff", -6),
        ];
        for (index, (code, name, rank)) in LEVELS.iter().enumerate() {
            sqlx::query(
                "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,\
updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
ON CONFLICT(company_id,code) DO UPDATE SET name=excluded.name,rank=excluded.rank,\
level_number=excluded.level_number,active=1,updated_at=excluded.updated_at",
            )
            .bind(&plan.organization_level_ids[index])
            .bind(company)
            .bind(*code)
            .bind(*name)
            .bind(*rank)
            .bind(index as i64 + 1)
            .bind(plan.requested_at)
            .bind(plan.requested_at)
            .execute(&mut *user)
            .await?;
        }

        sqlx::query(
            "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,\
level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) \
VALUES($1,$2,NULL,NULL,'Chief Executive Officer',$3,NULL,1,1,1,$4,$5,$6) \
ON CONFLICT(id) DO UPDATE SET level_id=excluded.level_id,is_ceo=1,active=1,\
version=organization_positions.version+1,updated_at=excluded.updated_at",
        )
        .bind(&plan.ceo_position_id)
        .bind(company)
        .bind(&plan.organization_level_ids[0])
        .bind(owner)
        .bind(plan.requested_at)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;

        // People, employment and the owner's holding assignment.
        sqlx::query(
            "INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,\
created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,'TH',$6,$7,$8) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.person_id)
        .bind(owner)
        .bind(company)
        .bind(&plan.owner_name)
        .bind(&plan.owner_email)
        .bind(owner)
        .bind(plan.requested_at)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,\
created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        )
        .bind(&plan.employee_id)
        .bind(owner)
        .bind(company)
        .bind(branch)
        .bind(department)
        .bind(&plan.owner_name)
        .bind(plan.requested_at)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,\
worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,\
updated_at,level_id) VALUES($1,$2,$3,$4,$5,'employee','salaried','active',\
to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'),'Team member','THB',$6,$7,$8,$9) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.engagement_id)
        .bind(&plan.person_id)
        .bind(company)
        .bind(branch)
        .bind(department)
        .bind(owner)
        .bind(plan.requested_at)
        .bind(plan.requested_at)
        .bind(&plan.member_level_id)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,\
valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) \
VALUES($1,$2,'holding_owner','[]',$3,NULL,$4,$5,'holding',NULL,NULL,'legacy') \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.assignment_id)
        .bind(owner)
        .bind(company)
        .bind(plan.requested_at)
        .bind(plan.assignment_valid_until())
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "UPDATE company_provisioning SET status='active',version=2,completed_at=$1,\
updated_at=$2,last_error=NULL WHERE company_id=$3 AND holding_id=$4",
        )
        .bind(plan.completed_at)
        .bind(plan.completed_at)
        .bind(company)
        .bind(holding)
        .execute(&mut *user)
        .await?;
        sqlx::query(
            "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,\
actor_id,detail_json,created_at) VALUES($1,$2,$3,'completed',2,$4,$5,$6) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.completed_event_id)
        .bind(company)
        .bind(holding)
        .bind(owner)
        .bind(
            serde_json::json!({
                "levels": 6,
                "ceoPositions": 1,
                "branches": 1,
                "departments": 1,
                "owner": owner,
            })
            .to_string(),
        )
        .bind(plan.completed_at)
        .execute(&mut *user)
        .await?;
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(&plan.bootstrap_key)
            .execute(&mut *user)
            .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        plan: &OnboardingBootstrapPlan,
        receipt: &OnboardingCommandReceipt,
    ) -> Result<(), Error> {
        let user = &mut *self.holding;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
result_json,created_at,authority_json) \
VALUES($1,'manage_erpx.onboarding.bootstrap',$2,$3,$4,$5,$6,$7) \
ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&plan.command_id)
        .bind(plan.owner_user_id.as_str())
        .bind(plan.request_hash())
        .bind(&plan.company_id)
        .bind(receipt.value().to_string())
        .bind(plan.completed_at)
        .bind(&plan.authority_json)
        .execute(&mut *user)
        .await
        .map_err(|error| {
            if unique_violation(&error) {
                conflict(
                    "ONBOARDING_CONCURRENT_COMMAND",
                    "This company setup is already completing. Refresh to see its current state.",
                )
                .into()
            } else {
                Error::from(error)
            }
        })?
        .rows_affected();
        if inserted != 1 {
            return Err(conflict(
                "ONBOARDING_CONCURRENT_COMMAND",
                "This company setup is already completing. Refresh to see its current state.",
            )
            .into());
        }

        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
record_id,record_version,purpose,created_at,authority_json) \
VALUES($1,$2,$3,$4,'onboarding.company.provisioned','company_provisioning',$5,2,\
'company_onboarding',$6,$7)",
        )
        .bind(format!("{}-audit", plan.command_id))
        .bind(&plan.command_id)
        .bind(plan.owner_user_id.as_str())
        .bind(&plan.assignment_id)
        .bind(&plan.company_id)
        .bind(plan.completed_at)
        .bind(&plan.authority_json)
        .execute(&mut *user)
        .await?;

        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
aggregate_version,payload_json,created_at) \
VALUES($1,$2,'company.provisioned','company_provisioning',$3,2,$4,$5)",
        )
        .bind(format!("{}-outbox", plan.command_id))
        .bind(&plan.command_id)
        .bind(&plan.company_id)
        .bind(
            serde_json::json!({
                "companyId": plan.company_id,
                "holdingId": plan.holding_id,
                "slug": plan.slug,
                "ownerUserId": plan.owner_user_id.as_str(),
                "state": "active",
                "version": 2,
            })
            .to_string(),
        )
        .bind(plan.completed_at)
        .execute(&mut *user)
        .await?;
        Ok(())
    }
}

impl OnboardingBootstrapPort for PgOnboardingBootstrap<'_> {
    type Error = Error;

    async fn bootstrap(
        &mut self,
        plan: &OnboardingBootstrapPlan,
    ) -> Result<OnboardingBootstrapOutcome, Self::Error> {
        ready(
            &mut *self.holding,
            "ONBOARDING_BOOTSTRAP_TRANSACTION_REQUIRED",
        )
        .await?;
        let stored = sqlx::query(
            "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(&plan.command_id)
        .fetch_optional(&mut *self.holding)
        .await?;
        if let Some(row) = stored {
            let request_hash: String = row.try_get("request_hash")?;
            if request_hash != plan.request_hash() {
                return Err(conflict(
                    "ONBOARDING_REPLAY_CONFLICT",
                    "This company setup receipt belongs to different details. Refresh and review again.",
                )
                .into());
            }
            let result_json: Option<String> = row.try_get("result_json")?;
            let value: Value = result_json
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .ok_or_else(|| {
                    unavailable(
                        "ONBOARDING_RECEIPT_INVALID",
                        "The company setup receipt is unavailable. Retry the current onboarding state.",
                    )
                })?;
            let receipt_valid = OnboardingCommandReceipt::from_receipt_json(&value, &plan.href);
            if receipt_valid.is_none() {
                return Err(unavailable(
                    "ONBOARDING_RECEIPT_INVALID",
                    "The company setup receipt is unavailable. Retry the current onboarding state.",
                )
                .into());
            }
            let completed = self
                .provisioning_status(&plan.company_id, &plan.holding_id)
                .await?
                .as_deref()
                == Some("active");
            return Ok(OnboardingBootstrapOutcome {
                completed,
                replayed: true,
            });
        }

        let already_active = self
            .provisioning_status(&plan.company_id, &plan.holding_id)
            .await?
            .as_deref()
            == Some("active");
        if !already_active {
            self.graph(plan).await?;
        }
        let completed = self
            .provisioning_status(&plan.company_id, &plan.holding_id)
            .await?
            .as_deref()
            == Some("active");
        if !completed {
            return Ok(OnboardingBootstrapOutcome {
                completed: false,
                replayed: false,
            });
        }
        self.retain(plan, &OnboardingCommandReceipt::active(&plan.slug))
            .await?;
        Ok(OnboardingBootstrapOutcome {
            completed: true,
            replayed: false,
        })
    }
}

impl OnboardingControlPort for PgOnboardingControl<'_> {
    type Error = Error;

    async fn owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        let row = sqlx::query(SELECT_SLOT)
            .bind(user_id.as_str())
            .fetch_optional(&mut *self.control)
            .await?;
        row.as_ref().map(slot_row).transpose().map_err(Into::into)
    }

    async fn current_owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        let row = sqlx::query(SELECT_ANY_SLOT)
            .bind(user_id.as_str())
            .fetch_optional(&mut *self.control)
            .await?;
        row.as_ref().map(slot_row).transpose().map_err(Into::into)
    }

    async fn slug_binding(&mut self, slug: &str) -> Result<bool, Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE slug=$1)",
        )
        .bind(slug)
        .fetch_one(&mut *self.control)
        .await?)
    }

    async fn claim_slot(
        &mut self,
        user_id: &UserId,
        input: &OnboardingCommandInput,
        now: i64,
    ) -> Result<OnboardingClaimOutcome, Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        match sqlx::query(CLAIM_SLOT)
            .bind(user_id.as_str())
            .bind(&input.slug)
            .bind(&input.company_name)
            .bind(&input.branch_name)
            .bind(now)
            .bind(user_id.as_str())
            .fetch_optional(&mut *self.control)
            .await
        {
            Ok(Some(row)) => Ok(OnboardingClaimOutcome::Claimed(slot_row(&row)?)),
            Ok(None) => Ok(OnboardingClaimOutcome::Unavailable),
            Err(error) if unique_violation(&error) => Ok(OnboardingClaimOutcome::Conflict),
            Err(error) => Err(error.into()),
        }
    }

    async fn waitlist(&mut self, user_id: &UserId, now: i64) -> Result<(), Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        sqlx::query(
            "INSERT INTO beta_waitlist(user_id,created_at) VALUES($1,$2) \
ON CONFLICT DO NOTHING",
        )
        .bind(user_id.as_str())
        .bind(now)
        .execute(&mut *self.control)
        .await?;
        Ok(())
    }

    async fn activate(&mut self, plan: &OnboardingActivationPlan) -> Result<(), Self::Error> {
        ready(
            &mut *self.control,
            "ONBOARDING_CONTROL_TRANSACTION_REQUIRED",
        )
        .await?;
        let user = &mut *self.control;
        sqlx::query(
            "INSERT INTO principals(id,user_id,display_name,created_at) VALUES($1,$2,$3,$4) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.principal_id)
        .bind(plan.user_id.as_str())
        .bind(&plan.display_name)
        .bind(plan.now)
        .execute(&mut *user)
        .await?;
        let binding = sqlx::query(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,\
files_binding,status) VALUES($1,$2,$3,$4,$5,$6,'active') \
ON CONFLICT(holding_id) DO NOTHING",
        )
        .bind(&plan.holding_id)
        .bind(&plan.slug)
        .bind(&plan.company_name)
        .bind(&plan.write_binding)
        .bind(&plan.read_binding)
        .bind(&plan.files_binding)
        .execute(&mut *user)
        .await;
        if let Err(error) = binding {
            if unique_violation(&error) {
                return Err(conflict("SLUG_UNAVAILABLE", "Choose another company URL.").into());
            }
            return Err(error.into());
        }
        sqlx::query(
            "INSERT INTO beta_billing(holding_id,updated_at) VALUES($1,$2) \
ON CONFLICT DO NOTHING",
        )
        .bind(&plan.holding_id)
        .bind(plan.now)
        .execute(&mut *user)
        .await?;
        let changed = sqlx::query(
            "UPDATE beta_slots SET state='active',updated_at=$3 \
WHERE holding_id=$1 AND owner_user_id=$2",
        )
        .bind(&plan.holding_id)
        .bind(plan.user_id.as_str())
        .bind(plan.now)
        .execute(&mut *user)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict(
                "ONBOARDING_SLOT_CHANGED",
                "The reserved company changed. Refresh and try again.",
            )
            .into());
        }
        Ok(())
    }
}

/// Convenience adapter for callers that already hold both caller-owned
/// transactions. Phase-by-phase callers use [`PgOnboardingControl`] and
/// [`PgOnboardingBootstrap`] so the Holding database can be selected after the
/// control-plane claim.
pub struct PgOnboardingCommand<'a> {
    control: PgOnboardingControl<'a>,
    holding: PgOnboardingBootstrap<'a>,
}

impl<'a> PgOnboardingCommand<'a> {
    pub fn new(control: &'a mut PgConnection, holding: &'a mut PgConnection) -> Self {
        Self {
            control: PgOnboardingControl::new(control),
            holding: PgOnboardingBootstrap::new(holding),
        }
    }
}

impl OnboardingControlPort for PgOnboardingCommand<'_> {
    type Error = Error;

    async fn owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        self.control.owner_slot(user_id).await
    }

    async fn current_owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        self.control.current_owner_slot(user_id).await
    }

    async fn slug_binding(&mut self, slug: &str) -> Result<bool, Self::Error> {
        self.control.slug_binding(slug).await
    }

    async fn claim_slot(
        &mut self,
        user_id: &UserId,
        input: &OnboardingCommandInput,
        now: i64,
    ) -> Result<OnboardingClaimOutcome, Self::Error> {
        self.control.claim_slot(user_id, input, now).await
    }

    async fn waitlist(&mut self, user_id: &UserId, now: i64) -> Result<(), Self::Error> {
        self.control.waitlist(user_id, now).await
    }

    async fn activate(&mut self, plan: &OnboardingActivationPlan) -> Result<(), Self::Error> {
        self.control.activate(plan).await
    }
}

impl OnboardingBootstrapPort for PgOnboardingCommand<'_> {
    type Error = Error;

    async fn bootstrap(
        &mut self,
        plan: &OnboardingBootstrapPlan,
    ) -> Result<OnboardingBootstrapOutcome, Self::Error> {
        self.holding.bootstrap(plan).await
    }
}
