import { hospitalityOwnerProjectionSources as financeProjectionSources } from "../../finance/public/hospitality-owner-projection-sources";
import "server-only";
import { type Filters } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { hotelOccupancyMetric, HotelRoomRevenueMetric, HotelOccupancyEvidenceRow, HotelGuestAccountBalanceEvidenceRow, HotelNightAuditBlockerEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
function hotelOccupancyRows(db: SqlDatabase, actor: WorkContext, filters: Filters, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric = hotelOccupancyMetric, query = "", cursor?: {
    businessDate: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "p.company_id=?",
        "(?::text IS NULL OR p.branch_id=?)",
        "r.kind='night'",
        "r.data_json::jsonb->>'state'='completed'",
        "r.data_json::jsonb->>'date' BETWEEN ? AND ?",
        hotelOccupancyValidSql(),
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to];
    if (metric !== hotelOccupancyMetric) {
        conditions.push("p.config_json::jsonb->>'currency'=?");
        values.push(filters.currency);
    }
    if (query) {
        conditions.push("position(lower(?) in lower(concat_ws(' ',r.id,p.name,r.data_json::jsonb->>'date'))) > 0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(r.data_json::jsonb->>'date',r.id)<(?,?)");
        values.push(cursor.businessDate, cursor.id);
    }
    if (recordId) {
        conditions.push("r.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT r.id,r.property_id,p.name property_name,p.config_json::jsonb->>'currency' property_currency,r.data_json::jsonb->>'date' business_date,
      r.data_json::jsonb->>'state' state,r.data_json::jsonb->'roomMetrics'->>'capacity' capacity,
      r.data_json::jsonb->'roomMetrics'->>'sold' sold,r.version
    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY r.data_json::jsonb->>'date' DESC,r.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}
function hotelOccupancyValidSql(alias = "r") {
    const capacity = `(${alias}.data_json::jsonb->'roomMetrics'->>'capacity')`;
    const sold = `(${alias}.data_json::jsonb->'roomMetrics'->>'sold')`;
    return `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
}
function hotelGuestAccountBalanceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: {
    propertyId: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "p.company_id=?",
        "(?::text IS NULL OR p.branch_id=?)",
        "p.config_json::jsonb->>'currency'=?",
        "f.kind='guest_account'",
        "f.data_json::jsonb->>'state' IN ('open','credit')",
        "e.kind='entry'",
        "j.company_id=p.company_id",
        "j.status='posted'",
        "l.account_id=p.config_json::jsonb->'accounts'->>'receivable'",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.currency];
    if (query) {
        conditions.push("position(lower(?) in lower(concat_ws(' ',f.id,p.name,p.config_json::jsonb->>'currency',f.data_json::jsonb->>'state'))) > 0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(p.id,f.id)>(?,?)");
        values.push(cursor.propertyId, cursor.id);
    }
    if (recordId) {
        conditions.push("f.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT f.id,p.id property_id,p.name property_name,
      p.config_json::jsonb->>'currency' currency,f.data_json::jsonb->>'state' state,
      sum(l.debit_minor::numeric-l.credit_minor::numeric)::text balance_minor,f.version
    FROM pms_records f
    JOIN pms_properties p ON p.id=f.property_id
    JOIN pms_records e ON e.property_id=p.id AND e.kind='entry' AND e.data_json::jsonb->>'guestAccountId'=f.id
    JOIN pms_postings pp ON pp.property_id=p.id AND pp.record_id=e.id
    JOIN (${financeProjectionSources.finance_journals}) j ON j.id=pp.journal_id
    JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.journal_id=j.id
    WHERE ${conditions.join(" AND ")}
    GROUP BY f.id,p.id,p.name,p.config_json,f.data_json,f.version
    HAVING sum(l.debit_minor::numeric-l.credit_minor::numeric)<>0
    ORDER BY p.id ASC,f.id ASC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}
function hotelNightAuditBlockerRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: {
    businessDate: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "p.company_id=?",
        "(?::text IS NULL OR p.branch_id=?)",
        "j.business_date BETWEEN ? AND ?",
        "j.state='blocked'",
        "r.kind='night'",
        "r.data_json::jsonb->>'date'=j.business_date",
        "r.data_json::jsonb->>'state'='blocked'",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to];
    if (query) {
        conditions.push("position(lower(?) in lower(concat_ws(' ',p.name,j.business_date,j.error_code,j.id))) > 0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(j.business_date,j.id)<(?,?)");
        values.push(cursor.businessDate, cursor.id);
    }
    if (recordId) {
        conditions.push("j.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT j.id,p.id property_id,p.name property_name,j.business_date,j.state,
      COALESCE(j.error_code,'PMS_JOB_FAILED') error_code,r.version
    FROM pms_jobs j JOIN pms_properties p ON p.id=j.property_id
    JOIN pms_records r ON r.id=j.id AND r.property_id=j.property_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY j.business_date DESC,j.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchHotelOccupancyEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "hotel-occupancy" | "hotel-adr" | "hotel-revpar";
            cursor?: string | undefined;
        }, cursor: {
            businessDate: string;
            id: string;
        } | null): Promise<{
            rows: HotelOccupancyEvidenceRow[];
        }> { const rows = (await hotelOccupancyRows(db, actor, input.filters, input.metric, input.query, cursor ?? undefined, undefined, input.limit).all<HotelOccupancyEvidenceRow>()).results; return { rows }; }, async factsHotelNightRoomRevenue(propertyId: string, actor: WorkContext, businessDate: string): Promise<{
            revenue_minor: string;
            invalid_revenue_count: string;
        } | null> {
            return db.prepare(`WITH RECURSIVE candidate_entries AS (
      SELECT e.id,e.property_id,e.data_json::jsonb->>'net' net_raw,
        e.data_json::jsonb->>'sourceKey' source_key,e.data_json::jsonb->>'originalId' original_id
      FROM pms_records e JOIN pms_properties p ON p.id=e.property_id
      WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND e.kind='entry' AND e.data_json::jsonb->>'date'=?
        AND e.data_json::jsonb->>'kind' IN ('charge','adjustment')
    ), lineage(entry_id,property_id,net_raw,current_id,source_key,original_id,visited) AS (
      SELECT id,property_id,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries
      UNION ALL
      SELECT l.entry_id,l.property_id,l.net_raw,parent.id,parent.data_json::jsonb->>'sourceKey',
        parent.data_json::jsonb->>'originalId',l.visited||parent.id
      FROM lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'
      WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))
    )
    SELECT COALESCE(sum(CASE WHEN e.net_raw ~ '^-?(0|[1-9][0-9]*)$' THEN e.net_raw::numeric ELSE 0::numeric END),0)::text revenue_minor,
      count(*) FILTER (WHERE e.net_raw IS NULL OR e.net_raw !~ '^-?(0|[1-9][0-9]*)$')::text invalid_revenue_count
    FROM candidate_entries e
    WHERE EXISTS(SELECT 1 FROM lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')`)
                .bind(propertyId, actor.companyId, actor.branchId, actor.branchId, businessDate)
                .first<{
                revenue_minor: string;
                invalid_revenue_count: string;
            }>();
        }, async factsReadHotelOccupancyEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, metric: "hotel-occupancy" | "hotel-adr" | "hotel-revpar", recordId: string): Promise<{
            row: HotelOccupancyEvidenceRow | null;
        }> { const row = await hotelOccupancyRows(db, actor, filters, metric, "", undefined, recordId, 1).first<HotelOccupancyEvidenceRow>(); return { row }; }, async factsSearchHotelGuestAccountBalanceEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "hotel-guest-account-balance";
            cursor?: string | undefined;
        }, cursor: {
            propertyId: string;
            id: string;
        } | null): Promise<{
            rows: HotelGuestAccountBalanceEvidenceRow[];
        }> { const rows = (await hotelGuestAccountBalanceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit).all<HotelGuestAccountBalanceEvidenceRow>()).results; return { rows }; }, async factsReadHotelGuestAccountBalanceEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, recordId: string): Promise<{
            row: HotelGuestAccountBalanceEvidenceRow | null;
        }> { const row = await hotelGuestAccountBalanceRows(db, actor, filters, "", undefined, recordId, 1).first<HotelGuestAccountBalanceEvidenceRow>(); return { row }; }, async factsSearchHotelNightAuditBlockerEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "hotel-night-audit-blockers";
            cursor?: string | undefined;
        }, cursor: {
            businessDate: string;
            id: string;
        } | null): Promise<{
            rows: HotelNightAuditBlockerEvidenceRow[];
        }> { const rows = (await hotelNightAuditBlockerRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit).all<HotelNightAuditBlockerEvidenceRow>()).results; return { rows }; }, async factsReadHotelNightAuditBlockerEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, recordId: string): Promise<{
            row: HotelNightAuditBlockerEvidenceRow | null;
        }> { const row = await hotelNightAuditBlockerRows(db, actor, filters, "", undefined, recordId, 1).first<HotelNightAuditBlockerEvidenceRow>(); return { row }; } };
}
