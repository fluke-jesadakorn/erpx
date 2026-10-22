import "server-only";
import { randomUUID } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { workContextGuards } from "@erpx/server-workflow/public/commands";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";

export async function assertPmsProviderAuthority(db: SqlDatabase,actor: WorkContext,permission: string) {
  requirePermission(actor,permission);
  const grant=await db.prepare(`SELECT id FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=?
    AND valid_from<=? AND valid_until>? AND revoked_at IS NULL AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) FOR SHARE`)
    .bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,Date.now(),Date.now(),actor.branchId,actor.branchId).first();
  if(!grant)throw new ApiError(403,"PMS_ACCESS_CHANGED","Select a current assignment before reviewing.");
  const key=`pms-provider-authority-${randomUUID()}`;
  await db.batch([...workContextGuards(db,actor,key),...iamMutationGuards(db,actor,key),db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
