import type { SpendingTemplate } from "./spending";
export const staffLevelTitles = ["CEO", "Executive / Director", "Branch / Department Manager", "Supervisor", "Senior Staff", "Staff"] as const;
export type OrganizationPosition = {
  id: string; company_id: string; branch_id: string | null; department_id: string | null;
  title: string; level_id: string; level_number: number; reports_to_position_id: string | null;
  requirements?: string;
  is_ceo: number; active: number; version: number;
};
export type OrganizationPerson = {
  person_id: string; user_id: string | null; legal_name: string; email: string | null; engagement_id: string;
  engagement_version: number; person_version: number; company_id: string; branch_id: string | null;
  department_id: string | null; position_id: string | null; manager_person_id: string | null;
  level_id: string | null; level_number: number | null; level_name: string | null;
  status: string; person_status: string; start_date: string; end_date: string | null; job_title: string;
  issues: string[];
};
export type OrganizationNode = { id: string; kind: string; slug: string; display_name: string; branch_id: string | null };
export type OrganizationTemplatePosition = {
  key: string; title: string; levelNumber: number; reportsToKey: string | null; isCeo: boolean;
};
export type OrganizationOverview = {
  positions: OrganizationPosition[]; people: OrganizationPerson[]; nodes: OrganizationNode[];
  levels: {id: string; name: string; level_number: number}[];
  templates: {id: string; name: string; version: number; positions: OrganizationTemplatePosition[]}[];
  canManage: boolean;
  authorizedChannelNodes?:string[];
  spendingTemplate?: SpendingTemplate;
  canManageSpending?: boolean;
};

/** Reports-to edges express actual management, not inferred rank adjacency. */
export function reportingIssues(person: OrganizationPerson, people: OrganizationPerson[], positions: OrganizationPosition[], today = new Date().toISOString().slice(0,10)): string[] {
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
