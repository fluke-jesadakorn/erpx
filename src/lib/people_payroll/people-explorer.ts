import type { OrganizationSelection } from "@/components/people_payroll/organization-explorer";
import type { HoldingPeopleDirectory } from "@/server/modules/organization/public/holding-people-directory";
import type { OrganizationAccessData } from "@/lib/organization/organization-access";
import { draftKey, projectOrganization, type DraftDefinition } from "@/lib/organization/organization-draft";

export function peopleExplorerTabs(kind: OrganizationSelection['kind'], manage: boolean, pay: boolean) {
  if (kind === 'person') return ['Profile', ...(manage ? ['Employment', 'History'] : []), ...(pay ? ['Compensation'] : [])];
  if (kind === 'position') return ['Overview', 'People', ...(pay ? ['Compensation'] : []), ...(manage ? ['History'] : [])];
  if (kind === 'company' || kind === 'branch') return ['Overview', 'People', 'Positions', ...(manage ? ['Changes', 'Settings'] : [])];
  if (kind === 'department' || kind === 'team') return ['Overview', 'People', 'Positions', ...(manage ? ['Approvals'] : [])];
  return ['Overview', 'Positions'];
}
export type ExplorerPanelState = { selection: OrganizationSelection | null; tab: string };
export type ExplorerPath = ExplorerPanelState[];
export function childSelections(data: HoldingPeopleDirectory, parent: OrganizationSelection): OrganizationSelection[] {
  const positions = data.positions.filter(p => parent.kind === 'company' ? p.companyId === parent.id && !p.branchId && !p.departmentId : parent.kind === 'branch' ? p.branchId === parent.id && !p.departmentId : parent.kind === 'department' || parent.kind === 'team' ? p.departmentId === parent.id : parent.kind === 'level' ? p.levelId === parent.id : false);
  const nodes = data.nodes.filter(n => n.kind !== 'company' && (parent.kind === 'company' ? n.companyId === parent.id && (n.kind === 'branch' || n.kind === 'headquarters' || !n.branchId && !data.nodes.some(p => p.id === n.parentId && (p.kind === 'department' || p.kind === 'team'))) : parent.kind === 'branch' ? n.branchId === parent.id && (n.kind === 'department' || n.kind === 'team') && !data.nodes.some(p => p.id === n.parentId && (p.kind === 'department' || p.kind === 'team')) : (parent.kind === 'department' || parent.kind === 'team') && n.parentId === parent.id));
  return [...nodes.map(n => ({kind: n.kind === 'headquarters' ? 'branch' as const : n.kind as 'branch' | 'department' | 'team', id:n.id})), ...positions.map(p => ({kind:'position' as const,id:p.id})), ...(parent.kind === 'position' || parent.kind === 'unassigned' ? data.people.filter(p => parent.kind === 'position' ? p.positionId === parent.id : p.companyId === parent.id && !p.positionId).map(p => ({kind:'person' as const,id:p.engagementId})) : []), ...(parent.kind === 'company' ? [{kind:'unassigned' as const,id:parent.id}, ...data.levels.filter(l => l.companyId === parent.id).map(l => ({kind:'level' as const,id:l.id}))] : [])];
}
/** Only the path on the clicked side is truncated. Employee details never create another row. */
export function selectPanel(path: ExplorerPath, row: number, selection: OrganizationSelection, data: HoldingPeopleDirectory): ExplorerPath {
  const next = path.slice(0,row);
  while (next.length < row) next.push({selection:null,tab:''});
  next.push({selection,tab:''});
  if (selection.kind !== 'person' && childSelections(data,selection).length) next.push({selection:null,tab:''});
  return next;
}
export function projectedDirectory(data: HoldingPeopleDirectory, organization: OrganizationAccessData | null, definition: DraftDefinition): HoldingPeopleDirectory {
  if (!organization) return data;
  const graph=projectOrganization(organization,definition);
  return {...data,nodes:data.nodes.map(n=>{const p=graph.nodes.find(p=>p.id===n.id);return p?{...n,branchId:p.branch_id,parentId:n.kind==='department'&&n.parentId===n.branchId?p.branch_id:n.parentId}:n;}),positions:data.positions.map(p=>{const next=graph.positions.find(n=>n.id===p.id);return next?{...p,branchId:next.branch_id,departmentId:next.department_id,levelId:next.level_id!,reportsToPositionId:next.reports_to_position_id}:p;}),people:data.people.map(p=>{const next=graph.people.find(n=>n.engagement_id===p.engagementId);return next?{...p,branchId:next.branch_id,departmentId:next.department_id,positionId:next.position_id,jobTitle:next.job_title}:p;})};
}
export function hiddenDraftKeys(data: HoldingPeopleDirectory, definition: DraftDefinition, source: boolean) {
  const keys=new Set(definition.items.filter(i=>source||i.parked).map(draftKey));
  let expanded=true;
  while(expanded){expanded=false;for(const n of data.nodes)if(n.parentId&&(keys.has(`department:${n.parentId}`)||keys.has(`team:${n.parentId}`))&&!keys.has(`${n.kind}:${n.id}`)){keys.add(`${n.kind}:${n.id}`);expanded=true;}}
  for (const p of data.positions) if (p.departmentId && (keys.has(`department:${p.departmentId}`)||keys.has(`team:${p.departmentId}`))) keys.add(`position:${p.id}`);
  for (const p of data.people) if (p.positionId && keys.has(`position:${p.positionId}`) || p.departmentId && (keys.has(`department:${p.departmentId}`)||keys.has(`team:${p.departmentId}`))) keys.add(`person:${p.engagementId}`);
  return keys;
}

export function trayKindsForParents(parents: (OrganizationSelection | null | undefined)[]): ('department' | 'position' | 'person')[] {
  const kinds = parents.filter((parent): parent is OrganizationSelection => !!parent).map(parent => parent.kind === 'position' || parent.kind === 'unassigned' ? 'person' as const : parent.kind === 'department' || parent.kind === 'team' || parent.kind === 'level' ? 'position' as const : 'department' as const);
  return kinds.length ? [...new Set(kinds)] : ['department'];
}
