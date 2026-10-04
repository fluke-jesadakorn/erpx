import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, TransactionalSqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { executeCommand, replayCommand } from "./commands";
import { hasPermission } from "@/lib/permissions";
import { sourceRecordLabel, type SourceRecordReference } from "@/lib/source-record";
import { hasStaffNotificationResolver, staffNotificationSourceTypes, visibleNotifications, type StaffNotification } from "./staff-notifications";

export type InboxCategory = "work" | "calendar" | "mention" | "notice" | "system";
export type InboxEventInput = {
  key: string; companyId: string; sourceType: string; sourceId: string;
  sourceVersion: number; actorUserId?: string | null; category: InboxCategory;
  preview: string; recipientIds: string[]; createdAt?: number;
};
export type InboxItem = {
  id: string; eventId: string; category: InboxCategory; message: string;
  href: string | null; createdAt: number; readAt: number | null;
  archivedAt: number | null; sourceType: string; sourceId: string;
  source?: SourceRecordReference;
};
type Candidate = {
  id: string; event_id: string; company_id: string; user_id: string;
  delivered_at: number; read_at: number | null; archived_at: number | null;
  source_type: string; source_id: string; source_version: number;
  category: InboxCategory; preview: string;
};

/** Call inside the source command transaction. Retrying a source event is harmless. */
export async function publishInboxEvent(db: SqlDatabase, input: InboxEventInput) {
  const now = input.createdAt ?? Date.now();
  const id = crypto.randomUUID();
  await db.prepare(`INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,actor_user_id,category,preview,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,event_key) DO NOTHING`)
    .bind(id,input.companyId,input.key,input.sourceType,input.sourceId,input.sourceVersion,input.actorUserId??null,input.category,input.preview,now).run();
  const event = await db.prepare("SELECT id FROM inbox_events WHERE company_id=? AND event_key=?")
    .bind(input.companyId,input.key).first<{id:string}>();
  if (!event) throw new Error("INBOX_EVENT_MISSING");
  for (const userId of new Set(input.recipientIds)) {
    await db.prepare(`INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)
      VALUES(?,?,?,?,?) ON CONFLICT(event_id,user_id) DO NOTHING`)
      .bind(crypto.randomUUID(),event.id,input.companyId,userId,now).run();
  }
  return event.id;
}

async function resolveCandidate(db:SqlDatabase,actor:WorkContext,row:Candidate):Promise<InboxItem|null> {
  let href:string|null=null;
  let source:SourceRecordReference|undefined;
  if(row.source_type==="notice") {
    const notice=await db.prepare("SELECT id FROM inbox_notices WHERE id=? AND company_id=?")
      .bind(row.source_id,actor.companyId).first();
    if(!notice)return null;
  } else if(row.source_type==="calendar_appointment" || row.source_type==="calendar_removed") {
    const appointment=await db.prepare("SELECT id,version FROM calendar_appointments WHERE id=? AND company_id=?")
      .bind(row.source_id,actor.companyId).first<{id:string;version:number}>();
    if(!appointment)return null;
    if(row.source_type==="calendar_appointment") {
      const invited=await db.prepare(`SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?
        UNION ALL SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=? AND created_by=? LIMIT 1`)
        .bind(row.source_id,actor.companyId,actor.userId,row.source_id,actor.companyId,actor.userId).first();
      if(!invited)return null;
      href=`/calendar?event=appointment:${encodeURIComponent(row.source_id)}`;
      source={kind:row.source_type,id:appointment.id,version:appointment.version,href,label:sourceRecordLabel(row.source_type)};
    }
  } else {
    if(!hasStaffNotificationResolver(row.source_type))return null;
    const legacy:Omit<StaffNotification,"href">={id:row.id,record_type:row.source_type,record_id:row.source_id,
      record_version:row.source_version,message:row.preview,created_at:row.delivered_at,read_at:row.read_at};
    const visible=await visibleNotifications(db,actor,[legacy]);
    if(!visible.length)return null;
    href=visible[0].href;
    source=visible[0].source;
  }
  return {id:row.id,eventId:row.event_id,category:row.category,message:row.preview,href,
    createdAt:row.delivered_at,readAt:row.read_at,archivedAt:row.archived_at,
    sourceType:row.source_type,sourceId:row.source_id,source};
}

const candidateSql=`SELECT d.*,e.source_type,e.source_id,e.source_version,e.category,e.preview
  FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
  WHERE d.company_id=? AND d.user_id=? AND d.archived_at IS NULL`;
function cursorDecode(value?:string) {
  if(!value)return null;
  try {
    if(value.length>512)throw Error();
    const v=JSON.parse(Buffer.from(value,"base64url").toString()) as {at:number;id:string};
    if(!Number.isSafeInteger(v.at)||typeof v.id!=="string"||v.id.length>180)throw Error();
    return v;
  } catch {throw new ApiError(422,"INVALID_CURSOR","Restart the Inbox list.");}
}
function cursorEncode(item:InboxItem){return Buffer.from(JSON.stringify({at:item.createdAt,id:item.id})).toString("base64url");}
export async function inboxPage(db:SqlDatabase,actor:WorkContext,query:{view?:string;category?:string;search?:string;cursor?:string;limit?:number}) {
  const limit=Math.max(1,Math.min(100,query.limit??30));
  const category=query.category&&["work","calendar","mention","notice","system"].includes(query.category)?query.category:null;
  const search=(query.search??"").trim().slice(0,100);
  let cursor=cursorDecode(query.cursor),exhausted=false;
  const items:InboxItem[]=[];
  while(!exhausted && items.length<=limit) {
    const rows=(await db.prepare(`${candidateSql}
      ${query.view==="all"?"":"AND d.read_at IS NULL"}
      ${category?"AND e.category=?":""}
      ${search?"AND e.preview ILIKE '%'||?||'%'":""}
      ${cursor?"AND (d.delivered_at,d.id)<(?,?)":""}
      ORDER BY d.delivered_at DESC,d.id DESC LIMIT 100`)
      .bind(actor.companyId,actor.userId,...(category?[category]:[]),...(search?[search]:[]),...(cursor?[cursor.at,cursor.id]:[]))
      .all<Candidate>()).results;
    if(rows.length<100)exhausted=true;
    if(!rows.length)break;
    for(const row of rows) {
      cursor={at:row.delivered_at,id:row.id};
      const resolved=await resolveCandidate(db,actor,row);
      if(resolved)items.push(resolved);
      if(items.length>limit)break;
    }
  }
  const hasMore=items.length>limit;
  const page=items.slice(0,limit);
  return {items:page,nextCursor:hasMore?cursorEncode(page[page.length-1]):null,hasMore};
}

export async function inboxSummary(db:SqlDatabase,actor:WorkContext) {
  const counts:Record<InboxCategory,number>={work:0,calendar:0,mention:0,notice:0,system:0};
  let cursor:string|undefined;
  for (;;) {
    const page=await inboxPage(db,actor,{view:"unread",cursor,limit:100});
    for(const item of page.items)counts[item.category]++;
    if(!page.nextCursor)break;
    cursor=page.nextCursor;
  }
  return {unread:Object.values(counts).reduce((a,b)=>a+b,0),categories:counts};
}

export async function changeInboxState(db:SqlDatabase,actor:WorkContext,id:string,state:{read?:boolean;archived?:boolean}) {
  const row=await db.prepare(`SELECT d.*,e.source_type,e.source_id,e.source_version,e.category,e.preview FROM inbox_deliveries d
    JOIN inbox_events e ON e.id=d.event_id WHERE d.id=? AND d.company_id=? AND d.user_id=?`)
    .bind(id,actor.companyId,actor.userId).first<Candidate>();
  if(!row||!await resolveCandidate(db,actor,row))throw new ApiError(404,"INBOX_ITEM_NOT_FOUND","This update is unavailable.");
  const readAt=state.read===undefined?row.read_at:state.read?(row.read_at??Date.now()):null;
  const archivedAt=state.archived===undefined?row.archived_at:state.archived?(row.archived_at??Date.now()):null;
  await db.prepare("UPDATE inbox_deliveries SET read_at=?,archived_at=? WHERE id=? AND company_id=? AND user_id=?")
    .bind(readAt,archivedAt,id,actor.companyId,actor.userId).run();
  if(row.event_id.startsWith("legacy:"))await db.prepare("UPDATE staff_notifications SET read_at=? WHERE id=? AND company_id=? AND user_id=?")
    .bind(readAt,id,actor.companyId,actor.userId).run();
  return {id,readAt,archivedAt};
}

export const noticeInput=z.object({
  title:z.string().trim().min(1).max(160),body:z.string().trim().min(1).max(5000),
  recipients:z.array(z.string().min(1).max(160)).max(100).default([]),
  audience:z.object({kind:z.enum(["company","branch","department"]),id:z.string().min(1).max(160)}).optional(),
}).refine(v=>Boolean(v.audience)!==(v.recipients.length>0),"Choose named recipients or one broadcast audience.");

export async function sendNotice(database:TransactionalSqlDatabase,actor:WorkContext,raw:unknown,key:string) {
  const input=noticeInput.parse(raw),id=`notice-${key}`,operation="inbox.notice.send";
  return database.transaction(async db=>{
    const replay=await replayCommand<{id:string;recipientCount:number}>(db,actor,key,operation,id,input);
    if(replay)return replay;
    const now=Date.now();
    if(input.audience){
      const allowed=hasPermission(actor,"inbox.notice.broadcast")&&await db.prepare("SELECT 1 WHERE iam_role_allows(?,?,?)")
        .bind(actor.role,actor.companyId,"inbox.notice.broadcast").first();
      if(!allowed)throw new ApiError(403,"BROADCAST_FORBIDDEN","Your role cannot broadcast notices.");
      const node=await db.prepare("SELECT id,kind,branch_id FROM organization_nodes WHERE id=? AND company_id=? AND status='active'")
        .bind(input.audience.id,actor.companyId).first<{id:string;kind:string;branch_id:string|null}>();
      if(!node||node.kind!==input.audience.kind||
        input.audience.kind==="company"&&input.audience.id!==actor.companyId||
        actor.departmentId&&input.audience.id!==actor.departmentId||
        actor.branchId&&input.audience.kind==="company"||
        actor.branchId&&input.audience.kind==="branch"&&input.audience.id!==actor.branchId||
        actor.branchId&&input.audience.kind==="department"&&node.branch_id!==actor.branchId)
        throw new ApiError(403,"BROADCAST_SCOPE","Choose an audience inside your current scope.");
    }
    const recent=await db.prepare("SELECT count(*)::int n FROM inbox_notices WHERE company_id=? AND sender_user_id=? AND created_at>?")
      .bind(actor.companyId,actor.userId,now-3600000).first<{n:number}>();
    if((recent?.n??0)>=20)throw new ApiError(429,"NOTICE_RATE_LIMIT","Try again after your notice limit resets.");
    let ids=input.recipients;
    if(input.audience){
      const audience=input.audience;
      ids=(await db.prepare(`SELECT DISTINCT a.user_id FROM assignments a
        LEFT JOIN hr_people p ON p.company_id=a.company_id AND p.user_id=a.user_id AND p.merged_into_id IS NULL
        LEFT JOIN hr_engagements h ON h.company_id=p.company_id AND h.person_id=p.id AND h.status='active'
        WHERE a.company_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?)
        AND iam_principal_active(a.user_id,a.company_id,?)
        AND (?='company' OR (?='branch' AND (a.scope_node_id=? OR h.branch_id=?))
             OR (?='department' AND (a.scope_node_id=? OR h.department_id=?)))
        ORDER BY a.user_id LIMIT 10001`).bind(actor.companyId,now,now,audience.kind,audience.kind,audience.id,audience.id,
          audience.kind,audience.id,audience.id).all<{user_id:string}>()).results.map(row=>row.user_id);
      if(ids.length>10000)throw new ApiError(422,"AUDIENCE_TOO_LARGE","Choose a smaller audience.");
    }
    ids=[...new Set(ids.filter(value=>value!==actor.userId))];
    if(!ids.length)throw new ApiError(422,"NO_RECIPIENTS","Choose at least one colleague.");
    const active=(await db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${ids.map(()=>"?").join(",")})
      AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`)
      .bind(actor.companyId,...ids,now,now).all<{user_id:string}>()).results.map(row=>row.user_id);
    if(active.length!==ids.length)
      throw new ApiError(422,"RECIPIENT_UNAVAILABLE","Choose active accounts in this Company.");
    const result={id,recipientCount:ids.length};
    await db.prepare("INSERT INTO inbox_notices(id,company_id,sender_user_id,title,body,audience_json,recipient_count,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?)")
      .bind(id,actor.companyId,actor.userId,input.title,input.body,JSON.stringify(input.audience??{kind:"named",ids}),ids.length,now).run();
    await publishInboxEvent(db,{key:`notice:${id}`,companyId:actor.companyId,sourceType:"notice",sourceId:id,
      sourceVersion:1,actorUserId:actor.userId,category:"notice",preview:`${input.title} — ${input.body}`,recipientIds:input.audience?[]:ids,createdAt:now});
    if(input.audience)for(const userId of ids)await db.prepare("INSERT INTO inbox_notice_targets(notice_id,company_id,user_id) VALUES(?,?,?)")
      .bind(id,actor.companyId,userId).run();
    await executeCommand(db,actor,key,operation,input,id,1,result,[]);
    return result;
  });
}

/** Bounded durable fanout. The unique delivery key makes worker replay safe. */
export async function drainInboxNotices(db:SqlDatabase) {
  const targets=(await db.prepare("SELECT notice_id,company_id,user_id FROM inbox_notice_targets WHERE delivered_at IS NULL ORDER BY notice_id,user_id LIMIT 50")
    .all<{notice_id:string;company_id:string;user_id:string}>()).results;
  for(const target of targets){
    const active=await db.prepare(`SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL
      AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) LIMIT 1`)
      .bind(target.company_id,target.user_id,Date.now(),Date.now()).first();
    if(active){
      const event=await db.prepare("SELECT id FROM inbox_events WHERE company_id=? AND event_key=?")
        .bind(target.company_id,`notice:${target.notice_id}`).first<{id:string}>();
      if(event)await db.prepare("INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES(?,?,?,?,?) ON CONFLICT(event_id,user_id) DO NOTHING")
        .bind(crypto.randomUUID(),event.id,target.company_id,target.user_id,Date.now()).run();
    }
    await db.prepare("UPDATE inbox_notice_targets SET delivered_at=? WHERE notice_id=? AND user_id=? AND delivered_at IS NULL")
      .bind(Date.now(),target.notice_id,target.user_id).run();
  }
  return targets.length;
}

/** Aggregate operational signals only; no recipient or message content is logged. */
export async function inboxOperationalMetrics(db:SqlDatabase,now=Date.now()){
  const pending=await db.prepare(`SELECT count(*)::int count,min(n.created_at) oldest
    FROM inbox_notice_targets t JOIN inbox_notices n ON n.id=t.notice_id WHERE t.delivered_at IS NULL`)
    .first<{count:number;oldest:number|null}>();
  const types=["notice","calendar_appointment","calendar_removed",...staffNotificationSourceTypes];
  const unresolved=await db.prepare(`SELECT count(*)::int count FROM inbox_events WHERE source_type NOT IN (${types.map(()=>"?").join(",")})`)
    .bind(...types).first<{count:number}>();
  const failed=await db.prepare("SELECT count(*)::int count FROM outbox_events WHERE dead_letter_at IS NOT NULL")
    .first<{count:number}>();
  return {pendingDeliveries:pending?.count??0,oldestDeliveryLagMs:pending?.oldest?now-pending.oldest:0,
    unresolvedEventTypes:unresolved?.count??0,failedJobs:failed?.count??0};
}
