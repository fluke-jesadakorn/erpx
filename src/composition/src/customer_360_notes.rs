//! Native Customer 360 contact-note composition.
//!
//! The HTTP transport supplies only a bounded source, note values and an
//! idempotency key. Current Account/Holding authority, source visibility,
//! same-person policy, the serializable unit and the Workflow receipt remain
//! here. No TypeScript note writer is used.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_customer_360::public::customer_notes::{
    ChangeCustomerNoteInput, CreateCustomerNoteInput, CustomerNoteCommandResult,
    CustomerNoteHistoryRow, CustomerNoteOrder, CustomerNoteRow, CustomerNoteSource,
    CustomerNotesReadActor, CustomerNotesView, customer_unavailable, note_changed,
    note_owner_required, note_unavailable, read_customer_notes,
};
use erpx_server_customer_360::public::customer_notes_http::{
    CustomerNotesHttpBackend, CustomerNotesHttpCommandRequest, CustomerNotesHttpFuture,
    CustomerNotesHttpReadRequest, CustomerNotesHttpTarget, customer_notes_http_router,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{holding_mutation_request, parse_inbound_account_request},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{
    PersistenceError, PgTransactionError, PgWorkFuture,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandEventScope, CommandField, CommandJson, CommandOutcome,
    PgCommandAuthority, PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_NOTE_UNAVAILABLE",
        "The contact note is unavailable.",
    )
}
fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_360_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}
fn command_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_NOTE_COMMAND_INVALID",
        "Use one bounded contact-note command.",
    )
}
fn command_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your work role changed. Reload and review before submitting.",
    )
}
fn same_or_manager(current: &AuthorizedWorkContext, created_by: &str) -> bool {
    current.same_person_user_raw(created_by) || current.has_permission("commerce.approve")
}

#[derive(Clone, Copy)]
struct NotesSources;

impl HoldingWorkSourcePreparation for NotesSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Debug)]
enum NoteCommand {
    CreateOrder {
        order_id: String,
        expected_version: i64,
        channel: String,
        occurred_at: i64,
        body: String,
    },
    CreateCustomer {
        customer_id: String,
        expected_version: i64,
        channel: String,
        occurred_at: i64,
        body: String,
    },
    Change {
        id: String,
        expected_version: i64,
        action: String,
        body: String,
        reason: String,
    },
}

impl NoteCommand {
    fn parse(body: &Value) -> Result<Self, BusinessError> {
        if body.get("id").is_some() {
            let input: ChangeCustomerNoteInput =
                serde_json::from_value(body.clone()).map_err(|_| command_invalid())?;
            input.validate()?;
            return Ok(Self::Change {
                id: input.id,
                expected_version: input.expected_version,
                action: input.action,
                body: input.body,
                reason: input.reason,
            });
        }
        let input: CreateCustomerNoteInput =
            serde_json::from_value(body.clone()).map_err(|_| command_invalid())?;
        input.validate(i64::MAX)?;
        match (input.order_id, input.customer_id) {
            (Some(order_id), None) => Ok(Self::CreateOrder {
                order_id,
                expected_version: input.expected_version,
                channel: input.channel,
                occurred_at: input.occurred_at,
                body: input.body,
            }),
            (None, Some(customer_id)) => Ok(Self::CreateCustomer {
                customer_id,
                expected_version: input.expected_version,
                channel: input.channel,
                occurred_at: input.occurred_at,
                body: input.body,
            }),
            _ => Err(command_invalid()),
        }
    }
    fn operation(&self) -> &'static str {
        match self {
            Self::CreateOrder { .. } | Self::CreateCustomer { .. } => "customer.note.create",
            Self::Change { action, .. } if action == "correct" => "customer.note.correct",
            Self::Change { .. } => "customer.note.archive",
        }
    }
    fn record_id(&self, key: &str) -> String {
        match self {
            Self::CreateOrder { .. } | Self::CreateCustomer { .. } => format!("contact-{key}"),
            Self::Change { id, .. } => id.clone(),
        }
    }
}

fn workflow_request(command: &NoteCommand, key: &str, body: &Value) -> WorkflowRequest {
    WorkflowRequest {
        key: key.to_owned(),
        operation: command.operation().to_owned(),
        record_id: command.record_id(key),
        input: CommandJson::from_serializable(body).expect("bounded JSON"),
    }
}

struct NotesAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    expected: WorkflowRequest,
}

impl<C: Clock + Sync> NotesAuthority<'_, C> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("commerce.sales")?;
        if request.key != self.expected.key
            || request.operation != self.expected.operation
            || request.record_id != self.expected.record_id
            || request.input.as_str() != self.expected.input.as_str()
        {
            return Err(command_changed());
        }
        if self.clock.now().get() >= self.current.expires_at() {
            return Err(command_changed());
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}

impl<C: Clock + Sync> PgCommandAuthority for NotesAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(request)?)
    }
    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(request)? != actor {
            return Err(command_changed().into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Prepared {
    Create {
        note_id: String,
        customer_id: String,
        order_id: Option<String>,
        channel: String,
        occurred_at: i64,
        body: String,
        event_scope: CommandEventScope,
    },
    Change {
        note_id: String,
        version: i64,
        status: String,
        body: String,
        reason: String,
        action: String,
        event_scope: CommandEventScope,
    },
}

struct NotesEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    command: NoteCommand,
    prepared: Option<Prepared>,
}

fn optional_string(field: &CommandField<String>) -> Option<&str> {
    match field {
        CommandField::Value(value) => Some(value.as_str()),
        CommandField::Null | CommandField::Omitted => None,
    }
}
fn event_scope(
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> CommandEventScope {
    CommandEventScope {
        company_id: company.clone(),
        branch_id: branch
            .map(|value| CommandField::Value(value.to_owned()))
            .unwrap_or(CommandField::Null),
        department_id: department
            .map(|value| CommandField::Value(value.to_owned()))
            .unwrap_or(CommandField::Null),
    }
}

impl<C: Clock + Sync> PgOwnerCommandEffects for NotesEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE customer_contact_notes, customer_contact_note_history \
             IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        )
        .execute(&mut *connection)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
        Ok(())
    }
    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        if command.operation != self.command.operation()
            || command.record_id != self.command.record_id(&command.key)
        {
            return Err(command_invalid().into());
        }
        let now = self.clock.now().get();
        let company = actor.company_id.as_str();
        let branch = optional_string(&actor.branch_id);
        let department = optional_string(&actor.department_id);
        let (prepared, result) = match &self.command {
            NoteCommand::CreateOrder {
                order_id,
                expected_version,
                channel,
                occurred_at,
                body,
            } => {
                let row = sqlx::query(
                    "SELECT o.party_id,o.branch_id,o.department_id,o.version \
                     FROM commerce_orders o JOIN finance_parties p \
                       ON p.id=o.party_id AND p.company_id=o.company_id \
                     WHERE o.id=$1 AND o.company_id=$2 AND o.kind IN ('sales','quote') \
                       AND p.kind='customer' \
                       AND ($3::text IS NULL OR o.branch_id=$3) \
                       AND ($4::text IS NULL OR o.department_id=$4) \
                     FOR SHARE OF o,p",
                )
                .bind(order_id)
                .bind(company)
                .bind(branch)
                .bind(department)
                .fetch_optional(&mut *connection)
                .await?;
                let Some(row) = row else {
                    return Err(note_changed().into());
                };
                if row.try_get::<i64, _>("version")? != *expected_version {
                    return Err(note_changed().into());
                }
                let note_id = format!("contact-{}", command.key);
                let order_branch: Option<String> = row.try_get("branch_id")?;
                let order_department: Option<String> = row.try_get("department_id")?;
                let prepared = Prepared::Create {
                    note_id: note_id.clone(),
                    customer_id: row.try_get("party_id")?,
                    order_id: Some(order_id.clone()),
                    channel: channel.clone(),
                    occurred_at: *occurred_at,
                    body: body.clone(),
                    event_scope: event_scope(
                        &actor.company_id,
                        order_branch.as_deref(),
                        order_department.as_deref(),
                    ),
                };
                (
                    prepared,
                    CommandJson::from_serializable(&CustomerNoteCommandResult {
                        id: note_id,
                        version: 1,
                        status: "active".into(),
                    })?,
                )
            }
            NoteCommand::CreateCustomer {
                customer_id,
                expected_version,
                channel,
                occurred_at,
                body,
            } => {
                let row = sqlx::query(
                    "SELECT id FROM finance_parties \
                     WHERE id=$1 AND company_id=$2 AND kind='customer' AND version=$3 \
                     FOR SHARE",
                )
                .bind(customer_id)
                .bind(company)
                .bind(expected_version)
                .fetch_optional(&mut *connection)
                .await?;
                if row.is_none() {
                    return Err(note_changed().into());
                }
                let note_id = format!("contact-{}", command.key);
                let prepared = Prepared::Create {
                    note_id: note_id.clone(),
                    customer_id: customer_id.clone(),
                    order_id: None,
                    channel: channel.clone(),
                    occurred_at: *occurred_at,
                    body: body.clone(),
                    event_scope: event_scope(&actor.company_id, None, None),
                };
                (
                    prepared,
                    CommandJson::from_serializable(&CustomerNoteCommandResult {
                        id: note_id,
                        version: 1,
                        status: "active".into(),
                    })?,
                )
            }
            NoteCommand::Change {
                id,
                expected_version,
                action,
                body,
                reason,
            } => {
                let row = sqlx::query(
                    "SELECT n.created_by,n.version,n.status,n.order_id,o.branch_id,o.department_id \
                     FROM customer_contact_notes n \
                     LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                     WHERE n.id=$1 AND n.company_id=$2 \
                     FOR SHARE OF n",
                )
                .bind(id)
                .bind(company)
                .fetch_optional(&mut *connection)
                .await?;
                let Some(row) = row else {
                    return Err(note_unavailable().into());
                };
                if row.try_get::<i64, _>("version")? != *expected_version
                    || row.try_get::<String, _>("status")? != "active"
                {
                    return Err(note_changed().into());
                }
                let created_by: String = row.try_get("created_by")?;
                if !same_or_manager(self.current, &created_by) {
                    return Err(note_owner_required().into());
                }
                let status = if action == "archive" {
                    "archived"
                } else {
                    "active"
                };
                let note_branch: Option<String> = row.try_get("branch_id")?;
                let note_department: Option<String> = row.try_get("department_id")?;
                let version = expected_version + 1;
                let prepared = Prepared::Change {
                    note_id: id.clone(),
                    version,
                    status: status.into(),
                    body: body.clone(),
                    reason: reason.clone(),
                    action: action.clone(),
                    event_scope: event_scope(
                        &actor.company_id,
                        note_branch.as_deref(),
                        note_department.as_deref(),
                    ),
                };
                (
                    prepared,
                    CommandJson::from_serializable(&CustomerNoteCommandResult {
                        id: id.clone(),
                        version,
                        status: status.into(),
                    })?,
                )
            }
        };
        if self.clock.now().get() < 0 || now < 0 {
            return Err(command_changed().into());
        }
        self.prepared = Some(prepared.clone());
        let event_scope = match &prepared {
            Prepared::Create { event_scope, .. } | Prepared::Change { event_scope, .. } => {
                event_scope.clone()
            }
        };
        Ok(WorkflowOutput {
            version: match &prepared {
                Prepared::Create { .. } => 1,
                Prepared::Change { version, .. } => *version,
            },
            result,
            event_scope: Some(event_scope),
        })
    }
    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        let now = self.clock.now().get();
        match prepared {
            Prepared::Create {
                note_id,
                customer_id,
                order_id,
                channel,
                occurred_at,
                body,
                ..
            } => {
                sqlx::query(
                    "INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,created_by,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)",
                )
                .bind(note_id)
                .bind(actor.company_id.as_str())
                .bind(customer_id)
                .bind(order_id)
                .bind(channel)
                .bind(occurred_at)
                .bind(body)
                .bind(actor.user_id.as_str())
                .bind(now)
                .execute(&mut *connection)
                .await?;
                sqlx::query(
                    "INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) \
                     VALUES($1,$2,1,'create',$3,'Initial contact record',$4,$5)",
                )
                .bind(format!("contact-event-{}", command.key))
                .bind(note_id)
                .bind(body)
                .bind(actor.user_id.as_str())
                .bind(now)
                .execute(&mut *connection)
                .await?;
            }
            Prepared::Change {
                note_id,
                version,
                status,
                body,
                reason,
                action,
                ..
            } => {
                let changed = sqlx::query(
                    "UPDATE customer_contact_notes SET body=$1,status=$2,version=$3,updated_at=$4 \
                     WHERE id=$5 AND company_id=$6",
                )
                .bind(body)
                .bind(status)
                .bind(version)
                .bind(now)
                .bind(note_id)
                .bind(actor.company_id.as_str())
                .execute(&mut *connection)
                .await?;
                if changed.rows_affected() != 1 {
                    return Err(note_changed().into());
                }
                sqlx::query(
                    "INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(format!("contact-event-{}", command.key))
                .bind(note_id)
                .bind(version)
                .bind(action)
                .bind(body)
                .bind(reason)
                .bind(actor.user_id.as_str())
                .bind(now)
                .execute(&mut *connection)
                .await?;
            }
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        match prepared {
            Prepared::Create {
                note_id,
                customer_id,
                order_id,
                channel,
                occurred_at,
                body,
                ..
            } => {
                let row = sqlx::query(
                    "SELECT customer_id,order_id,channel,occurred_at,body,status,version \
                     FROM customer_contact_notes WHERE id=$1 AND company_id=$2",
                )
                .bind(note_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *connection)
                .await?;
                let Some(row) = row else {
                    return Err(note_changed().into());
                };
                if row.try_get::<String, _>("customer_id")? != *customer_id
                    || row.try_get::<Option<String>, _>("order_id")? != *order_id
                    || row.try_get::<String, _>("channel")? != *channel
                    || row.try_get::<i64, _>("occurred_at")? != *occurred_at
                    || row.try_get::<String, _>("body")? != *body
                    || row.try_get::<String, _>("status")? != "active"
                    || row.try_get::<i64, _>("version")? != 1
                {
                    return Err(note_changed().into());
                }
            }
            Prepared::Change {
                note_id,
                version,
                status,
                body,
                ..
            } => {
                let row = sqlx::query(
                    "SELECT body,status,version FROM customer_contact_notes \
                     WHERE id=$1 AND company_id=$2",
                )
                .bind(note_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *connection)
                .await?;
                let Some(row) = row else {
                    return Err(note_changed().into());
                };
                if row.try_get::<String, _>("body")? != *body
                    || row.try_get::<String, _>("status")? != *status
                    || row.try_get::<i64, _>("version")? != *version
                {
                    return Err(note_changed().into());
                }
            }
        }
        Ok(())
    }
}

struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

async fn execute_note_command<C: Clock + Sync>(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    command: NoteCommand,
    request: &WorkflowRequest,
) -> Result<CommandOutcome, Error> {
    let mut authority = NotesAuthority {
        current,
        clock,
        expected: request.clone(),
    };
    let mut effects = NotesEffects {
        current,
        clock,
        command,
        prepared: None,
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        connection,
        &mut authority,
        clock,
        &mut Ids,
    )
    .execute(request, &mut effects)
    .await
}

pub struct CustomerNotesService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: crate::customer_360::Customer360Configuration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CustomerNotesService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: crate::customer_360::Customer360Configuration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }
    fn access(permission: &str, module: &str) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: permission.into(),
            module: Some(module.into()),
            destination: "customer_360".into(),
            api: true,
        }
    }
    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        customer_id: Option<String>,
        order_id: Option<String>,
    ) -> Result<CustomerNotesView, Error> {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                input,
                Self::access("commerce.read", "customer_360"),
                NotesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let customer_id = customer_id.clone();
                    let order_id = order_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = CustomerNotesReadActor {
                            company_id: current
                                .selection()
                                .workspace()
                                .company_id
                                .as_str()
                                .to_owned(),
                            branch_id: current
                                .selection()
                                .workspace()
                                .branch_id
                                .as_ref()
                                .map(|id| id.as_str().to_owned()),
                            department_id: current
                                .selection()
                                .workspace()
                                .department_id
                                .as_ref()
                                .map(|id| id.as_str().to_owned()),
                            user_id: current.user_id().as_str().to_owned(),
                            can_create: current.has_permission("commerce.sales"),
                        };
                        let mut view = read_customer_notes(
                            connection,
                            &actor,
                            customer_id.as_deref(),
                            order_id.as_deref(),
                        )
                        .await?;
                        for note in &mut view.notes {
                            note.can_change = actor.can_create
                                && same_or_manager(current, &note.created_by);
                        }
                        Ok(view)
                    })
                },
            )
            .await
    }
    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        mut input: HoldingWorkspaceRequest,
        key: String,
        body: Value,
    ) -> Result<CustomerNoteCommandResult, Error> {
        input.mutation = true;
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let command = NoteCommand::parse(&body).map_err(PgTransactionError::Business)?;
        let request = workflow_request(&command, &key, &body);
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                input,
                Self::access("commerce.sales", "customer_360"),
                NotesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let request = request.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        current.require_permission("commerce.sales")?;
                        let outcome = execute_note_command(
                            connection,
                            current,
                            clock.as_ref(),
                            command,
                            &request,
                        )
                        .await?;
                        let value: Value = serde_json::from_str(outcome.result.as_str())
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        let result: CustomerNoteCommandResult = serde_json::from_value(value)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(result)
                    })
                },
            )
            .await
    }
}

pub struct CustomerNotesBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CustomerNotesService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CustomerNotesBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CustomerNotesService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }
    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(customer_unavailable());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(customer_unavailable());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            holding_mutation_request(headers, branch, department)?,
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> CustomerNotesHttpBackend for CustomerNotesBackend<C> {
    fn read<'a>(
        &'a self,
        request: CustomerNotesHttpReadRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNotesView> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(
                    request.host,
                    proof,
                    selection,
                    request.customer_id.map(str::to_owned),
                    request.order_id.map(str::to_owned),
                )
                .await
                .map_err(business)
        })
    }
    fn command<'a>(
        &'a self,
        request: CustomerNotesHttpCommandRequest<'a>,
    ) -> CustomerNotesHttpFuture<'a, CustomerNoteCommandResult> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.key.to_owned(),
                    request.body.clone(),
                )
                .await
                .map_err(business)
        })
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CustomerNotesHttpTarget>) -> Result<axum::Router, BusinessError> {
    customer_notes_http_router(targets)
}
