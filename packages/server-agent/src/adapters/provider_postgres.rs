//! PostgreSQL AI provider settings. Settings and receipts are native; key
//! sealing, key opening and provider HTTP transport fail closed until the A1
//! credential/provider service is enabled. No plaintext credential is ever
//! written or returned by this adapter.
use crate::application::providers::{
    AiActor, AiProviderMutation, AiProviderMutationAction, AiProviderPorts, AiUsage, GenerateKind,
    SealedCredential,
};
use crate::domain::providers::{
    AiConnection, AiGeneration, AiProtocol, AiRevision, AiSelection, AuthMode, ImageMode,
    ProviderPreset, SelectionMode, credential_unavailable, transport_unavailable,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandEventScope, CommandJson, command_fingerprint};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

pub type AiProviderError = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", "The record changed.")
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "AI_AUTHORITY_CHANGED",
        "Your current AI administration authority is unavailable.",
    )
}

fn statement_error(error: sqlx::Error) -> AiProviderError {
    match error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
    {
        Some("23502" | "23503" | "23505" | "23514") => conflict().into(),
        _ => PgTransactionError::Persistence(PersistenceError::from(error)),
    }
}

fn parse_auth(value: &str) -> AuthMode {
    if value == "none" {
        AuthMode::None
    } else {
        AuthMode::Bearer
    }
}

fn parse_preset(value: &str) -> ProviderPreset {
    ProviderPreset::parse(value).unwrap_or(ProviderPreset::Custom)
}

fn parse_protocol(value: &str) -> AiProtocol {
    match value {
        "responses" => AiProtocol::Responses,
        "messages" => AiProtocol::Messages,
        _ => AiProtocol::ChatCompletions,
    }
}

fn parse_selection(value: &str) -> SelectionMode {
    SelectionMode::parse(value).unwrap_or(SelectionMode::Disabled)
}

fn parse_image_mode(value: &str) -> ImageMode {
    ImageMode::parse(value).unwrap_or(ImageMode::Default)
}

pub struct PgAiProvider<'c> {
    connection: &'c mut PgConnection,
    actor: &'c AiActor,
    now: i64,
}

impl<'c> PgAiProvider<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c AiActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), AiProviderError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("AI_COMMAND_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_connection(&mut self, id: &str) -> Result<AiConnection, AiProviderError> {
        let row = sqlx::query(
            "SELECT id,company_id,name,version,draft_revision,active_revision,disabled \
             FROM ai_connections WHERE id=$1 AND company_id=$2 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(crate::domain::providers::connection_not_found().into());
        };
        Ok(AiConnection {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            name: row.try_get("name")?,
            version: row.try_get("version")?,
            draft_revision: row.try_get("draft_revision")?,
            active_revision: row.try_get("active_revision")?,
            disabled: row.try_get::<i64, _>("disabled")? != 0,
        })
    }

    async fn insert_receipt(
        &mut self,
        mutation: &AiProviderMutation,
        result: &Value,
    ) -> Result<(), AiProviderError> {
        let digest = command_fingerprint(
            &self.actor.command,
            mutation.common.operation,
            &mutation.common.record_id,
            &mutation.common.input,
        )?;
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&mutation.common.key)
        .bind(mutation.common.operation)
        .bind(self.actor.user_id())
        .bind(digest)
        .bind(&mutation.common.record_id)
        .bind(result.to_string())
        .bind(mutation.common.now)
        .bind(self.actor.command.receipt_authority()?.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(statement_error)?;
        Ok(())
    }
}

impl AiProviderPorts for PgAiProvider<'_> {
    type Error = AiProviderError;

    fn actor(&self) -> &AiActor {
        self.actor
    }

    fn now(&self) -> i64 {
        self.now
    }

    async fn authorize(&mut self, owner_only: bool) -> Result<(), Self::Error> {
        if owner_only {
            self.actor.require_owner()?;
        } else {
            self.actor.require_admin()?;
        }
        let current: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=$1 AND g.user_id=$2 AND g.company_id=$3 \
             AND iam_assignment_active(g.id,$4) AND iam_principal_active(g.user_id,g.company_id,$4))",
        )
        .bind(self.actor.command.assignment_id.as_str())
        .bind(self.actor.user_id())
        .bind(self.actor.company_id())
        .bind(self.now)
        .fetch_one(&mut *self.connection)
        .await?;
        if !current {
            return Err(authority_changed().into());
        }
        Ok(())
    }

    async fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> Result<Option<Value>, Self::Error> {
        let row = sqlx::query(
            "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let saved_operation: String = row.try_get("operation")?;
        let saved_hash: String = row.try_get("request_hash")?;
        let digest = command_fingerprint(&self.actor.command, operation, record_id, input)?;
        if saved_operation != operation || saved_hash != digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let result: String = row.try_get("result_json")?;
        Ok(Some(serde_json::from_str::<Value>(&result).map_err(
            |_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "AI_COMMAND_RESULT_UNAVAILABLE",
                    "The retained AI command result is unavailable.",
                )
            },
        )?))
    }

    async fn connections(&mut self) -> Result<Vec<AiConnection>, Self::Error> {
        let rows = sqlx::query(
            "SELECT id,company_id,name,version,draft_revision,active_revision,disabled \
             FROM ai_connections WHERE company_id=$1 AND deleted_at IS NULL ORDER BY created_at,id",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AiConnection {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    version: row.try_get("version")?,
                    draft_revision: row.try_get("draft_revision")?,
                    active_revision: row.try_get("active_revision")?,
                    disabled: row.try_get::<i64, _>("disabled")? != 0,
                })
            })
            .collect()
    }

    async fn connection(&mut self, id: &str) -> Result<Option<AiConnection>, Self::Error> {
        let row = sqlx::query(
            "SELECT id,company_id,name,version,draft_revision,active_revision,disabled \
             FROM ai_connections WHERE id=$1 AND company_id=$2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        match row {
            Some(row) => Ok(Some(AiConnection {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                name: row.try_get("name")?,
                version: row.try_get("version")?,
                draft_revision: row.try_get("draft_revision")?,
                active_revision: row.try_get("active_revision")?,
                disabled: row.try_get::<i64, _>("disabled")? != 0,
            })),
            None => Ok(None),
        }
    }

    async fn revision(
        &mut self,
        connection: &AiConnection,
        active: bool,
    ) -> Result<Option<AiRevision>, Self::Error> {
        let revision = if active {
            connection.active_revision
        } else {
            Some(connection.draft_revision)
        };
        let Some(revision) = revision else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT connection_id,revision,base_url,auth_mode,provider_preset,text_protocol,image_protocol,\
             text_model,image_model,text_verified,image_verified,tested_at,test_error,credential_json IS NOT NULL has_credential \
             FROM ai_connection_revisions WHERE connection_id=$1 AND revision=$2",
        )
        .bind(&connection.id)
        .bind(revision)
        .fetch_optional(&mut *self.connection)
        .await?;
        match row {
            Some(row) => Ok(Some(AiRevision {
                connection_id: row.try_get("connection_id")?,
                revision: row.try_get("revision")?,
                base_url: row.try_get("base_url")?,
                auth_mode: parse_auth(row.try_get::<String, _>("auth_mode")?.as_str()),
                provider_preset: parse_preset(
                    row.try_get::<String, _>("provider_preset")?.as_str(),
                ),
                text_protocol: parse_protocol(row.try_get::<String, _>("text_protocol")?.as_str()),
                image_protocol: parse_protocol(
                    row.try_get::<String, _>("image_protocol")?.as_str(),
                ),
                text_model: row.try_get("text_model")?,
                image_model: row.try_get("image_model")?,
                text_verified: row.try_get::<i64, _>("text_verified")? != 0,
                image_verified: row.try_get::<i64, _>("image_verified")? != 0,
                tested_at: row.try_get("tested_at")?,
                test_error: row.try_get("test_error")?,
                has_credential: row.try_get("has_credential")?,
            })),
            None => Ok(None),
        }
    }

    async fn selection(&mut self) -> Result<Option<AiSelection>, Self::Error> {
        let row = sqlx::query(
            "SELECT company_id,version,mode,text_connection_id,image_connection_id,image_mode,home_ai_name \
             FROM ai_company_settings WHERE company_id=$1",
        )
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        match row {
            Some(row) => Ok(Some(AiSelection {
                company_id: row.try_get("company_id")?,
                version: row.try_get("version")?,
                mode: parse_selection(row.try_get::<String, _>("mode")?.as_str()),
                text_connection_id: row.try_get("text_connection_id")?,
                image_connection_id: row.try_get("image_connection_id")?,
                image_mode: parse_image_mode(row.try_get::<String, _>("image_mode")?.as_str()),
                home_ai_name: row.try_get("home_ai_name")?,
            })),
            None => Ok(None),
        }
    }

    async fn usage(&mut self, since: i64) -> Result<Vec<AiUsage>, Self::Error> {
        let rows = sqlx::query(
            "SELECT connection_id,model,COUNT(*) requests,SUM(input_tokens)::bigint input_tokens,\
             SUM(output_tokens)::bigint output_tokens,\
             SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END)::bigint failures \
             FROM ai_provider_usage WHERE company_id=$1 AND created_at>=$2 \
             GROUP BY connection_id,model",
        )
        .bind(self.actor.company_id())
        .bind(since)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AiUsage {
                    connection_id: row.try_get("connection_id")?,
                    model: row.try_get("model")?,
                    requests: row.try_get("requests")?,
                    input_tokens: row.try_get("input_tokens")?,
                    output_tokens: row.try_get("output_tokens")?,
                    failures: row.try_get("failures")?,
                })
            })
            .collect()
    }

    async fn seal_key(&mut self, _connection_id: &str, _key: &str) -> Result<String, Self::Error> {
        Err(credential_unavailable().into())
    }

    async fn open_key(&mut self, _connection_id: &str) -> Result<Option<String>, Self::Error> {
        Err(credential_unavailable().into())
    }

    async fn generate(
        &mut self,
        _connection: &AiConnection,
        _revision: &AiRevision,
        _kind: GenerateKind,
    ) -> Result<AiGeneration, Self::Error> {
        Err(transport_unavailable().into())
    }

    async fn request_models(
        &mut self,
        _base_url: &str,
        _api_key: Option<&str>,
    ) -> Result<Vec<String>, Self::Error> {
        Err(transport_unavailable().into())
    }

    async fn commit(
        &mut self,
        mutation: &AiProviderMutation,
        result: &Value,
    ) -> Result<(), Self::Error> {
        Self::require_transaction(&mut *self.connection).await?;
        sqlx::query(
            "SELECT id FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active' FOR UPDATE",
        )
        .bind(self.actor.company_id())
        .execute(&mut *self.connection)
        .await?;
        self.insert_receipt(mutation, result).await?;
        let company = self.actor.company_id().to_owned();
        let now = mutation.common.now;
        match &mutation.action {
            AiProviderMutationAction::Save {
                input,
                previous,
                existing,
                sealed_credential,
            } => {
                let record_id = &mutation.common.record_id;
                let draft_revision = match existing {
                    Some(row) => {
                        let locked = self.lock_connection(&row.id).await?;
                        if locked.version != input.expected_version {
                            return Err(conflict().into());
                        }
                        locked.draft_revision + 1
                    }
                    None => 1,
                };
                let credential = match sealed_credential {
                    SealedCredential::Replace(value) => Some(value.clone()),
                    SealedCredential::Clear => None,
                    SealedCredential::Keep => {
                        let from_revision: Option<String> = sqlx::query_scalar(
                            "SELECT credential_json FROM ai_connection_revisions WHERE connection_id=$1 AND revision=$2",
                        )
                        .bind(record_id)
                        .bind(previous.as_ref().map(|revision| revision.revision))
                        .fetch_optional(&mut *self.connection)
                        .await?
                        .flatten();
                        from_revision
                    }
                };
                match existing {
                    Some(row) => {
                        sqlx::query(
                            "UPDATE ai_connections SET name=$1,version=version+1,draft_revision=draft_revision+1,updated_at=$2 \
                             WHERE id=$3 AND company_id=$4 AND version=$5",
                        )
                        .bind(&input.name)
                        .bind(now)
                        .bind(&row.id)
                        .bind(&company)
                        .bind(input.expected_version)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                    None => {
                        sqlx::query(
                            "INSERT INTO ai_connections(id,company_id,name,version,draft_revision,active_revision,disabled,created_by,created_at,updated_at) \
                             VALUES($1,$2,$3,1,1,NULL,0,$4,$5,$5)",
                        )
                        .bind(record_id)
                        .bind(&company)
                        .bind(&input.name)
                        .bind(self.actor.user_id())
                        .bind(now)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                }
                sqlx::query(
                    "INSERT INTO ai_connection_revisions(connection_id,revision,base_url,auth_mode,credential_json,text_model,image_model,created_by,created_at,provider_preset,text_protocol,image_protocol) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
                )
                .bind(record_id)
                .bind(draft_revision)
                .bind(&input.base_url)
                .bind(input.auth_mode.as_str())
                .bind(credential)
                .bind(&input.text_model)
                .bind(if input.image_model.is_empty() {
                    None
                } else {
                    Some(input.image_model.clone())
                })
                .bind(self.actor.user_id())
                .bind(now)
                .bind(input.provider_preset.as_str())
                .bind(input.text_protocol().as_str())
                .bind(input.image_protocol().as_str())
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AiProviderMutationAction::Test {
                connection_id,
                revision,
                expected_version,
                text_verified,
                image_verified,
                error,
            } => {
                let locked = self.lock_connection(connection_id).await?;
                if locked.version != *expected_version {
                    return Err(conflict().into());
                }
                sqlx::query(
                    "UPDATE ai_connection_revisions SET tested_at=$1,text_verified=$2,image_verified=$3,test_error=$4 \
                     WHERE connection_id=$5 AND revision=$6",
                )
                .bind(now)
                .bind(i64::from(*text_verified))
                .bind(i64::from(*image_verified))
                .bind(error)
                .bind(connection_id)
                .bind(revision.revision)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
                sqlx::query(
                    "UPDATE ai_connections SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4",
                )
                .bind(now)
                .bind(connection_id)
                .bind(&company)
                .bind(expected_version)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
            AiProviderMutationAction::Activate {
                connection_id,
                expected_version,
            } => {
                let changed = sqlx::query(
                    "UPDATE ai_connections SET active_revision=draft_revision,disabled=0,version=version+1,updated_at=$1 \
                     WHERE id=$2 AND company_id=$3 AND version=$4",
                )
                .bind(now)
                .bind(connection_id)
                .bind(&company)
                .bind(expected_version)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
                if changed.rows_affected() != 1 {
                    return Err(conflict().into());
                }
            }
            AiProviderMutationAction::Disable {
                connection_id,
                expected_version,
                disable_uses,
                remove,
            } => {
                let settings = sqlx::query(
                    "SELECT version FROM ai_company_settings WHERE company_id=$1 FOR UPDATE",
                )
                .bind(&company)
                .fetch_optional(&mut *self.connection)
                .await?;
                let settings_version = settings
                    .as_ref()
                    .map(|row| row.try_get::<i64, _>("version"))
                    .transpose()?
                    .unwrap_or(0);
                if *disable_uses {
                    let changed = sqlx::query(
                        "UPDATE ai_company_settings SET \
                         mode=CASE WHEN text_connection_id=$1 THEN 'disabled' ELSE mode END,\
                         image_mode=CASE WHEN image_connection_id=$1 THEN 'disabled' ELSE image_mode END,\
                         version=version+1,updated_by=$2,updated_at=$3 WHERE company_id=$4 AND version=$5",
                    )
                    .bind(connection_id)
                    .bind(self.actor.user_id())
                    .bind(now)
                    .bind(&company)
                    .bind(settings_version)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                    if changed.rows_affected() != 1 {
                        return Err(conflict().into());
                    }
                }
                let changed = sqlx::query(
                    "UPDATE ai_connections SET disabled=1,deleted_at=$1,version=version+1,updated_at=$2 \
                     WHERE id=$3 AND company_id=$4 AND version=$5",
                )
                .bind(if *remove { Some(now) } else { None })
                .bind(now)
                .bind(connection_id)
                .bind(&company)
                .bind(expected_version)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
                if changed.rows_affected() != 1 {
                    return Err(conflict().into());
                }
                if *remove {
                    sqlx::query(
                        "UPDATE ai_connection_revisions SET credential_json=NULL WHERE connection_id=$1",
                    )
                    .bind(connection_id)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                }
            }
            AiProviderMutationAction::Select {
                input,
                current_name,
            } => {
                let row = sqlx::query(
                    "SELECT version FROM ai_company_settings WHERE company_id=$1 FOR UPDATE",
                )
                .bind(&company)
                .fetch_optional(&mut *self.connection)
                .await?;
                let current_version = row
                    .as_ref()
                    .map(|row| row.try_get::<i64, _>("version"))
                    .transpose()?
                    .unwrap_or(0);
                if current_version != input.expected_version {
                    return Err(conflict().into());
                }
                let next = input.expected_version + 1;
                let home_name = input
                    .home_ai_name
                    .as_deref()
                    .unwrap_or(current_name.as_str());
                if home_name != current_name {
                    sqlx::query(
                        "INSERT INTO ai_home_name_history(company_id,name) VALUES($1,$2) ON CONFLICT DO NOTHING",
                    )
                    .bind(&company)
                    .bind(home_name)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                }
                sqlx::query(
                    "INSERT INTO ai_company_settings(company_id,version,mode,text_connection_id,image_connection_id,image_mode,updated_by,updated_at,home_ai_name) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) \
                     ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,mode=excluded.mode,\
                     text_connection_id=excluded.text_connection_id,image_connection_id=excluded.image_connection_id,\
                     image_mode=excluded.image_mode,updated_by=excluded.updated_by,updated_at=excluded.updated_at,\
                     home_ai_name=excluded.home_ai_name",
                )
                .bind(&company)
                .bind(next)
                .bind(input.mode.as_str())
                .bind(input.text_connection_id.as_deref())
                .bind(input.image_connection_id.as_deref())
                .bind(input.image_mode.as_str())
                .bind(self.actor.user_id())
                .bind(now)
                .bind(home_name)
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
            }
        }
        insert_audit_and_outbox(&mut *self.connection, self.actor, mutation, result, now).await
    }
}

async fn insert_audit_and_outbox(
    connection: &mut PgConnection,
    actor: &AiActor,
    mutation: &AiProviderMutation,
    result: &Value,
    now: i64,
) -> Result<(), AiProviderError> {
    let operation = mutation.common.operation;
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$1))",
    )
    .bind(&mutation.common.key)
    .bind(actor.user_id())
    .bind(actor.command.assignment_id.as_str())
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    let state = result
        .get("state")
        .and_then(Value::as_str)
        .or_else(|| result.get("status").and_then(Value::as_str));
    let scope = CommandEventScope::from(&actor.command);
    let payload = actor.command.outbox_payload(
        &mutation.common.record_id,
        mutation.common.version,
        state,
        &scope,
    )?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&mutation.common.key)
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(payload.as_str())
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}
