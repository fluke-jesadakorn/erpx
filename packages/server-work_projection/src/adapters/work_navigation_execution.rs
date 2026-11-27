//! Owner-side execution backend for the native Work navigation HTTP ingress.
//!
//! Composition injects the registered Holding Account policy, its authorized
//! work lease and the configured Holding identity. Everything below is owner
//! code: it binds the request to one serializable Holding transaction, runs
//! the application query/command and executes owner SQL through the owner
//! port. No session logic or payload shaping remains in TypeScript.
use crate::adapters::work_navigation_http::{
    WorkNavigationHttpBackend, WorkNavigationHttpFuture, WorkNavigationMarkHttpRequest,
    WorkNavigationMarkHttpResponse, WorkNavigationReadHttpRequest,
    WorkNavigationReadHttpResponse,
};
use crate::adapters::work_capability_probes::InventoryPreparation;
use crate::application::work_navigation::{
    MarkHomeReadCommand, NavigationFacts, NavigationQuery, WorkNavigationPort,
    mark_home_read as execute_mark_home_read, navigation_workspace_base, read_navigation,
};
use erpx_server_collaboration::public::staff_facts::unread_home_count;
use erpx_server_identity_access::public::account_inbound_http::{
    CapturedHoldingMutation, holding_mutation_request, parse_inbound_account_request, unique_request_header,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
    WorkAuthorizationRequest,
};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_identity_access::public::AccountPolicy;
use erpx_server_identity_access::public::{DeviceProof, RequestAudience};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, FailureKind, HoldingId, TimestampMillis,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_NAVIGATION_UNAVAILABLE",
        "The native Work navigation is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_NAVIGATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct NavigationSources;

impl HoldingWorkSourcePreparation for NavigationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "self.tasks".into(),
        module: None,
        destination: "work/navigation".into(),
        api: true,
    }
}

fn slug(header: &HeaderMap, name: &'static str) -> Result<Option<String>, BusinessError> {
    unique_request_header(header, name)?
        .map(|value| {
            if value.is_empty()
                || value.len() > 128
                || !value.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
                })
            {
                return Err(BusinessError::invalid(
                    "INVALID_WORKSPACE_SELECTOR",
                    "Use an exact workspace slug.",
                ));
            }
            Ok(value.to_owned())
        })
        .transpose()
}

/// The registered-Holding backend injected by composition into
/// [`crate::adapters::work_navigation_http::work_navigation_router`].
pub struct WorkNavigationBackend {
    policy: Arc<AccountPolicy>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    holding_id: HoldingId,
    holding_slug: String,
}

impl WorkNavigationBackend {
    pub fn new(
        policy: Arc<AccountPolicy>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        holding_id: HoldingId,
        holding_slug: impl Into<String>,
    ) -> Self {
        Self {
            policy,
            authorizer,
            holding_id,
            holding_slug: holding_slug.into(),
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
        mutation: bool,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let branch = slug(headers, "x-erpx-workspace-branch")?;
        let department = slug(headers, "x-erpx-workspace-department")?;
        if mutation {
            // A mutation must present the complete rendered selection so the
            // authorizer can reject a stale role/revision before any write.
            return Ok((
                inbound.into_proof(),
                holding_mutation_request(headers, branch, department)?,
            ));
        }
        let selection = CapturedHoldingMutation::capture(headers, branch, department)
            .resolve_rendered_read()?;
        Ok((inbound.into_proof(), selection))
    }
}

/// One caller-held transaction implementing the application's owner port.
struct PgWorkNavigation<'c> {
    connection: &'c mut PgConnection,
}

impl WorkNavigationPort for PgWorkNavigation<'_> {
    type Error = Error;

    async fn unread_home<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        workspace_base: &'a str,
        now: i64,
    ) -> Result<i64, Self::Error> {
        let workspace = current.selection().workspace();
        unread_home_count(
            &mut *self.connection,
            workspace.company_id.as_str(),
            current.user_id().as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            workspace_base,
            now,
        )
        .await
        .map_err(Into::into)
    }

    async fn snapshot<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        request: crate::adapters::work_navigation_read::NativeWorkNavigationRequest,
        now: TimestampMillis,
    ) -> Result<crate::domain::work_navigation::WorkNavigationFact, Self::Error> {
        crate::adapters::work_navigation_read::read_native_work_navigation(
            &mut *self.connection,
            current,
            request,
            now,
        )
        .await
    }

    async fn mark_home_read<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        command: &'a MarkHomeReadCommand,
    ) -> Result<(), Self::Error> {
        let workspace = current.selection().workspace();
        let input = crate::domain::home_read_cursor::MarkHomeReadInput {
            company_id: workspace.company_id.as_str().to_owned(),
            user_id: current.user_id().as_str().to_owned(),
            workspace_base: command.workspace_base.clone(),
            read_through: command.read_through,
        };
        input.validate()?;
        // Capture admission and update the cursor in the same Holding transaction.
        // A retry arriving later must never mark posts published since the first call.
        let scope = serde_json::json!([
            input.company_id,
            input.user_id,
            input.workspace_base,
            command.idempotency_key,
        ]);
        let fingerprint = format!("{:x}", Sha256::digest(scope.to_string()));
        let admitted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) \
             VALUES($1,'work.navigation.read-home',$2,$3,$4,'{}',$5) \
             ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(format!("work-navigation-read-home:{fingerprint}"))
        .bind(&input.user_id)
        .bind(&fingerprint)
        .bind(&input.workspace_base)
        .bind(input.read_through)
        .execute(&mut *self.connection)
        .await?;
        if admitted.rows_affected() == 0 {
            return Ok(());
        }
        let mut repository = crate::adapters::home_read_cursor_postgres::PgHomeReadCursor::new(
            &mut *self.connection,
        );
        crate::application::home_read_cursor::mark_home_read(&mut repository, &input).await
    }
}

impl WorkNavigationHttpBackend for WorkNavigationBackend {
    fn read<'a>(
        &'a self,
        request: WorkNavigationReadHttpRequest<'a>,
    ) -> WorkNavigationHttpFuture<'a, WorkNavigationReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, false)?;
            let holding = self.holding_id.clone();
            let holding_slug = self.holding_slug.clone();
            let include_attention = request.include_attention;
            let now = request.read_through;
            let data = self
                .authorizer
                .execute_with_sources_deferred(
                    request.host,
                    proof,
                    move || Ok(selection),
                    access(),
                    NavigationSources,
                    move |connection, current| {
                        let holding = holding.clone();
                        let holding_slug = holding_slug.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding {
                                return Err(scope_error().into());
                            }
                            current.require_permission("self.tasks")?;
                            let workspace = current.selection().workspace();
                            let base = navigation_workspace_base(
                                &holding_slug,
                                workspace.branch_slug.as_deref(),
                                workspace.department_slug.as_deref(),
                            );
                            let can_prepare_inventory = workspace.department_id.is_none()
                                && current.has_permission("inventory.prepare");
                            let mut port = PgWorkNavigation { connection };
                            read_navigation(
                                &mut port,
                                NavigationQuery {
                                    include_attention,
                                    now: TimestampMillis::new(now),
                                },
                                current,
                                NavigationFacts {
                                    workspace_base: &base,
                                    beta: current.beta(),
                                    inventory: InventoryPreparation {
                                        can_prepare_counts: can_prepare_inventory,
                                        can_prepare_transfers: can_prepare_inventory,
                                    },
                                    // Native expense creation admission and cross-owner
                                    // workflow routing must be wired before GET cutover.
                                    expense_available: true,
                                    routing_attention: false,
                                },
                            )
                            .await
                        })
                    },
                )
                .await
                .map_err(business)?;
            // The application composes the canonical `{data: snapshot}`
            // envelope; the HTTP ingress validates and wraps the snapshot once.
            let data = data
                .get("data")
                .cloned()
                .ok_or_else(|| BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "WORK_NAVIGATION_RESPONSE_INVALID",
                    "The native Work navigation read is unavailable.",
                ))?;
            Ok(WorkNavigationReadHttpResponse { data })
        })
    }

    fn mark_home_read<'a>(
        &'a self,
        request: WorkNavigationMarkHttpRequest<'a>,
    ) -> WorkNavigationHttpFuture<'a, WorkNavigationMarkHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, true)?;
            let holding = self.holding_id.clone();
            let holding_slug = self.holding_slug.clone();
            let key = request.key.to_owned();
            let read_through = request.read_through;
            let home = self
                .authorizer
                .execute_with_sources_deferred(
                    request.host,
                    proof,
                    move || Ok(selection),
                    access(),
                    NavigationSources,
                    move |connection, current| {
                        let holding = holding.clone();
                        let holding_slug = holding_slug.clone();
                        let key = key.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding {
                                return Err(scope_error().into());
                            }
                            current.require_permission("self.tasks")?;
                            let workspace = current.selection().workspace();
                            let base = navigation_workspace_base(
                                &holding_slug,
                                workspace.branch_slug.as_deref(),
                                workspace.department_slug.as_deref(),
                            );
                            let command = MarkHomeReadCommand {
                                idempotency_key: key,
                                read_through,
                                workspace_base: base,
                            };
                            let mut port = PgWorkNavigation { connection };
                            let data = execute_mark_home_read(
                                &mut port,
                                &command,
                                current,
                                TimestampMillis::new(read_through),
                            )
                            .await?;
                            Ok(data["data"]["home"].as_i64().unwrap_or_default())
                        })
                    },
                )
                .await
                .map_err(business)?;
            Ok(WorkNavigationMarkHttpResponse { home })
        })
    }
}
