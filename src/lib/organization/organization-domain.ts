import type { SpendingTemplate } from "@/lib/shared/spending";
export const staffLevelTitles = ["CEO", "Executive / Director", "Branch / Department Manager", "Supervisor", "Senior Staff", "Staff"] as const;
export type OrganizationPosition = {
  id: string; company_id: string; branch_id: string | null; department_id: string | null;
  title: string; level_id: string; level_number: number; reports_to_position_id: string | null;
  requirements?: string;
  is_ceo: number; roster_required: number; active: number; version: number;
};
export type OrganizationPerson = {
  person_id: string; user_id: string | null; legal_name: string; email: string | null; engagement_id: string;
  engagement_version: number; person_version: number; company_id: string; branch_id: string | null;
  department_id: string | null; position_id: string | null; manager_person_id: string | null;
  level_id: string | null; level_number: number | null; level_name: string | null;
  status: string; person_status: string; start_date: string; end_date: string | null; job_title: string;
  issues: string[];
};
export type OrganizationNode = { id: string; kind: string; slug: string; display_name: string; branch_id: string | null; status: string; hierarchy_revision: number };
export type OrganizationNodeChange = {
  id:string; target_node_id:string|null; action:"create"|"rename"|"suspend"|"reactivate"|"archive"|"relocate";
  node_kind:"branch"|"department"; slug:string|null; display_name:string; branch_id:string|null; source_branch_id:string|null;
  expected_node_version:number|null; state:"draft"|"submitted"|"applied"|"rejected"|"cancelled";
  version:number; requested_by:string; reviewed_by:string|null; reason:string; review_reason:string|null;
  created_at:number; updated_at:number; reviewed_at:number|null; applied_at:number|null;
  events?: { action:string; actor_name:string; version:number; reason:string|null; created_at:number }[];
  canReview?: boolean;
};
export type OrganizationTemplatePosition = {
  key: string; title: string; levelNumber: number; reportsToKey: string | null; isCeo: boolean; rosterRequired?: boolean;
};
export type OrganizationOverview = {
  defaults?: {
    pay: {position_id:string;version:number;pay_group_id:string;currency:string;monthly_amount_minor:number;pay_group_name:string}[];
    leave: {level_id:string;version:number;policy_version_id:string;name:string;effective_from:string;annual_days:number;sick_days:number}[];
  };
  monthlyPayGroups?: {id:string;name:string;currency:string}[];
  positions: OrganizationPosition[]; people: OrganizationPerson[]; nodes: OrganizationNode[];
  levels: {id: string; name: string; level_number: number}[];
  templates: {id: string; name: string; version: number; positions: OrganizationTemplatePosition[]}[];
  nodeChanges: OrganizationNodeChange[];
  changeQueue: { recordId:string|null; state:string; query:string; page:number; pageSize:number; total:number; pages:number };
  canManageUnits: boolean;
  actorId: string;
  canManage: boolean;
  authorizedChannelNodes?:string[];
  spendingTemplate?: SpendingTemplate;
  canManageSpending?: boolean;
};

/** Reports-to edges express actual management, not inferred rank adjacency. */
export function reportingIssues(person: OrganizationPerson, people: OrganizationPerson[], positions: OrganizationPosition[], today: string): string[] {
  const issues: string[] = [];
  const position = positions.find(p => p.id === person.position_id && p.active === 1);
  if (!person.level_number) issues.push("Assign one of the six staff levels.");
  if (!position) issues.push("Assign an active position.");
  if (position && (position.company_id !== person.company_id || (position.branch_id && position.branch_id !== person.branch_id) || (position.department_id && position.department_id !== person.department_id))) issues.push("Position does not match staff placement.");
  if (position?.is_ceo && person.level_number !== 1) issues.push("The CEO must have Level 1.");
  if (!person.manager_person_id) {
    if (!position?.is_ceo) issues.push("Assign a reporting manager.");
    return issues;
  }
  const seen = new Set([person.person_id]);
  let current: OrganizationPerson | undefined = person;
  while (current?.manager_person_id) {
    if (seen.has(current.manager_person_id)) { issues.push("Correct the circular reporting relationship."); break; }
    seen.add(current.manager_person_id);
    const managers = people.filter(p => p.person_id === current!.manager_person_id && p.status === "active" && p.person_status === "active" && p.start_date <= today && (!p.end_date || p.end_date >= today));
    const manager = managers.length === 1 ? managers[0] : undefined;
    if (!manager || manager.company_id !== person.company_id) { issues.push("The reporting manager needs one active engagement in this company."); break; }
    if (!manager.level_number || !current.level_number || manager.level_number >= current.level_number) issues.push("The reporting manager must have a more senior level.");
    current = manager;
  }
  return issues;
}

export function organizationChangeHref(id:string,companyId:string) {
  const query=new URLSearchParams({view:"legacy",company:companyId,tab:"changes",changeId:id});
  return `/people/organization?${query}#organization-change-${encodeURIComponent(id)}`;
}
