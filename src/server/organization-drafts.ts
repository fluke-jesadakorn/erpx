import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import { draftDefinitionSchema, draftKey, emptyDraft, projectOrganization, changedPeople, organizationDraftDiff, draftGraphBlockers, personPlacement, positionPlacement, type DraftDefinition, type DraftItem, type OrganizationDraft, type DraftRequest } from "@/lib/organization-draft";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import { organizationAccessData } from "./organization-access";
import { previewOrganizationNodeChange, saveOrganizationNodeChange, organizationNodeChangeSchema } from "./organization";
import { executeCommand, replayCommand } from "./commands";
import { recommendationStatements } from "./staff-defaults";
import { cancelLifecycle } from "./people-lifecycle";
import { readGpoRules, gpoRuleMatchesPosition } from "./gpo-rules";
import { positionMoveDate } from "@/lib/position-moves";

const decode = <T,>(value:T|string):T => typeof value === "string" ? JSON.parse(value) as T : value;
const canonical = (value:unknown):unknown => Array.isArray(value) ? value.map(canonical) : value && typeof value === "object" ? Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>[k,canonical(v)])) : value;
const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(canonical(value))).digest("hex");
const fail = (message: string, code = "ORGANIZATION_DRAFT_CHANGED", status = 409): never => { throw new ApiError(status, code, message); };
export function draftAuthority(actor: WorkContext) {
  requirePermission(actor, "people.manage"); requirePermission(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId) fail("Select a Company workspace with HR authority.", "COMPANY_AUTHORITY_REQUIRED", 403);
}
type Stored = OrganizationDraft & { company_id: string; owner_id: string; base_snapshot:Record<string,unknown> };
function sourceSnapshot(data:OrganizationAccessData,item:DraftItem):unknown {
 const person=(p:OrganizationAccessData['people'][number])=>({id:p.engagement_id,...personPlacement(p),status:p.status,personStatus:p.person_status,startDate:p.start_date,endDate:p.end_date});
 if(item.kind==='person'){const p=data.people.find(p=>p.engagement_id===item.id);return p?person(p):null;}
 const positions=data.positions.filter(p=>item.kind==='position'?p.id===item.id:p.department_id===item.id).map(p=>({id:p.id,...positionPlacement(p),active:p.active,version:p.version})).sort((a,b)=>a.id.localeCompare(b.id));
 const people=data.people.filter(p=>item.kind==='position'?p.position_id===item.id:p.department_id===item.id).map(person).sort((a,b)=>a.id.localeCompare(b.id));
 const node=item.kind==='department'?data.nodes.find(n=>n.id===item.id):undefined;
 return {positions,people,...(node?{node:{id:node.id,branchId:node.branch_id,status:node.status,version:node.hierarchy_revision}}:{})};
}
export async function getOrganizationDraft(db: SqlDatabase, actor: WorkContext, id?: string) {
  draftAuthority(actor);
  const draft = await db.prepare(`SELECT * FROM organization_drafts WHERE company_id=? AND owner_id=? AND ${id ? "id=?" : "state='draft'"}`)
    .bind(actor.companyId, actor.userId, ...(id ? [id] : [])).first<Stored>();
  if(draft){draft.definition=decode(draft.definition);draft.base_snapshot=decode(draft.base_snapshot);}
  if (id && !draft) fail("Draft not found.", "DRAFT_NOT_FOUND", 404);
  const requests = draft ? (await db.prepare("SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=? ORDER BY created_at,id").bind(draft.id).all<DraftRequest>()).results : [];
  const recent=(await db.prepare("SELECT id,updated_at,jsonb_array_length(definition->'items') count FROM organization_drafts WHERE company_id=? AND owner_id=? AND state='submitted' ORDER BY updated_at DESC LIMIT 20").bind(actor.companyId,actor.userId).all<{id:string;updated_at:number;count:number}>()).results;
  return { draft, requests:requests.map(r=>({...r,item_keys:decode(r.item_keys)})),recent };
}
export async function openOrganizationDraft(db: PostgresDatabase, actor: WorkContext) {
  draftAuthority(actor);
  return db.transaction(async tx => {
    const now = Date.now();
    await tx.prepare("INSERT INTO organization_drafts(id,company_id,owner_id,definition,created_at,updated_at) VALUES(?,?,?,?::jsonb,?,?) ON CONFLICT(company_id,owner_id) WHERE state='draft' DO NOTHING")
      .bind(`org-draft-${randomUUID()}`, actor.companyId, actor.userId, JSON.stringify(emptyDraft()), now, now).run();
    return getOrganizationDraft(tx, actor);
  });
}
export async function saveOrganizationDraft(db: PostgresDatabase, actor: WorkContext, id: string, expectedVersion: number, definition: DraftDefinition, discard = false, rebase = false) {
  draftAuthority(actor);
  return db.transaction(async tx => {
    const { draft } = await getOrganizationDraft(tx, actor, id);
    if (draft!.state !== "draft" || draft!.version !== expectedVersion) fail("This draft changed in another tab. Reload the saved draft before editing.", "DRAFT_VERSION_CHANGED");
    const data=await organizationAccessData(tx,actor);
    const baseSnapshot=Object.fromEntries(definition.items.map(item=>[draftKey(item),!rebase&&Object.hasOwn(draft!.base_snapshot,draftKey(item))?draft!.base_snapshot[draftKey(item)]:sourceSnapshot(data,item)]));
    const updated = await tx.prepare("UPDATE organization_drafts SET definition=?::jsonb,base_snapshot=?::jsonb,state=?,version=version+1,updated_at=? WHERE id=? AND version=? RETURNING *")
      .bind(JSON.stringify(definition),JSON.stringify(baseSnapshot), discard ? "discarded" : "draft", Date.now(), id, expectedVersion).first<Stored>();
    if(updated){updated.definition=decode(updated.definition);updated.base_snapshot=decode(updated.base_snapshot);}
    if (!updated) fail("This draft changed. Reload before editing.");
    return { draft: updated, requests: [] };
  });
}
function relocation(data: OrganizationAccessData, definition: DraftDefinition, item: DraftItem) {
  const node = data.nodes.find(n => n.id === item.id)!;
  return organizationNodeChangeSchema.parse({ action: "relocate", targetNodeId: item.id, kind: "department", displayName: node.display_name, branchId: item.branchId, expectedNodeVersion: node.hierarchy_revision, expectedVersion: 0, reason: definition.reason });
}
function relevant(data: OrganizationAccessData, definition: DraftDefinition) {
  const next = projectOrganization(data, definition), people = new Set(changedPeople(data, next).map(p => p.engagement_id));
  const positions = new Set<string>(); const departments = new Set<string>();
  for (const item of definition.items) {
    if (item.kind === "person") { people.add(item.id); if (item.positionId) positions.add(item.positionId); const p = data.people.find(p => p.engagement_id === item.id); if (p?.position_id) positions.add(p.position_id); }
    if (item.kind === "position") positions.add(item.id);
    if (item.kind === "department") departments.add(item.id);
    if (item.departmentId) departments.add(item.departmentId);
  }
  for (const p of data.positions) if (departments.has(p.department_id ?? "")) positions.add(p.id);
  for (const p of data.people) if (positions.has(p.position_id ?? "") || departments.has(p.department_id ?? "")) people.add(p.engagement_id);
  return { people, positions, departments };
}
// Only placement/authorization inputs enter durable snapshots; no profile or pay data.
function snapshot(data: OrganizationAccessData, definition: DraftDefinition) {
  const keys = relevant(data, definition), next = projectOrganization(data, definition);
  const people = new Set(keys.people), positions = new Set(keys.positions);
  for (let count = 0; count < data.people.length; count++) {
    const size = people.size;
    for (const p of [...data.people, ...next.people]) if (people.has(p.engagement_id) && p.manager_person_id) for (const manager of data.people.filter(m => m.person_id === p.manager_person_id)) people.add(manager.engagement_id);
    if (people.size === size) break;
  }
  for (const p of data.positions) if (positions.has(p.id) && p.reports_to_position_id) positions.add(p.reports_to_position_id);
  const nodeIds = new Set([...keys.departments,...data.nodes.filter(n=>n.kind==="branch").map(n=>n.id)]);
  for (const p of data.positions.filter(p => positions.has(p.id))) { if (p.department_id) nodeIds.add(p.department_id); if (p.branch_id) nodeIds.add(p.branch_id); }
  for (const item of definition.items) if (item.branchId) nodeIds.add(item.branchId);
  return {
    people: data.people.filter(p => people.has(p.engagement_id)).map(p => ({ id: p.engagement_id, personId: p.person_id, ...personPlacement(p), status: p.status, personStatus: p.person_status, startDate: p.start_date, endDate: p.end_date })).sort((a,b) => a.id.localeCompare(b.id)),
    positions: data.positions.filter(p => positions.has(p.id)).map(p => ({ id: p.id, ...positionPlacement(p), active: p.active, version: p.version, title: p.title, isCeo: p.is_ceo })).sort((a,b) => a.id.localeCompare(b.id)),
    nodes: data.nodes.filter(n => nodeIds.has(n.id)).map(n => ({ id:n.id, branchId:n.branch_id, status:n.status, version:n.hierarchy_revision, slug:n.slug })).sort((a,b) => a.id.localeCompare(b.id)),
    members: data.people.filter(p => keys.people.has(p.engagement_id)).map(p => p.engagement_id).sort(),
  };
}
type Snapshot = ReturnType<typeof snapshot>;
type Mapping = { oldId: string; newId: string; branchId: string };
function remapDefinition(definition: DraftDefinition, mappings: Mapping[]) {
  return { ...definition, items: definition.items.map(item => ({ ...item, ...(item.departmentId ? { departmentId: mappings.find(m => m.oldId === item.departmentId)?.newId ?? item.departmentId } : {}) })) };
}
function remapSnapshot(saved: Snapshot, mappings: Mapping[]): Snapshot {
  return { ...saved,
    people: saved.people.map(p => { const m = mappings.find(m => m.oldId === p.departmentId); return m ? { ...p, departmentId:m.newId, branchId:m.branchId } : p; }),
    positions: saved.positions.map(p => { const m = mappings.find(m => m.oldId === p.departmentId); return m ? { ...p, departmentId:m.newId, branchId:m.branchId } : p; }),
    nodes: saved.nodes.map(n => { const m = mappings.find(m => m.oldId === n.id); return m ? { ...n, id:m.newId, branchId:m.branchId, status:"active", version:1 } : n; }).sort((a,b) => a.id.localeCompare(b.id)),
  };
}
async function policiesAndLeave(db: SqlDatabase, actor: WorkContext, definition:DraftDefinition, data:OrganizationAccessData) {
  const policies = (await db.prepare("SELECT id,target_kind,target_id,version,revision,active FROM organization_gpo_rules WHERE company_id=? ORDER BY id").bind(actor.companyId).all()).results;
  const leave = (await db.prepare("SELECT level_id,version,policy_version_id FROM level_leave_defaults WHERE company_id=? ORDER BY level_id,version").bind(actor.companyId).all()).results;
  const keys=relevant(data,definition);
  const assignments=(await db.prepare("SELECT engagement_id,id,policy_version_id,effective_from,effective_to,version FROM leave_policy_assignments WHERE company_id=? ORDER BY engagement_id,id").bind(actor.companyId).all<{engagement_id:string}>()).results.filter(row=>keys.people.has(row.engagement_id)&&(definition.leave[row.engagement_id]??definition.items.find(i=>i.kind==='person'&&i.id===row.engagement_id)?.acceptRecommendedLeave??false));
  return { policies, leave,assignments };
}
async function prepare(db: SqlDatabase, actor: WorkContext, definition: DraftDefinition) {
  const data = await organizationAccessData(db, actor), projected = projectOrganization(data, definition);
  const diff = organizationDraftDiff(data, definition), blockers = [...diff.flatMap(d => d.blockers.map(b => `${d.label}: ${b}`)), ...draftGraphBlockers(data, definition)];
  if (!definition.items.length) blockers.push("Add at least one change.");
  if (definition.reason.trim().length < 3) blockers.push("Enter a reason of at least three characters.");
  const keys = relevant(data, definition), people = changedPeople(data, projected);
  if(people.some(person=>data.people.filter(other=>other.person_id===person.person_id).length!==1))blockers.push("Each affected person must have one current engagement.");
  const options = Object.keys(definition.managers).concat(Object.keys(definition.leave));
  if (options.some(id => !keys.people.has(id))) blockers.push("Manager and leave choices must belong to affected employees.");
  const legacy: { engagementId: string; id: string; version: number; status: string }[] = [];
  const cases = (await db.prepare("SELECT id,engagement_id,person_id,workflow,status,version,definition FROM people_lifecycle_cases WHERE company_id=? AND status IN ('draft','waiting_it','pending_review','scheduled')").bind(actor.companyId).all<{id:string;engagement_id:string;person_id:string;workflow:string;status:string;version:number;definition:{positionId?:string}}>()).results;
  for (const c of cases.map(c=>({...c,definition:decode(c.definition)}))) if (keys.people.has(c.engagement_id) || keys.positions.has(c.definition.positionId ?? "")) {
    const item = definition.items.find(i => i.kind === "person" && i.id === c.engagement_id);
    if (c.workflow === "legacy" && item) {
      legacy.push({ engagementId:c.engagement_id, id:c.id, version:c.version, status:c.status });
      if (item.supersedeLegacyCase?.id !== c.id || item.supersedeLegacyCase.expectedVersion !== c.version) blockers.push("Confirm replacement of the older access request.");
    } else if (c.status !== "draft") blockers.push("An affected employee or position already has an open HR request.");
  }
  const pending = (await db.prepare("SELECT position_id FROM organization_position_moves WHERE company_id=? AND status='scheduled'").bind(actor.companyId).all<{position_id:string}>()).results;
  if (pending.some(p => keys.positions.has(p.position_id))) blockers.push("An affected position already has a scheduled move.");
  const reserved = (await db.prepare("SELECT kind,entity_id FROM organization_draft_reservations WHERE company_id=?").bind(actor.companyId).all<{kind:string;entity_id:string}>()).results;
  if (reserved.some(r => r.kind === "person" ? keys.people.has(r.entity_id) : keys.positions.has(r.entity_id))) blockers.push("An affected item belongs to another confirmed draft.");
  const relocations = new Map<string, Awaited<ReturnType<typeof previewOrganizationNodeChange>>>();
  for (const item of definition.items.filter(i => i.kind === "department" && !i.parked)) {
    if (!data.nodes.some(n => n.id === item.id)) continue;
    try { relocations.set(item.id, await previewOrganizationNodeChange(db, actor, relocation(data, definition, item))); }
    catch (error) { if (!(error instanceof ApiError)) throw error; blockers.push(error.message); }
  }
  const leaveImpact: { engagementId: string; note: string }[] = [];
  for (const person of people) {
    const original = data.people.find(p => p.engagement_id === person.engagement_id)!;
    const takeLeave = definition.leave[person.engagement_id] ?? definition.items.find(i => i.kind === "person" && i.id === person.engagement_id)?.acceptRecommendedLeave ?? false;
    try {
      if(takeLeave&&!await db.prepare("SELECT 1 FROM level_leave_defaults WHERE company_id=? AND level_id=?").bind(actor.companyId,person.level_id).first())blockers.push(`${person.legal_name}: this Level has no leave default.`);
      const statements = await recommendationStatements(db, actor, { engagementId:person.engagement_id, positionId:person.position_id, levelId:person.level_id, priorLevelId:original.level_id, startDate:positionMoveDate(definition.effectiveAt,definition.timezone), currency:"", acceptPay:false, acceptLeave:takeLeave, commandId:"preview-only" });
      leaveImpact.push({ engagementId:person.engagement_id, note:statements.length ? "Apply Level leave default under existing effective-date rules." : "Existing leave remains." });
    } catch (error) { if (!(error instanceof ApiError)) throw error; blockers.push(`${person.legal_name}: ${error.message}`); }
  }
  const rules = await policiesAndLeave(db, actor,definition,data);
  const gpoRules=await readGpoRules(db,actor.companyId), beforePolicies=new Set<string>(), afterPolicies=new Set<string>();
  for(const person of people) {
    const before=data.people.find(p=>p.engagement_id===person.engagement_id)!;
    const from=data.positions.find(p=>p.id===before.position_id);let to=projected.positions.find(p=>p.id===person.position_id);if(to&&definition.items.some(i=>i.kind==='department'&&i.id===to!.department_id))to={...to,department_id:`relocated:${to.department_id}`};
    for(const rule of gpoRules){if(from&&gpoRuleMatchesPosition(rule,from))beforePolicies.add(`${person.engagement_id}:${rule.id}`);if(to&&gpoRuleMatchesPosition(rule,to))afterPolicies.add(`${person.engagement_id}:${rule.id}`);}
  }
  const policyImpact={added:[...afterPolicies].filter(k=>!beforePolicies.has(k)).length,removed:[...beforePolicies].filter(k=>!afterPolicies.has(k)).length,retained:[...beforePolicies].filter(k=>afterPolicies.has(k)).length};
  const token = hash({ definition, snapshot:snapshot(data,definition), rules, legacy, relocations:[...relocations] });
  return { data, projected, diff, policyImpact, blockers:[...new Set(blockers)], legacy, leaveImpact, affectedPeople:people.map(p => ({ id:p.engagement_id, name:p.legal_name, before:personPlacement(data.people.find(old => old.engagement_id === p.engagement_id)!), after:personPlacement(p), direct:definition.items.some(i => i.kind === "person" && i.id === p.engagement_id) })), token, relocations, rules };
}
export async function previewOrganizationDraft(db: PostgresDatabase, actor: WorkContext, id: string, version: number) {
  draftAuthority(actor);
  return db.transaction(async tx => {
    const { draft } = await getOrganizationDraft(tx,actor,id);
    if (draft!.state !== "draft" || draft!.version !== version) fail("Reload the latest draft.","DRAFT_VERSION_CHANGED");
    const p = await prepare(tx,actor,draft!.definition);
    const staleSources=draft!.definition.items.filter(item=>Object.hasOwn(draft!.base_snapshot,draftKey(item))&&hash(draft!.base_snapshot[draftKey(item)])!==hash(sourceSnapshot(p.data,item))).map(draftKey);
    if(staleSources.length)p.blockers.push("Source records changed since this draft was arranged. Load the current organization and review again.");
    return { staleSources, ready:!p.blockers.length, token:p.token, version, diff:p.diff, blockers:p.blockers, legacy:p.legacy, affectedPeople:p.affectedPeople, leaveImpact:p.leaveImpact,policyImpact:p.policyImpact };
  });
}
export type OrganizationDraftPreview = Awaited<ReturnType<typeof previewOrganizationDraft>>;

// Connected placement changes are applied in one transaction, including people moved out of a moving position.
function components(data: OrganizationAccessData, definition: DraftDefinition) {
  const groups = definition.items.filter(i => i.kind !== "department").map(item => [item]);
  let merged = true;
  while (merged) {
    merged = false;
    outer: for (let a=0; a<groups.length; a++) for (let b=a+1; b<groups.length; b++) {
      const left = relevant(data,{...definition,items:groups[a]}), right = relevant(data,{...definition,items:groups[b]});
      if ([...left.people].some(id => right.people.has(id)) || [...left.positions].some(id => right.positions.has(id)) || data.people.some(person => { const manager = definition.managers[person.engagement_id] ?? person.manager_person_id; return (left.people.has(person.engagement_id) && data.people.some(m=>m.person_id===manager&&right.people.has(m.engagement_id))) || (right.people.has(person.engagement_id) && data.people.some(m=>m.person_id===manager&&left.people.has(m.engagement_id))); })) { groups[a].push(...groups[b]); groups.splice(b,1); merged=true; break outer; }
    }
  }
  return groups;
}
export async function submitOrganizationDraft(db: PostgresDatabase, actor: WorkContext, id: string, version: number, token: string, key: string) {
  draftAuthority(actor);
  const payload = { version,token }, operation = "organization.draft.submit";
  return db.transaction(async tx => {
    const prior = await replayCommand(tx,actor,key,operation,id,payload); if (prior) return prior;
    const { draft } = await getOrganizationDraft(tx,actor,id);
    if (draft!.state !== "draft" || draft!.version !== version) fail("Reload the latest draft.","DRAFT_VERSION_CHANGED");
    const def = draft!.definition, p = await prepare(tx,actor,def);
    if(def.items.some(item=>Object.hasOwn(draft!.base_snapshot,draftKey(item))&&hash(draft!.base_snapshot[draftKey(item)])!==hash(sourceSnapshot(p.data,item))))fail("Source records changed since the draft was arranged. Review the current organization.");
    if (token !== p.token || p.blockers.length) fail(p.blockers.join(" ") || "The organization changed. Review the draft again.");
    if (def.effectiveAt < Date.now()-60000) fail("Choose a current or future effective time.");
    const now=Date.now(), savedActor={...actor,authority:undefined,availableAssignments:undefined,assignments:[],workspaces:[]};
    for (const item of def.items) if (item.supersedeLegacyCase) await cancelLifecycle(tx,actor,item.supersedeLegacyCase.id,item.supersedeLegacyCase.expectedVersion,`draft-cancel-${hash([id,item.id]).slice(0,32)}`);
    const departments: { oldId:string; branchId:string; changeId:string }[]=[];
    for (const item of def.items.filter(i => i.kind === "department")) {
      const childKey=`draft-dept-${hash([id,item.id]).slice(0,32)}`;
      const change = await saveOrganizationNodeChange(tx,actor,null,{...relocation(p.data,def,item),previewSnapshot:p.relocations.get(item.id)!.previewSnapshot},childKey,true);
      departments.push({oldId:item.id,branchId:item.branchId!,changeId:String(change.id)});
      await tx.prepare("INSERT INTO organization_draft_requests(id,draft_id,company_id,item_keys,definition,baseline,dependencies,change_id,state,actor_json,effective_at,created_at,updated_at) VALUES(?,?,?,?::jsonb,?::jsonb,'{}','[]',?,'waiting_approval',?::jsonb,?,?,?)")
        .bind(`draft-request-${hash([id,item.id]).slice(0,32)}`,id,actor.companyId,JSON.stringify([draftKey(item)]),JSON.stringify({...def,items:[item]}),change.id,JSON.stringify(savedActor),def.effectiveAt,now,now).run();
    }
    for (const items of components(p.data,def)) {
      const group={...def,items}; const keys=relevant(p.data,group);
      group.managers=Object.fromEntries(Object.entries(def.managers).filter(([id]) => keys.people.has(id)));
      group.leave=Object.fromEntries(Object.entries(def.leave).filter(([id]) => keys.people.has(id)));
      const deps=departments.filter(d => keys.departments.has(d.oldId) || p.data.people.some(person => keys.people.has(person.engagement_id) && person.department_id===d.oldId) || p.data.positions.some(pos => keys.positions.has(pos.id) && pos.department_id===d.oldId));
      const requestId=`draft-request-${hash([id,items.map(draftKey)]).slice(0,32)}`;
      await tx.prepare("INSERT INTO organization_draft_requests(id,draft_id,company_id,item_keys,definition,baseline,dependencies,state,actor_json,effective_at,created_at,updated_at) VALUES(?,?,?,?::jsonb,?::jsonb,?::jsonb,?::jsonb,?,?::jsonb,?,?,?)")
        .bind(requestId,id,actor.companyId,JSON.stringify(items.map(draftKey)),JSON.stringify(group),JSON.stringify({snapshot:snapshot(p.data,group),rules:await policiesAndLeave(tx,actor,group,p.data)}),JSON.stringify(deps),deps.length?"waiting_dependency":"scheduled",JSON.stringify(savedActor),def.effectiveAt,now,now).run();
      for (const [kind,ids] of [["person",keys.people],["position",keys.positions]] as const) for (const entity of ids) await tx.prepare("INSERT INTO organization_draft_reservations(company_id,kind,entity_id,request_id) VALUES(?,?,?,?)").bind(actor.companyId,kind,entity,requestId).run();
    }
    return executeCommand(tx,actor,key,operation,payload,id,version+1,{id,version:version+1,state:"submitted"},[
      tx.prepare("UPDATE organization_drafts SET state='submitted',version=version+1,updated_at=? WHERE id=? AND state='draft' AND version=?").bind(now,id,version),
    ]);
  });
}

export async function organizationDraftApi(db: PostgresDatabase, actor: WorkContext, path: string[], body?: unknown, key?: string) {
  if (!body) return getOrganizationDraft(db,actor,path[0]);
  if (!path.length) return openOrganizationDraft(db,actor);
  const action=path[1], id=path[0];
  if (action === "save" || action === "discard" || action === "rebase") { const input=z.object({expectedVersion:z.number().int().positive(),definition:draftDefinitionSchema}).strict().parse(body); return saveOrganizationDraft(db,actor,id,input.expectedVersion,input.definition,action==="discard",action==="rebase"); }
  if (action === "preview") { const input=z.object({expectedVersion:z.number().int().positive()}).strict().parse(body); return previewOrganizationDraft(db,actor,id,input.expectedVersion); }
  if (action === "submit") { const input=z.object({expectedVersion:z.number().int().positive(),previewToken:z.string().min(1)}).strict().parse(body); return submitOrganizationDraft(db,actor,id,input.expectedVersion,input.previewToken,key!); }
  if (action === "continue") {
    const {draft,requests}=await getOrganizationDraft(db,actor,id);
    if(draft!.state!=="submitted") fail("Only confirmed drafts can be continued.");
    const unresolved=new Set(requests.filter(r=>r.state==="needs_review").flatMap(r=>r.item_keys));
    if(!unresolved.size) fail("There are no changes needing review.");
    const opened=await openOrganizationDraft(db,actor);
    if(opened.draft!.definition.items.length) fail("Finish or discard your existing draft first.");
    const mappings:Mapping[]=[];
    for(const request of requests.filter(r=>r.state==='applied'&&r.change_id)) {
      const item=draft!.definition.items.find(i=>request.item_keys.includes(draftKey(i)));
      const event=await db.prepare("SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1").bind(request.change_id).first<{snapshot_json:string}>();
      const newId=(JSON.parse(event?.snapshot_json??'{}') as {appliedNodeId?:string}).appliedNodeId;
      if(item?.branchId&&newId)mappings.push({oldId:item.id,newId,branchId:item.branchId});
    }
    return saveOrganizationDraft(db,actor,opened.draft!.id,opened.draft!.version,remapDefinition({...draft!.definition,effectiveAt:Date.now()+3600000,items:draft!.definition.items.filter(i=>unresolved.has(draftKey(i))),managers:{},leave:{}},mappings));
  }
  fail("Draft action not found.","NOT_FOUND",404);
}

export async function drainOrganizationDrafts(db: PostgresDatabase, now=Date.now()) {
  const rows=(await db.prepare("SELECT id FROM organization_draft_requests WHERE state IN ('scheduled','waiting_dependency','waiting_approval') ORDER BY effective_at,id LIMIT 100").all<{id:string}>()).results;
  let processed=0;
  for(const selected of rows) {
    try {
      await db.transaction(async tx=>{
        if(!(await tx.prepare("SELECT pg_try_advisory_xact_lock(821039320) acquired").first<{acquired:boolean}>())?.acquired)return;
        const row=await tx.prepare("SELECT * FROM organization_draft_requests WHERE id=? AND state IN ('scheduled','waiting_dependency','waiting_approval') FOR UPDATE").bind(selected.id).first<{id:string;company_id:string;draft_id:string;state:string;change_id:string|null;definition:DraftDefinition;baseline:{snapshot:Snapshot;rules:Awaited<ReturnType<typeof policiesAndLeave>>};dependencies:{oldId:string;branchId:string;changeId:string}[];actor_json:WorkContext;effective_at:number}>();
        if(!row)return;
        row.definition=decode(row.definition);row.baseline=decode(row.baseline);row.dependencies=decode(row.dependencies);row.actor_json=decode(row.actor_json);
        if(row.change_id) {
          const change=await tx.prepare("SELECT state FROM organization_node_changes WHERE id=? AND company_id=?").bind(row.change_id,row.company_id).first<{state:string}>();
          if(change?.state==='applied') { await tx.prepare("UPDATE organization_draft_requests SET state='applied',applied_at=?,updated_at=? WHERE id=?").bind(now,now,row.id).run(); processed++; }
          else if(!change || ['rejected','cancelled'].includes(change.state)) fail("The department move was rejected or cancelled.");
          return;
        }
        const mappings:Mapping[]=[];
        for(const dep of row.dependencies) {
          const change=await tx.prepare("SELECT state FROM organization_node_changes WHERE id=? AND company_id=?").bind(dep.changeId,row.company_id).first<{state:string}>();
          if(!change || ['rejected','cancelled'].includes(change.state))fail("A required department move was rejected. Review these related changes.");
          if(change!.state!=='applied')return;
          const event=await tx.prepare("SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1").bind(dep.changeId).first<{snapshot_json:string}>();
          const newId=(JSON.parse(event?.snapshot_json??'{}') as {appliedNodeId?:string}).appliedNodeId;
          if(!newId)fail("Approved department mapping is unavailable.");
          mappings.push({oldId:dep.oldId,newId:newId!,branchId:dep.branchId});
        }
        if(row.effective_at>now) { await tx.prepare("UPDATE organization_draft_requests SET state='scheduled',updated_at=? WHERE id=?").bind(now,row.id).run(); return; }
        const actor=row.actor_json;
        const authority=await tx.prepare("SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,'people.manage') AND iam_role_allows(a.role,a.company_id,'organization.manage')")
          .bind(actor.assignmentId,row.company_id,actor.userId,now,now).first();
        if(!authority)fail("The authorizing HR role changed.");
        const definition=remapDefinition(row.definition,mappings), data=await organizationAccessData(tx,actor);
        if(hash(snapshot(data,definition))!==hash(remapSnapshot(row.baseline.snapshot,mappings)) || hash(await policiesAndLeave(tx,actor,definition,data))!==hash(row.baseline.rules))fail("The reviewed organization or policy defaults changed. Review this group again.");
        const blockers=draftGraphBlockers(data,definition);
        if(blockers.length)fail(blockers.join(" "));
        const projected=projectOrganization(data,definition), people=changedPeople(data,projected);
        await tx.prepare("SELECT set_config('erpx.payroll_assignment_date',?,true)").bind(positionMoveDate(row.effective_at,definition.timezone)).first();
        for(const item of definition.items.filter(i=>i.kind==='position')) {
          const position=projected.positions.find(p=>p.id===item.id)!;
          await tx.prepare("UPDATE organization_positions SET branch_id=?,department_id=?,level_id=?,reports_to_position_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
            .bind(position.branch_id,position.department_id,position.level_id,position.reports_to_position_id,now,row.company_id,item.id).run();
        }
        for(const person of people) {
          const before=data.people.find(p=>p.engagement_id===person.engagement_id)!;
          await tx.prepare("UPDATE hr_engagements SET position_id=?,branch_id=?,department_id=?,level_id=?,manager_person_id=?,job_title=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
            .bind(person.position_id,person.branch_id,person.department_id,person.level_id,person.manager_person_id,person.job_title,now,row.company_id,person.engagement_id).run();
          await tx.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?").bind(person.branch_id,person.department_id,row.company_id,person.person_id).run();
          await tx.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(`history-${row.id}-${person.engagement_id}`,row.company_id,person.engagement_id,JSON.stringify(personPlacement(before)),JSON.stringify({...personPlacement(person),effectiveAt:row.effective_at}),actor.userId,definition.reason,row.id,now).run();
          if(before.level_id!==person.level_id)await tx.prepare("INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(`level-${row.id}-${person.engagement_id}`,person.engagement_id,before.level_id,person.level_id,actor.userId,definition.reason,row.id,now).run();
          const acceptLeave=definition.leave[person.engagement_id]??definition.items.find(i=>i.kind==='person'&&i.id===person.engagement_id)?.acceptRecommendedLeave??false;
          await tx.batch(await recommendationStatements(tx,actor,{engagementId:person.engagement_id,positionId:person.position_id,levelId:person.level_id,priorLevelId:before.level_id,startDate:positionMoveDate(row.effective_at,definition.timezone),currency:'',acceptPay:false,acceptLeave,commandId:`${row.id}-${person.engagement_id}`}));
        }
        await tx.prepare("UPDATE organization_draft_requests SET state='applied',applied_at=?,updated_at=? WHERE id=?").bind(now,now,row.id).run();
        await tx.prepare("DELETE FROM organization_draft_reservations WHERE request_id=?").bind(row.id).run();
        await tx.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)")
          .bind(`audit-${row.id}`,row.id,actor.userId,actor.assignmentId,row.id,definition.reason,now).run();
        await tx.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)")
          .bind(`event-${row.id}`,row.id,row.id,JSON.stringify({companyId:row.company_id,draftId:row.draft_id,affectedCount:people.length,state:'applied'}),now).run();
        processed++;
      });
    } catch(error) {
      await db.transaction(async tx=>{
        await tx.prepare("UPDATE organization_draft_requests SET state='needs_review',last_error=?,updated_at=? WHERE id=? AND state!='applied'").bind(error instanceof ApiError?error.message:"No changes applied. Review this group again.",now,selected.id).run();
        await tx.prepare("DELETE FROM organization_draft_reservations WHERE request_id=?").bind(selected.id).run();
      });
    }
  }
  return processed;
}
