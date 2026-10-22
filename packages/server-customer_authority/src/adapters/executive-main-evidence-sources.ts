import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { DuplicateReviewEvidenceRow, IdentityChangeEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
const identityChangeSource = `SELECT e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchDuplicateReviewEvidence(cursor: {
            updatedAt: number;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "duplicate-reviews-pending";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: DuplicateReviewEvidenceRow[];
        }> {
            const conditions = ["r.company_id=?", "r.status='pending'"];
            const values: unknown[] = [actor.companyId];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',r.id,r.status,l.name,l.code,q.name,q.code)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(r.updated_at<? OR (r.updated_at=? AND r.id>?))");
                values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
            }
            const rows = (await db.prepare(`SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'
    WHERE ${conditions.join(" AND ")} ORDER BY r.updated_at DESC,r.id ASC LIMIT ?`)
                .bind(...values, input.limit + 1).all<DuplicateReviewEvidenceRow>()).results;
            return { rows };
        }, async factsReadDuplicateReviewEvidence(actor: WorkContext, id: string): Promise<{
            row: DuplicateReviewEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'
    WHERE r.company_id=? AND r.status='pending' AND r.id=? LIMIT 1`).bind(actor.companyId, id).first<DuplicateReviewEvidenceRow>();
            return { row };
        }, async factsSearchIdentityChangeEvidence(cursor: {
            at: number;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "identity-change-events";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: IdentityChangeEvidenceRow[];
        }> {
            const conditions = [] as string[];
            const values: unknown[] = [actor.companyId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.action,c.status,p.name,p.code)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(e.created_at<? OR (e.created_at=? AND e.id>?))");
                values.push(cursor.at, cursor.at, cursor.id);
            }
            const rows = (await db.prepare(`${identityChangeSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY e.created_at DESC,e.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<IdentityChangeEvidenceRow>()).results;
            return { rows };
        }, async factsReadIdentityChangeEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, id: string): Promise<{
            row: IdentityChangeEvidenceRow | null;
        }> {
            const row = await db.prepare(`${identityChangeSource} AND e.id=? LIMIT 1`)
                .bind(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id).first<IdentityChangeEvidenceRow>();
            return { row };
        } };
}
