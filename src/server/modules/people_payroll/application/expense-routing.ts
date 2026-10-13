import type { OrganizationPerson } from "@/server/modules/organization/public/profile-facts";
import type { RecordScope, RouteStep, StaffPerson } from "@/server/modules/operations/public/staff-policy";
import type { ApprovalInstance, ApprovalStep } from "@/server/modules/workflow/public/approvals";
import type { SpendingKind, SpendingTemplate } from "@/lib/shared/spending";
import { hasIndependentSpendingTeam, independentAccountingCandidates, selectSpendingException } from "../domain/expense-routing-rules";
import type { ExpenseCommandPort } from "./expense-commands";

export type ExpenseRoutingGuard = { readonly native: unknown };
export type ExpenseRoutePlan = { scope: RecordScope; steps: RouteStep[]; blockedReason: string | null; guards?: ExpenseRoutingGuard[] };
export type ExpenseProfileRoster = { people: OrganizationPerson[]; issues(person: OrganizationPerson): string[] };
export type ExpenseSavedParticipant = { stage?: string; requiredRole?: string; candidateUserIds?: string[]; snapshot?: Record<string, unknown> };
export interface ExpenseRoutingPorts {
  error: ExpenseCommandPort["error"];
  /** Only native policy errors become a blocked route; infrastructure failures propagate. */
  rejection(error: unknown): string | null;
  authority(companyId: string): Promise<ExpenseRoutingGuard>;
  staff(companyId: string, userId: string): Promise<StaffPerson>;
  activeDepartment(companyId: string, departmentId: string): Promise<boolean>;
  levels(companyId: string): Promise<{ id: string; rank: number }[]>;
  staffPeople(companyId: string): Promise<StaffPerson[]>;
  reportingChain(people: StaffPerson[], personId: string): StaffPerson[];
  engagementGuard(staff: StaffPerson): ExpenseRoutingGuard;
  roleUsers(scope: RecordScope, role: string | null): Promise<string[]>;
  identities(companyId: string, users: string[]): Promise<string[]>;
  roster(companyId: string): Promise<ExpenseProfileRoster>;
  today(): string;
  template(companyId: string): Promise<SpendingTemplate>;
  department(companyId: string, userId: string, excluded: string[]): Promise<ExpenseRoutePlan>;
  departmentEligible(companyId: string, userId: string, snapshot: Record<string, unknown>): Promise<boolean>;
  spendingUsers(scope: RecordScope, role: string | null): Promise<OrganizationPerson[]>;
  claim(instance: ApprovalInstance): Promise<{ prepared_by: string; claimant_user_id: string; accounting_json: string } | null>;
}

export function createSpendingUsers(ports: Pick<ExpenseRoutingPorts, "roster" | "roleUsers" | "today">) {
  return async (scope: RecordScope, role: string | null) => {
    const roster = await ports.roster(scope.company_id), accounts = new Set(await ports.roleUsers(scope, role)), today = ports.today();
    return roster.people.filter(person => person.user_id && accounts.has(person.user_id) && person.status === "active" && person.person_status === "active" && person.start_date <= today && (!person.end_date || person.end_date >= today)
      && !roster.issues(person).length && (role !== "finance_approver" || (person.level_number !== null && person.level_number <= 4)));
  };
}

export function createLegacyExpenseRoute(ports: ExpenseRoutingPorts) {
  return async (companyId: string, userId: string, excluded: string[] = [], departmentOnly = false): Promise<ExpenseRoutePlan> => {
    let scope: RecordScope = { company_id: companyId, branch_id: null, department_id: null };
    try {
      const authoritySnapshot = await ports.authority(companyId), claimant = await ports.staff(companyId, userId);
      scope = claimant;
      if (!claimant.department_id) throw ports.error({ status: 409, code: "DEPARTMENT_REQUIRED", message: "Set the claimant's department in People before submitting." });
      if (!await ports.activeDepartment(companyId, claimant.department_id)) throw ports.error({ status: 409, code: "DEPARTMENT_REQUIRED", message: "Select an active department in People before submitting." });
      const ranks = new Map((await ports.levels(companyId)).map(level => [level.id, level.rank])), membersInScope = new Set(await ports.roleUsers(scope, null));
      if (!membersInScope.has(userId)) throw ports.error({ status: 409, code: "CLAIMANT_SCOPE_REQUIRED", message: "The claimant's access must cover their active branch and department. Review their People Access panel." });
      if (!claimant.level_id || !ranks.has(claimant.level_id)) throw ports.error({ status: 409, code: "LEVEL_REQUIRED", message: "Set the claimant's active staff level in People before submitting." });
      const roster = await ports.staffPeople(companyId), people = roster.filter(person => roster.filter(other => other.user_id === person.user_id).length === 1), chain = ports.reportingChain(people, claimant.person_id), forbidden = new Set([userId, ...excluded]);
      const department = people.filter(person => person.department_id === claimant.department_id && person.level_id && ranks.has(person.level_id)), highest = Math.max(...department.map(person => ranks.get(person.level_id!)!));
      let heads = department.filter(person => ranks.get(person.level_id!) === highest && !forbidden.has(person.user_id) && membersInScope.has(person.user_id)).map(person => person.user_id);
      let authority: "named" | "department" | "holding_owner" = "department";
      if (!heads.length) {
        const superior = chain.find(person => !forbidden.has(person.user_id) && membersInScope.has(person.user_id));
        if (superior) { heads = [superior.user_id]; authority = "named"; }
        else { heads = (await ports.roleUsers(scope, "holding_owner")).filter(id => !forbidden.has(id)); authority = "holding_owner"; }
      }
      if (!heads.length) throw ports.error({ status: 409, code: "INDEPENDENT_HEAD_REQUIRED", message: "Configure an independent superior or company owner for this top-level claim." });
      const headStep = () => ({ userId: heads[0], authority, candidateUserIds: heads, stage: "department_approval", snapshot: { departmentId: claimant.department_id, highestRank: highest, excluded } });
      if (departmentOnly) return { scope, blockedReason: null, steps: [headStep()], guards: [authoritySnapshot] };
      const [preparers, managers, payers] = await Promise.all(["finance_preparer", "finance_approver", "treasury"].map(async role => (await ports.roleUsers(scope, role)).filter(id => !forbidden.has(id))));
      if (!preparers.length || !managers.length || !payers.length) throw ports.error({ status: 409, code: "EXPENSE_TEAM_REQUIRED", message: "Configure Accounts reviewers, Accounts managers, and Treasury for this company or branch." });
      heads = heads.filter(head => hasIndependentSpendingTeam(preparers, managers, payers, [head]));
      if (!heads.length) throw ports.error({ status: 409, code: "EXPENSE_SEPARATION_REQUIRED", message: "Four different people must be available for department approval, Accounts review, Accounts manager approval, and payment." });
      return { scope, blockedReason: null, steps: [headStep(), { userId: preparers[0], authority: "named", candidateUserIds: preparers, requiredRole: "finance_preparer", stage: "accounts_review" }, { userId: managers[0], authority: "named", candidateUserIds: managers, requiredRole: "finance_approver", stage: "accounts_approval" }], guards: [authoritySnapshot, ports.engagementGuard(claimant)] };
    } catch (error) { const rejection = ports.rejection(error); if (rejection === null) throw error; return { scope, steps: [], blockedReason: rejection }; }
  };
}

export function createExpenseRoute(ports: ExpenseRoutingPorts) {
  return async (companyId: string, userId: string, excluded: string[] = [], departmentOnly = false, kind: SpendingKind = "expense"): Promise<ExpenseRoutePlan> => {
    if (departmentOnly) return ports.department(companyId, userId, excluded);
    let scope: RecordScope = { company_id: companyId, branch_id: null, department_id: null };
    try {
      const guard = await ports.authority(companyId), staff = await ports.staff(companyId, userId);
      scope = staff;
      const roster = await ports.roster(companyId), person = roster.people.find(person => person.engagement_id === staff.engagement_id);
      if (!person) throw ports.error({ status: 409, code: "STAFF_LINK_REQUIRED", message: "Complete the requester’s People profile." });
      const issues = roster.issues(person), template = await ports.template(companyId), exception = selectSpendingException(template.exceptions, person.position_id, kind), remaining = issues.filter(issue => !exception || issue !== "Assign a reporting manager.");
      if (remaining.length) throw ports.error({ status: 409, code: "REQUESTER_SETUP_REQUIRED", message: remaining.join(" ") });
      const forbidden = new Set([userId, ...excluded]), department = await ports.department(companyId, userId, excluded);
      if (department.blockedReason) throw ports.error({ status: 409, code: "DEPARTMENT_FLOW_REQUIRED", message: department.blockedReason });
      if (!(await ports.roleUsers(scope, null)).includes(userId)) throw ports.error({ status: 409, code: "REQUESTER_SCOPE_REQUIRED", message: "The requester’s account must cover this branch and department." });
      for (const step of department.steps) forbidden.add(step.userId);
      const [preparers, reviewers, payers] = await Promise.all(["finance_preparer", "finance_approver", "treasury"].map(async role => (await ports.spendingUsers(scope, role)).map(person => person.user_id!).filter(id => !forbidden.has(id))));
      if (!hasIndependentSpendingTeam(preparers, reviewers, payers)) throw ports.error({ status: 409, code: "SPENDING_TEAM_REQUIRED", message: "Complete independent Accounting preparer, Accounting approver (Level 1–4), and Finance/Treasury payer profiles and scoped access." });
      const snapshot = { spendingPolicy: 1, templateVersion: template.version, requestKind: kind, positionId: person.position_id, requesterLevel: person.level_number, exception: exception ?? null, excluded };
      return { scope, blockedReason: null, guards: [guard, ...(department.guards ?? [])], steps: [...department.steps.map(step => ({ ...step, snapshot: { ...snapshot, ...step.snapshot } })), { userId: preparers[0], authority: "named", candidateUserIds: preparers, requiredRole: "finance_preparer", stage: "accounts_review", snapshot }, { userId: reviewers[0], authority: "named", candidateUserIds: reviewers, requiredRole: "finance_approver", stage: "accounts_approval", snapshot }] };
    } catch (error) { const rejection = ports.rejection(error); if (rejection === null) throw error; return { scope, steps: [], blockedReason: rejection }; }
  };
}

export function createSpendingStepCandidates(ports: ExpenseRoutingPorts) {
  return async (scope: RecordScope, requester: string, preparers: string[], step: ExpenseSavedParticipant, decisions: string[]) => {
    if (step.stage === "department_approval" && step.snapshot && "departmentPolicyVersion" in step.snapshot) {
      const id = step.candidateUserIds?.[0];
      return id && ![requester, ...preparers, ...decisions].includes(id) && await ports.departmentEligible(scope.company_id, id, step.snapshot) ? [id] : [];
    }
    const forbidden = new Set(await ports.identities(scope.company_id, [requester, ...preparers, ...decisions])), saved = await ports.identities(scope.company_id, step.candidateUserIds ?? []), members = await ports.spendingUsers(scope, step.requiredRole ?? (step.stage === "department_approval" ? "people_manager" : null));
    const claimant = await ports.staff(scope.company_id, requester).catch(() => null);
    if (!claimant?.level_number) return [];
    const roster = await ports.roster(scope.company_id), person = roster.people.find(person => person.engagement_id === claimant.engagement_id), exception = step.snapshot?.exception as { action?: string } | undefined;
    if (!person || roster.issues(person).some(issue => !(issue === "Assign a reporting manager." && exception?.action))) return [];
    const candidates = members.filter(person => saved.includes(person.user_id!) && !forbidden.has(person.user_id!) && (step.stage !== "department_approval" || person.level_number! < claimant.level_number!)).map(person => person.user_id!);
    const payers = (await ports.spendingUsers(scope, "treasury")).map(person => person.user_id!).filter(id => !forbidden.has(id));
    if (step.stage === "accounts_approval") return independentAccountingCandidates(step.stage, candidates, [], payers);
    const reviewers = (await ports.spendingUsers(scope, "finance_approver")).map(person => person.user_id!).filter(id => !forbidden.has(id));
    return step.stage === "accounts_review" ? independentAccountingCandidates(step.stage, candidates, reviewers, payers) : candidates;
  };
}

export function createExpensePoolCandidates(ports: ExpenseRoutingPorts) {
  const legacyRoute = createLegacyExpenseRoute(ports), spendingCandidates = createSpendingStepCandidates(ports);
  return async (instance: ApprovalInstance, step: ApprovalStep, steps: ApprovalStep[]) => {
    const claim = await ports.claim(instance);
    if (!claim) return [];
    const snapshot = JSON.parse(step.route_snapshot_json ?? "{}") as { spendingPolicy?: number }, decisions = steps.filter(previous => previous.ordinal < step.ordinal && previous.state === "approved").map(previous => previous.approver_user_id);
    if (snapshot.spendingPolicy) return spendingCandidates(instance, claim.claimant_user_id, [claim.prepared_by], { snapshot: JSON.parse(step.route_snapshot_json ?? "{}"), stage: step.stage ?? undefined, requiredRole: step.required_role ?? undefined, candidateUserIds: JSON.parse(step.approver_pool_json ?? "[]") }, decisions);
    const forbidden = new Set(await ports.identities(instance.company_id, [claim.claimant_user_id, claim.prepared_by, ...decisions]));
    if (step.stage === "accounts_approval") { const accounting = JSON.parse(claim.accounting_json) as { preparedBy?: string }; if (accounting.preparedBy) for (const id of await ports.identities(instance.company_id, [accounting.preparedBy])) forbidden.add(id); }
    let current: string[] = [];
    if (step.stage === "department_approval") current = (await legacyRoute(instance.company_id, claim.claimant_user_id, [claim.prepared_by])).steps[0]?.candidateUserIds ?? [];
    else if (step.required_role) current = await ports.roleUsers(instance, step.required_role);
    const saved = await ports.identities(instance.company_id, JSON.parse(step.approver_pool_json ?? "[]") as string[]), candidates = current.filter(id => saved.includes(id) && !forbidden.has(id));
    if (step.stage === "department_approval") return candidates;
    const payers = (await ports.roleUsers(instance, "treasury")).filter(id => !forbidden.has(id));
    if (step.stage === "accounts_approval") return independentAccountingCandidates("accounts_approval", candidates, [], payers);
    const managers = (await ports.roleUsers(instance, "finance_approver")).filter(id => !forbidden.has(id));
    return independentAccountingCandidates("accounts_review", candidates, managers, payers);
  };
}
