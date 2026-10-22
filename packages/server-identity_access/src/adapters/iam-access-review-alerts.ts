import {insertIdentityAccessReviewAlerts} from "@erpx/server-collaboration/public/identity-notification-effects";
import { createAccessReviewAlerts, type AccessReviewAlertResult } from '@erpx/server-identity_access/application/access-review-alerts';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export type { AccessReviewAlertResult } from '@erpx/server-identity_access/application/access-review-alerts';
/**
 * Emits one durable reminder per eligible reviewer before a campaign is due,
 * and one further escalation after it becomes overdue. Current assignment and
 * principal authority are rechecked by PostgreSQL on every scheduled run.
 */
export async function escalateAccessReviews(db: SqlDatabase, now = Date.now(), warningWindowMs = 7 * 86400000): Promise<AccessReviewAlertResult> { return createAccessReviewAlerts({ emit: (phase, now, horizon, message) => insertIdentityAccessReviewAlerts(db, phase, now, horizon, message), read: async () => false }).escalate(now, warningWindowMs); }
export async function canReadAccessReviewAlert(db: SqlDatabase, companyId: string, userId: string, campaignId: string, now = Date.now()) {
    return Boolean(await db
        .prepare(`SELECT 1
         FROM iam_access_review_campaigns c
         JOIN assignments a ON a.company_id=c.company_id AND a.user_id=?
         WHERE c.id=? AND c.company_id=?
           AND a.scope_kind='holding'
           AND a.revoked_at IS NULL
           AND iam_assignment_active(a.id,?)
           AND iam_role_allows(a.role,a.company_id,'iam.review')
         LIMIT 1`)
        .bind(userId, campaignId, companyId, now)
        .first());
}
