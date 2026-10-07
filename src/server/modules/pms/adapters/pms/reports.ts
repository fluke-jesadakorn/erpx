import "server-only";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import type { WorkContext } from "../../../identity_access/public/authorization";
import { requirePermission } from "../../../identity_access/public/iam-permissions";
import { ApiError } from "../../../runtime/public/http";
import { pmsReportingBalances } from "../../../finance/public/pms-reporting-facts";
import { createPmsReportsApplication, type PmsReportsRepository } from "../../application/reports";
export { pmsReportRange, reportDates } from "../../domain/reports";
function repository(db: SqlDatabase): PmsReportsRepository {
    const finance = pmsReportingBalances(db);
    return {
        property: (facts: {
            propertyId: string;
            companyId: string;
        }) => db.prepare('SELECT branch_id,config_json FROM pms_properties WHERE id=? AND company_id=?').bind(facts.propertyId, facts.companyId).first<{
            branch_id: string;
            config_json: string;
        }>(),
        days: (facts: {
            from: string;
            to: string;
            propertyId: string;
            from2: string;
            to2: string;
            propertyId2: string;
            propertyId3: string;
            propertyId4: string;
            propertyId5: string;
            propertyId6: string;
            propertyId7: string;
            timezone: string;
            propertyId8: string;
            propertyId9: string;
        }) => db.prepare(`WITH RECURSIVE days AS (SELECT generate_series(?::date,?::date,interval '1 day')::date d),
 lineage AS (
  SELECT id root,id,data_json::jsonb data,ARRAY[id] seen FROM pms_records WHERE property_id=? AND kind='entry'
   AND data_json::jsonb->>'date'>=? AND data_json::jsonb->>'date'<=?
  UNION ALL SELECT l.root,e.id,e.data_json::jsonb,l.seen||e.id FROM lineage l JOIN pms_records e
   ON e.id=l.data->>'originalId' AND e.property_id=? AND e.kind='entry' WHERE NOT e.id=ANY(l.seen)
 ), room_entries AS (SELECT DISTINCT root FROM lineage WHERE data->>'sourceKey' LIKE 'night:%')
 SELECT d::text date,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'capacity')::bigint FROM pms_records WHERE property_id=? AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records r WHERE r.property_id=? AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'roomId'=r.id AND b.data_json::jsonb->>'from'<=d::text))) capacity,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'sold')::bigint FROM pms_records WHERE property_id=? AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state' IN ('in_house','checked_out') AND data_json::jsonb->>'arrival'<=d::text AND data_json::jsonb->>'departure'>d::text)) sold,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint) FILTER(WHERE e.id IN (SELECT root FROM room_entries)),0) room_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint+(e.data_json::jsonb->>'service')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) net_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'tax')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) tax,
 COALESCE(-SUM((e.data_json::jsonb->>'total')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind'='refund'),0) refunds,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state'='cancelled' AND to_char(to_timestamp(updated_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')=d::text) cancellations,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state'='no_show' AND data_json::jsonb->>'arrival'=d::text) no_shows
 FROM days LEFT JOIN pms_records e ON e.property_id=? AND e.kind='entry' AND e.data_json::jsonb->>'date'=d::text GROUP BY d ORDER BY d`).bind(facts.from, facts.to, facts.propertyId, facts.from2, facts.to2, facts.propertyId2, facts.propertyId3, facts.propertyId4, facts.propertyId5, facts.propertyId6, facts.propertyId7, facts.timezone, facts.propertyId8, facts.propertyId9).all<{
            date: string;
            capacity: number;
            sold: number;
            room_revenue: number;
            net_revenue: number;
            tax: number;
            refunds: number;
            cancellations: number;
            no_shows: number;
        }>(),
        balances: facts => finance.balances(facts),
        entries: (facts: {
            propertyId: string;
            date: string;
            offset: number;
        }) => db.prepare("SELECT id,data_json FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'date'=? ORDER BY updated_at,id LIMIT 51 OFFSET ?").bind(facts.propertyId, facts.date, facts.offset).all<{
            id: string;
            data_json: string;
        }>()
    };
}
const application = createPmsReportsApplication({ require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export function pmsRangeReport(db: SqlDatabase, actor: WorkContext, raw: unknown) { return application.pmsRangeReport(repository(db), actor, raw); }
export function pmsReportEntries(db: SqlDatabase, actor: WorkContext, propertyId: string, date: string, page: number) { return application.pmsReportEntries(repository(db), actor, propertyId, date, page); }
