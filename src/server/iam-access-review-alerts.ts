import "server-only";

import type { SqlDatabase } from "./runtime/types";

export type AccessReviewAlertResult = { dueSoon: number; overdue: number };

async function insertAccessReviewAlerts(
  db: SqlDatabase,
  phase: "due-soon" | "overdue",
  now: number,
  horizon: number,
) {
  const duePredicate =
    phase === "due-soon" ? "c.due_at>? AND c.due_at<=?" : "c.due_at<=?";
  const dueArguments = phase === "due-soon" ? [now, horizon] : [now];
  const message =
    phase === "due-soon"
      ? "Access certification is due within 7 days; review pending assignments or coordinate an independent reviewer"
      : "Access certification is overdue; complete the remaining independent decisions";
  const result = await db
    .prepare(
      `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
       SELECT 'iam-access-review-${phase}-'||md5(c.id||':'||c.due_at::text||':'||a.user_id),
              c.company_id,a.user_id,'iam_access_review',c.id,c.version,?,?
       FROM iam_access_review_campaigns c
       JOIN assignments a ON a.company_id=c.company_id
       WHERE c.status='open'
         AND EXISTS(SELECT 1 FROM iam_access_review_items i WHERE i.campaign_id=c.id AND i.decision='pending')
         AND ${duePredicate}
         AND a.scope_kind='holding'
         AND a.revoked_at IS NULL
         AND iam_assignment_active(a.id,?)
         AND iam_role_allows(a.role,a.company_id,'iam.review')
       GROUP BY c.id,c.company_id,c.version,c.due_at,a.user_id
       ON CONFLICT DO NOTHING`,
    )
    .bind(message, now, ...dueArguments, now)
    .run();
  return Number(result.meta.changes ?? 0);
}

/**
 * Emits one durable reminder per eligible reviewer before a campaign is due,
 * and one further escalation after it becomes overdue. Current assignment and
 * principal authority are rechecked by PostgreSQL on every scheduled run.
 */
export async function escalateAccessReviews(
  db: SqlDatabase,
  now = Date.now(),
  warningWindowMs = 7 * 86_400_000,
): Promise<AccessReviewAlertResult> {
  const dueSoon = await insertAccessReviewAlerts(
    db,
    "due-soon",
    now,
    now + warningWindowMs,
  );
  const overdue = await insertAccessReviewAlerts(db, "overdue", now, now);
  return { dueSoon, overdue };
}

export async function canReadAccessReviewAlert(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  campaignId: string,
  now = Date.now(),
) {
  return Boolean(
    await db
      .prepare(
        `SELECT 1
         FROM iam_access_review_campaigns c
         JOIN assignments a ON a.company_id=c.company_id AND a.user_id=?
         WHERE c.id=? AND c.company_id=?
           AND a.scope_kind='holding'
           AND a.revoked_at IS NULL
           AND iam_assignment_active(a.id,?)
           AND iam_role_allows(a.role,a.company_id,'iam.review')
         LIMIT 1`,
      )
      .bind(userId, campaignId, companyId, now)
      .first(),
  );
}
