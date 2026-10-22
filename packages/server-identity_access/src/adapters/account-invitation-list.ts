import "server-only";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { createAccountInvitationListApplication, type AccountInvitationListRepository } from "@erpx/server-identity_access/application/account-invitation-list";
import type { AccountInvitationListItem } from "@erpx/server-identity_access/domain/account-invitation-list";

export function accountInvitationListRepository(control: SqlDatabase): AccountInvitationListRepository {
  return {
    async list(scope) {
      return (await control.prepare(
        "SELECT i.id,i.person_id,i.email,CASE WHEN i.status='pending' AND i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'expired' ELSE i.status END status,i.expires_at,d.state delivery_state,d.attempts,d.error,d.sent_at FROM account_invitations i LEFT JOIN account_invitation_delivery_jobs d ON d.invitation_id=i.id WHERE i.holding_id=? AND i.company_id=? AND (?::text IS NULL OR i.branch_id=?) ORDER BY i.created_at DESC LIMIT 100",
      ).bind(scope.holdingId, scope.companyId, scope.branchId, scope.branchId).all<AccountInvitationListItem>()).results;
    },
  };
}

export async function accountInvitationList(actor: WorkContext, provided?: SqlDatabase) {
  return createAccountInvitationListApplication({
    list: async scope => accountInvitationListRepository(provided ?? (await runtimeEnv()).CONTROL_DB).list(scope),
  }).read(actor);
}
