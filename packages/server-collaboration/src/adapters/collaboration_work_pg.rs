//! Production Collaboration command/read unit over the caller's connection.
//!
//! Composition supplies the selected Work scope and a source port that
//! delegates to Records and the owning module. This module verifies the current
//! assignment, principal, module and `records.write` authority on the caller's
//! serializable connection, reads the owner rows, stages receipt/audit/outbox
//! and never commits, retries or opens a transaction.
use crate::adapters::collaboration_work::{
    CurrentCollaborationEffects, apply_plan, read_card, read_relation, retain_effects,
};
use crate::adapters::command_ledger::{self, Error, fingerprint, source_changed};
use crate::application::collaboration_work::{
    CollaborationSourcePort, CollaborationUnit, SourcePresentation,
};
use crate::domain::collaboration_work::{
    AuthorizedActor, CardAction, Change, CurrentCard, CurrentRelation, OwnerPlan, Receipt,
    WORKSPACE_SOURCE_TYPES,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, UserId,
};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

type Unit = (String, i32, String, String);
type Eligibility = (String, Option<String>, Option<String>);

fn changed() -> Error {
    source_changed(
        "COLLABORATION_SOURCE_CHANGED",
        "The collaboration record changed. Refresh and retry.",
    )
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    value.get(key).and_then(Value::as_str).ok_or_else(changed)
}

/// Selected Work facts supplied by composition. The unit revalidates the
/// assignment and principal on every call; the selected branch/department is
/// the same frame the retained action-card read applies.
#[derive(Clone, Debug)]
pub struct CollaborationScope {
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub assignment_version: i64,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub now: i64,
}

impl CollaborationScope {
    pub fn new(
        company_id: impl Into<String>,
        user_id: impl Into<String>,
        assignment_id: impl Into<String>,
        now: i64,
    ) -> Result<Self, BusinessError> {
        Ok(Self {
            user_id: UserId::new(user_id)?,
            company_id: CompanyId::new(company_id)?,
            assignment_id: AssignmentId::new(assignment_id)?,
            assignment_version: 0,
            branch_id: None,
            department_id: None,
            now,
        })
    }

    pub fn assignment_version(mut self, version: i64) -> Self {
        self.assignment_version = version;
        self
    }

    pub fn branch(mut self, id: Option<&str>) -> Self {
        self.branch_id = id.filter(|value| !value.is_empty()).map(str::to_owned);
        self
    }

    pub fn department(mut self, id: Option<&str>) -> Self {
        self.department_id = id.filter(|value| !value.is_empty()).map(str::to_owned);
        self
    }
}

/// Row caps retained for the bounded legacy workspace read.
pub const WORKSPACE_RELATION_LIMIT: i64 = 500;
pub const WORKSPACE_CARD_LIMIT: i64 = 500;
pub const WORKSPACE_PEOPLE_LIMIT: i64 = 200;

/// Display facts for one source the source owner made visible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSource {
    pub kind: SourceKind,
    pub id: String,
    pub title: String,
    pub href: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// Resolved display facts plus the retained source witness.
#[derive(Clone, Debug)]
pub struct VisibleSource<W> {
    pub source: ResolvedSource,
    witness: W,
}

impl<W> VisibleSource<W> {
    pub fn witness(&self) -> &W {
        &self.witness
    }

    pub fn into_parts(self) -> (ResolvedSource, W) {
        (self.source, self.witness)
    }
}

/// One simple `record_relations`/`action_cards` visibility read over the caller
/// connection plus the source witnesses it retained. No receipt or ledger row is
/// written and nothing is committed.
#[derive(Clone, Debug)]
pub struct CollaborationWorkspace<W> {
    pub relations: Vec<Value>,
    pub cards: Vec<Value>,
    pub people: Vec<Value>,
    pub source_types: Vec<&'static str>,
    pub can_write: bool,
    pub can_review: bool,
    sources: Vec<VisibleSource<W>>,
}

impl<W> CollaborationWorkspace<W> {
    pub fn witness_count(&self) -> usize {
        self.sources.len()
    }

    pub fn visible_sources(&self) -> &[VisibleSource<W>] {
        &self.sources
    }

    /// Revalidate every retained source witness on the caller's transaction.
    pub async fn require_current<S>(&self, sources: &mut S) -> Result<(), Error>
    where
        S: CollaborationSourcePort<Error = Error, Witness = W>,
        W: Sync,
    {
        for visible in &self.sources {
            sources.require_current(&visible.witness).await?;
        }
        Ok(())
    }
}

/// True when the source owner hid the record (retained 403/404 null behaviour).
/// Any other failure stays an error and is never turned into a hidden row.
pub fn hidden_source(error: &Error) -> bool {
    matches!(
        error,
        PgTransactionError::Business(BusinessError {
            kind: FailureKind::Forbidden | FailureKind::NotFound,
            ..
        })
    )
}

/// A source is visible only when the prepared record matches the requested
/// identity and the caller's Company. Titles fall back to the prepared record;
/// an absent destination is never invented.
pub fn resolve_source(
    company: &str,
    kind: SourceKind,
    id: &str,
    record: SourceRecord,
    presentation: SourcePresentation,
) -> Option<ResolvedSource> {
    if record.kind != kind || record.id != id || record.company_id != company {
        return None;
    }
    Some(ResolvedSource {
        kind,
        id: id.into(),
        title: presentation.title.unwrap_or(record.title),
        href: presentation.href,
        branch_id: record.branch_id,
        department_id: record.department_id,
    })
}

pub struct PgCollaborationUnit<'c, S>
where
    S: CollaborationSourcePort<Error = Error>,
{
    connection: &'c mut PgConnection,
    scope: CollaborationScope,
    key: CommandKey,
    sources: &'c mut S,
    actor: Option<AuthorizedActor>,
    can_write: Option<bool>,
    current: Option<CurrentCollaborationEffects>,
    unit: Option<Unit>,
    change: Option<Change>,
    request_fingerprint: Option<String>,
    receipt: Option<Receipt>,
    before_apply: bool,
    read_only: bool,
    eligibility: Vec<Eligibility>,
    card_scope: Option<(String, Option<String>, Option<String>)>,
    witnesses: Vec<S::Witness>,
}

impl<'c, S> PgCollaborationUnit<'c, S>
where
    S: CollaborationSourcePort<Error = Error>,
{
    /// The caller must already hold its serializable transaction and all common
    /// IAM/People/Records source locks plus
    /// [`crate::adapters::collaboration_work::lock_collaboration_work_sources`].
    pub fn new(
        connection: &'c mut PgConnection,
        scope: CollaborationScope,
        key: CommandKey,
        sources: &'c mut S,
    ) -> Self {
        Self {
            connection,
            scope,
            key,
            sources,
            actor: None,
            can_write: None,
            current: None,
            unit: None,
            change: None,
            request_fingerprint: None,
            receipt: None,
            before_apply: true,
            read_only: false,
            eligibility: vec![],
            card_scope: None,
            witnesses: vec![],
        }
    }

    /// Read-only composition convenience. No receipt or ledger row is written.
    pub fn read_only(
        connection: &'c mut PgConnection,
        scope: CollaborationScope,
        sources: &'c mut S,
    ) -> Self {
        let mut unit = Self::new(
            connection,
            scope,
            CommandKey::new("collaboration-read").expect("static read key"),
            sources,
        );
        unit.read_only = true;
        unit
    }

    async fn unit(&mut self) -> Result<Unit, Error> {
        command_ledger::require_serializable(
            self.connection,
            "COLLABORATION_SERIALIZABLE_REQUIRED",
        )
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

    async fn verify_authority(&mut self, write: bool) -> Result<AuthorizedActor, Error> {
        self.unit().await?;
        let row = sqlx::query(
            "SELECT a.version,a.role,a.modules_json::jsonb @> '[\"collaboration\"]'::jsonb \
             FROM assignments a \
             WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.revoked_at IS NULL \
             AND a.valid_from<=$4 AND a.valid_until>$4 \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)",
        )
        .bind(self.scope.assignment_id.as_str())
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.user_id.as_str())
        .bind(self.scope.now)
        .fetch_optional(&mut *self.connection)
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
        if self.scope.assignment_version > 0 && version != self.scope.assignment_version {
            return Err(source_changed(
                "COLLABORATION_SOURCE_CHANGED",
                "Your selected assignment changed. Refresh and retry.",
            ));
        }
        let role: String = row.try_get(1)?;
        if !row.try_get::<bool, _>(2)? {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Collaboration.",
            )
            .into());
        }
        let permission = if write {
            "records.write"
        } else {
            "records.read"
        };
        let permitted: bool = sqlx::query_scalar("SELECT iam_role_allows($1,$2,$3)")
            .bind(&role)
            .bind(self.scope.company_id.as_str())
            .bind(permission)
            .fetch_one(&mut *self.connection)
            .await?;
        if !permitted {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                if write {
                    "RECORDS_WRITE_REQUIRED"
                } else {
                    "RECORDS_READ_REQUIRED"
                },
                if write {
                    "Your current role cannot change module records."
                } else {
                    "Your current role cannot read module records."
                },
            )
            .into());
        }
        let can_review: bool = sqlx::query_scalar("SELECT iam_role_allows($1,$2,'records.review')")
            .bind(&role)
            .bind(self.scope.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        let can_write = if write {
            true
        } else {
            sqlx::query_scalar("SELECT iam_role_allows($1,$2,'records.write')")
                .bind(&role)
                .bind(self.scope.company_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?
        };
        self.can_write = Some(can_write);
        let actor = AuthorizedActor {
            user_id: self.scope.user_id.as_str().into(),
            company_id: self.scope.company_id.as_str().into(),
            can_review,
        };
        self.actor = Some(actor.clone());
        Ok(actor)
    }

    /// Resolve one stored source for display through the source port. Missing,
    /// foreign or source-owner-hidden rows are `None`, matching the retained
    /// `visibleSource` null behaviour; no destination is invented here.
    pub async fn visible_source(
        &mut self,
        kind: SourceKind,
        id: &str,
    ) -> Result<Option<VisibleSource<S::Witness>>, Error> {
        let reference = SourceReference {
            kind,
            id: id.into(),
            expected_version: None,
        };
        let connection = &mut *self.connection;
        let (record, witness) = match self.sources.prepare_with(connection, &reference).await {
            Ok(prepared) => prepared,
            Err(error) if hidden_source(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let presentation = self.sources.present(&reference).await?;
        Ok(resolve_source(
            self.scope.company_id.as_str(),
            kind,
            id,
            record,
            presentation,
        )
        .map(|source| VisibleSource { source, witness }))
    }

    /// Legacy Collaboration workspace read: visible relations, visible cards
    /// with source display facts, eligible people, the non-Field source list and
    /// the caller's current write/review authority. Read-only and bounded; no
    /// owner effects or ledger rows are written.
    pub async fn workspace(&mut self) -> Result<CollaborationWorkspace<S::Witness>, Error> {
        let actor = self.verify_authority(false).await?;
        let mut sources = Vec::new();
        let relation_rows: Vec<String> = sqlx::query_scalar(
            "WITH s AS MATERIALIZED (SELECT * FROM collaboration_record_relations r \
             WHERE r.company_id=$1 AND r.status='active' \
             AND r.source_type<>'field_service_order' AND r.target_type<>'field_service_order' \
             ORDER BY r.updated_at DESC,r.id LIMIT $2) \
             SELECT to_jsonb(s)::text FROM s",
        )
        .bind(self.scope.company_id.as_str())
        .bind(WORKSPACE_RELATION_LIMIT)
        .fetch_all(&mut *self.connection)
        .await?;
        let mut relations = Vec::new();
        for raw in relation_rows {
            let row: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            let Ok(source_kind) = SourceKind::parse(text(&row, "source_type")?) else {
                continue;
            };
            let Ok(target_kind) = SourceKind::parse(text(&row, "target_type")?) else {
                continue;
            };
            let Some(left) = self
                .visible_source(source_kind, text(&row, "source_id")?)
                .await?
            else {
                continue;
            };
            let Some(right) = self
                .visible_source(target_kind, text(&row, "target_id")?)
                .await?
            else {
                continue;
            };
            sources.push(left);
            sources.push(right);
            relations.push(row);
        }
        let card_rows: Vec<String> = sqlx::query_scalar(
            "WITH s AS MATERIALIZED (SELECT c.*, \
               CASE WHEN EXISTS(SELECT 1 FROM assignments x \
                 JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 WHERE x.company_id=c.company_id AND x.user_id=c.owner_id \
                 AND iam_assignment_active(x.id,$5) AND iam_principal_active(x.user_id,x.company_id,$5) \
                 AND iam_role_allows(x.role,x.company_id,'records.write') \
                 AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=c.branch_id) \
                   OR (x.scope_kind='department' AND x.scope_node_id=c.department_id)) \
                 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active') \
               THEN 1 ELSE 0 END owner_eligible, \
               (SELECT legal_name FROM hr_people WHERE company_id=c.company_id AND user_id=c.owner_id \
                 AND merged_into_id IS NULL LIMIT 1) owner_name \
             FROM collaboration_action_cards c \
             WHERE c.company_id=$1 AND c.source_type<>'field_service_order' \
             AND ($2::text IS NULL OR c.branch_id=$2) \
             AND ($3::text IS NULL OR c.department_id=$3) \
             ORDER BY c.updated_at DESC,c.id LIMIT $4) \
             SELECT to_jsonb(s)::text FROM s",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.branch_id.as_deref())
        .bind(self.scope.department_id.as_deref())
        .bind(WORKSPACE_CARD_LIMIT)
        .bind(self.scope.now)
        .fetch_all(&mut *self.connection)
        .await?;
        let mut cards = Vec::new();
        for raw in card_rows {
            let mut card: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            let Ok(kind) = SourceKind::parse(text(&card, "source_type")?) else {
                continue;
            };
            let Some(source) = self.visible_source(kind, text(&card, "source_id")?).await? else {
                continue;
            };
            if let Value::Object(fields) = &mut card {
                fields.insert(
                    "source_title".into(),
                    Value::String(source.source.title.clone()),
                );
                if let Some(href) = &source.source.href {
                    fields.insert("source_href".into(), Value::String(href.clone()));
                }
            }
            sources.push(source);
            cards.push(card);
        }
        let people_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT DISTINCT p.user_id,p.legal_name FROM hr_people p \
             JOIN assignments x ON x.user_id=p.user_id AND x.company_id=p.company_id \
             WHERE p.company_id=$1 AND p.status='active' \
             AND iam_assignment_active(x.id,$2) \
             AND iam_role_allows(x.role,x.company_id,'records.write') \
             ORDER BY p.legal_name LIMIT $3",
        )
        .bind(self.scope.company_id.as_str())
        .bind(self.scope.now)
        .bind(WORKSPACE_PEOPLE_LIMIT)
        .fetch_all(&mut *self.connection)
        .await?;
        Ok(CollaborationWorkspace {
            relations,
            cards,
            people: people_rows
                .into_iter()
                .map(|(id, name)| json!({"id": id, "name": name}))
                .collect(),
            source_types: WORKSPACE_SOURCE_TYPES.to_vec(),
            can_write: self.can_write.unwrap_or(false),
            can_review: actor.can_review,
            sources,
        })
    }

    /// Retained owner eligibility for one action-card owner. A missing active
    /// engagement/assignment or `records.write` makes the owner ineligible.
    async fn eligible(
        &mut self,
        user_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<bool, Error> {
        let eligible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments x \
             JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id \
             JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
             WHERE x.company_id=$1 AND x.user_id=$2 \
             AND iam_assignment_active(x.id,$3) AND iam_principal_active(x.user_id,x.company_id,$3) \
             AND iam_role_allows(x.role,x.company_id,'records.write') \
             AND (x.scope_kind='holding' \
               OR (x.scope_kind='branch' AND x.scope_node_id=$4) \
               OR (x.scope_kind='department' AND x.scope_node_id=$5)) \
             AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')",
        )
        .bind(self.scope.company_id.as_str())
        .bind(user_id)
        .bind(self.scope.now)
        .bind(branch_id)
        .bind(department_id)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(eligible)
    }

    fn operation(change: &Change) -> &'static str {
        match change {
            Change::CreateRelation { .. } => "collaboration.relation.create",
            Change::RemoveRelation { .. } => "collaboration.relation.remove",
            Change::CreateCard { .. } => "collaboration.action.create",
            Change::ChangeCard {
                action: CardAction::Complete,
                ..
            } => "collaboration.action.complete",
            Change::ChangeCard {
                action: CardAction::Reassign,
                ..
            } => "collaboration.action.reassign",
            Change::ChangeCard {
                action: CardAction::Cancel,
                ..
            } => "collaboration.action.cancel",
        }
    }

    fn record_id(key: &CommandKey, change: &Change) -> String {
        match change {
            Change::CreateRelation { .. } => format!("record-relation-{}", key.as_str()),
            Change::CreateCard { .. } => format!("action-card-{}", key.as_str()),
            Change::RemoveRelation { id, .. } | Change::ChangeCard { id, .. } => id.clone(),
        }
    }

    fn ledger_labels(plan: &OwnerPlan) -> (&'static str, &'static str, &'static str) {
        match plan {
            OwnerPlan::CreateRelation { .. } => (
                "collaboration_record_relation",
                "Link related records",
                "collaboration.relation.created",
            ),
            OwnerPlan::RemoveRelation { .. } => (
                "collaboration_record_relation",
                "Remove related-record link",
                "collaboration.relation.removed",
            ),
            OwnerPlan::CreateCard { .. } => (
                "collaboration_action_card",
                "Assign cross-record action",
                "collaboration.action.created",
            ),
            OwnerPlan::ChangeCard { .. } => (
                "collaboration_action_card",
                "Update cross-record action",
                "collaboration.action.updated",
            ),
        }
    }

    /// Current relation for one command; authority is revalidated first.
    pub async fn current_relation(&mut self, id: &str) -> Result<CurrentRelation, Error> {
        let write = !self.read_only;
        self.verify_authority(write).await?;
        read_relation(&mut *self.connection, &self.scope.company_id, id)
            .await
            .map(|v| v.0)
    }

    /// Current action card for one command, including retained People
    /// eligibility. Authority is revalidated first.
    pub async fn current_card(&mut self, id: &str) -> Result<CurrentCard, Error> {
        let write = !self.read_only;
        self.verify_authority(write).await?;
        let (row, _) = read_card(
            &mut *self.connection,
            &self.scope.company_id,
            id,
            self.scope.branch_id.as_deref(),
            self.scope.department_id.as_deref(),
        )
        .await?;
        let owner_eligible = self
            .eligible(
                &row.owner_id,
                row.branch_id.as_deref(),
                row.department_id.as_deref(),
            )
            .await?;
        self.card_scope = Some((
            row.id.clone(),
            row.branch_id.clone(),
            row.department_id.clone(),
        ));
        Ok(CurrentCard {
            id: row.id,
            version: row.version,
            open: row.open,
            owner_id: row.owner_id,
            due_at: row.due_at,
            owner_eligible,
            source: row.source,
        })
    }
}

fn decode_receipt(value: &Value) -> Result<Receipt, Error> {
    let status = match value["status"].as_str() {
        Some("active") => "active",
        Some("removed") => "removed",
        Some("open") => "open",
        Some("completed") => "completed",
        Some("cancelled") => "cancelled",
        _ => {
            return Err(source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Collaboration receipt has an unknown status.",
            ));
        }
    };
    let id = value["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Collaboration receipt has no identifier.",
            )
        })?;
    let version = value["version"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The stored Collaboration receipt has no version.",
            )
        })?;
    Ok(Receipt {
        id: id.into(),
        version,
        status,
    })
}

impl<S> CollaborationSourcePort for PgCollaborationUnit<'_, S>
where
    S: CollaborationSourcePort<Error = Error>,
    S::Witness: Sync,
{
    type Error = Error;
    type Witness = S::Witness;

    async fn prepare(
        &mut self,
        source: &SourceReference,
    ) -> Result<(SourceRecord, Self::Witness), Error> {
        self.sources.prepare(source).await
    }

    async fn require_current(&mut self, witness: &Self::Witness) -> Result<(), Error> {
        self.sources.require_current(witness).await
    }
}

impl<S> CollaborationUnit for PgCollaborationUnit<'_, S>
where
    S: CollaborationSourcePort<Error = Error>,
    S::Witness: Sync,
{
    async fn authorize(&mut self) -> Result<AuthorizedActor, Error> {
        if let Some(actor) = &self.actor {
            return Ok(actor.clone());
        }
        let write = !self.read_only;
        self.verify_authority(write).await
    }

    async fn replay(&mut self, change: &Change) -> Result<Option<Receipt>, Error> {
        self.verify_authority(true).await?;
        let actor = self.actor.clone().expect("verified actor");
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

    async fn relation(&mut self, id: &str) -> Result<CurrentRelation, Error> {
        self.current_relation(id).await
    }

    async fn card(&mut self, id: &str) -> Result<CurrentCard, Error> {
        self.current_card(id).await
    }

    async fn apply(&mut self, actor: &AuthorizedActor, plan: OwnerPlan) -> Result<Receipt, Error> {
        self.verify_authority(true).await?;
        if actor.company_id != self.scope.company_id.as_str()
            || actor.user_id != self.scope.user_id.as_str()
        {
            return Err(source_changed(
                "COLLABORATION_SOURCE_CHANGED",
                "The Collaboration actor changed inside the retained unit.",
            ));
        }
        let request_fingerprint = self.request_fingerprint.clone().ok_or_else(|| {
            source_changed(
                "COLLABORATION_REPLAY_REQUIRED",
                "The Collaboration command must resolve its receipt before applying.",
            )
        })?;
        let eligibility = match &plan {
            OwnerPlan::CreateCard {
                source, owner_id, ..
            } => Some((
                owner_id.clone(),
                source.branch_id.clone(),
                source.department_id.clone(),
            )),
            OwnerPlan::ChangeCard {
                current,
                action: CardAction::Reassign,
                owner_id: Some(user),
                ..
            } => {
                let (id, branch, department) = self.card_scope.as_ref().ok_or_else(|| {
                    source_changed(
                        "COLLABORATION_SOURCE_CHANGED",
                        "The Collaboration card context was lost.",
                    )
                })?;
                if id != &current.id {
                    return Err(source_changed(
                        "COLLABORATION_SOURCE_CHANGED",
                        "The Collaboration card changed inside the retained unit.",
                    ));
                }
                Some((user.clone(), branch.clone(), department.clone()))
            }
            _ => None,
        };
        if let Some((user, branch, department)) = &eligibility
            && !self
                .eligible(user, branch.as_deref(), department.as_deref())
                .await?
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ACTION_CARD_OWNER_INELIGIBLE",
                "Choose an eligible action owner.",
            )
            .into());
        }
        let receipt = apply_plan(
            &mut *self.connection,
            actor,
            &self.key,
            &plan,
            self.scope.now,
        )
        .await?;
        let effects = retain_effects(
            &mut *self.connection,
            actor,
            &self.key,
            &plan,
            &receipt,
            self.scope.now,
        )
        .await?;
        let (record_type, purpose, event_type) = Self::ledger_labels(&plan);
        let value = serde_json::to_value(&receipt).map_err(|_| {
            source_changed(
                "COLLABORATION_RECEIPT_INVALID",
                "The Collaboration result could not be encoded.",
            )
        })?;
        command_ledger::write_ledger(
            &mut *self.connection,
            command_ledger::LedgerRows {
                key: self.key.as_str(),
                operation: Self::operation(self.change.as_ref().ok_or_else(|| {
                    source_changed(
                        "COLLABORATION_REPLAY_REQUIRED",
                        "The Collaboration command lost its retained change.",
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
        if let Some(owner) = eligibility {
            self.eligibility.push(owner);
        }
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
        for witness in std::mem::take(&mut self.witnesses) {
            let connection = &mut *self.connection;
            self.sources.require_current_with(connection, &witness).await?;
            self.witnesses.push(witness);
        }
        self.actor = None;
        let write = !self.read_only;
        self.verify_authority(write).await?;
        if let Some(current) = &self.current {
            current.require_current(&mut *self.connection).await?;
        }
        for (user, branch, department) in self.eligibility.clone() {
            if !self
                .eligible(&user, branch.as_deref(), department.as_deref())
                .await?
            {
                return Err(source_changed(
                    "COLLABORATION_SOURCE_CHANGED",
                    "The action owner eligibility changed. Refresh and retry.",
                ));
            }
        }
        if self.before_apply {
            return Ok(());
        }
        let change = self.change.as_ref().ok_or_else(|| {
            source_changed(
                "COLLABORATION_SOURCE_CHANGED",
                "The Collaboration command lost its retained change.",
            )
        })?;
        let operation = Self::operation(change);
        let record_id = Self::record_id(&self.key, change);
        let request = self.request_fingerprint.as_deref().ok_or_else(|| {
            source_changed(
                "COLLABORATION_SOURCE_CHANGED",
                "The Collaboration command lost its request fingerprint.",
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
                "COLLABORATION_SOURCE_CHANGED",
                "The Collaboration receipt disappeared.",
            )
        })?;
        if let Some(expected) = &self.receipt
            && decode_receipt(&value)? != *expected
        {
            return Err(source_changed(
                "COLLABORATION_SOURCE_CHANGED",
                "The Collaboration receipt changed.",
            ));
        }
        Ok(())
    }
}

/// Composition-ready command helper. The caller has already resolved the
/// session/scope and opened the retained serializable unit; this never starts a
/// transaction or mints authority.
pub async fn execute_scope<S>(
    connection: &mut PgConnection,
    scope: CollaborationScope,
    key: CommandKey,
    sources: &mut S,
    change: Change,
) -> Result<Receipt, PgTransactionError<BusinessError>>
where
    S: CollaborationSourcePort<Error = PgTransactionError<BusinessError>>,
    S::Witness: Sync,
{
    let mut unit = PgCollaborationUnit::new(connection, scope, key, sources);
    crate::application::collaboration_work::execute(&mut unit, change).await
}

/// Read-only card/relation helper for composition HTTP reads.
pub async fn read_scope_card<S>(
    connection: &mut PgConnection,
    scope: CollaborationScope,
    sources: &mut S,
    id: &str,
) -> Result<CurrentCard, PgTransactionError<BusinessError>>
where
    S: CollaborationSourcePort<Error = PgTransactionError<BusinessError>>,
    S::Witness: Sync,
{
    PgCollaborationUnit::read_only(connection, scope, sources)
        .current_card(id)
        .await
}

/// Read-only relation helper for composition HTTP reads.
pub async fn read_scope_relation<S>(
    connection: &mut PgConnection,
    scope: CollaborationScope,
    sources: &mut S,
    id: &str,
) -> Result<CurrentRelation, PgTransactionError<BusinessError>>
where
    S: CollaborationSourcePort<Error = PgTransactionError<BusinessError>>,
    S::Witness: Sync,
{
    PgCollaborationUnit::read_only(connection, scope, sources)
        .current_relation(id)
        .await
}

/// Read-only legacy workspace helper for composition HTTP reads.
pub async fn read_scope_workspace<S>(
    connection: &mut PgConnection,
    scope: CollaborationScope,
    sources: &mut S,
) -> Result<CollaborationWorkspace<S::Witness>, PgTransactionError<BusinessError>>
where
    S: CollaborationSourcePort<Error = PgTransactionError<BusinessError>>,
    S::Witness: Sync,
{
    PgCollaborationUnit::read_only(connection, scope, sources)
        .workspace()
        .await
}
