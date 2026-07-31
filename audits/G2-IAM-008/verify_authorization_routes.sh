#!/usr/bin/env bash
set -euo pipefail

server="$(cd "$(dirname "$0")/../.." && pwd)/apps/server/src/main.rs"

check_marker() {
  local method="$1"
  local marker="$2"
  awk -v target="$method" -v marker="$marker" '
    /^[[:space:]]*(pub )?async fn / {
      if (in_method) {
        if (!found) exit 1
        target_seen = 1
      }
      in_method = ($0 ~ "async fn " target "\\(")
      found = 0
    }
    in_method && index($0, marker) { found = 1 }
    END {
      if (in_method && !found) exit 1
      if (in_method && found) target_seen = 1
      if (!target_seen) exit 1
    }
  ' "$server"
}

# Authenticated tenant mutations must use the single authoritative
# entitlement/RBAC intersection. The list names the implementation methods,
# not legacy generated RPC forwarding wrappers.
for method in \
  create_ticket update_ticket create_ticket_recurrence add_ticket_reply \
  add_ticket_internal_note request_support_session approve_support_session \
  save_employee save_leave save_expense save_payroll save_pos_sale \
  save_contract save_document record_access save_crm_handoff \
  save_crm_opportunity record_crm_attribution submit_marketing_form \
  erase_marketing_profile issue_marketing_preview revoke_marketing_preview \
  issue_marketing_domain_verification verify_marketing_domain bind_marketing_domain \
  configure_marketing_provider_account acknowledge_marketing_provider_verification \
  set_marketing_provider_account_state save_subledger save_allocation_batch \
  save_finance_period post_finance_journal save_finance_chart save_finance_close_review \
  register_bank_account \
  import_bank_statement save_bank_reconciliation save_bank_match_proposal \
  save_bank_match_decision; do
  check_marker "$method" "authorize_entitled_command"
done

# Platform/control-plane mutations use the narrower authoritative role policy
# and an explicit platform/control scope.
for method in begin_tenant_provisioning provision_tenant_resources \
  activate_tenant suspend_tenant enter_tenant_recovery \
  schedule_tenant_backup \
  start_projection_rebuild activate_projection_rebuild \
  create_incident update_incident revoke_support_session; do
  check_marker "$method" "authorize_role"
done

# Role lifecycle methods use the dedicated role-lifecycle policy, with the
# tenant branch delegating into the same entitlement intersection.
check_marker renew_role "authorize_role_lifecycle_command"
check_marker recover_final_role "authorize_independent_role"
check_marker list_tenant_backup_jobs "authorize_role"

echo "G2-IAM-008 authorization route inventory passed"
