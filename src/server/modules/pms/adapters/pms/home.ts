import "server-only";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import { holdingResources } from "../../../runtime/public/runtime";
import type { WorkContext } from "../../../identity_access/public/authorization";
import { requirePermission } from "../../../identity_access/public/iam-permissions";
import { ApiError } from "../../../runtime/public/http";
import { readListPage } from "../../../work_projection/public/list-page";
import { homePosts, postView } from "../../../collaboration/public/social-home";
import { readablePmsHomePosts } from "../../../collaboration/public/pms-home-read-facts";
import { pmsSourceRecipients } from "./source";
import { pmsSourcesSql, pmsSourceReadableSql } from "./source-policy";
import type { TaskRow } from "../../domain/home";
import { createPmsHomeApplication, type PmsHomeRepository, type PmsTaskPageOptions } from "../../application/home";
export type { PmsHomeView, PmsHomeTask, PmsHotelOverview } from "../../domain/home";
function taskQuery(actor: WorkContext, propertyId?: string, mine = false, selectedLabel?: string) {
    const can = (permission: string) => hasPermission(actor, permission) ? "TRUE" : "FALSE";
    const finance = can("finance.approve"), cashier = can("pms.cashier"), approve = can("pms.approve"), audit = can("pms.audit");
    const own = `iam_same_person(pr.data->>'userId',recipient.user_id,pr.company_id)`;
    const requestOwn = `iam_same_person(pr.data->>'requestedBy',recipient.user_id,pr.company_id)`;
    const recordedOwn = `iam_same_person(pr.data->>'recordedBy',recipient.user_id,pr.company_id)`;
    const actionable = `(CASE WHEN pr.kind='transfer' THEN ${finance} AND NOT ${recordedOwn}
    WHEN pr.kind='refund' AND pr.data->>'state'='approved' THEN ${cashier}
    WHEN pr.kind IN ('refund','correction') THEN ${approve} AND NOT ${requestOwn}
    WHEN pr.kind='shift' AND pr.data->>'state'='closed' THEN ${approve}
    WHEN pr.kind='task' AND pr.data->>'state'='clean' THEN ${can("pms.inspect")} AND NOT iam_same_person(pr.data->>'completedBy',recipient.user_id,pr.company_id)
    ELSE TRUE END)`;
    const label = `CASE WHEN pr.kind='stay' THEN CASE WHEN pr.data->>'state'='reserved' THEN 'arrivals' ELSE 'departures' END
    WHEN pr.kind='task' THEN CASE WHEN pr.data->>'state'='clean' THEN 'inspection' ELSE 'housekeeping' END
    WHEN pr.kind='room' THEN CASE WHEN pr.data->>'cleanliness'='clean' THEN 'inspection' ELSE 'housekeeping' END
    WHEN pr.kind='transfer' THEN 'transfers' WHEN pr.kind='correction' THEN 'corrections' WHEN pr.kind='refund' THEN 'refunds'
    WHEN pr.kind='shift' THEN CASE WHEN pr.data->>'state'='closed' THEN 'variance' ELSE 'shift' END ELSE 'night' END`;
    const urgent = `(pr.kind='night' OR (pr.kind='shift' AND pr.data->>'state'='closed') OR pr.data->>'priority'='urgent'
    OR (pr.kind='stay' AND pr.data->>'state'='in_house' AND pr.data->>'departure'<pr.business_date))`;
    return { sql: `WITH operational AS MATERIALIZED (
    SELECT pr.*
    FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
    WHERE pr.company_id=? AND (?::text IS NULL OR pr.branch_id=?) AND (?::text IS NULL OR pr.property_id=?)
      AND (
      (pr.kind='stay' AND ${can("pms.frontdesk")} AND ((pr.data->>'state'='reserved' AND pr.data->>'arrival'<=pr.business_date)
        OR (pr.data->>'state'='in_house' AND pr.data->>'departure'<=pr.business_date)))
      OR (pr.kind='task' AND pr.data->>'state' NOT IN ('done','superseded')
        AND (${can("pms.housekeeping")} OR ${can("pms.inspect")} OR ${can("pms.maintenance")}))
      OR (pr.kind='room' AND pr.data->>'active'='true' AND pr.data->>'cleanliness' IN ('dirty','clean')
        AND NOT EXISTS(SELECT 1 FROM pms_records t WHERE t.property_id=pr.property_id AND t.kind='task'
          AND t.data_json::jsonb->>'roomId'=pr.id AND t.data_json::jsonb->>'state' NOT IN ('done','superseded'))
        AND ((${can("pms.housekeeping")} AND pr.data->>'cleanliness'='dirty') OR (${can("pms.inspect")} AND pr.data->>'cleanliness'='clean')))
      OR (pr.kind='transfer' AND pr.data->>'state'='pending' AND ((${finance} AND NOT ${recordedOwn}) OR (${cashier} AND ${recordedOwn})))
      OR (pr.kind IN ('refund','correction') AND pr.data->>'state' IN ('requested','pending')
        AND ((${approve} AND NOT ${requestOwn}) OR (${cashier} AND ${requestOwn})))
      OR (pr.kind='refund' AND pr.data->>'state'='approved' AND ${cashier})
      OR (pr.kind='shift' AND ((pr.data->>'state'='open' AND ${cashier} AND ${own})
        OR (pr.data->>'state'='closed' AND (pr.data->>'variance')::bigint<>0 AND (${approve} OR ${audit}))))
      OR (pr.kind='night' AND pr.data->>'state'='blocked' AND ${audit})
    )) SELECT pr.*,${label} label,COALESCE(${urgent},FALSE) urgent,${actionable} actionable,
      rr.data_json::jsonb->>'number' room_number,
      CASE WHEN COALESCE(${urgent},FALSE) THEN '0' ELSE '1' END || lpad(pr.updated_at::text,16,'0') page_key
    FROM operational pr CROSS JOIN (SELECT ?::text user_id) recipient
    LEFT JOIN pms_records rr ON rr.property_id=pr.property_id AND rr.id=pr.data->>'roomId' AND rr.kind='room'
    WHERE ${pmsSourceReadableSql("pr", "recipient.user_id", "?::text")}
    ${mine ? `AND (${actionable}) AND (pr.kind<>'task' OR pr.data->>'state'='clean' OR pr.data->>'assigneeId'=recipient.user_id OR COALESCE(pr.data->>'assigneeId','')='')` : ""}
    AND (?::text IS NULL OR (${label})=?)`,
        args: [actor.userId, actor.companyId, actor.branchId, actor.branchId, propertyId ?? null, propertyId ?? null,actor.userId, actor.assignmentId, selectedLabel ?? null, selectedLabel ?? null] };
}
function repository(db: SqlDatabase): PmsHomeRepository {
    return {
        hotelMetrics: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true') room_count,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'
        AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=p.business_date AND b.data_json::jsonb->>'roomId'=r.id)) capacity,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state' IN ('in_house','checked_out')
      AND s.data_json::jsonb->>'arrival'<=p.business_date AND s.data_json::jsonb->>'departure'>p.business_date) sold,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures
    FROM pms_properties p WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY p.name,p.id`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            name: string;
            branch_id: string;
            business_date: string;
            state: string;
            currency: string;
            room_count: number;
            inspected: number;
            capacity: number;
            sold: number;
            arrivals: number;
            departures: number;
        }>(),
        hotelMoney: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            companyId2: string;
            branchId3: string | null;
            branchId4: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`WITH RECURSIVE entry_roots AS (
      SELECT r.id,r.property_id,r.id root_id FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'
      UNION ALL SELECT r.id,r.property_id,e.root_id FROM pms_records r JOIN entry_roots e ON r.property_id=e.property_id AND r.data_json::jsonb->>'originalId'=e.id
        WHERE r.kind='entry' AND r.id<>e.id
    ) SELECT p.id,
      COALESCE((SELECT sum((r.data_json::jsonb->>'net')::bigint) FROM pms_records r JOIN entry_roots e ON e.id=r.id AND e.property_id=r.property_id
        WHERE r.property_id=p.id AND r.data_json::jsonb->>'date'=p.business_date AND r.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) room_revenue,
      COALESCE((SELECT sum((r.data_json::jsonb->>'due')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'),0) receivable,
      COALESCE((SELECT sum((r.data_json::jsonb->>'advance')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'),0) advance,
      COALESCE((SELECT sum((r.data_json::jsonb->>'deposit')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'),0) deposit
    FROM pms_properties p WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.companyId2, facts.branchId3, facts.branchId4, facts.propertyId, facts.propertyId2).all<{
            id: string;
            room_revenue: number;
            receivable: number;
            advance: number;
            deposit: number;
        }>(),
        hotelSources: (facts: {
            id: string;
            id2: string;
            id3: string;
            metric: "occupancy" | "adr" | "revpar" | "room_revenue";
            businessDate: string;
            metric2: "occupancy" | "adr" | "revpar" | "room_revenue";
            businessDate2: string;
            businessDate3: string;
            metric3: "occupancy" | "adr" | "revpar" | "room_revenue";
            businessDate4: string;
            offset: number;
        }) => db.prepare(`WITH RECURSIVE roots AS (
 SELECT r.id FROM pms_records r WHERE r.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'
 UNION ALL SELECT r.id FROM pms_records r JOIN roots e ON r.data_json::jsonb->>'originalId'=e.id WHERE r.property_id=? AND r.kind='entry' AND r.id<>e.id
 ), sources AS (
 SELECT r.* FROM pms_records r WHERE r.property_id=? AND (
 (? IN ('occupancy','revpar') AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=? AND b.data_json::jsonb->>'roomId'=r.id))
 OR (? IN ('occupancy','adr') AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('in_house','checked_out') AND r.data_json::jsonb->>'arrival'<=? AND r.data_json::jsonb->>'departure'>?)
 OR (?<>'occupancy' AND r.kind='entry' AND r.id IN (SELECT id FROM roots) AND r.data_json::jsonb->>'date'=? AND r.data_json::jsonb->>'kind' IN ('charge','adjustment'))
 )) SELECT id,kind,version,data_json FROM sources ORDER BY kind,id LIMIT 31 OFFSET ?`).bind(facts.id, facts.id2, facts.id3, facts.metric, facts.businessDate, facts.metric2, facts.businessDate2, facts.businessDate3, facts.metric3, facts.businessDate4, facts.offset).all<{
            id: string;
            kind: string;
            version: number;
            data_json: string;
        }>(),
        taskPage: (actor, options) => { const query = taskQuery(actor, options.propertyId, options.mine, options.label); return readListPage<TaskRow>(db, { ...query, scope: [actor.companyId, actor.branchId, actor.userId, actor.assignmentId, options.propertyId, options.mine, options.label], cursor: options.cursor, limit: options.limit ?? 12 }); }, tasks: async (actor) => { const query = taskQuery(actor); return (await db.prepare(query.sql).bind(...query.args).all<TaskRow>()).results; }, readablePosts: (actor, loadedIds, now) => readablePmsHomePosts(db, actor, loadedIds, now), counts: (actor, propertyId, mine) => {
            const taskSql = taskQuery(actor, propertyId, mine);
            const write = db;
            return write.prepare(`SELECT label,count(*) count,count(*) FILTER (WHERE urgent) urgent FROM (${taskSql.sql}) tasks GROUP BY label ORDER BY urgent DESC,count DESC,label`)
                .bind(...taskSql.args).all<{
                label: string;
                count: number;
                urgent: number;
            }>();
        }, post: (actor, id) => postView(db, actor, id), posts: (actor, options) => homePosts(db, actor, options), recipients: actor => pmsSourceRecipients(db, actor),
    };
}
const application = createPmsHomeApplication({ resource: async (holdingId) => ({ write: repository((await holdingResources(holdingId)).write) }), clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export function pmsTaskPage(db: SqlDatabase, actor: WorkContext, options: PmsTaskPageOptions = {}) { return application.pmsTaskPage(repository(db), actor, options); }
export function pmsOperationalWork(db: SqlDatabase, actor: WorkContext) { return application.pmsOperationalWork(repository(db), actor); }
export function pmsHotelOverview(db: SqlDatabase, actor: WorkContext, propertyId?: string) { return application.pmsHotelOverview(repository(db), actor, propertyId); }
export function pmsHome(actor: WorkContext, options: Parameters<typeof application.pmsHome>[1] = {}) { return application.pmsHome(actor, options); }
export type PmsHome = Awaited<ReturnType<typeof pmsHome>>;
export function pmsHotelSources(actor: WorkContext, id: string, metric: "occupancy" | "adr" | "revpar" | "room_revenue", businessDate: string, page = 1) { return application.pmsHotelSources(actor, id, metric, businessDate, page); }
