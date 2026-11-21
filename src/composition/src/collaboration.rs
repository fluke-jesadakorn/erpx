//! Native Collaboration social composition.
//!
//! Account session, current IAM authority and one serializable Holding unit are
//! resolved here. Record mentions resolve through the Records owner read; no
//! Records SQL is duplicated. Messaging/inbox mounts are separate because they
//! need the Files and Work-projection ports.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::notification_statement::{
    crm_lead_notification_statement, finance_refund_notification_statement,
    learning_assignment_notification_statement,
    organization_change_notification_statement,
    legal_approval_notification_statement, legal_hold_notification_statement, notification_statement,
    company_acceptance_reviewer_statement, company_acceptance_result_statement, inventory_transit_notification_statement, legal_law_source_notification_statement, customer_portal_notification_statement, social_task_origin_statement, workflow_invoicing_notification_statement, workflow_notification_statement, roster_coverage_notification_statement, legal_feature_notice_statement, legal_sla_notice_statement, legal_escalation_notice_statement, legal_deadline_notice_statement, social_mention_statement, social_mention_notice_statement, social_agent_reply_statement, social_topic_statement, social_topic_binding_statement, attachment_access_event_statement, attachment_event_statement, legal_retire_source_statement, legal_retire_ledger_statement, legal_retire_event_statement, legal_bind_link_statement, legal_bind_set_statement, legal_bind_event_statement, expense_withdraw_link_statement, expense_withdraw_event_statement, expense_revoke_attachment_statement, doc_attachment_record_statement, doc_derivative_statement, doc_storage_ledger_statement, beta_expire_upload_statement, legal_access_allowed_statement, legal_access_denied_statement, legal_link_upload_statement, legal_pending_scan_statement, legal_stored_original_statement, legal_quarantine_event_statement, customer_portal_action_statement, release_event_submitted_statement, release_event_reviewed_statement, release_event_revoked_statement, customer_receipt_statement, legal_draft_set_statement, legal_draft_link_statement, legal_uploaded_intent_statement, legal_quarantined_source_statement, legal_file_preservation_statement, legal_retention_authorization_statement, finance_hotel_account_statement, finance_bank_ignore_statement, legal_feature_create_statement, legal_feature_update_statement, legal_feature_history_statement, legal_advance_record_statement, legal_feature_event_statement, legal_approval_lock_statement, legal_approval_retain_route_statement, legal_approval_record_statement, legal_approval_advance_statement, legal_approval_event_statement, legal_executed_review_statement, legal_executed_copy_state_statement, legal_signer_view_statement, legal_item_state_advance_statement, legal_native_reference_statement, legal_trust_addition_statement, legal_trust_decision_statement, legal_processing_job_statement, legal_access_event_statement, legal_intake_form_create_statement, legal_intake_form_approve_statement, legal_intake_apply_statement, legal_authority_create_statement, legal_authority_decide_statement, legal_retention_policy_create_statement, legal_retention_policy_decide_statement, legal_retention_assign_statement, legal_retention_request_statement, legal_retention_change_statement, legal_saved_view_statement, legal_derived_link_statement, legal_ai_document_statement, legal_proposal_accept_statement, legal_document_inspection_statement, legal_executed_copy_retain_statement, legal_canonical_document_insert_statement, legal_stored_document_insert_statement, legal_source_chunk_insert_statement, legal_law_source_register_statement, legal_law_revision_import_statement, legal_source_check_bind_statement, legal_text_review_import_statement, legal_applicability_propose_statement, legal_applicability_approve_statement, legal_source_check_review_statement, legal_settings_configure_statement, legal_item_create_statement, legal_subtype_create_statement, legal_member_add_statement, legal_requester_route_statement, legal_member_retain_statement, legal_hold_targets_expand_statement, legal_item_update_statement, legal_subtype_update_statement, legal_envelope_expire_statement, finance_book_adjustment_create_statement, finance_book_adjustment_decide_statement, finance_book_adjustment_reverse_statement, finance_plan_edition_create_statement, finance_plan_edition_review_statement, finance_report_snapshot_statement, finance_reversal_journal_statement, finance_reversal_lines_statement, finance_journal_reverse_statement, finance_tax_identity_upsert_statement, finance_tax_identity_supersede_statement, finance_tax_identity_decide_statement, finance_due_draft_schedule_statement, finance_due_draft_decide_statement, finance_intercompany_pair_create_statement, finance_intercompany_claim_statement, finance_intercompany_pair_decide_statement, finance_intercompany_claims_clear_statement, payroll_deduction_consumption_statement, payroll_repayment_collect_statement, payroll_advances_close_statement, payroll_final_pay_cancel_event_statement, payroll_run_cancel_statement, payroll_final_pay_prep_cancel_statement, expense_draft_write_statement, finance_sales_defaults_create_statement, finance_sales_defaults_supersede_statement, finance_sales_defaults_decide_statement, payroll_statutory_profile_insert_statement, payroll_statutory_profile_decide_statement, finance_accounting_profile_create_statement, finance_accounting_profile_decide_statement, hospitality_tax_doc_record_statement, hospitality_tax_doc_decide_statement, finance_pms_party_statement, finance_pms_payment_statement, finance_journal_create_statement, finance_journal_line_create_statement, finance_invoice_line_version_statement, finance_expansion_review_file_statement, finance_commerce_base_carry_statement, finance_payroll_remittance_match_statement, finance_pos_party_statement, finance_payroll_other_match_statement, finance_group_edition_prepare_statement, finance_group_edition_supersede_statement, finance_group_edition_decide_statement, finance_company_tool_set_statement, finance_payroll_batch_invoice_version_statement, finance_payroll_batch_bank_match_statement, finance_due_draft_complete_statement, finance_due_draft_block_statement, finance_supplier_bill_draft_void_statement, people_self_service_employee_statement, people_issued_payslip_statement, people_expense_payment_cancel_statement, people_leave_evidence_manifest_statement, people_prep_task_reschedule_statement, people_prep_task_complete_statement, people_statutory_remittance_statement, people_assign_level_statement, people_attachment_owner_version_statement, workflow_command_assertion_clear_statement, collaboration_org_team_statement, collaboration_org_room_statement, legal_backfill_document_attachment_statement, legal_backfill_external_attachment_statement, beta_ai_usage_expire_statement, beta_ai_draft_expire_statement, people_legacy_leave_employee_statement, people_legacy_leave_request_statement, people_worker_starter_statement, people_worker_placement_statement, people_beta_person_statement, people_beta_employee_statement, people_beta_engagement_statement, people_org_relocation_engagement_statement, people_org_relocation_employee_statement, people_handoff_person_statement, people_handoff_engagement_statement, people_handoff_compensation_statement, org_node_team_statement, org_node_room_statement, org_node_room_rename_statement, collaboration_backfill_messaging_attachment_statement, legal_job_health_renew_statement, legal_job_health_start_statement, legal_job_health_success_statement, legal_job_health_failure_statement, finance_collection_prepare_statement, finance_collection_supersede_statement, finance_collection_revise_statement, finance_collection_delivery_statement, finance_collection_decide_statement, finance_calc_run_prepare_statement, finance_calc_event_prepare_statement, finance_calc_run_decide_statement, finance_calc_event_decide_statement, finance_close_check_statement, finance_group_entity_statement, finance_intercompany_transaction_statement, finance_intercompany_event_statement, finance_intercompany_status_statement, finance_starter_chart_statement, finance_starter_account_statement, finance_starter_period_statement, finance_starter_dimension_statement, finance_starter_dimension_value_statement, people_draft_engagement_statement, people_draft_employee_statement, people_draft_history_statement, finance_statement_package_insert_statement, finance_statement_event_prepared_statement, finance_statement_package_decision_statement, finance_statement_event_decision_statement, finance_year_closing_insert_statement, finance_year_closing_decision_statement, inbox_event_insert_statement, inbox_delivery_insert_statement, inbox_delivery_state_statement, inbox_staff_notification_read_statement, inbox_notice_insert_statement, inbox_notice_target_insert_statement, inbox_notice_target_complete_statement, finance_accrual_schedule_insert_statement, finance_accrual_event_created_statement, finance_accrual_schedule_decision_statement, finance_accrual_event_decision_statement, finance_accrual_journal_insert_statement, finance_accrual_journal_lines_insert_statement, finance_accrual_run_insert_statement, finance_accrual_event_run_statement, finance_accrual_journal_post_statement, finance_accrual_run_post_statement, finance_accrual_schedule_post_statement, pms_finance_party_statement, pms_finance_invoice_statement, pms_finance_payment_statement, pms_finance_allocation_delete_statement, pms_finance_invoice_update_statement, pms_finance_allocation_insert_statement, pms_finance_bank_match_statement, books_opening_journal_statement, books_opening_journal_lines_statement, books_rule_decision_statement, books_dimension_definition_statement, books_dimension_value_statement, books_rule_prepare_statement, books_account_mapping_statement, wh_withholding_consumed_statement, wh_assessment_reversed_statement, wh_thai_document_cancel_statement, wh_assessment_insert_statement, wh_income_line_insert_statement, wh_payment_withholding_statement, wh_assessment_decision_statement, thai_doc_sequence_seed_statement, thai_doc_sequence_bump_statement, thai_doc_issue_statement, thai_doc_event_issued_statement, thai_doc_change_insert_statement, thai_doc_event_supersede_statement, thai_doc_change_decision_statement, workflow_command_assertion_clear_many_statement, legal_destruction_block_statement, legal_destruction_start_statement, legal_retention_started_event_statement, legal_destruction_count_attempt_statement, legal_destruction_retain_executing_statement, legal_disposal_retain_statement, legal_document_erase_statement, legal_processing_retire_statement, legal_inspections_erase_statement, legal_ai_retire_statement, legal_shares_revoke_statement, legal_destruction_complete_statement, legal_retention_destroyed_event_statement, legal_destruction_require_review_statement, legal_portal_access_event_statement, legal_portal_accept_share_statement, legal_portal_external_comment_statement, legal_portal_counsel_entry_statement, legal_portal_counsel_history_statement, legal_portal_interaction_event_statement, legal_portal_receipt_statement, legal_portal_external_upload_statement, legal_portal_advance_upload_statement, legal_portal_upload_event_statement, explicit_notification_statement, payroll_advance_notification_statement, payroll_correction_notification_statement,
    rfq_notification_statement,
    supplier_bill_notification_statement, supplier_promise_notification_statement,
    NotificationStatement, NotificationStatementInput,
};
use erpx_server_collaboration::public::notification_statement_http::{
    StatementBackend, StatementFuture, StatementRequest,
};
use erpx_server_collaboration::public::social::{
    PgSocialUnit, SocialRecordFuture, SocialRecordPort, SocialScope, SocialUploadFilesFuture,
    SocialUploadFilesPort, execute, lock_social_sources,
};
use erpx_server_collaboration::public::messaging_http::{
    MessagingAttachmentKind, MessagingAttachmentReply, MessagingChannelsData,
    MessagingFileBinding, MessagingFileContent, MessagingFileDiscardReceipt,
    MessagingFileDiscardRequest, MessagingFilesBindRequest, MessagingFilesContentRequest,
    MessagingFilesPort, MessagingHistoryData, MessagingHttpAttachmentRequest, MessagingHttpBackend,
    MessagingHttpCommand, MessagingHttpCommandRequest, MessagingHttpFuture, MessagingHttpReadReply,
    MessagingHttpReadRequest, MessagingHttpReceipt, MessagingHttpTarget, MessagingMembersData,
    MessagingOverviewData, MessagingReadKind,
};
use erpx_server_collaboration::public::messaging_native::{
    Change as RoomChange, HistoryCursor, MessagingActor, PgMessagingUnit, Receipt as RoomReceipt,
    RoomView, lock_messaging_sources, execute as execute_room,
};
use erpx_server_collaboration::public::messaging_file_discard::{
    MessagingFileDiscardPort, MessagingFileWithdrawRequest, discard_file,
};
use erpx_server_collaboration::public::messaging_teams::{
    Change as TeamChange, PgMessagingTeamUnit, Receipt as TeamReceipt,
    lock_messaging_team_sources, execute as execute_team,
};
use erpx_server_collaboration::public::social_http::{
    SocialCommentsData, SocialFeedData, SocialHttpBackend, SocialHttpCommentsRequest,
    SocialHttpCommandRequest, SocialHttpFuture, SocialHttpReadReply, SocialHttpReadRequest,
    SocialHttpRevisionsRequest, SocialHttpTarget, SocialReadKind, SocialThreadData,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_files::public::messaging_files::{
    MessagingFilesActor, MessagingFilesAudience, MessagingFilesFuture,
};
use erpx_server_files::public::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawTarget, withdraw_attachment,
};
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
use erpx_server_records::public::record_context::{
    PgRecordContext, RecordContextActor, record_context,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COLLABORATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_UNAVAILABLE",
        "The Collaboration service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CollaborationSources;

impl HoldingWorkSourcePreparation for CollaborationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

/// Write preparation: Collaboration rows plus every owner record-source table
/// are fenced before the first selected IAM snapshot.
#[derive(Clone, Copy)]
struct CollaborationWorkSources;

impl HoldingWorkSourcePreparation for CollaborationWorkSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c)
                .await?;
            erpx_server_commerce::public::record_source::lock_record_sources(c).await?;
            erpx_server_support::public::record_source::lock_record_sources(c).await?;
            erpx_server_maintenance::public::record_source::lock_record_sources(c).await?;
            erpx_server_operations::public::record_source::lock_record_sources(c).await?;
            erpx_server_pos::public::record_source::lock_record_sources(c).await?;
            erpx_server_pms::public::record_source::lock_record_sources(c).await?;
            erpx_server_assets::public::record_source::lock_record_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct SocialConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SocialConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

/// Records-backed mention resolver. It runs the owner contract read on the same
/// unit, so visibility and link admission stay in the Records owner.
struct SocialRecordsPort {
    actor: RecordContextActor,
    now: i64,
}

/// Files owner port for canceled Social upload drafts. Files owns the
/// attachment revocation and upload-intent expiry on the caller's retained
/// serializable connection; Collaboration never writes those tables.
struct SocialUploadFilesPortImpl;

impl SocialUploadFilesPort for SocialUploadFilesPortImpl {
    fn revoke_draft_uploads<'a>(
        &'a mut self,
        connection: &'a mut sqlx::PgConnection,
        draft_id: &'a str,
        now: i64,
    ) -> SocialUploadFilesFuture<'a> {
        Box::pin(async move {
            erpx_server_files::public::social_draft_uploads::revoke_social_draft_uploads(
                connection,
                erpx_server_files::public::social_draft_uploads::SocialDraftUploadRevokeRequest {
                    draft_id,
                    now,
                },
            )
            .await
            .map_err(business)
        })
    }
}

impl SocialRecordPort for SocialRecordsPort {
    fn resolve_record<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        module: &'a str,
        id: &'a str,
    ) -> SocialRecordFuture<'a> {
        Box::pin(async move {
            let mut repository = PgRecordContext::new(connection);
            let mut guard = crate::records::RecordsContextGuardImpl;
            let document = record_context(
                &mut repository,
                &mut guard,
                &self.actor,
                module,
                id,
                self.now,
            )
            .await
            .map_err(business)?;
            Ok(Some(
                document
                    .get("record")
                    .and_then(|record| record.get("title"))
                    .and_then(|title| title.as_str())
                    .unwrap_or(id)
                    .to_owned(),
            ))
        })
    }
}

async fn display_name(
    connection: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(NULLIF(preferred_name,''),legal_name) FROM hr_people \
         WHERE company_id=$1 AND user_id=$2 AND status='active' AND merged_into_id IS NULL LIMIT 1",
    )
    .bind(company.as_str())
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| user_id.to_owned())
}

fn social_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    name: String,
    now: i64,
    use_assignment_scope: bool,
) -> Result<SocialScope, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let (branch, department) = if use_assignment_scope {
        match assignment.scope_kind {
            WorkScopeKind::Holding => (None, None),
            WorkScopeKind::Branch => (
                assignment.scope_node_id.as_ref().map(|id| id.as_str().to_owned()),
                None,
            ),
            WorkScopeKind::Department => {
                let department = assignment
                    .scope_node_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned());
                (
                    workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
                    department,
                )
            }
        }
    } else {
        (
            workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
            workspace
                .department_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
        )
    };
    let can_share_company = assignment.scope_kind == WorkScopeKind::Holding;
    let can_moderate = current.has_permission("records.review");
    let scope = SocialScope::new(
        company.as_str(),
        current.user_id().as_str(),
        assignment.id.as_str(),
        name,
        now,
    )?
    .branch(branch.as_deref())?
    .department(department.as_deref())?
    .assignment_version(assignment.version)
    .share_company(can_share_company)
    .moderate(can_moderate)
    .hide_posts(assignment.role == "holding_owner");
    Ok(scope)
}

pub struct SocialService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SocialService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
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

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: SocialReadKind,
    ) -> Result<SocialHttpReadReply, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, false)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut unit =
                            PgSocialUnit::read_only(connection, scope, &mut records);
                        Ok(match kind {
                            SocialReadKind::Feed {
                                filter,
                                cursor,
                                limit,
                            } => {
                                let (items, has_more) =
                                    unit.feed_page(filter, cursor.as_ref(), limit).await?;
                                let next_cursor = if has_more {
                                    items.last().map(|post| {
                                        erpx_server_collaboration::public::social::FeedCursor {
                                            at: post.created_at,
                                            id: post.id.clone(),
                                        }
                                        .encode()
                                    })
                                } else {
                                    None
                                };
                                SocialHttpReadReply::Feed(Box::new(SocialFeedData {
                                    items,
                                    next_cursor,
                                    has_more,
                                }))
                            }
                            SocialReadKind::Thread { post_id } => {
                                let posts = unit.thread(&post_id).await?;
                                SocialHttpReadReply::Thread(Box::new(SocialThreadData { posts }))
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn comments_page(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        post_id: String,
        cursor: Option<erpx_server_collaboration::public::social::FeedCursor>,
        limit: usize,
    ) -> Result<SocialCommentsData, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let post_id = post_id.clone();
                    let cursor = cursor.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, false)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut unit =
                            PgSocialUnit::read_only(connection, scope, &mut records);
                        let (items, next) = unit
                            .comments_page(&post_id, cursor.as_ref(), limit, 20)
                            .await?;
                        Ok(SocialCommentsData::new(items, next))
                    })
                },
            )
            .await
    }

    pub async fn revisions(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        post_id: String,
    ) -> Result<Vec<erpx_server_collaboration::public::social::RevisionRow>, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let post_id = post_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, false)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut unit =
                            PgSocialUnit::read_only(connection, scope, &mut records);
                        Ok(unit.revisions(&post_id).await?)
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        change: erpx_server_collaboration::public::social::Change,
        key: erpx_server_shared::public::CommandKey,
        now_body: serde_json::Value,
    ) -> Result<erpx_server_collaboration::public::social::Receipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let change = change.clone();
                    let key = key.clone();
                    let _ = now_body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        lock_social_sources(connection).await?;
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, true)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut uploads = SocialUploadFilesPortImpl;
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records)
                            .with_upload_files(&mut uploads);
                        let receipt = execute(&mut unit, &key, change).await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }
}

pub struct SocialBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SocialService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SocialBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SocialService<C>>,
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
            return Err(not_found());
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
}

impl<C: Clock + Send + Sync + 'static> SocialHttpBackend for SocialBackend<C> {
    fn read<'a>(
        &'a self,
        request: SocialHttpReadRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: SocialHttpCommandRequest<'a>,
    ) -> SocialHttpFuture<'a, erpx_server_collaboration::public::social::Receipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.change,
                    request.key,
                    request.input,
                )
                .await
                .map_err(business)
        })
    }

    fn comments_page<'a>(
        &'a self,
        request: SocialHttpCommentsRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialCommentsData> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .comments_page(
                    request.host,
                    proof,
                    selection,
                    request.post_id.to_owned(),
                    request.cursor,
                    request.limit,
                )
                .await
                .map_err(business)
        })
    }

    fn revisions<'a>(
        &'a self,
        request: SocialHttpRevisionsRequest<'a>,
    ) -> SocialHttpFuture<'a, Vec<erpx_server_collaboration::public::social::RevisionRow>> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .revisions(
                    request.host,
                    proof,
                    selection,
                    request.post_id.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}

pub fn social_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn SocialHttpBackend>,
) -> SocialHttpTarget {
    SocialHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}

fn messaging_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    name: String,
    now: i64,
) -> Result<erpx_server_collaboration::public::messaging_native::MessagingScope, BusinessError> {
    erpx_server_collaboration::public::messaging_native::MessagingScope::new(
        company.as_str(),
        current.user_id().as_str(),
        current.selection().assignment().id.as_str(),
        name,
        now,
    )
}

fn room_view(room: &erpx_server_collaboration::public::messaging_native::RoomSnapshot) -> RoomView {
    RoomView {
        id: room.id.clone(),
        kind: room.kind.as_str().to_owned(),
        name: room.name.clone(),
        owner_id: room.owner_id.clone(),
        version: room.version,
        last_seq: room.last_seq,
        unread: 0,
        members: room.members.clone(),
        last_message_at: None,
    }
}

async fn messaging_member_names(
    control: &erpx_server_runtime::public::postgres::DatabasePool,
    ids: &[String],
) -> Result<Vec<serde_json::Value>, BusinessError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut connection = control
        .raw()
        .acquire()
        .await
        .map_err(|_| unavailable())?;
    let rows = erpx_server_identity_access::public::messaging_people::control_account_names(
        &mut connection,
        ids,
    )
    .await
    .map_err(|_| unavailable())?;
    serde_json::to_value(rows)
        .map_err(|_| unavailable())?
        .as_array()
        .cloned()
        .ok_or_else(unavailable)
}

pub struct MessagingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    control: erpx_server_runtime::public::postgres::DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MessagingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
        control: erpx_server_runtime::public::postgres::DatabasePool,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            control,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: MessagingReadKind,
    ) -> Result<MessagingHttpReadReply, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let control = self.control.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "messaging".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    let control = control.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let scope = messaging_scope(current, &company, name, now)?;
                        Ok(match kind {
                            MessagingReadKind::Overview => {
                                let mut rooms = PgMessagingUnit::read_only(connection, scope.clone());
                                let rooms = rooms.overview().await?;
                                let mut teams =
                                    PgMessagingTeamUnit::read_only(connection, scope);
                                let teams = teams.teams().await?;
                                MessagingHttpReadReply::Overview(Box::new(
                                    MessagingOverviewData { teams, rooms },
                                ))
                            }
                            MessagingReadKind::History {
                                room_id,
                                before,
                                after,
                                limit,
                            } => {
                                let mut unit =
                                    PgMessagingUnit::read_only(connection, scope);
                                let room = unit.room(&room_id).await?.map(|room| room_view(&room));
                                let messages =
                                    unit.history(&room_id, before, after, limit).await?;
                                let has_more = messages.len() >= limit;
                                let ids: Vec<String> =
                                    messages.iter().map(|message| message.id.clone()).collect();
                                let files = unit.files_for_messages(&ids).await?;
                                let messages: Vec<serde_json::Value> = messages
                                    .iter()
                                    .map(|message| {
                                        let mut value = serde_json::to_value(message)
                                            .map_err(|_| unavailable())?;
                                        let row_files: Vec<serde_json::Value> = files
                                            .iter()
                                            .filter(|file| {
                                                file.get("message_id")
                                                    .and_then(|id| id.as_str())
                                                    == Some(message.id.as_str())
                                            })
                                            .cloned()
                                            .collect();
                                        if let Some(object) = value.as_object_mut() {
                                            object.insert(
                                                "files".into(),
                                                serde_json::Value::Array(row_files),
                                            );
                                        }
                                        Ok(value)
                                    })
                                    .collect::<Result<Vec<_>, BusinessError>>()
                                    .map_err(Error::Business)?;
                                let room = room.ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::NotFound,
                                        "NOT_FOUND",
                                        "Not found.",
                                    )
                                })?;
                                MessagingHttpReadReply::History(Box::new(MessagingHistoryData {
                                    room,
                                    messages,
                                    has_more,
                                }))
                            }
                            MessagingReadKind::Members { id, is_team } => {
                                let (ids, detail) = if is_team {
                                    let mut teams =
                                        PgMessagingTeamUnit::read_only(connection, scope);
                                    let team = teams.team(&id).await?.ok_or_else(|| {
                                        BusinessError::new(
                                            FailureKind::NotFound,
                                            "NOT_FOUND",
                                            "Not found.",
                                        )
                                    })?;
                                    let ids = teams.members(&id).await?;
                                    let detail = serde_json::json!({
                                        "id": team.id,
                                        "companyId": team.company_id,
                                        "name": team.name,
                                        "ownerId": team.owner_id,
                                        "version": team.version,
                                        "createdAt": team.created_at,
                                        "kind": "team",
                                    });
                                    (ids, detail)
                                } else {
                                    let mut unit =
                                        PgMessagingUnit::read_only(connection, scope);
                                    let room = unit.room(&id).await?.ok_or_else(|| {
                                        BusinessError::new(
                                            FailureKind::NotFound,
                                            "NOT_FOUND",
                                            "Not found.",
                                        )
                                    })?;
                                    let ids = unit.room_members(&id).await?;
                                    let detail = serde_json::json!({
                                        "id": room.id,
                                        "companyId": room.company_id,
                                        "kind": room.kind.as_str(),
                                        "name": room.name,
                                        "ownerId": room.owner_id,
                                        "version": room.version,
                                        "lastSeq": room.last_seq,
                                        "createdAt": room.created_at,
                                    });
                                    (ids, detail)
                                };
                                let members = messaging_member_names(&control, &ids).await?;
                                MessagingHttpReadReply::Members(Box::new(MessagingMembersData {
                                    id,
                                    is_team,
                                    members,
                                    detail,
                                }))
                            }
                            MessagingReadKind::TeamChannels { team_id } => {
                                let mut teams =
                                    PgMessagingTeamUnit::read_only(connection, scope);
                                let team = teams.team(&team_id).await?.ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::NotFound,
                                        "NOT_FOUND",
                                        "Not found.",
                                    )
                                })?;
                                let channels = teams.channels(&team_id, 100).await?;
                                MessagingHttpReadReply::Channels(Box::new(
                                    MessagingChannelsData { team, channels },
                                ))
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        command: MessagingHttpCommand,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<MessagingHttpReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "messaging".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let scope = messaging_scope(current, &company, name, now)?;
                        Ok(match command {
                            MessagingHttpCommand::Room(change) => {
                                lock_messaging_sources(connection).await?;
                                let mut unit =
                                    PgMessagingUnit::new(connection, scope, key.clone());
                                let receipt = execute_room(&mut unit, &key, change).await?;
                                MessagingHttpReceipt::Room { receipt }
                            }
                            MessagingHttpCommand::Team(change) => {
                                lock_messaging_team_sources(connection).await?;
                                let mut unit =
                                    PgMessagingTeamUnit::new(connection, scope, key.clone());
                                let receipt = execute_team(&mut unit, &key, change).await?;
                                MessagingHttpReceipt::Team { receipt }
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn discard(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        file_id: String,
        reason: String,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<MessagingFileDiscardReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected || !request.mutation {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "messaging".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let file_id = file_id.clone();
                    let reason = reason.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        current.require_permission("records.write")?;
                        current.require_permission("records.read")?;
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = MessagingActor {
                            user_id: current.user_id().as_str().to_owned(),
                            display_name: name,
                            company_id: company.as_str().to_owned(),
                        };
                        // Retained order: Company package fence before the
                        // Messaging/owner source locks.
                        let mut purchase = begin_messaging_file_discard_purchase(
                            connection,
                            &holding,
                            &company,
                            key.as_str(),
                        )
                        .await?;
                        lock_messaging_sources(connection).await?;
                        sqlx::query(
                            "LOCK TABLE messaging_files IN SHARE ROW EXCLUSIVE MODE",
                        )
                        .execute(&mut *connection)
                        .await?;
                        let authority = MessageFileWithdrawAuthority {
                            company_id: company.clone(),
                            current,
                        };
                        let port = MessageFileDiscardFilesPort {
                            authority: &authority,
                        };
                        let receipt = discard_file(
                            connection,
                            &actor,
                            current.selection().assignment().id.as_str(),
                            key.as_str(),
                            &file_id,
                            &reason,
                            now,
                            &port,
                        )
                        .await?;
                        if let Some((mut boundary, catalogue)) = purchase.take() {
                            boundary.complete(connection, &holding, &catalogue).await?;
                        }
                        Ok(MessagingFileDiscardReceipt {
                            id: receipt.id,
                            state: receipt.state,
                        })
                    })
                },
            )
            .await
    }
}

pub struct MessagingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MessagingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MessagingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MessagingService<C>>,
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
            return Err(not_found());
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

    fn mutation_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let company =
            unique_request_header(headers, "x-erpx-context-company")?.ok_or_else(scope_error)?;
        if company != self.service.company().as_str() {
            return Err(scope_error());
        }
        let assignment = unique_request_header(headers, "x-erpx-context-assignment")?
            .ok_or_else(scope_error)?;
        let revision: i64 = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(scope_error)?
            .parse()
            .map_err(|_| scope_error())?;
        let assignment_version: i64 =
            unique_request_header(headers, "x-erpx-context-assignment-version")?
                .ok_or_else(scope_error)?
                .parse()
                .map_err(|_| scope_error())?;
        if !(1..=9_007_199_254_740_991).contains(&revision)
            || !(1..=9_007_199_254_740_991).contains(&assignment_version)
        {
            return Err(scope_error());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(
                    erpx_server_identity_access::public::work_context::ExpectedWorkSelection {
                        revision,
                        assignment_id: erpx_server_shared::public::AssignmentId::new(assignment)?,
                        assignment_version,
                    },
                ),
                mutation: true,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> MessagingHttpBackend for MessagingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MessagingHttpReadRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: MessagingHttpCommandRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.command,
                    request.key,
                )
                .await
                .map_err(business)
        })
    }

    fn attachment<'a>(
        &'a self,
        request: MessagingHttpAttachmentRequest<'a>,
        files: &'a dyn MessagingFilesPort,
    ) -> MessagingHttpFuture<'a, MessagingAttachmentReply> {
        Box::pin(async move {
            let MessagingHttpAttachmentRequest {
                host,
                holding_slug,
                headers,
                kind,
                ..
            } = request;
            Ok(match kind {
                MessagingAttachmentKind::Bind {
                    room_id,
                    attachment_set_ids,
                    message_id,
                } => MessagingAttachmentReply::Bound(
                    files
                        .bind(MessagingFilesBindRequest {
                            host,
                            holding_slug,
                            headers,
                            room_id,
                            attachment_set_ids,
                            message_id,
                        })
                        .await?,
                ),
                MessagingAttachmentKind::Content { file_id, variant } => {
                    MessagingAttachmentReply::Content(
                        files
                            .content(MessagingFilesContentRequest {
                                host,
                                holding_slug,
                                headers,
                                file_id,
                                variant,
                            })
                            .await?,
                    )
                }
            })
        })
    }

    fn discard<'a>(
        &'a self,
        request: MessagingFileDiscardRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileDiscardReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.mutation_context(request.headers)?;
            self.service
                .discard(
                    request.host,
                    proof,
                    selection,
                    request.file_id.to_owned(),
                    request.reason.to_owned(),
                    request.key,
                )
                .await
                .map_err(business)
        })
    }
}

/// Composition-owned Messaging attachment audience. It re-resolves the exact
/// current Account/Holding session through the same authorizer as Messaging
/// reads, then rechecks current conversation membership on the caller's
/// existing Holding unit. It never commits that unit.
pub struct CompositionMessagingFilesAudience {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    holding: HoldingId,
    company: CompanyId,
}

impl CompositionMessagingFilesAudience {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        holding: HoldingId,
        company: CompanyId,
    ) -> Self {
        Self {
            policy,
            scopes,
            authorizer,
            holding,
            company,
        }
    }
}

impl MessagingFilesAudience for CompositionMessagingFilesAudience {
    fn current<'a>(
        &'a self,
        _connection: &'a mut PgConnection,
        host: &'a str,
        _holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> MessagingFilesFuture<'a, MessagingFilesActor> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(not_found());
            }
            let (branch_slug, department_slug) = self.scopes.slugs(headers)?;
            let expected = self.company.clone();
            let company = self.company.clone();
            let holding = self.holding.clone();
            self.authorizer
                .execute_with_sources_deferred(
                    host,
                    inbound.into_proof(),
                    move || {
                        Ok(HoldingWorkspaceRequest {
                            company_id: expected,
                            branch_slug,
                            department_slug,
                            expected: None,
                            mutation: false,
                        })
                    },
                    WorkAuthorizationRequest {
                        permission: "records.read".into(),
                        module: Some("collaboration".into()),
                        destination: "messaging".into(),
                        api: true,
                    },
                    CollaborationSources,
                    move |_connection, current| {
                        let company = company.clone();
                        let holding = holding.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope_error().into());
                            }
                            Ok(MessagingFilesActor {
                                company_id: company,
                                user_id: current.user_id().as_str().to_owned(),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn require_room<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a MessagingFilesActor,
        room_id: &'a str,
    ) -> MessagingFilesFuture<'a, ()> {
        Box::pin(async move {
            let member: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM messaging_room_members m \
                 JOIN messaging_rooms r ON r.id=m.room_id AND r.company_id=$3 \
                 WHERE m.room_id=$1 AND m.user_id=$2)",
            )
            .bind(room_id)
            .bind(&actor.user_id)
            .bind(actor.company_id.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| unavailable())?;
            if !member {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "MESSAGING_ROOM_FORBIDDEN",
                    "Choose a conversation you belong to.",
                ));
            }
            Ok(())
        })
    }
}

/// Files owner authority for a Messaging unsent-file discard. The current
/// Account/Holding/Company scope and the `collaboration` module are required,
/// and the exact `messaging_room` target must stay in this Company. Room
/// membership is rechecked by the Messaging unit before and after the Files
/// write; this port never mints a Customer, Account or File permission.
struct MessageFileWithdrawAuthority<'a> {
    company_id: CompanyId,
    current: &'a AuthorizedWorkContext,
}

impl MessageFileWithdrawAuthority<'_> {
    fn require(&self, target: &WithdrawTarget<'_>) -> Result<(), BusinessError> {
        if target.company_id != self.company_id.as_str()
            || target.target_type != "messaging_room"
        {
            return Err(scope_error());
        }
        self.current
            .require_permission("records.write")
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot discard this file.",
                )
            })?;
        self.current
            .require_permission("records.read")
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot discard this file.",
                )
            })?;
        if !self
            .current
            .modules()
            .iter()
            .any(|value| value == "collaboration")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ATTACHMENT_FORBIDDEN",
                "Your current assignment cannot discard this file.",
            ));
        }
        Ok(())
    }
}

impl AttachmentWithdrawAuthority for MessageFileWithdrawAuthority<'_> {
    fn authorize_withdraw<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.require(target)?;
            Ok(())
        })
    }

    fn revalidate_withdraw<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.require(target).map_err(Error::Business)?;
            let now = self.current.expires_at();
            let sampled = sqlx::query_scalar::<_, i64>(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await?;
            if sampled >= now {
                return Err(Error::Business(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_WITHDRAW_AUTHORITY_CHANGED",
                    "The selected role or file record changed. Reload and retry.",
                )));
            }
            require_selected_actor_current(connection, self.current, sampled).await?;
            Ok(())
        })
    }
}

/// In-process Files owner port for the Messaging unit. It calls the Files
/// generic remove command on the caller's connection; both owners commit or
/// roll back together.
struct MessageFileDiscardFilesPort<'a> {
    authority: &'a MessageFileWithdrawAuthority<'a>,
}

impl MessagingFileDiscardPort for MessageFileDiscardFilesPort<'_> {
    fn withdraw<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        request: MessagingFileWithdrawRequest<'a>,
    ) -> erpx_server_collaboration::public::messaging_file_discard::MessagingFileDiscardFuture<'a, ()>
    {
        Box::pin(async move {
            withdraw_attachment(
                connection,
                self.authority,
                WithdrawAttachmentRequest {
                    actor_id: request.actor_id,
                    company_id: request.company_id,
                    link_id: request.link_id,
                    reason: request.reason,
                    command_key: request.command_key,
                    now: request.now,
                },
            )
            .await
            .map(|_| ())
            .map_err(business)
        })
    }
}

fn messaging_file_discard_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_APPS_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").as_deref() != Ok("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

/// Settle the existing Messages purchase obligation for an unsent file
/// discard. The boundary attaches to the Messaging serializable unit before the
/// owner write and re-evaluates `work.messages`/`settle-existing` afterwards.
async fn begin_messaging_file_discard_purchase(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    key: &str,
) -> Result<Option<(PurchaseCommandBoundary, CommercialCatalogue)>, Error> {
    let mode = messaging_file_discard_mode().map_err(Error::Business)?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let catalogue = current_package_catalogue().map_err(Error::Business)?;
    let purchase_key = format!("{key}:file-discard");
    let Some(mut boundary) = PurchaseCommandBoundary::new(
        mode,
        purchase_key,
        PurchaseCommandScope {
            company_id: company.clone(),
            capability: "work.messages".into(),
            purpose: CapabilityPurpose::SettleExisting,
        },
    )
    .map_err(Error::Business)?
    else {
        return Ok(None);
    };
    boundary
        .begin(connection, holding, &catalogue)
        .await?;
    Ok(Some((boundary, catalogue)))
}

fn inbox_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    now: i64,
) -> Result<erpx_server_collaboration::public::inbox::InboxScope, BusinessError> {
    erpx_server_collaboration::public::inbox::InboxScope::new(
        company.as_str(),
        current.user_id().as_str(),
        current.selection().assignment().id.as_str(),
        now,
    )
    .map(|scope| scope.assignment_version(current.selection().assignment().version))
}

pub struct InboxService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> InboxService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
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

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: erpx_server_collaboration::public::inbox::InboxReadKind,
    ) -> Result<
        erpx_server_collaboration::public::inbox::InboxHttpReadReply,
        Error,
    > {
        use erpx_server_collaboration::public::inbox::{
            InboxHttpReadReply, InboxNotificationQuery, InboxReadKind, PgInboxTargets,
            PgInboxUnit,
        };
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "inbox".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let scope = inbox_scope(current, &company, now)?;
                        let mut targets = PgInboxTargets::new(
                            workspace.branch_id.as_ref().map(|id| id.as_str()),
                            workspace.department_id.as_ref().map(|id| id.as_str()),
                        )
                        .assignment(Some(selected.assignment().id.as_str()));
                        let mut unit = PgInboxUnit::new(connection, scope, &mut targets);
                        Ok(match kind {
                            InboxReadKind::Summary => {
                                InboxHttpReadReply::Summary(Box::new(unit.summary().await?))
                            }
                            InboxReadKind::Notifications {
                                view,
                                category,
                                search,
                                cursor,
                                limit,
                            } => InboxHttpReadReply::Page(Box::new(
                                unit.notifications(InboxNotificationQuery {
                                    view,
                                    category,
                                    search,
                                    cursor,
                                    limit,
                                })
                                .await?,
                            )),
                            InboxReadKind::People { query } => {
                                InboxHttpReadReply::People(
                                    serde_json::to_value(unit.people(&query).await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                            InboxReadKind::Notices => {
                                InboxHttpReadReply::Notices(
                                    serde_json::to_value(unit.notices().await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                            InboxReadKind::Notice { id } => {
                                InboxHttpReadReply::Notice(
                                    serde_json::to_value(unit.notice(&id).await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                        })
                    })
                },
            )
            .await
    }
}

pub struct InboxBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<InboxService<C>>,
}

impl<C: Clock + Send + Sync + 'static> InboxBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<InboxService<C>>,
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
            return Err(not_found());
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
}

impl<C: Clock + Send + Sync + 'static>
    erpx_server_collaboration::public::inbox::InboxHttpBackend for InboxBackend<C>
{
    fn read<'a>(
        &'a self,
        request: erpx_server_collaboration::public::inbox::InboxHttpReadRequest<'a>,
    ) -> erpx_server_collaboration::public::inbox::InboxHttpFuture<
        'a,
        erpx_server_collaboration::public::inbox::InboxHttpReadReply,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }
}

fn collaboration_invalid_source() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SOURCE_RECORD_NOT_FOUND",
        "Source record is unavailable in this work scope.",
    )
}

fn source_href(kind: erpx_server_shared::public::record_source_contract::SourceKind, id: &str) -> Option<String> {
    use erpx_server_shared::public::record_source_contract::SourceKind;
    let encoded = erpx_server_collaboration::public::inbox::encode_uri_component(id);
    let prefix = match kind {
        SourceKind::CommerceOrder => "/commerce/orders/",
        SourceKind::SupportTicket => "/support/",
        SourceKind::FieldServiceOrder => "/field-service/",
        SourceKind::PosSale => "/pos?sale=",
        SourceKind::Asset => "/assets/",
        SourceKind::MaintenancePlan => "/maintenance/plans/",
        SourceKind::MaintenanceWork => "/maintenance/",
        SourceKind::StaffTask => "/staff-tasks/",
        SourceKind::PmsRecord => return None,
    };
    Some(format!("{prefix}{encoded}"))
}

enum CollaborationSourceWitness {
    Pos(erpx_server_pos::public::record_source::CurrentRecordSource),
    Pms(erpx_server_pms::public::record_source::CurrentRecordSource),
    Assets(erpx_server_assets::public::record_source::CurrentRecordSource),
    Commerce(erpx_server_commerce::public::record_source::CurrentRecordSource),
    Support(erpx_server_support::public::record_source::CurrentRecordSource),
    Maintenance(erpx_server_maintenance::public::record_source::CurrentRecordSource),
    Operations(erpx_server_operations::public::record_source::CurrentRecordSource),
}

/// Composition-owned source resolver over the retained serializable unit.
struct CollaborationSourceResolver {
    company: CompanyId,
}

impl erpx_server_collaboration::public::collaboration_work::CollaborationSourcePort
    for CollaborationSourceResolver
{
    type Error = Error;
    type Witness = CollaborationSourceWitness;

    fn prepare(
        &mut self,
        _source: &erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            (
                erpx_server_shared::public::record_source_contract::SourceRecord,
                Self::Witness,
            ),
            Self::Error,
        >,
    > + Send {
        Box::pin(async { Err(collaboration_invalid_source().into()) })
    }

    fn prepare_with<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        source: &'a erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            (
                erpx_server_shared::public::record_source_contract::SourceRecord,
                Self::Witness,
            ),
            Self::Error,
        >,
    > + Send {
        use erpx_server_shared::public::record_source_contract::SourceKind;
        Box::pin(async move {
            let company = self.company.clone();
            let _ = connection;
            match source.kind {
                SourceKind::PosSale => {
                    let current = erpx_server_pos::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Pos(current)))
                }
                SourceKind::PmsRecord => {
                    let current = erpx_server_pms::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Pms(current)))
                }
                SourceKind::Asset => {
                    let current = erpx_server_assets::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Assets(current)))
                }
                SourceKind::CommerceOrder => {
                    let current = erpx_server_commerce::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Commerce(current)))
                }
                SourceKind::SupportTicket => {
                    let current = erpx_server_support::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Support(current)))
                }
                SourceKind::MaintenancePlan | SourceKind::MaintenanceWork => {
                    let current = erpx_server_maintenance::public::record_source::read_record_source(
                        connection,
                        &company,
                        source.kind,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Maintenance(current)))
                }
                SourceKind::StaffTask => {
                    let current = erpx_server_operations::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Operations(current)))
                }
                SourceKind::FieldServiceOrder => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "NATIVE_FIELD_INBOX_REQUIRED",
                    "Field sources use the native Field workspace.",
                )
                .into()),
            }
        })
    }

    fn require_current(
        &mut self,
        _witness: &Self::Witness,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        Box::pin(async { Ok(()) })
    }

    fn require_current_with<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        witness: &'a Self::Witness,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        Box::pin(async move {
            match witness {
                CollaborationSourceWitness::Pos(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Pms(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Assets(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Commerce(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Support(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Maintenance(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Operations(current) => {
                    current.require_current(connection).await?
                }
            }
            Ok(())
        })
    }

    fn present(
        &mut self,
        source: &erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            erpx_server_collaboration::public::collaboration_work::SourcePresentation,
            Self::Error,
        >,
    > + Send {
        use erpx_server_collaboration::public::collaboration_work::SourcePresentation;
        Box::pin(async move {
            Ok(SourcePresentation {
                title: None,
                href: source_href(source.kind, &source.id),
            })
        })
    }
}

pub struct CollaborationWorkService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

fn collaboration_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    now: i64,
) -> Result<
    erpx_server_collaboration::public::collaboration_work::CollaborationScope,
    BusinessError,
> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(
        erpx_server_collaboration::public::collaboration_work::CollaborationScope::new(
            company.as_str(),
            current.user_id().as_str(),
            selected.assignment().id.as_str(),
            now,
        )?
        .assignment_version(selected.assignment().version)
        .branch(workspace.branch_id.as_ref().map(|id| id.as_str()))
        .department(workspace.department_id.as_ref().map(|id| id.as_str())),
    )
}

impl<C: Clock + Send + Sync + 'static> CollaborationWorkService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
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

    /// Authoritative notification statement on the caller-authorized Holding unit.
    pub async fn notification_statement(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        input: NotificationStatementInput,
    ) -> Result<NotificationStatement, Error> {
        let expected = self.configuration.company.clone();
        let company = self.configuration.company.clone();
        selection.mutation = false;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    if selection.company_id != expected {
                        return Err(scope_error());
                    }
                    Ok(selection)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let company = company.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        current.require_permission("records.read")?;
                        let company_id = CompanyId::new(company.as_str())?;
                        if input.kind.as_deref() == Some("legal_portal_access_event") {
                            legal_portal_access_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_accept_share") {
                            legal_portal_accept_share_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_external_comment") {
                            legal_portal_external_comment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_counsel_entry") {
                            legal_portal_counsel_entry_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_counsel_history") {
                            legal_portal_counsel_history_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_interaction_event") {
                            legal_portal_interaction_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_receipt") {
                            legal_portal_receipt_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_external_upload") {
                            legal_portal_external_upload_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_advance_upload") {
                            legal_portal_advance_upload_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_portal_upload_event") {
                            legal_portal_upload_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_block") {
                            legal_destruction_block_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_start") {
                            legal_destruction_start_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_started_event") {
                            legal_retention_started_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_count_attempt") {
                            legal_destruction_count_attempt_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_retain_executing") {
                            legal_destruction_retain_executing_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_disposal_retain") {
                            legal_disposal_retain_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_document_erase") {
                            legal_document_erase_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_processing_retire") {
                            legal_processing_retire_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_inspections_erase") {
                            legal_inspections_erase_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_ai_retire") {
                            legal_ai_retire_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_shares_revoke") {
                            legal_shares_revoke_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_complete") {
                            legal_destruction_complete_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_destroyed_event") {
                            legal_retention_destroyed_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_destruction_require_review") {
                            legal_destruction_require_review_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_plan_edition_create") {
                            finance_plan_edition_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_plan_edition_review") {
                            finance_plan_edition_review_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_report_snapshot") {
                            finance_report_snapshot_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("workflow_command_assertion_clear_many") {
                            workflow_command_assertion_clear_many_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_backfill_document_attachment") {
                            legal_backfill_document_attachment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_backfill_external_attachment") {
                            legal_backfill_external_attachment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_sequence_seed") {
                            thai_doc_sequence_seed_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_sequence_bump") {
                            thai_doc_sequence_bump_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_issue") {
                            thai_doc_issue_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_event_issued") {
                            thai_doc_event_issued_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_change_insert") {
                            thai_doc_change_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_event_supersede") {
                            thai_doc_event_supersede_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("thai_doc_change_decision") {
                            thai_doc_change_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_withholding_consumed") {
                            wh_withholding_consumed_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_assessment_reversed") {
                            wh_assessment_reversed_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_thai_document_cancel") {
                            wh_thai_document_cancel_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_assessment_insert") {
                            wh_assessment_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_income_line_insert") {
                            wh_income_line_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_payment_withholding") {
                            wh_payment_withholding_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("wh_assessment_decision") {
                            wh_assessment_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_opening_journal") {
                            books_opening_journal_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_opening_journal_lines") {
                            books_opening_journal_lines_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_rule_decision") {
                            books_rule_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_dimension_definition") {
                            books_dimension_definition_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_dimension_value") {
                            books_dimension_value_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_rule_prepare") {
                            books_rule_prepare_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("books_account_mapping") {
                            books_account_mapping_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_party") {
                            pms_finance_party_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_invoice") {
                            pms_finance_invoice_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_payment") {
                            pms_finance_payment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_allocation_delete") {
                            pms_finance_allocation_delete_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_invoice_update") {
                            pms_finance_invoice_update_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_allocation_insert") {
                            pms_finance_allocation_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("pms_finance_bank_match") {
                            pms_finance_bank_match_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_schedule_insert") {
                            finance_accrual_schedule_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_event_created") {
                            finance_accrual_event_created_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_schedule_decision") {
                            finance_accrual_schedule_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_event_decision") {
                            finance_accrual_event_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_journal_insert") {
                            finance_accrual_journal_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_journal_lines_insert") {
                            finance_accrual_journal_lines_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_run_insert") {
                            finance_accrual_run_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_event_run") {
                            finance_accrual_event_run_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_journal_post") {
                            finance_accrual_journal_post_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_run_post") {
                            finance_accrual_run_post_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accrual_schedule_post") {
                            finance_accrual_schedule_post_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_event_insert") {
                            inbox_event_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_delivery_insert") {
                            inbox_delivery_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_delivery_state") {
                            inbox_delivery_state_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_staff_notification_read") {
                            inbox_staff_notification_read_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_notice_insert") {
                            inbox_notice_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_notice_target_insert") {
                            inbox_notice_target_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inbox_notice_target_complete") {
                            inbox_notice_target_complete_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_statement_package_insert") {
                            finance_statement_package_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_statement_event_prepared") {
                            finance_statement_event_prepared_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_statement_package_decision") {
                            finance_statement_package_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_statement_event_decision") {
                            finance_statement_event_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_year_closing_insert") {
                            finance_year_closing_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_year_closing_decision") {
                            finance_year_closing_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_draft_engagement") {
                            people_draft_engagement_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_draft_employee") {
                            people_draft_employee_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_draft_history") {
                            people_draft_history_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_starter_chart") {
                            finance_starter_chart_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_starter_account") {
                            finance_starter_account_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_starter_period") {
                            finance_starter_period_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_starter_dimension") {
                            finance_starter_dimension_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_starter_dimension_value") {
                            finance_starter_dimension_value_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_group_entity") {
                            finance_group_entity_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_transaction") {
                            finance_intercompany_transaction_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_event") {
                            finance_intercompany_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_status") {
                            finance_intercompany_status_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_calc_run_prepare") {
                            finance_calc_run_prepare_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_calc_event_prepare") {
                            finance_calc_event_prepare_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_calc_run_decide") {
                            finance_calc_run_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_calc_event_decide") {
                            finance_calc_event_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_close_check") {
                            finance_close_check_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_collection_prepare") {
                            finance_collection_prepare_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_collection_supersede") {
                            finance_collection_supersede_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_collection_revise") {
                            finance_collection_revise_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_collection_delivery") {
                            finance_collection_delivery_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_collection_decide") {
                            finance_collection_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_job_health_renew") {
                            legal_job_health_renew_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_job_health_start") {
                            legal_job_health_start_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_job_health_success") {
                            legal_job_health_success_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_job_health_failure") {
                            legal_job_health_failure_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("collaboration_backfill_messaging_attachment") {
                            collaboration_backfill_messaging_attachment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("org_node_team") {
                            org_node_team_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("org_node_room") {
                            org_node_room_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("org_node_room_rename") {
                            org_node_room_rename_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_handoff_person") {
                            people_handoff_person_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_handoff_engagement") {
                            people_handoff_engagement_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_handoff_compensation") {
                            people_handoff_compensation_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_org_relocation_engagement") {
                            people_org_relocation_engagement_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_org_relocation_employee") {
                            people_org_relocation_employee_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_beta_person") {
                            people_beta_person_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_beta_employee") {
                            people_beta_employee_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_beta_engagement") {
                            people_beta_engagement_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_worker_starter") {
                            people_worker_starter_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_worker_placement") {
                            people_worker_placement_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_legacy_leave_employee") {
                            people_legacy_leave_employee_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_legacy_leave_request") {
                            people_legacy_leave_request_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("beta_ai_usage_expire") {
                            beta_ai_usage_expire_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("beta_ai_draft_expire") {
                            beta_ai_draft_expire_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("collaboration_org_team") {
                            collaboration_org_team_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("collaboration_org_room") {
                            collaboration_org_room_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("workflow_command_assertion_clear") {
                            workflow_command_assertion_clear_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_statutory_remittance") {
                            people_statutory_remittance_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_assign_level") {
                            people_assign_level_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_attachment_owner_version") {
                            people_attachment_owner_version_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_leave_evidence_manifest") {
                            people_leave_evidence_manifest_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_prep_task_reschedule") {
                            people_prep_task_reschedule_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_prep_task_complete") {
                            people_prep_task_complete_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_self_service_employee") {
                            people_self_service_employee_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_issued_payslip") {
                            people_issued_payslip_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("people_expense_payment_cancel") {
                            people_expense_payment_cancel_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_supplier_bill_draft_void") {
                            finance_supplier_bill_draft_void_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_due_draft_complete") {
                            finance_due_draft_complete_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_due_draft_block") {
                            finance_due_draft_block_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_payroll_batch_invoice_version") {
                            finance_payroll_batch_invoice_version_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_payroll_batch_bank_match") {
                            finance_payroll_batch_bank_match_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_company_tool_set") {
                            finance_company_tool_set_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_group_edition_prepare") {
                            finance_group_edition_prepare_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_group_edition_supersede") {
                            finance_group_edition_supersede_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_group_edition_decide") {
                            finance_group_edition_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_payroll_other_match") {
                            finance_payroll_other_match_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_pos_party") {
                            finance_pos_party_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_payroll_remittance_match") {
                            finance_payroll_remittance_match_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_commerce_base_carry") {
                            finance_commerce_base_carry_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_expansion_review_file") {
                            finance_expansion_review_file_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_invoice_line_version") {
                            finance_invoice_line_version_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_journal_create") {
                            finance_journal_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_journal_line_create") {
                            finance_journal_line_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_pms_party") {
                            finance_pms_party_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_pms_payment") {
                            finance_pms_payment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("hospitality_tax_doc_record") {
                            hospitality_tax_doc_record_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("hospitality_tax_doc_decide") {
                            hospitality_tax_doc_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accounting_profile_create") {
                            finance_accounting_profile_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_accounting_profile_decide") {
                            finance_accounting_profile_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_statutory_profile_insert") {
                            payroll_statutory_profile_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_statutory_profile_decide") {
                            payroll_statutory_profile_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_sales_defaults_create") {
                            finance_sales_defaults_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_sales_defaults_supersede") {
                            finance_sales_defaults_supersede_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_sales_defaults_decide") {
                            finance_sales_defaults_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("expense_draft_write") {
                            expense_draft_write_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_final_pay_cancel_event") {
                            payroll_final_pay_cancel_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_run_cancel") {
                            payroll_run_cancel_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_final_pay_prep_cancel") {
                            payroll_final_pay_prep_cancel_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_deduction_consumption") {
                            payroll_deduction_consumption_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_repayment_collect") {
                            payroll_repayment_collect_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_advances_close") {
                            payroll_advances_close_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_pair_create") {
                            finance_intercompany_pair_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_claim") {
                            finance_intercompany_claim_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_pair_decide") {
                            finance_intercompany_pair_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_intercompany_claims_clear") {
                            finance_intercompany_claims_clear_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_due_draft_schedule") {
                            finance_due_draft_schedule_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_due_draft_decide") {
                            finance_due_draft_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_tax_identity_upsert") {
                            finance_tax_identity_upsert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_tax_identity_supersede") {
                            finance_tax_identity_supersede_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_tax_identity_decide") {
                            finance_tax_identity_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_reversal_journal") {
                            finance_reversal_journal_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_reversal_lines") {
                            finance_reversal_lines_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_journal_reverse") {
                            finance_journal_reverse_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_book_adjustment_create") {
                            finance_book_adjustment_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_book_adjustment_decide") {
                            finance_book_adjustment_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_book_adjustment_reverse") {
                            finance_book_adjustment_reverse_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_envelope_expire") {
                            legal_envelope_expire_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_item_update") {
                            legal_item_update_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_subtype_update") {
                            legal_subtype_update_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_settings_configure") {
                            legal_settings_configure_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_item_create") {
                            legal_item_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_subtype_create") {
                            legal_subtype_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_member_add") {
                            legal_member_add_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_requester_route") {
                            legal_requester_route_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_member_retain") {
                            legal_member_retain_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_hold_targets_expand") {
                            legal_hold_targets_expand_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_source_chunk_insert") {
                            legal_source_chunk_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_law_source_register") {
                            legal_law_source_register_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_law_revision_import") {
                            legal_law_revision_import_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_source_check_bind") {
                            legal_source_check_bind_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_text_review_import") {
                            legal_text_review_import_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_applicability_propose") {
                            legal_applicability_propose_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_applicability_approve") {
                            legal_applicability_approve_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_source_check_review") {
                            legal_source_check_review_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_document_inspection") {
                            legal_document_inspection_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_executed_copy_retain") {
                            legal_executed_copy_retain_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_canonical_document_insert") {
                            legal_canonical_document_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_stored_document_insert") {
                            legal_stored_document_insert_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_saved_view") {
                            legal_saved_view_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_derived_link") {
                            legal_derived_link_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_ai_document") {
                            legal_ai_document_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_proposal_accept") {
                            legal_proposal_accept_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_policy_create") {
                            legal_retention_policy_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_policy_decide") {
                            legal_retention_policy_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_assign") {
                            legal_retention_assign_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_request") {
                            legal_retention_request_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_change") {
                            legal_retention_change_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_intake_form_create") {
                            legal_intake_form_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_intake_form_approve") {
                            legal_intake_form_approve_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_intake_apply") {
                            legal_intake_apply_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_authority_create") {
                            legal_authority_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_authority_decide") {
                            legal_authority_decide_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_access_event") {
                            legal_access_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_trust_addition") {
                            legal_trust_addition_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_trust_decision") {
                            legal_trust_decision_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_processing_job") {
                            legal_processing_job_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_signer_view") {
                            legal_signer_view_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_item_state_advance") {
                            legal_item_state_advance_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_native_reference") {
                            legal_native_reference_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_executed_review") {
                            legal_executed_review_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_executed_copy_state") {
                            legal_executed_copy_state_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval_lock") {
                            legal_approval_lock_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval_retain_route") {
                            legal_approval_retain_route_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval_record") {
                            legal_approval_record_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval_advance") {
                            legal_approval_advance_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval_event") {
                            legal_approval_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_feature_create") {
                            legal_feature_create_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_feature_update") {
                            legal_feature_update_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_feature_history") {
                            legal_feature_history_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_advance_record") {
                            legal_advance_record_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_feature_event") {
                            legal_feature_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_bank_ignore") {
                            finance_bank_ignore_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_hotel_account") {
                            finance_hotel_account_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retention_authorization") {
                            legal_retention_authorization_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_file_preservation") {
                            legal_file_preservation_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_draft_set") {
                            legal_draft_set_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_draft_link") {
                            legal_draft_link_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_uploaded_intent") {
                            legal_uploaded_intent_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_quarantined_source") {
                            legal_quarantined_source_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("customer_receipt") {
                            customer_receipt_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("release_event_submitted") {
                            release_event_submitted_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("release_event_reviewed") {
                            release_event_reviewed_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("release_event_revoked") {
                            release_event_revoked_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("customer_portal_action") {
                            customer_portal_action_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_link_upload") {
                            legal_link_upload_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_pending_scan") {
                            legal_pending_scan_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_stored_original") {
                            legal_stored_original_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_quarantine_event") {
                            legal_quarantine_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_access_allowed") {
                            legal_access_allowed_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_access_denied") {
                            legal_access_denied_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("beta_expire_upload") {
                            beta_expire_upload_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("doc_attachment_record") {
                            doc_attachment_record_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("doc_derivative") {
                            doc_derivative_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("doc_storage_ledger") {
                            doc_storage_ledger_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("expense_withdraw_link") {
                            expense_withdraw_link_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("expense_withdraw_event") {
                            expense_withdraw_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("expense_revoke_attachment") {
                            expense_revoke_attachment_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_bind_link") {
                            legal_bind_link_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_bind_set") {
                            legal_bind_set_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_bind_event") {
                            legal_bind_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retire_source") {
                            legal_retire_source_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retire_ledger") {
                            legal_retire_ledger_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_retire_event") {
                            legal_retire_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("attachment_event") {
                            attachment_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("attachment_access_event") {
                            attachment_access_event_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_mention") {
                            social_mention_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_mention_notice") {
                            social_mention_notice_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_agent_reply") {
                            social_agent_reply_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_topic") {
                            social_topic_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_topic_binding") {
                            social_topic_binding_statement(connection, &company_id, &input).await.map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_feature") {
                            legal_feature_notice_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_sla") {
                            legal_sla_notice_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_escalation") {
                            legal_escalation_notice_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_deadline") {
                            legal_deadline_notice_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("roster_coverage") {
                            roster_coverage_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("workflow_notification") {
                            workflow_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("workflow_invoicing") {
                            workflow_invoicing_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("social_task_origin") {
                            social_task_origin_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("inventory_transit") {
                            inventory_transit_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_law_source") {
                            legal_law_source_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("company_acceptance_reviewer") {
                            company_acceptance_reviewer_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("company_acceptance_result") {
                            company_acceptance_result_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("customer_portal") {
                            customer_portal_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("explicit") {
                            explicit_notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_advance") {
                            payroll_advance_notification_statement(
                                connection,
                                &company_id,
                                input.record_id.as_str(),
                                input.timestamp.unwrap_or_default(),
                                input.user_id.as_str(),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("payroll_correction") {
                            payroll_correction_notification_statement(
                                connection,
                                &company_id,
                                input.record_id.as_str(),
                                input.timestamp.unwrap_or_default(),
                                input.user_id.as_str(),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_hold") {
                            legal_hold_notification_statement(
                                connection,
                                &company_id,
                                input.timestamp.unwrap_or_default(),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("learning_assignment") {
                            learning_assignment_notification_statement(
                                connection,
                                &company_id,
                                &input.idempotency_key,
                                input.record_id.as_str(),
                                input.change.as_deref().unwrap_or("changed"),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("supplier_promise") {
                            supplier_promise_notification_statement(
                                connection,
                                &company_id,
                                input.order_id.as_deref().unwrap_or_default(),
                                &input.idempotency_key,
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("organization_change") {
                            organization_change_notification_statement(
                                connection,
                                &company_id,
                                input.record_id.as_str(),
                                &input.idempotency_key,
                                input.event.as_deref().unwrap_or("submitted"),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("finance_refund") {
                            finance_refund_notification_statement(
                                connection,
                                &company_id,
                                input.record_id.as_str(),
                                &input.idempotency_key,
                                &input.message,
                                &input.user_id,
                                input.audience.as_deref().unwrap_or("approval"),
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("procurement_rfq") {
                            rfq_notification_statement(
                                connection,
                                &company_id,
                                input.rfq_id.as_deref().unwrap_or_default(),
                                &input.idempotency_key,
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("supplier_bill") {
                            supplier_bill_notification_statement(
                                connection,
                                &company_id,
                                input.bill_id.as_deref().unwrap_or_default(),
                                &input.idempotency_key,
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("legal_approval") {
                            legal_approval_notification_statement(
                                connection,
                                &company_id,
                                input.identifier.as_deref().unwrap_or_default(),
                                input.approver_user_id.as_deref().unwrap_or_default(),
                                input.item_id.as_deref().unwrap_or_default(),
                                input.record_version,
                                input.now,
                            )
                            .await
                            .map_err(Into::into)
                        } else if input.kind.as_deref() == Some("crm_lead") {
                            let lead_id = input.lead_id.as_deref().unwrap_or_default();
                            crate::collaboration::crm_lead_notification_statement(
                                connection, &company_id, lead_id, &input.idempotency_key,
                            )
                            .await
                            .map_err(Into::into)
                        } else {
                            notification_statement(connection, &company_id, &input)
                                .await
                                .map_err(Into::into)
                        }
                    })
                },
            )
            .await
    }

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkReadKind,
    ) -> Result<
        erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadReply,
        Error,
    > {
        use erpx_server_collaboration::public::notification_statement::{
    crm_lead_notification_statement, finance_refund_notification_statement,
    learning_assignment_notification_statement,
    organization_change_notification_statement,
    legal_approval_notification_statement, legal_hold_notification_statement, notification_statement,
    company_acceptance_reviewer_statement, company_acceptance_result_statement, inventory_transit_notification_statement, legal_law_source_notification_statement, customer_portal_notification_statement, social_task_origin_statement, workflow_invoicing_notification_statement, workflow_notification_statement, roster_coverage_notification_statement, legal_feature_notice_statement, legal_sla_notice_statement, legal_escalation_notice_statement, legal_deadline_notice_statement, social_mention_statement, social_mention_notice_statement, social_agent_reply_statement, social_topic_statement, social_topic_binding_statement, attachment_access_event_statement, attachment_event_statement, legal_retire_source_statement, legal_retire_ledger_statement, legal_retire_event_statement, legal_bind_link_statement, legal_bind_set_statement, legal_bind_event_statement, expense_withdraw_link_statement, expense_withdraw_event_statement, expense_revoke_attachment_statement, doc_attachment_record_statement, doc_derivative_statement, doc_storage_ledger_statement, beta_expire_upload_statement, legal_access_allowed_statement, legal_access_denied_statement, legal_link_upload_statement, legal_pending_scan_statement, legal_stored_original_statement, legal_quarantine_event_statement, customer_portal_action_statement, release_event_submitted_statement, release_event_reviewed_statement, release_event_revoked_statement, customer_receipt_statement, legal_draft_set_statement, legal_draft_link_statement, legal_uploaded_intent_statement, legal_quarantined_source_statement, legal_file_preservation_statement, legal_retention_authorization_statement, finance_hotel_account_statement, finance_bank_ignore_statement, legal_feature_create_statement, legal_feature_update_statement, legal_feature_history_statement, legal_advance_record_statement, legal_feature_event_statement, legal_approval_lock_statement, legal_approval_retain_route_statement, legal_approval_record_statement, legal_approval_advance_statement, legal_approval_event_statement, legal_executed_review_statement, legal_executed_copy_state_statement, legal_signer_view_statement, legal_item_state_advance_statement, legal_native_reference_statement, legal_trust_addition_statement, legal_trust_decision_statement, legal_processing_job_statement, legal_access_event_statement, legal_intake_form_create_statement, legal_intake_form_approve_statement, legal_intake_apply_statement, legal_authority_create_statement, legal_authority_decide_statement, legal_retention_policy_create_statement, legal_retention_policy_decide_statement, legal_retention_assign_statement, legal_retention_request_statement, legal_retention_change_statement, legal_saved_view_statement, legal_derived_link_statement, legal_ai_document_statement, legal_proposal_accept_statement, legal_document_inspection_statement, legal_executed_copy_retain_statement, legal_canonical_document_insert_statement, legal_stored_document_insert_statement, legal_source_chunk_insert_statement, legal_law_source_register_statement, legal_law_revision_import_statement, legal_source_check_bind_statement, legal_text_review_import_statement, legal_applicability_propose_statement, legal_applicability_approve_statement, legal_source_check_review_statement, legal_settings_configure_statement, legal_item_create_statement, legal_subtype_create_statement, legal_member_add_statement, legal_requester_route_statement, legal_member_retain_statement, legal_hold_targets_expand_statement, legal_item_update_statement, legal_subtype_update_statement, legal_envelope_expire_statement, finance_book_adjustment_create_statement, finance_book_adjustment_decide_statement, finance_book_adjustment_reverse_statement, finance_plan_edition_create_statement, finance_plan_edition_review_statement, finance_report_snapshot_statement, finance_reversal_journal_statement, finance_reversal_lines_statement, finance_journal_reverse_statement, finance_tax_identity_upsert_statement, finance_tax_identity_supersede_statement, finance_tax_identity_decide_statement, finance_due_draft_schedule_statement, finance_due_draft_decide_statement, finance_intercompany_pair_create_statement, finance_intercompany_claim_statement, finance_intercompany_pair_decide_statement, finance_intercompany_claims_clear_statement, payroll_deduction_consumption_statement, payroll_repayment_collect_statement, payroll_advances_close_statement, payroll_final_pay_cancel_event_statement, payroll_run_cancel_statement, payroll_final_pay_prep_cancel_statement, expense_draft_write_statement, finance_sales_defaults_create_statement, finance_sales_defaults_supersede_statement, finance_sales_defaults_decide_statement, payroll_statutory_profile_insert_statement, payroll_statutory_profile_decide_statement, finance_accounting_profile_create_statement, finance_accounting_profile_decide_statement, hospitality_tax_doc_record_statement, hospitality_tax_doc_decide_statement, finance_pms_party_statement, finance_pms_payment_statement, finance_journal_create_statement, finance_journal_line_create_statement, finance_invoice_line_version_statement, finance_expansion_review_file_statement, finance_commerce_base_carry_statement, finance_payroll_remittance_match_statement, finance_pos_party_statement, finance_payroll_other_match_statement, finance_group_edition_prepare_statement, finance_group_edition_supersede_statement, finance_group_edition_decide_statement, finance_company_tool_set_statement, finance_payroll_batch_invoice_version_statement, finance_payroll_batch_bank_match_statement, finance_due_draft_complete_statement, finance_due_draft_block_statement, finance_supplier_bill_draft_void_statement, people_self_service_employee_statement, people_issued_payslip_statement, people_expense_payment_cancel_statement, people_leave_evidence_manifest_statement, people_prep_task_reschedule_statement, people_prep_task_complete_statement, people_statutory_remittance_statement, people_assign_level_statement, people_attachment_owner_version_statement, workflow_command_assertion_clear_statement, collaboration_org_team_statement, collaboration_org_room_statement, legal_backfill_document_attachment_statement, legal_backfill_external_attachment_statement, beta_ai_usage_expire_statement, beta_ai_draft_expire_statement, people_legacy_leave_employee_statement, people_legacy_leave_request_statement, people_worker_starter_statement, people_worker_placement_statement, people_beta_person_statement, people_beta_employee_statement, people_beta_engagement_statement, people_org_relocation_engagement_statement, people_org_relocation_employee_statement, people_handoff_person_statement, people_handoff_engagement_statement, people_handoff_compensation_statement, org_node_team_statement, org_node_room_statement, org_node_room_rename_statement, collaboration_backfill_messaging_attachment_statement, legal_job_health_renew_statement, legal_job_health_start_statement, legal_job_health_success_statement, legal_job_health_failure_statement, finance_collection_prepare_statement, finance_collection_supersede_statement, finance_collection_revise_statement, finance_collection_delivery_statement, finance_collection_decide_statement, finance_calc_run_prepare_statement, finance_calc_event_prepare_statement, finance_calc_run_decide_statement, finance_calc_event_decide_statement, finance_close_check_statement, finance_group_entity_statement, finance_intercompany_transaction_statement, finance_intercompany_event_statement, finance_intercompany_status_statement, finance_starter_chart_statement, finance_starter_account_statement, finance_starter_period_statement, finance_starter_dimension_statement, finance_starter_dimension_value_statement, people_draft_engagement_statement, people_draft_employee_statement, people_draft_history_statement, finance_statement_package_insert_statement, finance_statement_event_prepared_statement, finance_statement_package_decision_statement, finance_statement_event_decision_statement, finance_year_closing_insert_statement, finance_year_closing_decision_statement, inbox_event_insert_statement, inbox_delivery_insert_statement, inbox_delivery_state_statement, inbox_staff_notification_read_statement, inbox_notice_insert_statement, inbox_notice_target_insert_statement, inbox_notice_target_complete_statement, finance_accrual_schedule_insert_statement, finance_accrual_event_created_statement, finance_accrual_schedule_decision_statement, finance_accrual_event_decision_statement, finance_accrual_journal_insert_statement, finance_accrual_journal_lines_insert_statement, finance_accrual_run_insert_statement, finance_accrual_event_run_statement, finance_accrual_journal_post_statement, finance_accrual_run_post_statement, finance_accrual_schedule_post_statement, pms_finance_party_statement, pms_finance_invoice_statement, pms_finance_payment_statement, pms_finance_allocation_delete_statement, pms_finance_invoice_update_statement, pms_finance_allocation_insert_statement, pms_finance_bank_match_statement, books_opening_journal_statement, books_opening_journal_lines_statement, books_rule_decision_statement, books_dimension_definition_statement, books_dimension_value_statement, books_rule_prepare_statement, books_account_mapping_statement, wh_withholding_consumed_statement, wh_assessment_reversed_statement, wh_thai_document_cancel_statement, wh_assessment_insert_statement, wh_income_line_insert_statement, wh_payment_withholding_statement, wh_assessment_decision_statement, thai_doc_sequence_seed_statement, thai_doc_sequence_bump_statement, thai_doc_issue_statement, thai_doc_event_issued_statement, thai_doc_change_insert_statement, thai_doc_event_supersede_statement, thai_doc_change_decision_statement, workflow_command_assertion_clear_many_statement, legal_destruction_block_statement, legal_destruction_start_statement, legal_retention_started_event_statement, legal_destruction_count_attempt_statement, legal_destruction_retain_executing_statement, legal_disposal_retain_statement, legal_document_erase_statement, legal_processing_retire_statement, legal_inspections_erase_statement, legal_ai_retire_statement, legal_shares_revoke_statement, legal_destruction_complete_statement, legal_retention_destroyed_event_statement, legal_destruction_require_review_statement, legal_portal_access_event_statement, legal_portal_accept_share_statement, legal_portal_external_comment_statement, legal_portal_counsel_entry_statement, legal_portal_counsel_history_statement, legal_portal_interaction_event_statement, legal_portal_receipt_statement, legal_portal_external_upload_statement, legal_portal_advance_upload_statement, legal_portal_upload_event_statement, explicit_notification_statement, payroll_advance_notification_statement, payroll_correction_notification_statement,
    rfq_notification_statement,
    supplier_bill_notification_statement, supplier_promise_notification_statement,
    NotificationStatement, NotificationStatementInput,
};
use erpx_server_collaboration::public::notification_statement_http::{
    StatementBackend, StatementFuture, StatementRequest,
};
use erpx_server_collaboration::public::collaboration_work::{
            read_scope_card, read_scope_relation, read_scope_workspace,
        };
        use erpx_server_collaboration::public::collaboration_work_http::{
            CollaborationCardData, CollaborationRelationData, CollaborationWorkHttpReadReply,
            CollaborationWorkReadKind, CollaborationWorkspaceData,
        };
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "collaboration".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let scope = collaboration_scope(current, &company, now)?;
                        let mut sources = CollaborationSourceResolver {
                            company: company.clone(),
                        };
                        let can_write = current.has_permission("records.write");
                        let can_review = current.has_permission("records.review");
                        Ok(match kind {
                            CollaborationWorkReadKind::Workspace => {
                                let workspace =
                                    read_scope_workspace(connection, scope, &mut sources).await?;
                                CollaborationWorkHttpReadReply::Workspace(Box::new(
                                    CollaborationWorkspaceData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        relations: workspace.relations,
                                        cards: workspace.cards,
                                        people: workspace.people,
                                        source_types: workspace
                                            .source_types
                                            .into_iter()
                                            .map(str::to_owned)
                                            .collect(),
                                    },
                                ))
                            }
                            CollaborationWorkReadKind::Card { id } => {
                                let card =
                                    read_scope_card(connection, scope, &mut sources, &id).await?;
                                CollaborationWorkHttpReadReply::Card(Box::new(
                                    CollaborationCardData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        card: serde_json::json!({
                                            "id": card.id,
                                            "version": card.version,
                                            "open": card.open,
                                            "status": if card.open { "open" } else { "closed" },
                                            "owner_id": card.owner_id,
                                            "due_at": card.due_at,
                                            "owner_eligible": if card.owner_eligible { 1 } else { 0 },
                                            "source_type": card.source.kind.as_str(),
                                            "source_id": card.source.id,
                                        }),
                                    },
                                ))
                            }
                            CollaborationWorkReadKind::Relation { id } => {
                                let relation =
                                    read_scope_relation(connection, scope, &mut sources, &id)
                                        .await?;
                                CollaborationWorkHttpReadReply::Relation(Box::new(
                                    CollaborationRelationData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        relation: serde_json::json!({
                                            "id": relation.id,
                                            "version": relation.version,
                                            "status": if relation.active { "active" } else { "removed" },
                                            "source_type": relation.left.kind.as_str(),
                                            "source_id": relation.left.id,
                                            "target_type": relation.right.kind.as_str(),
                                            "target_id": relation.right.id,
                                        }),
                                    },
                                ))
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        change: erpx_server_collaboration::public::collaboration_work::Change,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<erpx_server_collaboration::public::collaboration_work::Receipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "collaboration".into(),
                    api: true,
                },
                CollaborationWorkSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let change = change.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let scope = collaboration_scope(current, &company, now)?;
                        let mut sources = CollaborationSourceResolver {
                            company: company.clone(),
                        };
                        erpx_server_collaboration::public::collaboration_work::execute_scope(
                            connection,
                            scope,
                            key,
                            &mut sources,
                            change,
                        )
                        .await
                    })
                },
            )
            .await
    }
}

pub struct CollaborationWorkBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CollaborationWorkService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CollaborationWorkBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CollaborationWorkService<C>>,
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
            return Err(not_found());
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
}

impl<C: Clock + Send + Sync + 'static>
    erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpBackend
    for CollaborationWorkBackend<C>
{
    fn read<'a>(
        &'a self,
        request: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadRequest<'a>,
    ) -> erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpFuture<
        'a,
        erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadReply,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpCommandRequest<'a>,
    ) -> erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpFuture<
        'a,
        erpx_server_collaboration::public::collaboration_work::Receipt,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(request.host, proof, selection, request.change, request.key)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> StatementBackend for CollaborationWorkBackend<C> {
    fn statement<'a>(&'a self, request: StatementRequest<'a>) -> StatementFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .notification_statement(request.host, proof, selection, request.input.clone())
                .await
                .map_err(business)
        })
    }
}
