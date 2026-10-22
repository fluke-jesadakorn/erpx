import {reportingIssues,type OrganizationPerson,type OrganizationPosition} from "@erpx/server-organization/public/rules/organization-domain";
export const decode = <T>(value: T | string): T => typeof value === "string" ? JSON.parse(value) as T : value;
export const scoped = (actor: {branchId:string|null;departmentId:string|null}, value: { branch_id: string | null; department_id: string | null }) => (!actor.branchId || actor.branchId === value.branch_id) && (!actor.departmentId || actor.departmentId === value.department_id);
export type Leave = { engagement_id: string; id: string; effective_from: string; effective_to: string | null; policy_key: string; version: number };
export type LeaveDefault = { level_id: string; policy_version_id: string; effective_from: string; version: number };

/** Validate the projected whole graph, including reports outside the moved cohort. */
export function positionMoveGraphIssues(people: OrganizationPerson[], positions: OrganizationPosition[], candidate: OrganizationPosition, holders: OrganizationPerson[], date: string) {
  const issues: string[] = [];
  const projectedPositions = positions.map(position => position.id === candidate.id ? candidate : position);
  const projectedPeople = people.map(person => holders.find(holder => holder.engagement_id === person.engagement_id) ?? person);
  if (candidate.is_ceo && (candidate.level_number !== 1 || candidate.reports_to_position_id)) issues.push("The CEO position must remain Level 1 without a reporting position.");
  if (!candidate.is_ceo && !candidate.reports_to_position_id) issues.push("Choose a reporting position.");
  for (const position of projectedPositions.filter(item => item.active === 1)) {
    const parent = projectedPositions.find(item => item.id === position.reports_to_position_id && item.active === 1);
    if ((position.id === candidate.id || position.reports_to_position_id === candidate.id) && position.reports_to_position_id && (!parent || parent.level_number >= position.level_number))
      issues.push(`${position.title}: the reporting position must have a more senior Level.`);
    const seen = new Set([position.id]); let next = position.reports_to_position_id;
    while (next) {
      if (seen.has(next)) { issues.push("Correct the circular position reporting relationship."); break; }
      seen.add(next); next = projectedPositions.find(item => item.id === next)?.reports_to_position_id ?? null;
    }
  }
  const holderIds = new Set(holders.map(holder => holder.engagement_id));
  for (const person of projectedPeople) {
    const before = people.find(item => item.engagement_id === person.engagement_id)!;
    const prior = reportingIssues(before, people, positions, date);
    const next = reportingIssues(person, projectedPeople, projectedPositions, date);
    for (const issue of next.filter(issue => holderIds.has(person.engagement_id) || !prior.includes(issue))) issues.push(`${person.legal_name}: ${issue}`);
  }
  return [...new Set(issues)];
}

export function positionMoveLeaveImpact(prior: Leave | undefined, policy: LeaveDefault | undefined, beforeLevelId: string | null, afterLevelId: string, date: string) {
  if (beforeLevelId === afterLevelId) return { leaveEffectiveFrom: null, leaveNote: "Actual Level is unchanged; existing leave remains.", blocker: null };
  if (!policy) return { leaveEffectiveFrom: null, leaveNote: "No leave default is configured for the new Level.", blocker: null };
  if (prior && prior.policy_key !== "legacy-untracked" && !/^level-[1-6]-default$/.test(prior.policy_key))
    return { leaveEffectiveFrom: null, leaveNote: "Custom leave policy is preserved.", blocker: null };
  const effective = beforeLevelId ? `${Number(date.slice(0, 4)) + 1}-01-01` : date < policy.effective_from ? policy.effective_from : date;
  return { leaveEffectiveFrom: effective, leaveNote: `Level leave default starts ${effective}. Salary is unchanged.`, blocker: prior && prior.effective_from >= effective ? "Review the overlapping future leave assignment." : null };
}


export const positionGpoMatches=(rule:{active:boolean;target_kind:string;target_id:string},position:Pick<OrganizationPosition,'id'>)=>rule.active&&rule.target_kind==='position'&&rule.target_id===position.id;
