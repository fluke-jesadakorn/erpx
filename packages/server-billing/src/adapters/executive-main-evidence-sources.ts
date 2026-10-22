import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { type CommercialSubscriptionRow, type OverdueInvoiceEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
function commercialSubscriptionSource() {
    return `WITH subscriptions AS (
      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,
        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,
        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version
      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id
      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
      UNION ALL
      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,
        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version
      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
    )`;
}
function overdueInvoiceSource() {
    return `SELECT i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month
    FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id AND p.holding_id=i.holding_id
    WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?`;
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    // Billing evidence is recorded in its control store; the caller Holding DB owns its audit event.
    void db;
    return { async factsSearchCommercialSubscriptions(cursor: {
            sourceKind: "enrollment" | "legacy";
            recordId: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "active-subscriptions";
            cursor?: string | undefined;
        }, actor: WorkContext, now: number): Promise<{
            rows: CommercialSubscriptionRow[];
        }> {
            const sourceDb = (await runtimeEnv()).CONTROL_DB;
            const values: unknown[] = [actor.holdingId, now, now, actor.holdingId, now];
            const conditions: string[] = [];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',plan_name,status,source_kind,started_at::text,effective_at::text)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(source_kind>? OR (source_kind=? AND record_id>?))");
                values.push(cursor.sourceKind, cursor.sourceKind, cursor.recordId);
            }
            const rows = (await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
    ${conditions.length ? `WHERE ${conditions.join(" AND ")}` : ""} ORDER BY source_kind,record_id LIMIT ?`)
                .bind(...values, input.limit + 1).all<CommercialSubscriptionRow>()).results;
            return { rows };
        }, async factsReadCommercialSubscription(decoded: {
            sourceKind: "enrollment" | "legacy";
            recordId: string;
        }, actor: WorkContext, now: number): Promise<{
            row: CommercialSubscriptionRow | null;
        }> {
            const sourceDb = (await runtimeEnv()).CONTROL_DB;
            const row = decoded.sourceKind === "enrollment"
                ? await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
        WHERE source_kind='enrollment' AND record_id=? LIMIT 1`).bind(actor.holdingId, now, now, actor.holdingId, now, decoded.recordId).first<CommercialSubscriptionRow>()
                : await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
        WHERE source_kind='legacy' AND record_id=? LIMIT 1`).bind(actor.holdingId, now, now, actor.holdingId, now, decoded.recordId).first<CommercialSubscriptionRow>();
            return { row };
        }, async factsSearchOverdueInvoiceEvidence(cursorValue: {
            dueAt: number;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "overdue-invoices";
            cursor?: string | undefined;
        }, actor: WorkContext, end: number, now: number): Promise<{
            rows: OverdueInvoiceEvidenceRow[];
        }> {
            const sourceDb = (await runtimeEnv()).CONTROL_DB;
            const values: unknown[] = [actor.holdingId, Date.parse(input.filters.from), end, now];
            const conditions: string[] = [];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',i.id,i.kind,i.status,p.month)))>0");
                values.push(input.query);
            }
            if (cursorValue) {
                conditions.push("(i.due_at>? OR (i.due_at=? AND i.id>?))");
                values.push(cursorValue.dueAt, cursorValue.dueAt, cursorValue.id);
            }
            const rows = (await sourceDb.prepare(`${overdueInvoiceSource()}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY i.due_at,i.id LIMIT ?`).bind(...values, input.limit + 1).all<OverdueInvoiceEvidenceRow>()).results;
            return { rows };
        }, async factsReadOverdueInvoiceEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, now: number, id: string): Promise<{
            row: OverdueInvoiceEvidenceRow | null;
        }> {
            const sourceDb = (await runtimeEnv()).CONTROL_DB;
            const row = await sourceDb.prepare(`${overdueInvoiceSource()} AND i.id=? LIMIT 1`)
                .bind(actor.holdingId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, now, id).first<OverdueInvoiceEvidenceRow>();
            return { row };
        } };
}
