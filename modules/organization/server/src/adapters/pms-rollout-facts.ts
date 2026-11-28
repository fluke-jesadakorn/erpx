import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeOrgOne } from "./native-organization-owner-write";

/** Native-owned PMS rollout active-branch fact. The authoritative SQL and
 * Company scope live in the Organization owner crate; this adapter sends only
 * the typed kind and the caller's Company value and keeps the row-returning
 * shape. The single downstream consumer reduces it with Boolean(), so a null
 * or row reply preserves parity. */
export function pmsRolloutOrganizationFacts(db: SqlDatabase) {
    return {
        organization: (companyId: string) => nativeOrgOne(db, companyId, 'pms_rollout_active_branch', [companyId])
    };
}
