import {BusinessRuleError} from '../../shared';
import {organizationNodeChangeSchema} from './organization';
import {projectOrganization,changedPeople,personPlacement,positionPlacement,type DraftDefinition,type DraftItem} from "@/lib/organization/organization-draft-domain";
import type {WorkContext} from '../../identity_access/public/authorization';
import type {OrganizationAccessData} from "@/lib/organization/organization-access";
export const decode=<T,>(value:T|string):T=>typeof value==='string'?JSON.parse(value) as T:value;
export const canonical=(value:unknown):unknown=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>[k,canonical(v)])):value;
export const fail=(message:string,code='ORGANIZATION_DRAFT_CHANGED',status=409):never=>{throw new BusinessRuleError(status,code,message);};
export function sourceSnapshot(data:OrganizationAccessData,item:DraftItem):unknown {
 const person=(p:OrganizationAccessData['people'][number])=>({id:p.engagement_id,...personPlacement(p),status:p.status,personStatus:p.person_status,startDate:p.start_date,endDate:p.end_date});
 if(item.kind==='person'){const p=data.people.find(p=>p.engagement_id===item.id);return p?person(p):null;}
 const positions=data.positions.filter(p=>item.kind==='position'?p.id===item.id:p.department_id===item.id).map(p=>({id:p.id,...positionPlacement(p),active:p.active,version:p.version})).sort((a,b)=>a.id.localeCompare(b.id));
 const people=data.people.filter(p=>item.kind==='position'?p.position_id===item.id:p.department_id===item.id).map(person).sort((a,b)=>a.id.localeCompare(b.id));
 const node=item.kind==='department'?data.nodes.find(n=>n.id===item.id):undefined;
 return {positions,people,...(node?{node:{id:node.id,branchId:node.branch_id,status:node.status,version:node.hierarchy_revision}}:{})};
}
export function relocation(data: OrganizationAccessData, definition: DraftDefinition, item: DraftItem) {
  const node = data.nodes.find(n => n.id === item.id)!;
  return organizationNodeChangeSchema.parse({ action: "relocate", targetNodeId: item.id, kind: "department", displayName: node.display_name, branchId: item.branchId, expectedNodeVersion: node.hierarchy_revision, expectedVersion: 0, reason: definition.reason });
}
export function relevant(data: OrganizationAccessData, definition: DraftDefinition) {
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
export function snapshot(data: OrganizationAccessData, definition: DraftDefinition) {
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
export type Snapshot = ReturnType<typeof snapshot>;
export type Mapping = { oldId: string; newId: string; branchId: string };
export function remapDefinition(definition: DraftDefinition, mappings: Mapping[]) {
  return { ...definition, items: definition.items.map(item => ({ ...item, ...(item.departmentId ? { departmentId: mappings.find(m => m.oldId === item.departmentId)?.newId ?? item.departmentId } : {}) })) };
}
export function remapSnapshot(saved: Snapshot, mappings: Mapping[]): Snapshot {
  return { ...saved,
    people: saved.people.map(p => { const m = mappings.find(m => m.oldId === p.departmentId); return m ? { ...p, departmentId:m.newId, branchId:m.branchId } : p; }),
    positions: saved.positions.map(p => { const m = mappings.find(m => m.oldId === p.departmentId); return m ? { ...p, departmentId:m.newId, branchId:m.branchId } : p; }),
    nodes: saved.nodes.map(n => { const m = mappings.find(m => m.oldId === n.id); return m ? { ...n, id:m.newId, branchId:m.branchId, status:"active", version:1 } : n; }).sort((a,b) => a.id.localeCompare(b.id)),
  };
}
export function components(data: OrganizationAccessData, definition: DraftDefinition) {
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

export function requireCompanyDraftScope(actor:WorkContext){if(actor.scope.kind!=='holding'||actor.assignmentScope!=='holding'||actor.branchId||actor.departmentId)fail('Select a Company workspace with HR authority.','COMPANY_AUTHORITY_REQUIRED',403);}
