import { monthBounds } from "@/lib/billing";
import type { SqlDatabase } from "./runtime/types";

export type SeatInterval = [number,number];
export type HoldingSeatMeter = {
  coverage: "measured" | "unavailable";
  users: {userId:string;intervals:SeatInterval[]}[];
};
export type ActiveSeatSnapshot = {status:"measured";count:number;asOf:number} |
  {status:"unavailable";count:null;asOf:null};
export type MonthlySeatSummary = {status:"measured";distinctUsers:number;activeSeatMilliseconds:number} |
  {status:"unavailable";distinctUsers:null;activeSeatMilliseconds:null};
type Event = {event_id:number;captured_at:number;present:boolean;state_json:string|null};
type AssignmentEvent = Event & {assignment_id:string};
type MemberEvent = Event & {company_id:string;user_id:string};
type RoleEvent = Event & {company_id:string;role_id:string};
type Span<T> = {start:number;end:number;state:T|null};
type AssignmentState = {userId:string;companyId:string;role:string;source:string;parentId:string|null;validFrom:number;validUntil:number;revokedAt:number|null};
type MemberState = {status:string;loginPolicy:string};
type RoleState = {status:string;publishedRevision:number};

function fail(): HoldingSeatMeter {return {coverage:"unavailable",users:[]};}
function integer(value:unknown):value is number {return typeof value==="number"&&Number.isSafeInteger(value);}
function parse<T>(json:string|null):T|null {return json===null?null:JSON.parse(json) as T;}
function intervalsOverlap(a:SeatInterval,b:SeatInterval):SeatInterval|null {
  const start=Math.max(a[0],b[0]),end=Math.min(a[1],b[1]);
  return start<end?[start,end]:null;
}
export function canonicalSeatIntervals(intervals:SeatInterval[]):SeatInterval[] {
  const sorted=intervals.filter(([start,end])=>integer(start)&&integer(end)&&start<end)
    .sort((a,b)=>a[0]-b[0]||a[1]-b[1]);
  const result:SeatInterval[]=[];
  for(const interval of sorted){
    const prior=result.at(-1);
    if(prior&&interval[0]<=prior[1])prior[1]=Math.max(prior[1],interval[1]);
    else result.push([...interval]);
  }
  return result;
}
function intersectSets(left:SeatInterval[],right:SeatInterval[]):SeatInterval[] {
  const output:SeatInterval[]=[];
  for(const a of left)for(const b of right){const item=intervalsOverlap(a,b);if(item)output.push(item);}
  return canonicalSeatIntervals(output);
}
function eventSpans<T>(events:Event[],start:number,end:number):Span<T>[] {
  const sorted=[...events].sort((a,b)=>a.captured_at-b.captured_at||a.event_id-b.event_id);
  const spans:Span<T>[]=[];
  for(let i=0;i<sorted.length;i++){
    const event=sorted[i],next=sorted[i+1];
    const from=Math.max(start,event.captured_at),until=Math.min(end,next?.captured_at??end);
    if(from<until)spans.push({start:from,end:until,state:event.present?parse<T>(event.state_json):null});
  }
  return spans;
}
function group<T extends Event>(rows:T[],key:(row:T)=>string) {
  const groups=new Map<string,T[]>();
  for(const row of rows){const id=key(row);const list=groups.get(id)??[];list.push(row);groups.set(id,list);}
  return groups;
}
function statusIntervals<T>(events:Event[]|undefined,start:number,end:number,active:(state:T|null)=>boolean):SeatInterval[] {
  if(!events)return [[start,end]]; // No IAM member row means active in iam_principal_active().
  const first=Math.min(...events.map(event=>event.captured_at));
  const before:SeatInterval[]=first>start?[[start,Math.min(end,first)]]:[];
  return [...before,...eventSpans<T>(events,start,end).filter(span=>active(span.state)).map(span=>[span.start,span.end] as SeatInterval)];
}

/** Historical source evaluation is intentionally narrow; uncertain IAM inputs block the entire month. */
export function deriveHoldingSeatIntervals(input:{month:string;historyStartedAt:number|null;assignments:AssignmentEvent[];members:MemberEvent[];roles:RoleEvent[];hasCutoffDependency:boolean}):HoldingSeatMeter {
  const {start,end}=monthBounds(input.month);
  if(input.historyStartedAt===null||input.historyStartedAt>start||input.hasCutoffDependency)return fail();
  const assignments=group(input.assignments,row=>row.assignment_id);
  const members=group(input.members,row=>`${row.company_id}\0${row.user_id}`);
  const roles=group(input.roles,row=>`${row.company_id}\0${row.role_id}`);
  const perUser=new Map<string,SeatInterval[]>();
  for(const events of assignments.values())for(const span of eventSpans<AssignmentState>(events,start,end)){
    const state=span.state;
    if(!state)continue;
    if(!state.userId||!state.companyId||!state.role||!integer(state.validFrom)||!integer(state.validUntil)||
      state.validUntil<=state.validFrom||state.revokedAt!==null&&!integer(state.revokedAt))return fail();
    const grant=intervalsOverlap([span.start,span.end],[state.validFrom,state.validUntil]);
    if(!grant||state.revokedAt!==null)continue;
    if(!["manual","legacy"].includes(state.source)||state.parentId!==null)return fail();
    if(/^(system:|service:|platform:|bot:)/i.test(state.userId))continue;
    const memberKey=`${state.companyId}\0${state.userId}`;
    const memberRows=members.get(memberKey);
    if(memberRows&&eventSpans<MemberState>(memberRows,start,end).some(part=>
      intervalsOverlap(grant,[part.start,part.end])&&part.state?.status==="active"&&part.state.loginPolicy!=="local"))return fail();
    const memberActive=statusIntervals<MemberState>(memberRows,start,end,s=>s===null||s.status==="active"&&s.loginPolicy==="local");
    const localRole=roles.get(`${state.companyId}\0${state.role}`);
    const globalRole=roles.get(`*\0${state.role}`);
    if(!localRole&&!globalRole)return fail();
    // IAM accepts either the company role or its global counterpart. Their
    // histories can overlap, or one can replace the other during a month.
    const roleActive=canonicalSeatIntervals([localRole,globalRole].flatMap(roleRows => roleRows
        ? eventSpans<RoleState>(roleRows,start,end)
          .filter(part=>part.state?.status==="active"&&integer(part.state.publishedRevision)&&part.state.publishedRevision>0)
          .map(part=>[part.start,part.end] as SeatInterval)
        : []));
    const active=intersectSets(intersectSets([grant],memberActive),roleActive);
    if(!active.length)continue;
    perUser.set(state.userId,[...(perUser.get(state.userId)??[]),...active]);
  }
  return {coverage:"measured",users:[...perUser].map(([userId,intervals])=>({userId,intervals:canonicalSeatIntervals(intervals)}))
    .filter(user=>user.intervals.length>0).sort((a,b)=>a.userId.localeCompare(b.userId))};
}

export async function readHoldingSeatIntervals(db:SqlDatabase,month:string):Promise<HoldingSeatMeter> {
  const {start,end}=monthBounds(month);
  if(end>Date.now())return fail();
  const [coverage,assignments,members,roles,cutoffs]=await Promise.all([
    db.prepare("SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1").first<{started_at:number}>(),
    db.prepare("SELECT event_id,assignment_id,captured_at,present,state::text state_json FROM billing_seat_assignment_events WHERE captured_at<? ORDER BY assignment_id,captured_at,event_id")
      .bind(end).all<AssignmentEvent>(),
    db.prepare("SELECT event_id,company_id,user_id,captured_at,present,state::text state_json FROM billing_seat_member_events WHERE captured_at<? ORDER BY company_id,user_id,captured_at,event_id")
      .bind(end).all<MemberEvent>(),
    db.prepare("SELECT event_id,company_id,role_id,captured_at,present,state::text state_json FROM billing_seat_role_events WHERE captured_at<? ORDER BY company_id,role_id,captured_at,event_id")
      .bind(end).all<RoleEvent>(),
    db.prepare(`SELECT EXISTS(SELECT 1 FROM people_access_cutoffs
      WHERE effective_at<? AND (restored_at IS NULL OR restored_at>?)
        AND (cancelled_at IS NULL OR cancelled_at>?)) dependency`).bind(end,start,start).first<{dependency:boolean}>(),
  ]);
  return deriveHoldingSeatIntervals({month,historyStartedAt:coverage?.started_at??null,
    assignments:assignments.results,members:members.results,roles:roles.results,
    hasCutoffDependency:cutoffs?.dependency??true});
}

/** Operational point-in-time count. This is never evidence for monthly rating. */
export async function readActiveHoldingSeatCount(
  holding:SqlDatabase,control:SqlDatabase,asOf=Date.now(),
):Promise<ActiveSeatSnapshot> {
  const candidates=(await holding.prepare(`SELECT DISTINCT a.user_id
    FROM assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id)
      AND r.status='active'
    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id
      AND v.revision=r.published_revision
    WHERE a.scope_kind IN ('holding','branch','department')
      AND a.user_id !~* '^(system:|service:|platform:|bot:)'
      AND jsonb_array_length(v.permissions_json)>0
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)`)
    .bind(asOf,asOf).all<{user_id:string}>()).results;
  if(!candidates.length)return {status:"measured",count:0,asOf};
  let count=0;
  for(let offset=0;offset<candidates.length;offset+=500){
    const ids=candidates.slice(offset,offset+500).map(row=>row.user_id);
    const rows=(await control.prepare(`SELECT u.id user_id,p.status,
      EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity
      FROM "user" u LEFT JOIN principals p ON p.user_id=u.id
      WHERE u.id IN (${ids.map(()=>"?").join(",")})`)
      .bind(...ids).all<{user_id:string;status:string|null;platform_identity:boolean}>()).results;
    // A stale Holding assignment without a Control account is not a usable
    // workspace seat. A real account missing principal status is uncertain.
    if(rows.some(row=>row.status===null))return {status:"unavailable",count:null,asOf:null};
    count+=rows.filter(row=>row.status==="active"&&!row.platform_identity).length;
  }
  return {status:"measured",count,asOf};
}

/** Read the exact projected interval set used by the Sandbox rating engine. */
export async function readControlSeatSummary(
  control:SqlDatabase,holdingId:string,month:string,
):Promise<MonthlySeatSummary> {
  const snapshot=await control.prepare(`SELECT snapshot_id,seat_coverage,seat_source_count
    FROM billing_usage_snapshots WHERE holding_id=? AND month=?`)
    .bind(holdingId,month).first<{snapshot_id:string;seat_coverage:string;seat_source_count:number}>();
  if(snapshot?.seat_coverage!=="measured")return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};
  const rows=(await control.prepare(`SELECT intervals_json::text intervals_json,snapshot_id
    FROM billing_seat_usage WHERE holding_id=? AND month=?`)
    .bind(holdingId,month).all<{intervals_json:string;snapshot_id:string}>()).results;
  if(rows.length!==snapshot.seat_source_count||rows.some(row=>row.snapshot_id!==snapshot.snapshot_id))
    return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};
  const {start,end}=monthBounds(month);
  let milliseconds=0;
  for(const row of rows){
    let parsed:unknown;
    try{parsed=JSON.parse(row.intervals_json);}catch{return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};}
    if(!Array.isArray(parsed)||!parsed.length||parsed.some(interval=>
      !Array.isArray(interval)||interval.length!==2||!integer(interval[0])||!integer(interval[1])||
      interval[0]<start||interval[1]>end||interval[0]>=interval[1]))
      return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};
    const intervals=parsed as SeatInterval[];
    if(JSON.stringify(intervals)!==JSON.stringify(canonicalSeatIntervals(intervals)))
      return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};
    for(const [from,until] of intervals)milliseconds+=until-from;
  }
  if(!Number.isSafeInteger(milliseconds))return {status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null};
  return {status:"measured",distinctUsers:rows.length,activeSeatMilliseconds:milliseconds};
}
