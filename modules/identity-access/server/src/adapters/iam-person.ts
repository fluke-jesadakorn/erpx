import 'server-only';
import { hasPolicyReadSnapshot } from "@erpx/identity-access-server/public/iam-read-snapshot";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeIdentityIds, nativePrincipals, type NativeLane11Actor } from "./native-lane-11/client";

/**
 * Person-identity reads are owned by the native lane 11 axum service over the
 * Identity Access `iam_members` table. This adapter is now a strict transport:
 * no SQL, no local projection and no fallback read remain here. The retained
 * `db` argument keeps existing port signatures unchanged; it is deliberately
 * unused because the owner pool is bound per Holding at composition time.
 *
 * With a WorkContext the registered Holding is selected by holdingId +
 * holdingSlug. Legacy callers that pass only a Company id are resolved by the
 * single Holding whose configured `companyId` matches; zero or multiple
 * matches fail closed. There is no first-Holding default and no SQL fallback.
 */
export async function personIdentityIds(
    _db: SqlDatabase,
    companyId: string,
    ids: readonly string[],
    actor?: NativeLane11Actor,
) {
    if (!ids.length) return [];
    return nativeIdentityIds(actor, ids, companyId);
}

export async function currentPersonPrincipals(
    _db: SqlDatabase,
    companyId: string,
    ids: readonly string[],
    actor?: NativeLane11Actor,
) {
    if (!ids.length) return new Map<string, string>();
    return nativePrincipals(actor, ids, hasPolicyReadSnapshot(), companyId);
}
