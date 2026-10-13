import { hospitalityOwnerProjectionSources as financeProjectionSources } from "../../finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/hospitality-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "hotel-rooms") {
        sql = "SELECT p.name label,count(*)::text value FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='room' GROUP BY p.id,p.name ORDER BY 1";
        values = [a.companyId, a.branchId, a.branchId];
    }
    if (id === "hotel-occupancy") {
        const label = breakdown === "month" ? "substring(r.data_json::jsonb->>'date',1,7)"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=p.branch_id AND n.company_id=p.company_id),'Unassigned branch')`
                : breakdown === "property" ? "p.name" : "'Hotel occupancy'";
        const capacity = "(r.data_json::jsonb->'roomMetrics'->>'capacity')";
        const sold = "(r.data_json::jsonb->'roomMetrics'->>'sold')";
        const valid = `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
        sql = `SELECT ${label} label,
        COALESCE(sum(CASE WHEN ${valid} THEN ${sold}::numeric ELSE 0::numeric END),0)::text numerator,
        COALESCE(sum(CASE WHEN ${valid} THEN ${capacity}::numeric ELSE 0::numeric END),0)::text denominator,
        count(*) FILTER (WHERE NOT ${valid})::text partial_count
      FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='night'
        AND r.data_json::jsonb->>'state'='completed' AND r.data_json::jsonb->>'date' BETWEEN ? AND ?
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
    }
    if (id === "hotel-adr" || id === "hotel-revpar") {
        const label = breakdown === "month" ? "substring(n.business_date,1,7)"
            : breakdown === "branch" ? `COALESCE((SELECT n2.display_name FROM (${organizationProjectionSources.organization_nodes}) n2 WHERE n2.id=p.branch_id AND n2.company_id=p.company_id),'Unassigned branch')`
                : breakdown === "property" ? "p.name" : id === "hotel-adr" ? "'Hotel ADR'" : "'Hotel RevPAR'";
        const capacity = "(r.data_json::jsonb->'roomMetrics'->>'capacity')";
        const sold = "(r.data_json::jsonb->'roomMetrics'->>'sold')";
        const validMetrics = `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
        const denominator = id === "hotel-adr" ? "n.sold" : "n.capacity";
        sql = `WITH RECURSIVE scoped_properties AS (
        SELECT id,company_id,branch_id,name,config_json FROM pms_properties
        WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND config_json::jsonb->>'currency'=?
      ), candidate_entries AS (
        SELECT e.id,e.property_id,e.data_json::jsonb->>'date' business_date,
          e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'net' net_raw,
          e.data_json::jsonb->>'sourceKey' source_key,e.data_json::jsonb->>'originalId' original_id
        FROM pms_records e JOIN scoped_properties p ON p.id=e.property_id
        WHERE e.kind='entry' AND e.data_json::jsonb->>'date' BETWEEN ? AND ?
          AND e.data_json::jsonb->>'kind' IN ('charge','adjustment')
      ), entry_lineage(entry_id,property_id,business_date,net_raw,current_id,source_key,original_id,visited) AS (
        SELECT id,property_id,business_date,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries
        UNION ALL
        SELECT l.entry_id,l.property_id,l.business_date,l.net_raw,parent.id,
          parent.data_json::jsonb->>'sourceKey',parent.data_json::jsonb->>'originalId',l.visited||parent.id
        FROM entry_lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'
        WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))
      ), room_revenue AS (
        SELECT e.property_id,e.business_date,
          COALESCE(sum(CASE WHEN e.net_raw ~ '^-?(0|[1-9][0-9]*)$' THEN e.net_raw::numeric ELSE 0::numeric END),0) revenue_minor,
          count(*) FILTER (WHERE e.net_raw IS NULL OR e.net_raw !~ '^-?(0|[1-9][0-9]*)$') invalid_revenue_count
        FROM candidate_entries e
        WHERE EXISTS(SELECT 1 FROM entry_lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')
        GROUP BY e.property_id,e.business_date
      ), audit_nights AS (
        SELECT r.id,r.property_id,p.branch_id,p.name,r.data_json::jsonb->>'date' business_date,
          CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' THEN ${capacity}::numeric ELSE 0::numeric END capacity,
          CASE WHEN ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN ${sold}::numeric ELSE 0::numeric END sold,
          ${validMetrics} valid_metrics
        FROM pms_records r JOIN scoped_properties p ON p.id=r.property_id
        WHERE r.kind='night' AND r.data_json::jsonb->>'state'='completed'
          AND r.data_json::jsonb->>'date' BETWEEN ? AND ?
      )
      SELECT ${label} label,
        COALESCE(sum(CASE WHEN n.valid_metrics THEN COALESCE(rr.revenue_minor,0::numeric) ELSE 0::numeric END),0)::text numerator,
        COALESCE(sum(CASE WHEN n.valid_metrics THEN ${denominator} ELSE 0::numeric END),0)::text denominator,
        (count(*) FILTER (WHERE NOT n.valid_metrics)+COALESCE(sum(CASE WHEN n.valid_metrics THEN COALESCE(rr.invalid_revenue_count,0) ELSE 0 END),0))::text partial_count
      FROM audit_nights n JOIN scoped_properties p ON p.id=n.property_id
      LEFT JOIN room_revenue rr ON rr.property_id=n.property_id AND rr.business_date=n.business_date
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, f.from, f.to];
    }
    if (id === "hotel-guest-account-balance") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=p.branch_id AND n.company_id=p.company_id),'Unassigned branch')`
            : breakdown === "property" ? "p.name" : "'Open guest account balances'";
        sql = `SELECT ${label} label,
        COALESCE(sum(l.debit_minor::numeric-l.credit_minor::numeric),0)::text value
      FROM pms_properties p
      JOIN pms_records f ON f.property_id=p.id AND f.kind='guest_account'
        AND f.data_json::jsonb->>'state' IN ('open','credit')
      JOIN pms_records e ON e.property_id=p.id AND e.kind='entry'
        AND e.data_json::jsonb->>'guestAccountId'=f.id
      JOIN pms_postings pp ON pp.property_id=p.id AND pp.record_id=e.id
      JOIN (${financeProjectionSources.finance_journals}) j ON j.id=pp.journal_id AND j.status='posted'
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.journal_id=j.id
        AND l.account_id=p.config_json::jsonb->'accounts'->>'receivable'
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND p.config_json::jsonb->>'currency'=?
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.currency];
    }
    if (id === "hotel-night-audit-blockers") {
        const label = breakdown === "month" ? "substring(j.business_date,1,7)"
            : breakdown === "status" ? "COALESCE(j.error_code,'PMS_JOB_FAILED')"
                : breakdown === "property" ? "p.name" : "'Blocked Night Audit jobs'";
        sql = `SELECT ${label} label,count(*)::text value
      FROM pms_jobs j JOIN pms_properties p ON p.id=j.property_id
      JOIN pms_records r ON r.id=j.id AND r.property_id=j.property_id AND r.kind='night'
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND j.business_date BETWEEN ? AND ? AND j.state='blocked'
        AND r.data_json::jsonb->>'date'=j.business_date AND r.data_json::jsonb->>'state'='blocked'
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
