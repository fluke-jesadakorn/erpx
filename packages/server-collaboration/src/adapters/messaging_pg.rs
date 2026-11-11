//! Production owner units for Messaging rooms, messages, read markers, teams
//! and channels.
//!
//! Composition supplies the selected Work scope. The units verify the current
//! assignment/principal on the caller's serializable connection, delegate Files
//! attachment binding to [`MessagingFilesPort`] and never create file storage,
//! open a transaction or commit.
use crate::adapters::command_ledger::{self, Error, fingerprint, source_changed};
use crate::adapters::messaging_http::{
    MessagingFileBinding, MessagingFileContent, MessagingFilesBindRequest,
    MessagingFilesContentRequest, MessagingFilesPort,
};
use crate::adapters::messaging_native::{
    CurrentMessagingEffects, MessageRow, RoomView, apply_plan, read_history, read_overview,
    read_room, retain_effects,
};
use crate::adapters::messaging_teams::{
    ChannelView, apply_plan as apply_team_plan, read_team, read_team_channels,
    retain_effects as retain_team_effects,
};
use crate::application::messaging_native::{Access as RoomAccess, MessagingUnit};
use crate::application::messaging_teams::{Access as TeamAccess, MessagingTeamUnit};
use crate::domain::messaging_native::{
    Change, HistoryCursor, MessageSnapshot, MessagingActor, OwnerPlan, Receipt, RoomKind,
    RoomSnapshot, room_not_found,
};
use crate::domain::messaging_teams::{
    Change as TeamChange, OwnerPlan as TeamPlan, Receipt as TeamReceipt, TeamSnapshot,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, UserId,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};

type Unit = (String, i32, String, String);

/// Selected Work facts supplied by composition. The unit revalidates the
/// assignment and principal on every call.
#[derive(Clone, Debug)]
pub struct MessagingScope {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
    pub display_name: String,
    pub now: i64,
}

impl MessagingScope {
    pub fn new(
        company_id: impl Into<String>,
        user_id: impl Into<String>,
        assignment_id: impl Into<String>,
        display_name: impl Into<String>,
        now: i64,
    ) -> Result<Self, BusinessError> {
        let display_name = display_name.into();
        let user_id = UserId::new(user_id)?;
        Ok(Self {
            display_name: if display_name.trim().is_empty() {
                user_id.as_str().to_owned()
            } else {
                display_name
            },
            user_id,
            company_id: CompanyId::new(company_id)?,
            assignment_id: AssignmentId::new(assignment_id)?,
            assignment_version: 0,
            now,
        })
    }

    pub fn assignment_version(mut self, version: i64) -> Self {
        self.assignment_version = version;
        self
    }

    pub fn actor(&self) -> MessagingActor {
        MessagingActor {
            user_id: self.user_id.as_str().to_owned(),
            display_name: self.display_name.clone(),
            company_id: self.company_id.as_str().to_owned(),
        }
    }
}

/// Exact stable direct-conversation key input from the original TypeScript
/// implementation: `<company>:<JSON array of sorted user ids>`.
pub fn direct_key_input(company_id: &str, members: &[String]) -> String {
    format!(
        "{company_id}:{}",
        serde_json::to_string(members).expect("member identifiers are JSON strings")
    )
}

fn sha256_query_input(company_id: &str, members: &[String]) -> String {
    direct_key_input(company_id, members)
}

fn decode_receipt(value: &Value) -> Result<Receipt, Error> {
    let status = match value["status"].as_str() {
        Some("created") => "created",
        Some("sent") => "sent",
        Some("read") => "read",
        _ => {
            return Err(source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging receipt has an unknown status.",
            ));
        }
    };
    let id = value["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging receipt has no identifier.",
            )
        })?;
    let version = value["version"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging receipt has no version.",
            )
        })?;
    let seq = value["seq"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging receipt has no sequence.",
            )
        })?;
    Ok(Receipt {
        id: id.into(),
        version,
        status,
        seq,
    })
}

fn decode_team_receipt(value: &Value) -> Result<TeamReceipt, Error> {
    let status = match value["status"].as_str() {
        Some("created") => "created",
        _ => {
            return Err(source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging team receipt has an unknown status.",
            ));
        }
    };
    let id = value["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging team receipt has no identifier.",
            )
        })?;
    let version = value["version"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Messaging team receipt has no version.",
            )
        })?;
    Ok(TeamReceipt {
        id: id.into(),
        version,
        status,
    })
}

async fn verify_scope(
    connection: &mut PgConnection,
    scope: &MessagingScope,
) -> Result<MessagingActor, Error> {
    let row = sqlx::query(
        "SELECT a.version FROM assignments a \
         WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.revoked_at IS NULL \
         AND a.valid_from<=$4 AND a.valid_until>$4 \
         AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)",
    )
    .bind(scope.assignment_id.as_str())
    .bind(scope.company_id.as_str())
    .bind(scope.user_id.as_str())
    .bind(scope.now)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current Company membership is required.",
        )
        .into());
    };
    let version: i64 = row.try_get(0)?;
    if scope.assignment_version > 0 && version != scope.assignment_version {
        return Err(source_changed(
            "MESSAGING_SOURCE_CHANGED",
            "Your selected assignment changed. Refresh and retry.",
        ));
    }
    Ok(scope.actor())
}

async fn company_member(
    connection: &mut PgConnection,
    scope: &MessagingScope,
    user_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
         AND a.revoked_at IS NULL AND a.valid_from<=$3 AND a.valid_until>$3 \
         AND iam_assignment_active(a.id,$3) AND iam_principal_active(a.user_id,a.company_id,$3))",
    )
    .bind(scope.company_id.as_str())
    .bind(user_id)
    .bind(scope.now)
    .fetch_one(&mut *connection)
    .await?)
}

pub struct PgMessagingUnit<'c> {
    connection: &'c mut PgConnection,
    scope: MessagingScope,
    key: CommandKey,
    actor: Option<MessagingActor>,
    current: Option<CurrentMessagingEffects>,
    unit: Option<Unit>,
    change: Option<Change>,
    request_fingerprint: Option<String>,
    receipt: Option<Receipt>,
    before_apply: bool,
}

impl<'c> PgMessagingUnit<'c> {
    /// The caller must already hold its serializable transaction and
    /// [`crate::adapters::messaging_native::lock_messaging_sources`].
    pub fn new(connection: &'c mut PgConnection, scope: MessagingScope, key: CommandKey) -> Self {
        Self {
            connection,
            scope,
            key,
            actor: None,
            current: None,
            unit: None,
            change: None,
            request_fingerprint: None,
            receipt: None,
            before_apply: true,
        }
    }

    /// Read-only composition convenience. No receipt or ledger row is written.
    pub fn read_only(connection: &'c mut PgConnection, scope: MessagingScope) -> Self {
        Self::new(
            connection,
            scope,
            CommandKey::new("messaging-read").expect("static read key"),
        )
    }

    async fn unit(&mut self) -> Result<Unit, Error> {
        command_ledger::require_serializable(self.connection, "MESSAGING_SERIALIZABLE_REQUIRED")
            .await?;
        if let Some(unit) = &self.unit {
            return Ok(unit.clone());
        }
        let unit = sqlx::query_as::<_, Unit>(
            "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
        )
        .fetch_one(&mut *self.connection)
        .await?;
        self.unit = Some(unit.clone());
        Ok(unit)
    }

    async fn verify_authority(&mut self) -> Result<MessagingActor, Error> {
        self.unit().await?;
        let actor = verify_scope(&mut *self.connection, &self.scope).await?;
        self.actor = Some(actor.clone());
        Ok(actor)
    }

    async fn direct_room(&mut self, members: &[String]) -> Result<String, Error> {
        let mut members = members.to_vec();
        members.sort();
        let digest: String =
            sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                .bind(sha256_query_input(self.scope.company_id.as_str(), &members))
                .fetch_one(&mut *self.connection)
                .await?;
        Ok(format!("dm-{digest}"))
    }

    fn operation(change: &Change) -> &'static str {
        match change {
            Change::CreateRoom { .. } => "messaging.create",
            Change::SendMessage { .. } => "messaging.send",
            Change::MarkRead { .. } => "messaging.read",
        }
    }

    async fn record_id(&mut self, change: &Change) -> Result<String, Error> {
        Ok(match change {
            Change::CreateRoom { kind, members, .. } => {
                if *kind == RoomKind::Direct {
                    let members = crate::domain::messaging_native::members(
                        self.scope.user_id.as_str(),
                        members,
                    )?;
                    self.direct_room(&members).await?
                } else {
                    format!("room-{}", self.key.as_str())
                }
            }
            Change::SendMessage { .. } => crate::domain::messaging_native::message_id(&self.key),
            Change::MarkRead { room_id, .. } => room_id.clone(),
        })
    }

    /// Existing Company conversation overview. The caller must use the same
    /// transaction for the returned rows.
    pub async fn overview(&mut self) -> Result<Vec<RoomView>, Error> {
        let actor = self.verify_authority().await?;
        read_overview(&mut *self.connection, &actor).await
    }

    pub async fn history(
        &mut self,
        room_id: &str,
        before: Option<HistoryCursor>,
        after: Option<HistoryCursor>,
        limit: usize,
    ) -> Result<Vec<MessageRow>, Error> {
        let actor = self.verify_authority().await?;
        read_history(&mut *self.connection, &actor, room_id, before, after, limit).await
    }

    pub async fn room(&mut self, id: &str) -> Result<Option<RoomSnapshot>, Error> {
        read_room(&mut *self.connection, &self.scope.company_id, id).await
    }

    pub async fn message(&mut self, id: &str) -> Result<Option<MessageSnapshot>, Error> {
        crate::adapters::messaging_native::read_message(&mut *self.connection, id).await
    }

    pub async fn room_members(&mut self, id: &str) -> Result<Vec<String>, Error> {
        self.verify_authority().await?;
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM messaging_room_members WHERE room_id=$1 ORDER BY user_id",
        )
        .bind(id)
        .fetch_all(&mut *self.connection)
        .await?)
    }

    pub async fn team_members(&mut self, id: &str) -> Result<Vec<String>, Error> {
        self.verify_authority().await?;
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM messaging_team_members WHERE team_id=$1 ORDER BY user_id",
        )
        .bind(id)
        .fetch_all(&mut *self.connection)
        .await?)
    }

    /// Active People rows in this Company, optionally narrowed to one team.
    pub async fn people(
        &mut self,
        query: &str,
        team_id: Option<&str>,
    ) -> Result<Vec<Value>, Error> {
        self.verify_authority().await?;
        let rows = sqlx::query(
            "SELECT p.user_id,COALESCE(NULLIF(p.preferred_name,''),p.legal_name),p.email \
             FROM hr_people p \
             WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL \
             AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id \
               AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$2 \
               AND iam_assignment_active(a.id,$2) AND iam_principal_active(a.user_id,a.company_id,$2)) \
             AND ($3='' OR COALESCE(NULLIF(p.preferred_name,''),p.legal_name) ILIKE '%'||$3||'%' OR p.email ILIKE '%'||$3||'%') \
             AND ($4::text IS NULL OR EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=$4 AND tm.user_id=p.user_id)) \
             ORDER BY COALESCE(NULLIF(p.preferred_name,''),p.legal_name),p.user_id LIMIT 100",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.now)
        .bind(query)
        .bind(team_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(serde_json::json!({
                    "id": row.try_get::<String, _>(0)?,
                    "name": row.try_get::<String, _>(1)?,
                    "email": row.try_get::<Option<String>, _>(2)?,
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Bind an attachment set through the Files owner port. The room membership
    /// check runs first; an absent port fails closed.
    pub async fn bind_attachment(
        &mut self,
        host: &str,
        holding_slug: &str,
        headers: &erpx_server_runtime::public::http_support::HeaderMap,
        room_id: &str,
        attachment_set_ids: &[String],
        message_id: Option<&str>,
        files: Option<&dyn MessagingFilesPort>,
    ) -> Result<MessagingFileBinding, Error> {
        let files = files.ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "FILES_PORT_UNAVAILABLE",
                "Attachment access is unavailable.",
            )
        })?;
        if attachment_set_ids.is_empty() || attachment_set_ids.len() > 10 {
            return Err(BusinessError::invalid(
                "MESSAGING_INPUT_INVALID",
                "Choose current attachment sets to bind.",
            )
            .into());
        }
        let actor = self.verify_authority().await?;
        let room = self.room(room_id).await?.ok_or_else(|| room_not_found())?;
        crate::domain::messaging_native::require_member(&actor, &room)?;
        files
            .bind(MessagingFilesBindRequest {
                host,
                holding_slug,
                headers,
                room_id,
                attachment_set_ids,
                message_id,
            })
            .await
            .map_err(Into::into)
    }

    /// Read file content through the Files owner port after current membership.
    pub async fn read_file(
        &mut self,
        host: &str,
        holding_slug: &str,
        headers: &erpx_server_runtime::public::http_support::HeaderMap,
        file_id: &str,
        variant: &str,
        files: Option<&dyn MessagingFilesPort>,
    ) -> Result<MessagingFileContent, Error> {
        let files = files.ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "FILES_PORT_UNAVAILABLE",
                "Attachment access is unavailable.",
            )
        })?;
        self.verify_authority().await?;
        files
            .content(MessagingFilesContentRequest {
                host,
                holding_slug,
                headers,
                file_id,
                variant,
            })
            .await
            .map_err(Into::into)
    }
}

impl MessagingUnit for PgMessagingUnit<'_> {
    type Error = Error;

    async fn authorize(&mut self, _access: RoomAccess) -> Result<MessagingActor, Error> {
        if let Some(actor) = &self.actor {
            return Ok(actor.clone());
        }
        self.verify_authority().await
    }

    async fn replay(&mut self, change: &Change) -> Result<Option<Receipt>, Error> {
        let actor = self.verify_authority().await?;
        let operation = Self::operation(change);
        let record_id = self.record_id(change).await?;
        let request = fingerprint(operation, change);
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            actor.user_id.as_str(),
            &record_id,
            &request,
        )
        .await?;
        self.change = Some(change.clone());
        self.request_fingerprint = Some(request);
        self.before_apply = true;
        match value {
            Some(value) => {
                let receipt = decode_receipt(&value)?;
                self.receipt = Some(receipt.clone());
                Ok(Some(receipt))
            }
            None => {
                self.receipt = None;
                Ok(None)
            }
        }
    }

    async fn room(&mut self, id: &str) -> Result<Option<RoomSnapshot>, Error> {
        self.room(id).await
    }

    async fn is_company_member(&mut self, user_id: &str) -> Result<bool, Error> {
        company_member(&mut *self.connection, &self.scope, user_id).await
    }

    async fn direct_room_id(&mut self, members: &[String]) -> Result<String, Error> {
        self.direct_room(members).await
    }

    async fn apply(
        &mut self,
        actor: &MessagingActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, Error> {
        self.verify_authority().await?;
        if actor.company_id != self.scope.company_id.as_str()
            || actor.user_id != self.scope.user_id.as_str()
        {
            return Err(source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging actor changed inside the retained unit.",
            ));
        }
        if key.as_str() != self.key.as_str() {
            return Err(source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging command key changed inside the retained unit.",
            ));
        }
        let request_fingerprint = self.request_fingerprint.clone().ok_or_else(|| {
            source_changed(
                "MESSAGING_REPLAY_REQUIRED",
                "The Messaging command must resolve its receipt before applying.",
            )
        })?;
        let receipt = apply_plan(&mut *self.connection, actor, key, &plan, self.scope.now).await?;
        let effects = retain_effects(&mut *self.connection, &plan, &receipt).await?;
        let (record_type, purpose, event_type) = match &plan {
            OwnerPlan::CreateRoom { .. } => (
                "messaging_room",
                "Start workplace conversation",
                "messaging.room.created",
            ),
            OwnerPlan::SendMessage { .. } => (
                "messaging_message",
                "Send workplace message",
                "messaging.message.sent",
            ),
            OwnerPlan::MarkRead { .. } => (
                "messaging_room",
                "Mark conversation read",
                "messaging.room.read",
            ),
        };
        let value = serde_json::to_value(&receipt).map_err(|_| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The Messaging result could not be encoded.",
            )
        })?;
        command_ledger::write_ledger(
            &mut *self.connection,
            command_ledger::LedgerRows {
                key: key.as_str(),
                operation: Self::operation(self.change.as_ref().ok_or_else(|| {
                    source_changed(
                        "MESSAGING_REPLAY_REQUIRED",
                        "The Messaging command lost its retained change.",
                    )
                })?),
                actor: actor.user_id.as_str(),
                assignment: self.scope.assignment_id.as_str(),
                record_id: &receipt.id,
                record_version: receipt.version,
                fingerprint: &request_fingerprint,
                result: &value,
                record_type,
                purpose,
                event_type,
                now: self.scope.now,
            },
        )
        .await?;
        self.current = Some(effects);
        self.actor = Some(actor.clone());
        self.receipt = Some(receipt.clone());
        self.before_apply = false;
        Ok(receipt)
    }

    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn require_unit_current(&mut self) -> Result<(), Error> {
        self.actor = None;
        self.verify_authority().await?;
        if let Some(current) = &self.current {
            current.require_current(&mut *self.connection).await?;
        }
        if self.before_apply {
            return Ok(());
        }
        let change = self.change.clone().ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging command lost its retained change.",
            )
        })?;
        let operation = Self::operation(&change);
        let record_id = self.record_id(&change).await?;
        let request = self.request_fingerprint.as_deref().ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging command lost its request fingerprint.",
            )
        })?;
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            self.scope.user_id.as_str(),
            &record_id,
            request,
        )
        .await?
        .ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging receipt disappeared.",
            )
        })?;
        if let Some(expected) = &self.receipt {
            if decode_receipt(&value)? != *expected {
                return Err(source_changed(
                    "MESSAGING_SOURCE_CHANGED",
                    "The Messaging receipt changed.",
                ));
            }
        }
        Ok(())
    }
}

pub struct PgMessagingTeamUnit<'c> {
    connection: &'c mut PgConnection,
    scope: MessagingScope,
    key: CommandKey,
    actor: Option<MessagingActor>,
    current: Option<crate::adapters::messaging_teams::CurrentTeamEffects>,
    change: Option<TeamChange>,
    request_fingerprint: Option<String>,
    receipt: Option<TeamReceipt>,
    before_apply: bool,
}

impl<'c> PgMessagingTeamUnit<'c> {
    /// The caller must already hold its serializable transaction and
    /// [`crate::adapters::messaging_teams::lock_messaging_team_sources`].
    pub fn new(connection: &'c mut PgConnection, scope: MessagingScope, key: CommandKey) -> Self {
        Self {
            connection,
            scope,
            key,
            actor: None,
            current: None,
            change: None,
            request_fingerprint: None,
            receipt: None,
            before_apply: true,
        }
    }

    /// Read-only composition convenience. No receipt or ledger row is written.
    pub fn read_only(connection: &'c mut PgConnection, scope: MessagingScope) -> Self {
        Self::new(
            connection,
            scope,
            CommandKey::new("messaging-team-read").expect("static read key"),
        )
    }

    async fn verify_authority(&mut self) -> Result<MessagingActor, Error> {
        command_ledger::require_serializable(self.connection, "MESSAGING_SERIALIZABLE_REQUIRED")
            .await?;
        let actor = verify_scope(&mut *self.connection, &self.scope).await?;
        self.actor = Some(actor.clone());
        Ok(actor)
    }

    fn operation(change: &TeamChange) -> &'static str {
        match change {
            TeamChange::CreateTeam { .. } => "messaging.team-create",
            TeamChange::CreateChannel { .. } => "messaging.channel-create",
        }
    }

    fn record_id(key: &CommandKey, change: &TeamChange) -> String {
        match change {
            TeamChange::CreateTeam { .. } => format!("team-{}", key.as_str()),
            TeamChange::CreateChannel { .. } => format!("channel-{}", key.as_str()),
        }
    }

    pub async fn team(&mut self, id: &str) -> Result<Option<TeamSnapshot>, Error> {
        read_team(&mut *self.connection, &self.scope.company_id, id).await
    }

    pub async fn channels(
        &mut self,
        team_id: &str,
        limit: usize,
    ) -> Result<Vec<ChannelView>, Error> {
        self.verify_authority().await?;
        read_team_channels(
            &mut *self.connection,
            &self.scope.company_id,
            team_id,
            limit,
        )
        .await
    }

    pub async fn members(&mut self, team_id: &str) -> Result<Vec<String>, Error> {
        self.verify_authority().await?;
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM messaging_team_members WHERE team_id=$1 ORDER BY user_id",
        )
        .bind(team_id)
        .fetch_all(&mut *self.connection)
        .await?)
    }
}

impl MessagingTeamUnit for PgMessagingTeamUnit<'_> {
    type Error = Error;

    async fn authorize(&mut self, _access: TeamAccess) -> Result<MessagingActor, Error> {
        if let Some(actor) = &self.actor {
            return Ok(actor.clone());
        }
        self.verify_authority().await
    }

    async fn replay(&mut self, change: &TeamChange) -> Result<Option<TeamReceipt>, Error> {
        let actor = self.verify_authority().await?;
        let operation = Self::operation(change);
        let record_id = Self::record_id(&self.key, change);
        let request = fingerprint(operation, change);
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            actor.user_id.as_str(),
            &record_id,
            &request,
        )
        .await?;
        self.change = Some(change.clone());
        self.request_fingerprint = Some(request);
        self.before_apply = true;
        match value {
            Some(value) => {
                let receipt = decode_team_receipt(&value)?;
                self.receipt = Some(receipt.clone());
                Ok(Some(receipt))
            }
            None => {
                self.receipt = None;
                Ok(None)
            }
        }
    }

    async fn team(&mut self, id: &str) -> Result<Option<TeamSnapshot>, Error> {
        self.team(id).await
    }

    async fn is_company_member(&mut self, user_id: &str) -> Result<bool, Error> {
        company_member(&mut *self.connection, &self.scope, user_id).await
    }

    async fn apply(
        &mut self,
        actor: &MessagingActor,
        key: &CommandKey,
        plan: TeamPlan,
    ) -> Result<TeamReceipt, Error> {
        self.verify_authority().await?;
        if actor.company_id != self.scope.company_id.as_str()
            || actor.user_id != self.scope.user_id.as_str()
        {
            return Err(source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging actor changed inside the retained unit.",
            ));
        }
        if key.as_str() != self.key.as_str() {
            return Err(source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging command key changed inside the retained unit.",
            ));
        }
        let request_fingerprint = self.request_fingerprint.clone().ok_or_else(|| {
            source_changed(
                "MESSAGING_REPLAY_REQUIRED",
                "The Messaging team command must resolve its receipt before applying.",
            )
        })?;
        let receipt =
            apply_team_plan(&mut *self.connection, actor, key, &plan, self.scope.now).await?;
        let effects = retain_team_effects(&mut *self.connection, &plan, &receipt).await?;
        let (record_type, purpose, event_type) = match &plan {
            TeamPlan::CreateTeam { .. } => (
                "messaging_team",
                "Create workplace team",
                "messaging.team.created",
            ),
            TeamPlan::CreateChannel { .. } => (
                "messaging_room",
                "Create workplace channel",
                "messaging.channel.created",
            ),
        };
        let value = serde_json::to_value(&receipt).map_err(|_| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The Messaging team result could not be encoded.",
            )
        })?;
        command_ledger::write_ledger(
            &mut *self.connection,
            command_ledger::LedgerRows {
                key: key.as_str(),
                operation: Self::operation(self.change.as_ref().ok_or_else(|| {
                    source_changed(
                        "MESSAGING_REPLAY_REQUIRED",
                        "The Messaging team command lost its retained change.",
                    )
                })?),
                actor: actor.user_id.as_str(),
                assignment: self.scope.assignment_id.as_str(),
                record_id: &receipt.id,
                record_version: receipt.version,
                fingerprint: &request_fingerprint,
                result: &value,
                record_type,
                purpose,
                event_type,
                now: self.scope.now,
            },
        )
        .await?;
        self.current = Some(effects);
        self.actor = Some(actor.clone());
        self.receipt = Some(receipt.clone());
        self.before_apply = false;
        Ok(receipt)
    }

    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn require_unit_current(&mut self) -> Result<(), Error> {
        self.actor = None;
        self.verify_authority().await?;
        if let Some(current) = &self.current {
            current.require_current(&mut *self.connection).await?;
        }
        if self.before_apply {
            return Ok(());
        }
        let change = self.change.as_ref().ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging team command lost its retained change.",
            )
        })?;
        let operation = Self::operation(change);
        let record_id = Self::record_id(&self.key, change);
        let request = self.request_fingerprint.as_deref().ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging team command lost its request fingerprint.",
            )
        })?;
        let value = command_ledger::replay(
            &mut *self.connection,
            self.key.as_str(),
            operation,
            self.scope.user_id.as_str(),
            &record_id,
            request,
        )
        .await?
        .ok_or_else(|| {
            source_changed(
                "MESSAGING_SOURCE_CHANGED",
                "The Messaging team receipt disappeared.",
            )
        })?;
        if let Some(expected) = &self.receipt {
            if decode_team_receipt(&value)? != *expected {
                return Err(source_changed(
                    "MESSAGING_SOURCE_CHANGED",
                    "The Messaging team receipt changed.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_key_matches_the_stable_sorted_pair_contract() {
        assert_eq!(
            direct_key_input("company", &["a".into(), "b".into()]),
            "company:[\"a\",\"b\"]"
        );
    }

    #[test]
    fn scope_requires_identifiers_and_falls_back_to_the_user_label() {
        let scope = MessagingScope::new("company", "user", "assignment", " ", 1).unwrap();
        assert_eq!(scope.actor().display_name, "user");
        assert!(MessagingScope::new("company", "", "assignment", "User", 1).is_err());
    }
}
