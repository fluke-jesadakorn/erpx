import type { AccessProblem } from "@erpx/feature-shared/lib/access-problem";
export {safeReturnPath} from '@erpx/contracts/safe-return-path';
export type AccessAssignment = {
    applies: boolean;
    validRole: boolean;
    valid_from: number;
    valid_until: number;
    revoked_at: number | null;
};
/** Classify the user's own failed access; a usable alternative takes priority over historical rows. */
export function assignmentProblem(rows: AccessAssignment[], now: number): AccessProblem {
    const matching = rows.filter(r => r.applies);
    const valid = matching.filter(r => r.validRole);
    const unrevoked = valid.filter(r => r.revoked_at === null);
    if (unrevoked.some(r => r.valid_from <= now && r.valid_until > now))
        return "permission";
    if (unrevoked.some(r => r.valid_from > now && r.valid_until > r.valid_from))
        return "scheduled";
    if (unrevoked.some(r => r.valid_until <= now))
        return "expired";
    if (valid.length && valid.every(r => r.revoked_at !== null))
        return "revoked";
    if (matching.some(r => !r.validRole))
        return "role";
    if (rows.some(r => r.validRole && r.revoked_at === null && r.valid_from <= now && r.valid_until > now))
        return "scope";
    return "no_access";
}
