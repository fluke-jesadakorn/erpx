//! PostgreSQL adapter for Organization setup commands.
//!
//! The caller owns the Holding transaction, current session proof and Company
//! lease. This adapter re-checks the retained IAM source, template versions and
//! writes the command receipt, audit and outbox rows in the same unit.
use crate::application::{
    organization_nodes::{OrganizationCommandActor, OrganizationFuture},
    organization_setup::{
        ApplyTemplateCommand, InstallLevelsCommand, OrganizationSetupRepository,
        SaveTemplateCommand, TemplateRow,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandEventScope, CommandField, CommandJson, WorkAuthority, command_fingerprint,
};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn command_actor(actor: &OrganizationCommandActor) -> Result<CommandActor, BusinessError> {
    let command = &actor.command;
    Ok(CommandActor {
        user_id: command.user_id.clone(),
        assignment_id: command.assignment_id.clone(),
        role: command.role.clone(),
        company_id: command.company_id.clone(),
        branch_id: command
            .branch_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        department_id: command
            .department_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        authority: command
            .authority
            .as_ref()
            .map(|authority| {
                CommandField::Value(WorkAuthority {
                    session_id: authority.session_id.clone(),
                    revision: authority.revision,
                    assignment_version: authority.assignment_version,
                })
            })
            .unwrap_or(CommandField::Null),
        iam: CommandField::Value(CommandJson::parse(&command.iam_json)?),
        assignment_scope_kind: command.assignment_scope_kind.clone(),
        assignment_scope_node_id: command.assignment_scope_node_id.clone(),
    })
}

fn fingerprint(
    actor: &OrganizationCommandActor,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> Result<String, BusinessError> {
    command_fingerprint(
        &command_actor(actor)?,
        operation,
        record_id,
        &CommandJson::from_serializable(input)?,
    )
}

fn conflict(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", message)
}

fn conflict_code(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_READ_UNAVAILABLE",
        "Organization facts are unavailable.",
    )
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_AUTHORITY_CHANGED",
        "Your current organization authority is unavailable.",
    )
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "23505")
    {
        return conflict("This organization record already exists.").into();
    }
    error.into()
}

fn aggregate(operation: &str) -> String {
    operation.split('.').take(2).collect::<Vec<_>>().join("_")
}

pub struct PgOrganizationSetup<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOrganizationSetup<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn transaction(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize(&mut self, actor: &OrganizationCommandActor, now: i64) -> Result<(), Error> {
        self.transaction()?;
        let command = &actor.command;
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments g \
             JOIN organization_nodes c ON c.id=g.company_id AND c.kind='company' AND c.status='active' \
             WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
             AND g.revoked_at IS NULL \
             AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
             AND iam_role_allows(g.role,g.company_id,'organization.manage'))",
        )
        .bind(command.assignment_id.as_str())
        .bind(command.user_id.as_str())
        .bind(&command.role)
        .bind(command.company_id.as_str())
        .bind(now)
        .fetch_one(&mut *self.connection)
        .await?;
        if !allowed {
            return Err(authority_changed().into());
        }
        Ok(())
    }

    async fn receipt(&mut self, key: &str, hash: &str) -> Result<Option<Value>, Error> {
        let row = sqlx::query(
            "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let saved: String = row.try_get("request_hash")?;
        if saved != hash {
            return Err(conflict_code(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        raw.and_then(|value| serde_json::from_str(&value).ok())
            .map(Some)
            .ok_or_else(|| {
                conflict_code(
                    "IDEMPOTENCY_CONFLICT",
                    "The retained result is unavailable.",
                )
                .into()
            })
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_receipt(
        &mut self,
        actor: &OrganizationCommandActor,
        key: &str,
        operation: &str,
        record_id: &str,
        hash: &str,
        result: &Value,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        let command_actor = command_actor(actor)?;
        let authority = command_actor.receipt_authority()?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(actor.command.user_id.as_str())
        .bind(hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(authority.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?
        .rows_affected()
            == 1;
        if !inserted {
            return Err(conflict_code(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let aggregate = aggregate(operation);
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
        )
        .bind(format!("audit-{key}"))
        .bind(key)
        .bind(actor.command.user_id.as_str())
        .bind(actor.command.assignment_id.as_str())
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        let scope = CommandEventScope::from(&command_actor);
        let payload = command_actor.outbox_payload(record_id, version, None, &scope)?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("outbox-{key}"))
        .bind(key)
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(payload.as_str())
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn template_row(
        &mut self,
        company: &CompanyId,
        id: &str,
    ) -> Result<Option<TemplateRow>, Error> {
        let row = sqlx::query(
            "SELECT positions_json,version::bigint version FROM organization_templates \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(TemplateRow {
                positions_json: row.try_get("positions_json")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }
}

impl OrganizationSetupRepository for PgOrganizationSetup<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let hash = fingerprint(actor, operation, record_id, input)?;
            self.receipt(key, &hash).await
        })
    }

    fn template<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> OrganizationFuture<'a, Option<TemplateRow>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.template_row(company, id).await
        })
    }

    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active')",
            )
            .bind(branch)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 \
                 AND kind='department' AND status='active' AND branch_id IS NOT DISTINCT FROM $3)",
            )
            .bind(company.as_str())
            .bind(department)
            .bind(branch)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn levels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, Vec<(i64, String)>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT id,level_number::bigint level_number FROM staff_levels \
                 WHERE company_id=$1 AND active=1 AND level_number IS NOT NULL ORDER BY level_number",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok((
                        row.try_get::<i64, _>("level_number")?,
                        row.try_get::<String, _>("id")?,
                    ))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn ceo_position<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM organization_positions WHERE company_id=$1 AND is_ceo=1 AND active=1 \
                 ORDER BY id LIMIT 1",
            )
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn persist_install_levels<'a>(
        &'a mut self,
        command: &'a InstallLevelsCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.authorize(&command.actor, now).await?;
            let company = command.actor.command.company_id.as_str();
            for (index, title) in command.titles.iter().enumerate() {
                let number = index as i64 + 1;
                sqlx::query(
                    "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,1,$7,$7) ON CONFLICT DO NOTHING",
                )
                .bind(format!("org-level-{company}-{number}"))
                .bind(company)
                .bind(format!("ORG_L{number}"))
                .bind(title)
                .bind(-number)
                .bind(number)
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let result = json!({"id": company});
            let hash = fingerprint(
                &command.actor,
                "organization.levels.install",
                company,
                &Value::Object(serde_json::Map::new()),
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                "organization.levels.install",
                company,
                &hash,
                &result,
                1,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn persist_save_template<'a>(
        &'a mut self,
        command: &'a SaveTemplateCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.authorize(&command.actor, now).await?;
            let company = command.actor.command.company_id.as_str();
            if command.id.is_some() {
                let version: Option<i64> = sqlx::query_scalar(
                    "SELECT version::bigint FROM organization_templates WHERE id=$1 AND company_id=$2 FOR UPDATE",
                )
                .bind(&command.record_id)
                .bind(company)
                .fetch_optional(&mut *self.connection)
                .await?;
                match version {
                    None => {
                        return Err(BusinessError::new(
                            FailureKind::NotFound,
                            "TEMPLATE_NOT_FOUND",
                            "Organization template not found.",
                        )
                        .into());
                    }
                    Some(version) if version != command.input.expected_version => {
                        return Err(conflict(
                            "The organization template changed. Reload it and review the revision.",
                        )
                        .into());
                    }
                    Some(_) => {}
                }
            }
            let positions_json =
                serde_json::to_string(&command.input.positions).map_err(|_| unavailable())?;
            sqlx::query(
                "INSERT INTO organization_templates(id,company_id,name,positions_json,version,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$7) \
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,positions_json=excluded.positions_json,\
                 version=excluded.version,updated_at=excluded.updated_at",
            )
            .bind(&command.record_id)
            .bind(company)
            .bind(&command.input.name)
            .bind(positions_json)
            .bind(command.version)
            .bind(command.actor.command.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let result = json!({"id": command.record_id, "version": command.version});
            let receipt = serde_json::to_value(&command.input).map_err(|_| unavailable())?;
            let hash = fingerprint(
                &command.actor,
                "organization.template.save",
                &command.record_id,
                &receipt,
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                "organization.template.save",
                &command.record_id,
                &hash,
                &result,
                command.version,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn persist_apply_template<'a>(
        &'a mut self,
        command: &'a ApplyTemplateCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            self.authorize(&command.actor, now).await?;
            let company = command.actor.command.company_id.as_str();
            let version: Option<i64> = sqlx::query_scalar(
                "SELECT version::bigint FROM organization_templates WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(&command.template_id)
            .bind(company)
            .fetch_optional(&mut *self.connection)
            .await?;
            match version {
                None => {
                    return Err(BusinessError::new(
                        FailureKind::NotFound,
                        "TEMPLATE_NOT_FOUND",
                        "Organization template not found.",
                    )
                    .into());
                }
                Some(version) if version != command.input.expected_version => {
                    return Err(conflict_code(
                        "STALE_VERSION",
                        "The organization template changed. Reload it and review the revision.",
                    )
                    .into());
                }
                Some(_) => {}
            }
            for position in &command.positions {
                sqlx::query(
                    "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,\
                     reports_to_position_id,is_ceo,roster_required,active,created_by,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10,$11,$11)",
                )
                .bind(&position.id)
                .bind(company)
                .bind(&command.input.branch_id)
                .bind(&command.input.department_id)
                .bind(&position.title)
                .bind(&position.level_id)
                .bind(&position.reports_to_position_id)
                .bind(if position.is_ceo { 1_i64 } else { 0_i64 })
                .bind(if position.roster_required { 1_i64 } else { 0_i64 })
                .bind(command.actor.command.user_id.as_str())
                .bind(now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let result = json!({
                "id": command.template_id,
                "positionIds": command.position_ids,
            });
            let receipt = serde_json::to_value(&command.input).map_err(|_| unavailable())?;
            let hash = fingerprint(
                &command.actor,
                "organization.template.apply",
                &command.template_id,
                &receipt,
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                "organization.template.apply",
                &command.template_id,
                &hash,
                &result,
                command.version,
                now,
            )
            .await?;
            Ok(result)
        })
    }
}
