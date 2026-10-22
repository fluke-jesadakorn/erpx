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
lastAuthor(facts:{id:string;}):Promise<{ actor_id: string; } | null>;
updateRecord(facts:{title:string;description:string;classification:"internal" | "confidential" | "restricted";country:string;jurisdiction:string;language:string;currency:string;timezone:string;priority:"normal" | "high" | "urgent";dueDate:string | null;ownerId:string;id:string;}):LegalWrite;
updateSubtype(facts:{dataJson:string;id:string;recordKind:LegalItem["kind"];}):LegalWrite;
retainOwnerMembership(facts:{id:string;ownerId:string;}):LegalWrite;
assignOwner(facts:{ownerId:string;id:string;}):LegalWrite;
retainAssignedMembership(facts:{id:string;ownerId:string;}):LegalWrite;
executedContract(facts:{id:string;id2:string;id3:string;}):Promise<Record<string, unknown> | null>;
cancelEnvelopes(facts:{id:string;}):LegalWrite;
removeMember(facts:{id:string;userId:string;}):LegalWrite;
setMember(facts:{id:string;userId:string;access:"read" | "edit";}):LegalWrite;
documentBelongs(facts:{documentId:string;id:string;}):Promise<Record<string, unknown> | null>;
addComment(facts:{identifier:string;id:string;section:string;body:string;visibility:"internal" | "external";actorId:string;timestamp:number;documentId:string | null;}):LegalWrite;
nextDocumentVersion(facts:{id:string;}):Promise<{ n: number; } | null>;
addAuthoredDocument(facts:{identifier:string;id:string;next:number;name:string;checksum:string;byteLength:number;body:string;actorId:string;timestamp:number;}):LegalWrite;
approvalPending(facts:{id:string;}):Promise<Record<string, unknown> | null>;
approvalDocument(facts:{documentId:string;id:string;}):Promise<DocumentVersion | null>;
isExecutedCopy(facts:{documentId:string;}):Promise<Record<string, unknown> | null>;
internalSigner(facts:{id:string;signer:string;}):Promise<Record<string, unknown> | null>;
externalSigner(facts:{id:string;documentId:string;signer:string;timestamp:number;}):Promise<Record<string, unknown> | null>;
lockApprovedDocument(facts:{documentId:string;}):LegalWrite;
recordApproval(facts:{id:string;nextVersion:number;documentId:string;documentChecksum:string;signerIdsJson:string;actorId:string;timestamp:number;}):LegalWrite;
linkRecord(facts:{id:string;targetId:string;relation:"obligation" | "related" | "evidence" | "amends" | "derived";}):LegalWrite;
preservationNotice(facts:{holdId:string;id:string;actorId:string;}):Promise<Record<string, unknown> | null>;
acknowledgeNotice(facts:{timestamp:number;note:string;holdId:string;id:string;actorId:string;}):LegalWrite;
createHold(facts:{hold:string;id:string;note:string;actorId:string;timestamp:number;}):LegalWrite;
includeHoldTarget(facts:{hold:string;id:string;}):LegalWrite;
hold(facts:{requestedId:string;id:string;}):Promise<{ id: string; state: string; created_by: string; release_requested_by: string | null; } | null>;
requestHoldRelease(facts:{actorId:string;holdId:string;}):LegalWrite;
releaseHold(facts:{actorId:string;timestamp:number;holdId:string;}):LegalWrite;
shareDocumentBelongs(facts:{id:string;documentId:string;}):Promise<Record<string, unknown> | null>;
shareDocument(facts:{shareId:string;id:string;documentId:string;value3:string;expiresAt:number;actorId:string;timestamp:number;}):LegalWrite;
queueInvitation(facts:{identifier:string;shareId:string;holdingSlug:string;timestamp:number;}):LegalWrite;
revokeShare(facts:{timestamp:number;share:string;id:string;}):LegalWrite;
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
  if (
    b.action === "approve" ||
    (b.action === "transition" && input.state === "approved")
  ) {
    const lastAuthor = await ports.repository.lastAuthor({id:id});
    if (
      ports.samePerson(r.created_by) ||
      (lastAuthor && ports.samePerson(lastAuthor.actor_id))
    )
      fail(
        "The creator or latest content author cannot approve their own work.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
  }
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
  } else if(b.action==="assign"){
    const x=z.object({ownerId:z.string().min(1).max(200),reason:z.string().trim().min(3).max(4000)}).parse(input);
    if(["archived","cancelled","closed","completed","terminated","expired"].includes(r.state))fail("Reopen this record before changing its owner.","LEGAL_TRANSITION",409);
    await ports.eligible(x.ownerId,r);
    steps.push(ports.repository.personGuard(key,x.ownerId,r),ports.repository.assignOwner({ownerId:x.ownerId,id:id}),ports.repository.retainAssignedMembership({id:id,ownerId:x.ownerId}));
    note=x.reason;
  } else if (b.action === "transition") {
    state = z.string().parse(input.state);
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    if (!allowedTransitions(r.kind, r.state).includes(state))
      fail(`Cannot move ${r.state} to ${state}.`, "LEGAL_TRANSITION", 409);
    if (state === "approved" && r.kind === "contract")
      fail(
        "Approve a locked document and named signer list using contract approval.",
      );
    if (["approved", "rejected"].includes(state) && ports.samePerson(r.created_by))
      fail(
        "An independent reviewer must decide.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    if (
      state === "approved" &&
      r.kind === "rule" &&
      (!r.data.reviewNote || !r.data.effectiveDate)
    )
      fail("Record the legal review evidence before activating a rule.");
    if (
      state === "active" &&
      r.kind === "contract" &&
      !(await ports.repository.executedContract({id:id,id2:id,id3:id}))
    )
      fail("An independently confirmed executed copy is required.");
    if (
      state === "completed" &&
      r.kind === "obligation" &&
      !r.data.completionEvidence
    )
      fail("Completion evidence is required.");
    if (state === "cancelled")
      steps.push(
        ports.repository.cancelEnvelopes({id:id}),
      );
  } else if (b.action === "member") {
    const x = z
      .object({
        userId: z.string().min(1),
        access: z.enum(["read", "edit", "remove"]),
      })
      .parse(input);
    if(x.access!=="remove")await ports.eligible(x.userId,r);
    if (x.access === "remove" && [r.created_by, r.owner_id].includes(x.userId))
      fail(
        "Reassign ownership before removing the owner; creator membership is retained.",
      );
    steps.push(
      ...(x.access!=="remove"?[ports.repository.personGuard(key,x.userId,r)]:[]),
      x.access === "remove"
        ? ports.repository.removeMember({id:id,userId:x.userId})
        : ports.repository.setMember({id:id,userId:x.userId,access:x.access}),
    );
    note = `${x.userId}: ${x.access}`;
  } else if (b.action === "comment") {
    const x = z
      .object({
        body: z.string().trim().min(1).max(20000),
        visibility: z.enum(["internal", "external"]).default("internal"),
        section: z.string().max(200).default(""),
        documentId: z.string().default(""),
      })
      .parse(input);
    if (x.visibility === "external") {
      ports.require("legal.share");
      if (!x.documentId)
        fail("Choose the document version for external discussion.");
    }
    if (
      x.documentId &&
      !(await ports.repository.documentBelongs({documentId:x.documentId,id:id}))
    )
      fail("Choose a document belonging to this record.");
    steps.push(
      ports.repository.addComment({identifier:ports.identifiers.next(),id:id,section:x.section,body:x.body,visibility:x.visibility,actorId:a.userId,timestamp:ports.clock.now(),documentId:x.documentId || null}),
    );
    note = x.visibility;
  } else if (b.action === "document") {
    if (r.state !== "draft") fail("New content requires a draft record.");
    const x = z
      .object({
        name: z.string().min(1).max(200),
        body: z.string().min(1).max(200000),
      })
      .parse(input);
    const next = (await ports.repository.nextDocumentVersion({id:id}))!.n;
    steps.push(
      ports.repository.addAuthoredDocument({identifier:ports.identifiers.next(),id:id,next:next,name:x.name,checksum:ports.hash(x.body),byteLength:ports.utf8Length(x.body),body:x.body,actorId:a.userId,timestamp:ports.clock.now()}),
    );
    note = `Document version ${next}`;
  } else if (b.action === "approve") {
    if (
      await ports.repository.approvalPending({id:r.id})
    )
      fail(
        "Use the assigned approval route to decide this contract.",
        "LEGAL_APPROVAL_ROUTE",
        409,
      );
    if (r.kind !== "contract" || r.state !== "approval")
      fail("Contract must be awaiting approval.");
    if (ports.samePerson(r.created_by))
      fail(
        "An independent reviewer must approve.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    const x = z
      .object({
        documentId: z.string(),
        signerIds: z.array(z.string().min(1)).min(1).max(20),
        reason: z.string().trim().min(3).max(4000),
      })
      .parse(input);
    if (new Set(x.signerIds).size !== x.signerIds.length)
      fail("Each signer must be unique.");
    const d = await ports.repository.approvalDocument({documentId:x.documentId,id:id});
    if (!d) fail("Choose a document belonging to this contract.");
    if(await ports.repository.isExecutedCopy({documentId:d.id}))fail("Approve the draft document; imported executed copies are reviewed separately.");
    if (ports.samePerson(d.created_by))
      fail(
        "The document author cannot approve their own document.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    for (const signer of x.signerIds) {
      const internal = await ports.repository.internalSigner({id:id,signer:signer});
      const external = await ports.repository.externalSigner({id:id,documentId:d.id,signer:signer,timestamp:ports.clock.now()});
      if (!internal && !external)
        fail("Every signer needs explicit access to this document.");
    }
    steps.push(
      ports.repository.lockApprovedDocument({documentId:d.id}),
      ports.repository.recordApproval({id:id,nextVersion:version + 1,documentId:d.id,documentChecksum:d.checksum,signerIdsJson:JSON.stringify(x.signerIds),actorId:a.userId,timestamp:ports.clock.now()}),
    );
    state = "approved";
    note = x.reason;
  } else if (b.action === "link") {
    const x = z
      .object({
        targetId: z.string(),
        relation: z.enum([
          "related",
          "evidence",
          "amends",
          "obligation",
          "derived",
        ]),
      })
      .parse(input);
    await ports.access(x.targetId);
    if (id === x.targetId) fail("Choose another record.");
    steps.push(
      ports.repository.linkRecord({id:id,targetId:x.targetId,relation:x.relation}),
    );
    note = `${x.relation}: ${x.targetId}`;
  } else if (b.action === "hold.acknowledge") {
    const holdId = z.string().parse(input.holdId);
    note = z.string().trim().min(10).max(4000).parse(input.reason);
    if (
      !(await ports.repository.preservationNotice({holdId:holdId,id:id,actorId:a.userId}))
    )
      fail("This preservation notice is not assigned to you.");
    steps.push(
      ports.repository.acknowledgeNotice({timestamp:ports.clock.now(),note:note,holdId:holdId,id:id,actorId:a.userId}),
    );
  } else if (b.action === "hold.create") {
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    const hold = ports.identifiers.next();
    steps.push(
      ports.repository.createHold({hold:hold,id:id,note:note,actorId:a.userId,timestamp:ports.clock.now()}),
      ports.repository.includeHoldTarget({hold:hold,id:id}),
    );
  } else if (
    b.action === "hold.request-release" ||
    b.action === "hold.release"
  ) {
    const h = await ports.repository.hold({requestedId:z.string().parse(input.holdId),id:id});
    if (!h) fail("Hold not found.");
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    if (b.action === "hold.request-release") {
      if (h.state !== "active") fail("Hold is not active.");
      steps.push(
        ports.repository.requestHoldRelease({actorId:a.userId,holdId:h.id}),
      );
    } else {
      if (
        h.state !== "release_requested" ||
        ports.samePerson(h.release_requested_by) ||
        ports.samePerson(h.created_by)
      )
        fail(
          "An independent reviewer must release this hold.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        ports.repository.releaseHold({actorId:a.userId,timestamp:ports.clock.now(),holdId:h.id}),
      );
    }
  } else if (b.action === "share") {
    const x = z
      .object({
        documentId: z.string(),
        email: z.email(),
        expiresAt: z
          .number()
          .int()
          .min(ports.clock.now() + 60000)
          .max(ports.clock.now() + 90 * 86400000),
      })
      .parse(input);
    if (
      !(await ports.repository.shareDocumentBelongs({id:id,documentId:x.documentId}))
    )
      throw ports.error(422, "LEGAL_INVALID", "Select a document from this record.");
    const shareId = ports.identifiers.next();
    steps.push(
      ports.repository.shareDocument({shareId:shareId,id:id,documentId:x.documentId,value3:x.email.toLowerCase(),expiresAt:x.expiresAt,actorId:a.userId,timestamp:ports.clock.now()}),
      ports.repository.queueInvitation({identifier:ports.identifiers.next(),shareId:shareId,holdingSlug:a.holdingSlug,timestamp:ports.clock.now()}),
    );
    note = `Shared selected version with ${x.email}`;
  } else if (b.action === "share.revoke") {
    const share = z.string().parse(input.shareId);
    steps.push(
      ports.repository.revokeShare({timestamp:ports.clock.now(),share:share,id:id}),
    );
    note = "External access revoked";
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
