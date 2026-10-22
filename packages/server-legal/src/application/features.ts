import {z} from "zod";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import type {LegalWrite,LegalRows} from "@erpx/server-legal/application/service";
import {featureSpecs,featureKinds,parseFeature,privacyDecisionError,type FeatureRow,type FeatureSpec} from "@erpx/feature-legal/lib/legal/features-domain";
import type {LegalItem} from "@erpx/server-legal/domain/model";
export type LegalFeaturesRepository={
featureCount(facts:{id:string;search:string;search2:string;capability:(typeof featureKinds)[number];}):Promise<Record<string, unknown> | null>;
features(facts:{id:string;search:string;search2:string;fact3:number;capability:(typeof featureKinds)[number];}):Promise<LegalRows<FeatureRow>>;
feature(facts:{featureId:string;itemId:string;capability:(typeof featureKinds)[number];}):Promise<FeatureRow | null>;
ownerMembership(facts:{itemId:string;ownerId:string;}):Promise<Record<string, unknown> | null>;
attachedDocument(facts:{documentId:string;itemId:string;}):Promise<Record<string, unknown> | null>;
createFeature(facts:{id:string;itemId:string;category:"case_event" | "compliance_action" | "privacy_case" | "corporate_action" | "ip_action" | "counsel_entry" | "schedule" | "custody" | "policy_ack" | "intake";title:string;owner_id:string;due_date:string | null;state:string;dataJson:string;actorId:string;actorId2:string;updated_at:number;capability:(typeof featureKinds)[number];}):LegalWrite;
conflictCleared(facts:{itemId:string;}):Promise<Record<string, unknown> | null>;
approvedFees(facts:{itemId:string;id:string;}):Promise<{ amount: number; } | null>;
updateFeature(facts:{state:string;dataJson:string;actorId:string;updated_at:number;id:string;version:number;capability:(typeof featureKinds)[number];}):LegalWrite;
recordFeatureHistory(facts:{identifier:string;itemId:string;id:string;category:"case_event" | "compliance_action" | "privacy_case" | "corporate_action" | "ip_action" | "counsel_entry" | "schedule" | "custody" | "policy_ack" | "intake";version:number;snapshotJson:string;actorId:string;reason:string;now:number;}):LegalWrite;
advanceRecord(facts:{now:number;itemId:string;}):LegalWrite;
guardChanges(id:string,version:number,key:string,edit?:boolean):LegalWrite[];
companyLock():LegalWrite;
personGuard(key:string,userId:string,scope?:LegalItem,permission?:string):LegalWrite;
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalFeaturesApplication(ports:{actor:WorkContext;repository:LegalFeaturesRepository;clock:Clock;identifiers:IdentifierSource;samePerson(userId:string|null|undefined):boolean;require(permission:string):void;access(id:string,permission?:string):Promise<LegalItem>;eligible(id:string,item?:LegalItem,permission?:string):Promise<void>;error(status:number,code:string,message:string):Error}){const a=ports.actor;
function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function featureDetail(
  id: string,
  query: Record<string, unknown> = {},
) {
  const item = await ports.access(id);
  const q = z
    .object({
      category: z.enum(featureKinds).optional(),
      page: z.coerce.number().int().min(1).max(10000).default(1),
      search: z.string().max(200).default(""),
    })
    .parse(query);
  const groups = await Promise.all(
    (Object.entries(featureSpecs) as [(typeof featureKinds)[number], FeatureSpec][])
      .filter(
        ([category, s]) =>
          (s.kinds as string[]).includes(item.kind) &&
          (!q.category || q.category === category),
      )
      .map(async ([category]) => ({
        category,
        page: q.page,
        total: Number(
          await ports.repository.featureCount({id:id,search:q.search,search2:q.search,capability:category}),
        ),
        rows: (
          await ports.repository.features({id:id,search:q.search,search2:q.search,fact3:(q.page - 1) * 50,capability:category})
        ).results.map((r) => ({
          ...r,
          data: typeof r.data === "string" ? JSON.parse(r.data) : r.data,
        })),
      })),
  );
  return groups;
}
async function featureCommand(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "feature.create",
        "feature.update",
        "feature.transition",
      ]),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      category: z.enum(featureKinds),
      featureId: z.string().optional(),
      featureVersion: z.number().int().positive().optional(),
      state: z.string().optional(),
      reason: z.string().max(4000).default(""),
      data: z.record(z.string(), z.string().max(12000)).optional(),
    })
    .parse(raw);
  const spec: FeatureSpec = featureSpecs[x.category],
    item = await ports.access(x.id);
  if(a.role==="legal_requester")fail("Reply to the Legal team through the request discussion.","LEGAL_REQUESTER_SCOPE",403);
  const prior = await ports.repository.replay<{ id: string; version: number }>(key,x.action,item.id,raw);
  if (prior) return prior;
  if (!(spec.kinds as string[]).includes(item.kind))
    fail("This workflow does not belong to this register.");
  const row = x.featureId
    ? await ports.repository.feature({featureId:x.featureId,itemId:item.id,capability:x.category})
    : null;
  if (row && typeof row.data === "string") row.data = JSON.parse(row.data);
  const ownerResponse =
    !!row &&
    ports.samePerson(row.owner_id) &&
    ["intake", "policy_ack"].includes(x.category) &&
    x.action !== "feature.create";
  if (!ownerResponse) await ports.access(item.id,"legal.edit");
  const steps: LegalWrite[] = ports.repository.guardChanges(item.id,x.expectedVersion,key,!ownerResponse);
  let snapshot: FeatureRow;
  if (x.action === "feature.create") {
    const input = parseFeature(raw,ports.clock.now());
    await ports.eligible(input.ownerId,item);
    steps.push(ports.repository.personGuard(key,input.ownerId,item));
    if (
      !(await ports.repository.ownerMembership({itemId:item.id,ownerId:input.ownerId}))
    )
      fail("Grant explicit record access to the workflow owner first.");
    snapshot = {
      id: ports.identifiers.next(),
      item_id: item.id,
      category: x.category,
      title: input.title,
      owner_id: input.ownerId,
      due_date: input.dueDate,
      state: spec.initial,
      version: 1,
      data: input.data,
      created_by: a.userId,
      updated_by: a.userId,
      updated_at: ports.clock.now(),
    };
    if (
      ["custody", "policy_ack"].includes(x.category) &&
      !(await ports.repository.attachedDocument({documentId:input.data.documentId,itemId:item.id}))
    )
      fail("Choose an immutable document version on this record.");
    if (
      x.category === "counsel_entry" &&
      !["conflict", "time", "fee", "invoice"].includes(input.data.entryType)
    )
      fail("Choose conflict, time, fee or invoice.");
    if (
      x.category === "privacy_case" &&
      !["subject_request", "breach", "processing"].includes(input.data.caseType)
    )
      fail("Choose subject_request, breach or processing.");
    steps.push(
      ports.repository.createFeature({id:snapshot.id,itemId:item.id,category:x.category,title:snapshot.title,owner_id:snapshot.owner_id,due_date:snapshot.due_date,state:snapshot.state,dataJson:JSON.stringify(snapshot.data),actorId:a.userId,actorId2:a.userId,updated_at:snapshot.updated_at,capability:x.category}),
    );
  } else {
    if (!row || row.version !== x.featureVersion)
      fail(
        "Workflow changed. Refresh before saving.",
        "LEGAL_VERSION_CONFLICT",
        409,
      );
    const data = { ...row.data, ...x.data };
    parseFeature({
      category: x.category,
      title: row.title,
      ownerId: row.owner_id,
      dueDate: row.due_date,
      data,
    },ports.clock.now());
    if (
      ownerResponse &&
      x.data &&
      Object.keys(x.data).some(
        (k) =>
          !["response", "evidence"].includes(k) && x.data![k] !== row.data[k],
      )
    )
      fail("Only the response and acknowledgement may be changed.");
    if (
      x.action === "feature.update" &&
      ![
        "draft",
        "planned",
        "preparing",
        "open",
        "remediation",
        "in_progress",
        "received",
        "awaiting_information",
        "assessed",
        "requested",
        "pending",
      ].includes(row.state)
    )
      fail("Return this workflow for changes before editing it.");
    const state =
      x.action === "feature.transition" ? z.string().parse(x.state) : row.state;
    if (x.action === "feature.transition") {
      if (!spec.states[row.state]?.includes(state))
        fail(
          "This workflow transition is unavailable.",
          "LEGAL_TRANSITION",
          409,
        );
      if (spec.evidenceStates.includes(state) && !data.evidence?.trim())
        fail("Evidence is required for this decision.");
      if (
        x.category === "intake" &&
        state === "responded" &&
        !data.response?.trim()
      )
        fail("A response is required.");
      if (x.category === "policy_ack" && !ports.samePerson(row.owner_id))
        fail("Only the assigned recipient can acknowledge this policy.");
      if (spec.independentStates.includes(state)) {
        ports.require("legal.approve");
        if (ports.samePerson(row.created_by) || ports.samePerson(row.updated_by))
          fail(
            "An independent reviewer must decide.",
            "LEGAL_INDEPENDENT_REVIEW",
            403,
          );
        if (x.data && Object.entries(x.data).some(([field, value]) => field !== "evidence" && value !== (row.data[field] ?? "")))
          fail("Return substantive changes for a new independent review.", "LEGAL_REVIEW_CONTENT_CHANGED", 409);
      }
      if (x.category === "privacy_case") {
        const error = privacyDecisionError(data, state);
        if (error) fail(error, "LEGAL_PRIVACY_REVIEW_REQUIRED", 409);
      }
      if (
        x.category === "counsel_entry" &&
        data.entryType !== "conflict" &&
        state === "approved"
      ) {
        if (
          !(await ports.repository.conflictCleared({itemId:item.id}))
        )
          fail("Clear the conflict review before approving fees or time.");
        const total = await ports.repository.approvedFees({itemId:item.id,id:row.id});
        if (
          ["fee", "invoice"].includes(data.entryType) &&
          BigInt(total?.amount ?? 0) + BigInt(data.amount || 0) >
            BigInt(String(item.data.budget || 0))
        )
          fail(
            "Approved fees would exceed the counsel budget. Update and review the budget first.",
          );
      }
    }
    if (x.category === "privacy_case" && row.data.knownAt && data.knownAt !== row.data.knownAt && !x.reason.trim())
      fail("Explain the correction to the known event time.", "LEGAL_PRIVACY_CORRECTION_REQUIRED", 409);
    snapshot = {
      ...row,
      state,
      data,
      version: row.version + 1,
      updated_by: a.userId,
      updated_at: ports.clock.now(),
    };
    steps.push(
      ports.repository.updateFeature({state:state,dataJson:JSON.stringify(data),actorId:a.userId,updated_at:snapshot.updated_at,id:row.id,version:row.version,capability:x.category}),
    );
  }
  steps.push(
    ports.repository.recordFeatureHistory({identifier:ports.identifiers.next(),itemId:item.id,id:snapshot.id,category:x.category,version:snapshot.version,snapshotJson:JSON.stringify(snapshot),actorId:a.userId,reason:x.reason,now:ports.clock.now()}),
    ports.repository.advanceRecord({now:ports.clock.now(),itemId:item.id}),
    ports.repository.event(item.id,x.action,`${spec.label}: ${snapshot.title} → ${snapshot.state}`,item.version + 1,ports.identifiers.next(),ports.clock.now()),
  );
  return ports.repository.commit(key,x.action,raw,item.id,item.version + 1,{ id: item.id, version: item.version + 1 },steps);
}

return {featureDetail,featureCommand};}
