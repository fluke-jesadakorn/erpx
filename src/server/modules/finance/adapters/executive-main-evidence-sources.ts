import {requireFinanceMetricPurchase} from "./source-purchases";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { InvoiceEvidenceRow, CloseBlockerEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
function closeBlockerSource() {
    return `WITH current_checks AS (
      SELECT DISTINCT ON (c.period_id,c.check_key) c.id,c.period_id,c.check_key,c.source_version,c.status,c.count_value,c.checked_at,c.resolved_at,
        p.name period_name,p.end_date,p.state period_state
      FROM finance_close_checks c JOIN finance_periods p ON p.id=c.period_id AND p.company_id=c.company_id
      WHERE c.company_id=? AND p.company_id=? AND p.end_date BETWEEN ? AND ? AND p.state IN ('open','closing')
      ORDER BY c.period_id,c.check_key,c.checked_at DESC,c.id DESC
    ) SELECT latest.id,latest.period_id,latest.period_name,latest.end_date,latest.period_state,latest.check_key,latest.source_version,
      latest.status,latest.count_value,latest.checked_at,latest.resolved_at FROM current_checks latest`;
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchFinanceEvidence(type: {
            kind: string;
            statuses: string[];
        }, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "receivables" | "payables" | "invoiced-sales";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            invoiceDate: string;
            id: string;
        } | null): Promise<{
            rows: InvoiceEvidenceRow[];
        }> {
            await requireFinanceMetricPurchase(db,actor,input.metric);
            const conditions = ["company_id=?", "kind=?", "currency=?", "invoice_date BETWEEN ? AND ?", `status IN (${type.statuses.map(() => "?").join(",")})`];
            const values: unknown[] = [actor.companyId, type.kind, input.filters.currency, input.filters.from, input.filters.to, ...type.statuses];
            if (input.query) {
                conditions.push("position(lower(?) in lower(invoice_number))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(invoice_date,id)<(?,?)");
                values.push(cursor.invoiceDate, cursor.id);
            }
            const rows = (await db.prepare(`SELECT id,invoice_number,kind,invoice_date,due_date,currency,total_minor::text total_minor,
      outstanding_minor::text outstanding_minor,status,version
    FROM finance_invoices WHERE ${conditions.join(" AND ")}
    ORDER BY invoice_date DESC,id DESC LIMIT ?`).bind(...values, input.limit + 1).all<InvoiceEvidenceRow>()).results;
            return { rows };
        }, async factsReadFinanceEvidence(type: {
            kind: string;
            statuses: string[];
        }, recordId: string, actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: (InvoiceEvidenceRow & {
                subtotal_minor: string;
                tax_minor: string;
                posted_at: number | null;
                party_name: string;
            }) | null;
        }> {
            await requireFinanceMetricPurchase(db,actor,type.kind==="payable"?"payables":"receivables");
            const row = await db.prepare(`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.subtotal_minor::text subtotal_minor,
      i.tax_minor::text tax_minor,i.total_minor::text total_minor,i.outstanding_minor::text outstanding_minor,i.status,i.version,i.posted_at,p.name party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id
    WHERE i.id=? AND i.company_id=? AND i.kind=? AND i.currency=? AND i.invoice_date BETWEEN ? AND ?
      AND i.status IN (${type.statuses.map(() => "?").join(",")})`)
                .bind(recordId, actor.companyId, type.kind, filters.currency, filters.from, filters.to, ...type.statuses).first<InvoiceEvidenceRow & {
                subtotal_minor: string;
                tax_minor: string;
                posted_at: number | null;
                party_name: string;
            }>();
            return { row };
        }, async factsSearchCloseBlockerEvidence(cursor: {
            endDate: string;
            checkKey: string;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "close-blockers";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: CloseBlockerEvidenceRow[];
        }> {
            await requireFinanceMetricPurchase(db,actor,"close-blockers");
            const conditions = ["latest.status='block'", "latest.resolved_at IS NULL"];
            const values: unknown[] = [actor.companyId, actor.companyId, input.filters.from, input.filters.to];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',latest.id,latest.period_name,latest.end_date,latest.check_key,latest.status)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(latest.end_date<? OR (latest.end_date=? AND (latest.check_key>? OR (latest.check_key=? AND latest.id>?))))");
                values.push(cursor.endDate, cursor.endDate, cursor.checkKey, cursor.checkKey, cursor.id);
            }
            const rows = (await db.prepare(`${closeBlockerSource()} WHERE ${conditions.join(" AND ")}
    ORDER BY latest.end_date DESC,latest.check_key ASC,latest.id ASC LIMIT ?`)
                .bind(...values, input.limit + 1).all<CloseBlockerEvidenceRow>()).results;
            return { rows };
        }, async factsReadCloseBlockerEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, id: string): Promise<{
            row: CloseBlockerEvidenceRow | null;
        }> {
            await requireFinanceMetricPurchase(db,actor,"close-blockers");
            const row = await db.prepare(`${closeBlockerSource()}
    WHERE latest.id=? AND latest.status='block' AND latest.resolved_at IS NULL LIMIT 1`)
                .bind(actor.companyId, actor.companyId, filters.from, filters.to, id).first<CloseBlockerEvidenceRow>();
            return { row };
        } };
}
