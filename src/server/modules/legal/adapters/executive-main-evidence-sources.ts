import {legalMetricReadScope} from "./metric-purchases";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { visibility as legalVisibility } from "@/server/modules/legal/public/legal/access";
import { LegalEvidenceRow, LegalHoldEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
function activeSchedulePredicate() {
    return `i.kind IN ('contract','obligation','license','ip','matter')
    AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed')
    AND s.state NOT IN ('completed','waived','cancelled','closed','acknowledged','accepted','billed')`;
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchLegalEvidence(actor: WorkContext, isObligation: boolean, cursor: {
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "active-legal-obligations" | "active-legal-holds";
            cursor?: string | undefined;
        }): Promise<{
            rows: LegalEvidenceRow[] | LegalHoldEvidenceRow[];
        }> {
            const visibility = legalVisibility(actor),purchase=await legalMetricReadScope(db,actor,isObligation?"active-legal-obligations":"active-legal-holds");
            const conditions = [visibility.sql,purchase.where, isObligation
                    ? activeSchedulePredicate()
                    : "h.state IN ('active','release_requested')"];
            const values: unknown[] = [...visibility.values];
            if (input.query) {
                conditions.push(isObligation
                    ? "(position(lower(?) in lower(s.title||' '||s.category||' '||i.title))>0)"
                    : "(position(lower(?) in lower(i.title||' '||i.kind||' '||h.state))>0)");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push(`${isObligation ? "s" : "h"}.id>?`);
                values.push(cursor.id);
            }
            const rows = isObligation
                ? (await db.prepare(`SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
        WHERE ${conditions.join(" AND ")} ORDER BY s.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalEvidenceRow>()).results
                : (await db.prepare(`SELECT h.id,h.item_id,h.state,h.created_at,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
        WHERE ${conditions.join(" AND ")} ORDER BY h.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalHoldEvidenceRow>()).results;
            return { rows };
        }, async factsReadLegalEvidence(actor: WorkContext, isObligation: boolean, recordId: string): Promise<{
            row: LegalEvidenceRow | LegalHoldEvidenceRow | null;
        }> {
            const visibility = legalVisibility(actor),purchase=await legalMetricReadScope(db,actor,isObligation?"active-legal-obligations":"active-legal-holds");
            const row = isObligation
                ? await db.prepare(`SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
        WHERE s.id=? AND ${visibility.sql} AND ${purchase.where} AND ${activeSchedulePredicate()}`)
                    .bind(recordId, ...visibility.values).first<LegalEvidenceRow>()
                : await db.prepare(`SELECT h.id,h.item_id,h.state,h.created_at,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
        WHERE h.id=? AND ${visibility.sql} AND ${purchase.where} AND h.state IN ('active','release_requested')`)
                    .bind(recordId, ...visibility.values).first<LegalHoldEvidenceRow>();
            return { row };
        } };
}
