import {legalCorePermission} from '@erpx/server-legal/domain/purchases';
import {z} from "zod";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import {kinds,parseItem,allowedTransitions,type LegalEvent,type LegalItem,type LegalData,type LegalDetail,type DocumentVersion} from "@erpx/server-legal/domain/model";
export type LegalWrite={readonly owner:"legal"};
export type LegalRows<T>={results:T[]};
export type LegalFact=Record<string,unknown>;
export type LegalListQuery={kind?:LegalItem["kind"];search:string;state:string;page:number;mine:"true"|"false"};
export type LegalRegisterRepository={
items(query:LegalListQuery):Promise<LegalRows<LegalItem>>;
total(query:LegalListQuery):Promise<{ n: number; } | null>;
counts():Promise<LegalRows<{ state: string; count: number; }>>;
documents(id:string):Promise<LegalRows<DocumentVersion>>;
events(id:string):Promise<LegalRows<LegalEvent>>;
members(id:string):Promise<LegalRows<{ user_id: string; access: string; }>>;
holds(id:string):Promise<LegalRows<{ id: string; reason: string; state: string; direct: boolean; acknowledged_at: number | null; }>>;
links(id:string):Promise<LegalRows<{ target_id: string; relation: string; title: string; }>>;
comments(id:string):Promise<LegalRows<{ id: string; body: string; visibility: string; created_by: string; created_at: number; }>>;
envelopes(id:string):Promise<LegalRows<Omit<{ id: string; state: string; document_id: string; expires_at: number; consent: string; signers: { user_id: string; ordinal: number; state: string; signed_at: number | null; }[]; }, "signers">>>;
signers(envelopeId:string):Promise<LegalRows<{ user_id: string; ordinal: number; state: string; signed_at: number | null; }>>;
recordRead(id:string,identifier:string,now:number):Promise<void>;
shares(id:string):Promise<LegalRows<{ id: string; email: string; document_id: string; user_id: string | null; expires_at: number; revoked_at: number | null; }>>;
submissions(id:string):Promise<LegalRows<{ id: string; name: string; checksum: string; created_by: string; created_at: number; }>>;
configure(facts:{companyId:string;enabled:boolean;aiEnabled:boolean;reminderDaysJson:string | null;reminderDaysJson2:string | null;}):LegalWrite;
managerAvailable(facts:{companyId:string;branchId:string | null;departmentId:string | null;}):Promise<Record<string, unknown> | null>;
createRecord(facts:{id:string;companyId:string;branchId:string | null;departmentId:string | null;kind:"request" | "matter" | "contract" | "obligation" | "compliance" | "license" | "corporate" | "ip" | "privacy" | "counsel" | "template" | "rule";title:string;description:string;classification:"internal" | "confidential" | "restricted";country:string;jurisdiction:string;language:string;currency:string;timezone:string;priority:"normal" | "high" | "urgent";dueDate:string | null;ownerId:string;actorId:string;now:number;now2:number;}):LegalWrite;
createSubtype(facts:{id:string;dataJson:string;recordKind:LegalItem["kind"];}):LegalWrite;
addInitialMember(facts:{id:string;u:string;}):LegalWrite;
routeRequester(facts:{id:string;companyId:string;branchId:string | null;departmentId:string | null;}):LegalWrite;
updateRecord(facts:{title:string;description:string;classification:"internal" | "confidential" | "restricted";country:string;jurisdiction:string;language:string;currency:string;timezone:string;priority:"normal" | "high" | "urgent";dueDate:string | null;ownerId:string;id:string;}):LegalWrite;
updateSubtype(facts:{dataJson:string;id:string;recordKind:LegalItem["kind"];}):LegalWrite;
retainOwnerMembership(facts:{id:string;ownerId:string;}):LegalWrite;
advanceRecord(facts:{state:string;timestamp:number;id:string;}):LegalWrite;
expandHoldTargets():LegalWrite;
notifyHoldTargets(facts:{timestamp:number;companyId:string;}):LegalWrite;
requireEnabled(key:string):LegalWrite;
requireRequesterRouting(key:string):LegalWrite;
requireHoldPreservable(key:string,id:string):LegalWrite;
requireHoldSourcesPreservable(key:string,id:string):LegalWrite;
enabled():Promise<boolean>;
companyLock():LegalWrite;
guardChanges(id:string,version:number,key:string,edit?:boolean,allowFrozen?:boolean,preparation?:boolean):LegalWrite[];
personGuard(key:string,userId:string,scope?:{branch_id:string|null;department_id:string|null},permission?:string|null):LegalWrite;
featureGuard(key:string):LegalWrite[];
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalRegisterApplication(ports:{actor:WorkContext;repository:LegalRegisterRepository;clock:Clock;identifiers:IdentifierSource;require(permission:string):void;can(permission:string):boolean;samePerson(userId:string|null|undefined):boolean;permissionCatalog:readonly {id:string}[];error(status:number,code:string,message:string):Error;hash(value:string|Uint8Array):string;utf8Length(value:string):number;access(id:string,permission?:string):Promise<LegalItem>;eligible(userId:string,scope?:{branch_id:string|null;department_id:string|null},permission?:string|null):Promise<void>;people():Promise<{id:string;name:string;role:string}[]>}){
const a=ports.actor;
function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function legalRecordData():Promise<LegalData>{
  ports.require("legal.read");
  const people=await ports.people();
  return {
    enabled: await ports.repository.enabled(),
    items: [], total: 0, page: 1, counts: [],
    permissions: ports.permissionCatalog
      .filter((p) => p.id.startsWith("legal.") && ports.can(p.id))
      .map((p) => p.id),
    userId: a.userId,
    workContext:{companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId,assignmentId:a.assignmentId,userId:a.userId},
    people: people.map((p) =>
      p.id === a.userId ? { ...p, name: a.name } : p,
    ),
  };
}
async function legalData(
  query: Record<string, unknown> = {},
): Promise<LegalData> {
  ports.require("legal.read");
  const q = z
      .object({
        kind: z.enum(kinds).optional(),
        search: z.string().max(200).default(""),
        state: z.string().max(40).default(""),
        page: z.coerce.number().int().min(1).max(100000).default(1),
        mine: z.enum(["true", "false"]).default("false"),
      })
      .parse(query);
  const [items, total, counts, context] = await Promise.all([
    ports.repository.items(q),
    ports.repository.total(q),
    ports.repository.counts(),
    legalRecordData(),
  ]);
  return {...context,items:items.results.map(i=>({...i,data:{}})),total:total?.n??0,page:q.page,counts:counts.results};
}
async function detail(
  id: string,
): Promise<LegalDetail> {
  const item = await ports.access(id);
  const [documents, events, members, holds, links, comments, envelopes] =
    await Promise.all([
      ports.repository.documents(id),
      ports.repository.events(id),
      ports.repository.members(id),
      ports.repository.holds(id),
      ports.repository.links(id),
      ports.repository.comments(id),
      ports.repository.envelopes(id),
    ]);
  const enriched = await Promise.all(
    envelopes.results.map(async (e) => ({
      ...e,
      signers: (
        await ports.repository.signers(e.id)
      ).results,
    })),
  );
  await ports.repository.recordRead(id,ports.identifiers.next(),ports.clock.now());
  const shares = ports.can("legal.share")
    ? (
        await ports.repository.shares(id)
      ).results
    : [];
  const submissions = (
    await ports.repository.submissions(id)
  ).results;
  return {
    submissions,
    shares,
    item,
    documents: documents.results.map((d) => ({ ...d, object_key: null })),
    events: events.results,
    members: members.results,
    holds: holds.results,
    links: links.results,
    comments: comments.results,
    envelopes: enriched,
  };
}
const commandBase = z.object({
  action: z.string(),
  id: z.string().optional(),
  expectedVersion: z.number().int().positive().optional(),
});
async function legalCommand(
  raw: unknown,
  key: string,
) {
  const b = commandBase.parse(raw),
    input = raw as Record<string, unknown>;
  if (b.action === "settings") {
    ports.require("legal.manage");
    if (a.branchId || a.departmentId)
      fail(
        "Company settings require a Holding assignment.",
        "LEGAL_SCOPE",
        403,
      );
    const x = z
      .object({
        action: z.literal("settings"),
        enabled: z.boolean(),
        aiEnabled: z.boolean().default(false),
        reminderDays: z
          .array(z.number().int().min(1).max(365))
          .min(1)
          .max(20)
          .optional(),
      })
      .parse(raw);
    return ports.repository.commit(
      key,
      "legal.settings",
      x,
      a.companyId,
      1,
      { id: a.companyId, version: 1 },
      [
        ports.repository.companyLock(),
        ports.repository.configure({companyId:a.companyId,enabled:x.enabled,aiEnabled:x.aiEnabled,reminderDaysJson:x.reminderDays
              ? JSON.stringify(
                  [...new Set(x.reminderDays)].sort((a, b) => b - a),
                )
              : null,reminderDaysJson2:x.reminderDays
              ? JSON.stringify(
                  [...new Set(x.reminderDays)].sort((a, b) => b - a),
                )
              : null}),
      ],
    );
  }
  if (b.action === "create") {
    ports.require("legal.create");
    if (!(await ports.repository.enabled()))
      fail("Enable Legal for this Company first.", "LEGAL_DISABLED", 409);
    let x;
    try {
      x = parseItem(input.item);
    } catch (e) {
      fail(e instanceof Error ? e.message : "Invalid legal record.");
    }
    await ports.eligible(x.ownerId);
    if(x.kind==="obligation"&&x.data.escalationOwner)await ports.eligible(String(x.data.escalationOwner));
    if(a.role==="legal_requester"&&(x.kind!=="request"||x.ownerId!==a.userId||x.data.advice))fail("Create a request owned by you; the Legal team records advice.","LEGAL_REQUESTER_SCOPE",403);
    const prior = await ports.repository.replay<{ id: string; version: number }>(key,"legal.create",key,x);
    if (prior) return prior;
    if(a.role==="legal_requester"&&!(await ports.repository.managerAvailable({companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId})))fail("A current Legal Manager must be assigned to this Company before requests can be submitted.","LEGAL_ROUTING_UNAVAILABLE",409);
    const id = key,
      now = ports.clock.now();
    return ports.repository.commit(
      key,
      "legal.create",
      x,
      key,
      1,
      { id, version: 1 },
      [
        ports.repository.companyLock(),
        ports.repository.personGuard(key,x.ownerId),
        ...(x.kind==="obligation"&&x.data.escalationOwner&&x.data.escalationOwner!==x.ownerId?[ports.repository.personGuard(key,String(x.data.escalationOwner))]:[]),
        ...(["rule","template"].includes(x.kind)&&!a.branchId&&!a.departmentId&&ports.can("legal.manage")?[]:ports.repository.featureGuard(key)),
        ports.repository.requireEnabled(key),
        ports.repository.createRecord({id:id,companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId,kind:x.kind,title:x.title,description:x.description,classification:x.classification,country:x.country,jurisdiction:x.jurisdiction,language:x.language,currency:x.currency,timezone:x.timezone,priority:x.priority,dueDate:x.dueDate,ownerId:x.ownerId,actorId:a.userId,now:now,now2:now}),
        ports.repository.createSubtype({id:id,dataJson:JSON.stringify(x.data),recordKind:x.kind}),
        ...Array.from(new Set([a.userId, x.ownerId])).map((u) =>
          ports.repository.addInitialMember({id:id,u:u}),
        ),
        ...(a.role==="legal_requester"?[ports.repository.requireRequesterRouting(key),ports.repository.routeRequester({id:id,companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId})]:[]),
        ports.repository.event(id,"created","",1,ports.identifiers.next(),ports.clock.now()),
      ],
    );
  }
  const id = z.string().min(1).parse(b.id),
    version = z.number().int().positive().parse(b.expectedVersion);
  const permission=legalCorePermission(a.role,b.action,input.state);
  const r = await ports.access(id,permission);
  if(a.role==="legal_requester"&&(!["update","comment","document","transition"].includes(b.action)||b.action==="transition"&&input.state!=="review"))fail("Requesters can edit their draft, submit it and reply to the Legal team.","LEGAL_REQUESTER_SCOPE",403);
  const prior = await ports.repository.replay<{ id: string; version: number }>(key,`legal.${b.action}`,id,raw);
  if (prior) return prior;
  if(a.role==="legal_requester"&&["update","document","transition"].includes(b.action)&&r.state!=="draft")fail("Only your draft question may be changed or submitted.","LEGAL_REQUESTER_SCOPE",403);
  if (r.version !== version)
    fail(
      "This record changed. Refresh before saving.",
      "LEGAL_VERSION_CONFLICT",
      409,
    );
  const steps: LegalWrite[] = ports.repository.guardChanges(id,version,key,permission === "legal.edit",false,["rule","template"].includes(r.kind));
  let state = r.state,
    note = "";
  if (b.action === "update") {
    if (
      !["draft", "active"].includes(r.state) ||
      (["contract", "rule", "template"].includes(r.kind) && r.state !== "draft")
    )
      fail("Return the record to draft before changing approved content.");
    let x;
    try {
      x = parseItem(input.item);
    } catch (e) {
      fail(e instanceof Error ? e.message : "Invalid record.");
    }
    if (x.kind !== r.kind) fail("Record type cannot change.");
    if(a.role==="legal_requester"&&(r.state!=="draft"||x.ownerId!==r.owner_id||(x.data.advice??"")!==(r.data.advice??"")||x.classification!==r.classification||x.country!==r.country||x.jurisdiction!==r.jurisdiction||x.language!==r.language||x.currency!==r.currency||x.timezone!==r.timezone||x.priority!==r.priority||x.dueDate!==r.due_date))fail("Only your draft question may be edited.","LEGAL_REQUESTER_SCOPE",403);
    await ports.eligible(x.ownerId,r);
    if(x.kind==="obligation"&&x.data.escalationOwner)await ports.eligible(String(x.data.escalationOwner),r);
    steps.push(
      ports.repository.personGuard(key,x.ownerId,r),
      ...(x.kind==="obligation"&&x.data.escalationOwner&&x.data.escalationOwner!==x.ownerId?[ports.repository.personGuard(key,String(x.data.escalationOwner),r)]:[]),
      ports.repository.updateRecord({title:x.title,description:x.description,classification:x.classification,country:x.country,jurisdiction:x.jurisdiction,language:x.language,currency:x.currency,timezone:x.timezone,priority:x.priority,dueDate:x.dueDate,ownerId:x.ownerId,id:id}),
      ports.repository.updateSubtype({dataJson:JSON.stringify(x.data),id:id,recordKind:r.kind}),
      ports.repository.retainOwnerMembership({id:id,ownerId:x.ownerId}),
    );
  } else fail("Unsupported legal action.");
  steps.push(
    ports.repository.advanceRecord({state:state,timestamp:ports.clock.now(),id:id}),
    ports.repository.event(id,b.action,note,version + 1,ports.identifiers.next(),ports.clock.now()),
  );
  // A fixed point with UNION (not UNION ALL) terminates on cycles. Newly linked evidence is protected too.
  steps.push(
    ports.repository.expandHoldTargets(),
  );
  if (["hold.create", "link"].includes(b.action))
    steps.push(
      ports.repository.requireHoldPreservable(key,id),
      ports.repository.requireHoldSourcesPreservable(key,id),
    );
  steps.push(
    ports.repository.notifyHoldTargets({timestamp:ports.clock.now(),companyId:a.companyId}),
  );
  return ports.repository.commit(key,`legal.${b.action}`,raw,id,version + 1,{ id, version: version + 1, state },steps);
}

return {legalData,legalRecordData,detail,legalCommand};}
