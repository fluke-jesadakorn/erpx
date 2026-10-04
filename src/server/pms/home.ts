import "server-only";
import { hasPermission } from "@/lib/permissions";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { holdingResources } from "../runtime";
import { requirePermission } from "../iam-permissions";
import { ApiError } from "../http";
import { readListPage } from "../list-page";
import { homePosts, postView } from "../social-home";
import { sourceAccessSql } from "../social-source";
import { decodePmsSource, pmsSourceLink, pmsSourceRecipients, type PmsSource } from "./source";
import { pmsSourcesSql, pmsSourceReadableSql } from "./source-policy";

export type PmsHomeView = "mine" | "team" | "handover" | "company";
export type PmsHomeTask = WorkItem & { sourceKind:"pms_record"; sourceVersion:number;
  propertyId:string; propertyName:string; businessDate:string; section:string; urgent:boolean; label:string };
type TaskRow = PmsSource & { page_key:string; label:string; urgent:boolean; actionable:boolean; room_number:string|null };

function taskQuery(actor: WorkContext, propertyId?: string, mine=false, selectedLabel?:string) {
  const can=(permission:string)=>hasPermission(actor,permission)?"TRUE":"FALSE";
  const finance=can("finance.approve"), cashier=can("pms.cashier"), approve=can("pms.approve"), audit=can("pms.audit");
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
  return {sql:`SELECT pr.*,${label} label,COALESCE(${urgent},FALSE) urgent,${actionable} actionable,
      rr.data_json::jsonb->>'number' room_number,
      CASE WHEN COALESCE(${urgent},FALSE) THEN '0' ELSE '1' END || lpad(pr.updated_at::text,16,'0') page_key
    FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
    LEFT JOIN pms_records rr ON rr.property_id=pr.property_id AND rr.id=pr.data->>'roomId' AND rr.kind='room'
    WHERE pr.company_id=? AND (?::text IS NULL OR pr.branch_id=?) AND (?::text IS NULL OR pr.property_id=?)
      AND ${pmsSourceReadableSql("pr","recipient.user_id","?::text")} AND (
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
    ) ${mine ? `AND (${actionable}) AND (pr.kind<>'task' OR pr.data->>'state'='clean' OR pr.data->>'assigneeId'=recipient.user_id OR COALESCE(pr.data->>'assigneeId','')='')` : ""}
    AND (?::text IS NULL OR (${label})=?)`,
    args:[actor.userId,actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null,actor.assignmentId,selectedLabel??null,selectedLabel??null]};
}

const labels: Record<string,string> = {arrivals:"Check in",departures:"Check out",housekeeping:"Prepare room",inspection:"Inspect room",
  transfers:"Verify bank transfer",corrections:"Review charge correction",refunds:"Review refund",variance:"Review cash variance",shift:"Close cashier shift",night:"Resolve day-close blockers"};
function taskView(row: TaskRow, actor: WorkContext): PmsHomeTask {
  row=decodePmsSource(row);
  const link=pmsSourceLink(row);
  const query=new URLSearchParams(link.href.split("?")[1]);
  const payout=row.kind==="refund"&&row.data.state==="approved";
  const action=payout?"Pay approved refund":labels[row.label];
  const approval=["transfer","correction","refund"].includes(row.kind)&&row.actionable&&!payout;
  if (!approval) query.delete("queue");
  if (row.kind==="stay") query.set("queue",row.label);
  const href=`/pms/${link.section}?${query}`;
  const room=row.room_number??(row.kind==="room"?row.data.number:undefined);
  const assigned="assigneeId" in row.data&&row.data.assigneeId===actor.userId;
  return {id:row.id,type:"pms",sourceKind:"pms_record",sourceVersion:Number(row.version),propertyId:row.property_id,
    source: {kind:row.kind,id:row.id,version:Number(row.version),href,label:link.label},actionKind:approval?"approval":"work",
    propertyName:row.property_name,businessDate:row.business_date,section:link.section,label:row.label,urgent:row.urgent,
    title:`${row.property_name}${room?" · "+room:""} · ${action}`,
    detail:`Business date ${row.business_date}`,href,status:row.data.state??("cleanliness" in row.data?String(row.data.cleanliness):"pending"),
    updatedAt:Number(row.updated_at),queue:row.actionable?"todo":"waiting",canAct:row.actionable,
    responsibleQueue:row.kind==="transfer"?"Finance approver":payout?"Cashier":undefined,
    nextAction:row.actionable?(row.kind==="transfer"?"Match imported bank evidence to this hotel transfer":action):"Waiting for independent review",blocker:row.urgent?labels[row.label]:null,
    relationships:[approval?"approval":assigned||payout?"assigned":"team"]};
}

export async function pmsTaskPage(db:SqlDatabase,actor:WorkContext,options:{propertyId?:string;mine?:boolean;label?:string;cursor?:string;limit?:number}={}) {
  const query=taskQuery(actor,options.propertyId,options.mine,options.label);
  const page=await readListPage<TaskRow>(db,{...query,scope:[actor.companyId,actor.branchId,actor.userId,actor.assignmentId,options.propertyId,options.mine,options.label],cursor:options.cursor,limit:options.limit??12});
  return {...page,items:page.items.map(row=>taskView(row,actor))};
}

/** Work reuses source IDs and policy, without loading historical folios/entries. */
export async function pmsOperationalWork(db:SqlDatabase,actor:WorkContext):Promise<WorkItem[]> {
  const query=taskQuery(actor);
  const rows=(await db.prepare(query.sql).bind(...query.args).all<TaskRow>()).results;
  return rows.map(row=>taskView(row,actor));
}

export type PmsHotelOverview={id:string;name:string;branchId:string;businessDate:string;state:string;capacity:number;sold:number;
  inspected:number;roomCount:number;arrivals:number;departures:number;currency:string;occupancy:number|null;
  roomRevenue:number|null;adr:number|null;revpar:number|null;receivable:number|null;advance:number|null;deposit:number|null};

export async function pmsHotelOverview(db:SqlDatabase,actor:WorkContext,propertyId?:string):Promise<PmsHotelOverview[]> {
  const financial=hasPermission(actor,"pms.reports");
  const rows=(await db.prepare(`SELECT p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,
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
    ORDER BY p.name,p.id`).bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null).all<{
      id:string;name:string;branch_id:string;business_date:string;state:string;currency:string;room_count:number;inspected:number;capacity:number;sold:number;arrivals:number;departures:number}>()).results;
  const money=financial?(await db.prepare(`WITH RECURSIVE entry_roots AS (
      SELECT r.id,r.property_id,r.id root_id FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'
      UNION ALL SELECT r.id,r.property_id,e.root_id FROM pms_records r JOIN entry_roots e ON r.property_id=e.property_id AND r.data_json::jsonb->>'originalId'=e.id
        WHERE r.kind='entry' AND r.id<>e.id
    ) SELECT p.id,
      COALESCE((SELECT sum((r.data_json::jsonb->>'net')::bigint) FROM pms_records r JOIN entry_roots e ON e.id=r.id AND e.property_id=r.property_id
        WHERE r.property_id=p.id AND r.data_json::jsonb->>'date'=p.business_date AND r.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) room_revenue,
      COALESCE((SELECT sum((r.data_json::jsonb->>'due')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='folio'),0) receivable,
      COALESCE((SELECT sum((r.data_json::jsonb->>'advance')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='folio'),0) advance,
      COALESCE((SELECT sum((r.data_json::jsonb->>'deposit')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='folio'),0) deposit
    FROM pms_properties p WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)`)
    .bind(actor.companyId,actor.branchId,actor.branchId,actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null)
    .all<{id:string;room_revenue:number;receivable:number;advance:number;deposit:number}>()).results:[];
  return rows.map(row=>{
    const financialRow=money.find(item=>item.id===row.id);
    const capacity=Number(row.capacity),sold=Number(row.sold),revenue=financialRow?Number(financialRow.room_revenue):null;
    return {id:row.id,name:row.name,branchId:row.branch_id,businessDate:row.business_date,state:row.state,currency:row.currency,
      capacity,sold,roomCount:Number(row.room_count),inspected:Number(row.inspected),arrivals:Number(row.arrivals),departures:Number(row.departures),
      occupancy:capacity?sold/capacity:null,roomRevenue:revenue,adr:revenue!==null&&sold?revenue/sold:null,revpar:revenue!==null&&capacity?revenue/capacity:null,
      receivable:financialRow?Number(financialRow.receivable):null,advance:financialRow?Number(financialRow.advance):null,deposit:financialRow?Number(financialRow.deposit):null};
  });
}

export async function pmsHome(actor:WorkContext,options:{propertyId?:string;view?:PmsHomeView;label?:string;taskCursor?:string;before?:number;beforeId?:string;postId?:string;loadedIds?:string[]}={}) {
  requirePermission(actor,"pms.read");
  if(actor.departmentId||actor.beta)throw new ApiError(403,"PMS_BRANCH_REQUIRED","Choose a company or branch.");
  const {write}=await holdingResources(actor.holdingId);
  const hotels=await pmsHotelOverview(write,actor,options.propertyId);
  if(options.propertyId&&!hotels.length)throw new ApiError(404,"PMS_NOT_FOUND","Hotel not found in this workspace.");
  const taskSql=taskQuery(actor,options.propertyId,options.view==="mine");
  const loadedIds=options.loadedIds??[],now=Date.now();
  const readableIds=loadedIds.length?(await write.prepare(`SELECT p.id FROM social_posts p WHERE p.id IN (${loadedIds.map(()=>"?").join(",")})
    AND p.company_id=? AND p.state='published' AND (?::text IS NULL OR p.branch_id IS NULL OR p.branch_id=?) AND ${sourceAccessSql("p","?::text")}`)
    .bind(...loadedIds,actor.companyId,actor.branchId,actor.branchId,actor.userId,now,now,actor.assignmentId).all<{id:string}>()).results.map(post=>post.id):[];
  const [tasks,counts,posts,people]=await Promise.all([
    pmsTaskPage(write,actor,{propertyId:options.propertyId,mine:options.view==="mine",label:options.label,cursor:options.taskCursor}),
    write.prepare(`SELECT label,count(*) count,count(*) FILTER (WHERE urgent) urgent FROM (${taskSql.sql}) tasks GROUP BY label ORDER BY urgent DESC,count DESC,label`)
      .bind(...taskSql.args).all<{label:string;count:number;urgent:number}>(),
    options.postId?postView(write,actor,options.postId).then(post=>[post]):homePosts(write,actor,{limit:8,before:options.before,beforeId:options.beforeId,
      propertyId:options.propertyId,purpose:options.view==="handover"?"handover":undefined}),
    pmsSourceRecipients(write,actor),
  ]);
  return {updatedAt:Date.now(),hotels,tasks,counts:counts.results.map(row=>({label:row.label,count:Number(row.count),urgent:Number(row.urgent)})),posts,readablePostIds:readableIds,
    canFinance:hasPermission(actor,"finance.read")&&hasPermission(actor,"pms.reports"),
    feedHasMore:posts.length===8,people,userId:actor.userId,view:options.view??"team"};
}
export type PmsHome=Awaited<ReturnType<typeof pmsHome>>;
