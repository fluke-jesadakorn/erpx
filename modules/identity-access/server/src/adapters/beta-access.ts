import {identitySnapshotSources as manageerpxSnapshotSources} from '@erpx/manage-erpx-server/public/identity-snapshot-sources';
const manageerpxSources=manageerpxSnapshotSources();
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createBetaAccess } from "@erpx/identity-access-server/application/beta-access";
export { betaDestinationAllowed } from "@erpx/identity-access-server/domain/beta-access";
export async function betaAccess(db: SqlDatabase, companyId: string, requestHeaders: Headers) { return createBetaAccess({ enabled: async (companyId) => !!await db.prepare(`SELECT 1 ${manageerpxSources.beta_access_beta_policy_from} WHERE company_id=?`).bind(companyId).first(), error: (status, code, message) => new ApiError(status, code, message) })(companyId, requestHeaders); }
