import {legalMetricReadScope} from "@erpx/legal-server/adapters/metric-purchases";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { visibility as legalVisibility } from "@erpx/legal-server/public/legal/access";
import { LegalEvidenceRow, LegalHoldEvidenceRow } from '@erpx/platform-runtime-server/public/executive/evidence-contract';
import { legalExecutiveObligationSpineHead, legalExecutiveHoldSpineHead } from './legal-read-tail-r46-sources';
import contract from './executive-main-evidence-sources.json' with { type: 'json' };
/** Native-owned evidence projections shared by the search and point reads.
 * Every call site still prepares and executes its own complete statement. */
const evidenceObligationSelect = legalExecutiveObligationSpineHead(
    contract.obligationProjection, '        ');
const evidenceHoldSelect = legalExecutiveHoldSpineHead(
    contract.holdProjection, '        ');
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
                ? (await db.prepare(`${evidenceObligationSelect}${conditions.join(" AND ")} ORDER BY s.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalEvidenceRow>()).results
                : (await db.prepare(`${evidenceHoldSelect}${conditions.join(" AND ")} ORDER BY h.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalHoldEvidenceRow>()).results;
            return { rows };
        }, async factsReadLegalEvidence(actor: WorkContext, isObligation: boolean, recordId: string): Promise<{
            row: LegalEvidenceRow | LegalHoldEvidenceRow | null;
        }> {
            const visibility = legalVisibility(actor),purchase=await legalMetricReadScope(db,actor,isObligation?"active-legal-obligations":"active-legal-holds");
            const row = isObligation
                ? await db.prepare(`${evidenceObligationSelect}s.id=? AND ${visibility.sql} AND ${purchase.where} AND ${activeSchedulePredicate()}`)
                    .bind(recordId, ...visibility.values).first<LegalEvidenceRow>()
                : await db.prepare(`${evidenceHoldSelect}h.id=? AND ${visibility.sql} AND ${purchase.where} AND h.state IN ('active','release_requested')`)
                    .bind(recordId, ...visibility.values).first<LegalHoldEvidenceRow>();
            return { row };
        } };
}
