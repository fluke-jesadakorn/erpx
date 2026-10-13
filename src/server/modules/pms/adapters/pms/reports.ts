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
        pace: facts => db.prepare(`WITH observations AS (
          SELECT 0 position,COALESCE(?::timestamptz,transaction_timestamp()) as_of UNION ALL SELECT 1,COALESCE(?::timestamptz,COALESCE(?::timestamptz,transaction_timestamp())-interval '7 days')
        ), covered AS (
          SELECT o.*,c.started_at coverage_since,COALESCE(o.as_of>=c.started_at,false) covered
          FROM observations o LEFT JOIN pms_booking_history_coverage c ON c.property_id=?
        ), latest AS (
          SELECT DISTINCT ON (o.position,h.record_id) o.position,h.record_id,h.kind,h.facts
          FROM covered o JOIN pms_booking_observations h ON h.property_id=? AND h.observed_at<=o.as_of
          WHERE o.covered AND h.kind='stay' ORDER BY o.position,h.record_id,h.observed_at DESC,h.observation_id DESC
        ), active_stays AS (
          SELECT s.position,s.record_id,r.record_id reservation_id,
          GREATEST(0,LEAST((s.facts->>'departure')::date,?::date+1)-GREATEST((s.facts->>'arrival')::date,?::date)) nights,
          COALESCE((SELECT SUM((price->>'total')::bigint) FROM jsonb_array_elements(COALESCE(NULLIF(s.facts->'prices','null'::jsonb),'[]'::jsonb)) price WHERE price->>'date'>=? AND price->>'date'<=?),0) quoted
          FROM latest s JOIN covered observation ON observation.position=s.position
          JOIN LATERAL (
            SELECT h.record_id,h.facts FROM pms_booking_observations h
            WHERE h.property_id=? AND h.kind='reservation' AND h.record_id=s.facts->>'reservationId'
              AND h.observed_at<=observation.as_of
            ORDER BY h.observed_at DESC,h.observation_id DESC LIMIT 1
          ) r ON true
          WHERE s.kind='stay' AND s.facts->>'state' IN ('reserved','in_house','checked_out') AND r.facts->>'state' IN ('confirmed','completed')
          AND s.facts->>'arrival'<=? AND s.facts->>'departure'>?
        ) SELECT o.as_of::text,o.coverage_since::text,o.covered,count(DISTINCT s.reservation_id)::integer reservations,
          COALESCE(sum(s.nights),0)::bigint room_nights,COALESCE(sum(s.quoted),0)::bigint quoted_total
          FROM covered o LEFT JOIN active_stays s ON s.position=o.position
          GROUP BY o.position,o.as_of,o.coverage_since,o.covered ORDER BY o.position`)
          .bind(facts.asOf,facts.compareAsOf,facts.asOf,facts.propertyId,facts.propertyId,facts.to,facts.from,facts.from,facts.to,facts.propertyId,facts.to,facts.from)
          .all<import('../../application/reports').PmsPacePoint>(),
        channels: facts => db.prepare(`WITH reservation_facts AS (
          SELECT r.id,COALESCE(NULLIF(r.data_json::jsonb->>'source',''),'unknown') source,r.data_json::jsonb->>'state' state,
            CASE WHEN jsonb_typeof(r.data_json::jsonb->'history'->0->'at')='number' THEN to_timestamp((r.data_json::jsonb->'history'->0->>'at')::numeric/1000.0) END created
          FROM pms_records r WHERE r.property_id=? AND r.kind='reservation'
        ), stay_facts AS (
          SELECT s.data_json::jsonb->>'reservationId' reservation_id,
            GREATEST(0,LEAST((s.data_json::jsonb->>'departure')::date,?::date+1)-GREATEST((s.data_json::jsonb->>'arrival')::date,?::date)) nights,
            COALESCE((SELECT SUM((price->>'total')::bigint) FROM jsonb_array_elements(COALESCE(s.data_json::jsonb->'prices','[]'::jsonb)) price WHERE price->>'date'>=? AND price->>'date'<=?),0) quoted,
            s.data_json::jsonb->>'state' state
          FROM pms_records s WHERE s.property_id=? AND s.kind='stay' AND s.data_json::jsonb->>'arrival'<=? AND s.data_json::jsonb->>'departure'>?
        ) SELECT r.source,count(DISTINCT r.id)::integer reservations,
          COALESCE(sum(s.nights) FILTER(WHERE s.state IN ('reserved','in_house','checked_out') AND r.state<>'payment_pending'),0)::bigint room_nights,
          COALESCE(sum(s.quoted) FILTER(WHERE s.state IN ('reserved','in_house','checked_out') AND r.state<>'payment_pending'),0)::bigint quoted_total,
          count(*) FILTER(WHERE s.state='cancelled')::integer cancelled,
          count(*) FILTER(WHERE s.state='no_show')::integer no_shows,
          count(DISTINCT r.id) FILTER(WHERE r.created>=?::timestamptz-interval '7 days' AND r.created<=?::timestamptz)::integer recent_pickup,
          count(DISTINCT r.id) FILTER(WHERE r.created IS NULL)::integer unknown_creation
          FROM reservation_facts r JOIN stay_facts s ON s.reservation_id=r.id GROUP BY r.source ORDER BY r.source`)
          .bind(facts.propertyId,facts.to,facts.from,facts.from,facts.to,facts.propertyId,facts.to,facts.from,facts.asOf,facts.asOf).all<import('../../application/reports').PmsChannelReport>(),
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
const application = createPmsReportsApplication({ clock:{isoNow:()=>new Date().toISOString()}, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export function pmsRangeReport(db: SqlDatabase, actor: WorkContext, raw: unknown) { return application.pmsRangeReport(repository(db), actor, raw); }
export function pmsReportEntries(db: SqlDatabase, actor: WorkContext, propertyId: string, date: string, page: number) { return application.pmsReportEntries(repository(db), actor, propertyId, date, page); }
