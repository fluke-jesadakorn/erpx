//! Work Projection card-preference command over the Workflow command ledger.
use crate::domain::work_card_preferences::{
    SaveWorkCardPreferences, WorkCardPreferences, invalid, member_required, parse_pinned,
    pinned_json, result_wire,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, CommandOutcome, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CARD_PREFERENCES_CHANGED",
        "The saved shortcuts changed while they were being written.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_CARD_PREFERENCES_UNAVAILABLE",
        "The work-card preference is unavailable.",
    )
}

fn optional<'a>(field: &'a CommandField<String>) -> Option<&'a str> {
    match field {
        CommandField::Value(value) => Some(value.as_str()),
        CommandField::Null | CommandField::Omitted => None,
    }
}

struct Prepared {
    pinned_json: Option<String>,
    version: i64,
}

pub struct WorkCardPreferenceEffects<'a, C> {
    clock: &'a C,
    available: &'a [String],
    prepared: Option<Prepared>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for WorkCardPreferenceEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("LOCK TABLE work_card_preferences IN SHARE ROW EXCLUSIVE MODE NOWAIT")
            .execute(&mut *connection)
            .await
            .map_err(
                erpx_server_runtime::public::postgres::PersistenceError::pre_snapshot_lock_conflict,
            )?;
        Ok(())
    }

    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        if command.operation != "work.cards.preferences"
            || command.record_id
                != format!("{}:{}", actor.company_id.as_str(), actor.user_id.as_str())
        {
            return Err(invalid().into());
        }
        let now = self.clock.now().get();
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a \
             WHERE a.company_id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL \
             AND a.valid_from<=$5 AND a.valid_until>$5 \
             AND (a.scope_kind='holding' \
                  OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
                  OR (a.scope_kind='department' AND a.scope_node_id=$4)))",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(optional(&actor.branch_id))
        .bind(optional(&actor.department_id))
        .bind(now)
        .fetch_one(&mut *connection)
        .await?;
        if !member {
            return Err(member_required().into());
        }
        let row = sqlx::query(
            "SELECT pinned_ids_json,version FROM work_card_preferences \
             WHERE company_id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let (current_pins, current_version) = match row {
            Some(row) => (
                parse_pinned(
                    row.try_get::<Option<String>, _>("pinned_ids_json")?
                        .as_deref(),
                )?,
                row.try_get::<i64, _>("version")?,
            ),
            None => (None, 0),
        };
        let input = SaveWorkCardPreferences::parse(command.input.as_str())?;
        let plan = input.validate(
            &WorkCardPreferences {
                pinned_ids: current_pins,
                version: current_version,
            },
            self.available,
        )?;
        let pinned_json = pinned_json(plan.pinned_ids.as_ref())?;
        self.prepared = Some(Prepared {
            pinned_json: pinned_json.clone(),
            version: plan.version,
        });
        Ok(WorkflowOutput {
            version: plan.version,
            result: CommandJson::from_serializable(&result_wire(&plan)?)?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        let now = self.clock.now().get();
        sqlx::query(
            "INSERT INTO work_card_preferences(company_id,user_id,pinned_ids_json,version,updated_at) \
             VALUES($1,$2,$3,$4,$5) \
             ON CONFLICT(company_id,user_id) DO UPDATE SET \
             pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,updated_at=excluded.updated_at",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(&prepared.pinned_json)
        .bind(prepared.version)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        let row: Option<(Option<String>, i64)> = sqlx::query_as(
            "SELECT pinned_ids_json,version FROM work_card_preferences \
             WHERE company_id=$1 AND user_id=$2",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some((pinned, version)) = row else {
            return Err(changed().into());
        };
        if pinned != prepared.pinned_json || version != prepared.version {
            return Err(changed().into());
        }
        Ok(())
    }
}

pub async fn execute_work_card_preferences<A, C, I>(
    company: CompanyId,
    connection: &mut PgConnection,
    authority: &mut A,
    clock: &C,
    identifiers: &mut I,
    command: &WorkflowRequest,
    available: &[String],
) -> Result<CommandOutcome, Error>
where
    A: PgCommandAuthority,
    C: Clock + Sync,
    I: IdentifierSource,
{
    let mut effects = WorkCardPreferenceEffects {
        clock,
        available,
        prepared: None,
    };
    PgCommandLedger::new(company, connection, authority, clock, identifiers)
        .execute(command, &mut effects)
        .await
}

pub async fn read_work_card_preferences(
    connection: &mut PgConnection,
    actor: &CommandActor,
    now: i64,
) -> Result<WorkCardPreferences, Error> {
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a \
         WHERE a.company_id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL \
         AND a.valid_from<=$5 AND a.valid_until>$5 \
         AND (a.scope_kind='holding' \
              OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
              OR (a.scope_kind='department' AND a.scope_node_id=$4)))",
    )
    .bind(actor.company_id.as_str())
    .bind(actor.user_id.as_str())
    .bind(optional(&actor.branch_id))
    .bind(optional(&actor.department_id))
    .bind(now)
    .fetch_one(&mut *connection)
    .await?;
    if !member {
        return Err(member_required().into());
    }
    let row = sqlx::query(
        "SELECT pinned_ids_json,version FROM work_card_preferences \
         WHERE company_id=$1 AND user_id=$2",
    )
    .bind(actor.company_id.as_str())
    .bind(actor.user_id.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    match row {
        Some(row) => Ok(WorkCardPreferences {
            pinned_ids: parse_pinned(
                row.try_get::<Option<String>, _>("pinned_ids_json")?
                    .as_deref(),
            )?,
            version: row.try_get("version")?,
        }),
        None => Ok(WorkCardPreferences {
            pinned_ids: None,
            version: 0,
        }),
    }
}
