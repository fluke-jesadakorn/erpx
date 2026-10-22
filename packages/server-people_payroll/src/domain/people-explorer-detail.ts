export type OrganizationSelection={kind:'company'|'branch'|'department'|'team'|'position'|'person'|'level'|'unassigned';id:string};export type PeopleExplorerChangeSearch=Partial<Record<'changeId'|'changeState'|'changeQuery'|'changePage',string>>;
export function peopleExplorerTabs(kind: OrganizationSelection['kind'], manage: boolean, pay: boolean) {
  if (kind === 'person') return ['Profile', ...(manage ? ['Employment', 'History'] : []), ...(pay ? ['Compensation'] : [])];
  if (kind === 'position') return ['Overview', 'People', ...(pay ? ['Compensation'] : []), ...(manage ? ['History'] : [])];
  if (kind === 'company' || kind === 'branch') return ['Overview', 'People', 'Positions', ...(manage ? ['Changes', 'Settings'] : [])];
  if (kind === 'department' || kind === 'team') return ['Overview', 'People', 'Positions', ...(manage ? ['Approvals'] : [])];
  return ['Overview', 'Positions'];
}
