import {identitySnapshotSources as manageerpxSnapshotSources} from '../../manage_erpx/public/identity-snapshot-sources';
const manageerpxSources=manageerpxSnapshotSources();
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { createBetaAccess } from "../application/beta-access";
export { betaDestinationAllowed } from "../domain/beta-access";
export async function betaAccess(db: SqlDatabase, companyId: string, requestHeaders: Headers) { return createBetaAccess({ enabled: async (companyId) => !!await db.prepare(`SELECT 1 ${manageerpxSources.beta_access_beta_policy_from} WHERE company_id=?`).bind(companyId).first(), error: (status, code, message) => new ApiError(status, code, message) })(companyId, requestHeaders); }
