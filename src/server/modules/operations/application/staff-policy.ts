import { permissionForRoles } from "@/lib/permissions";
import { type StaffActor, type RecordScope, type StaffPerson, scopeActor, type RouteStep, type ApprovalRoute, reportingChain } from '../domain/staff-policy';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { OperationsCommands } from '../domain/commands';
export interface StaffPolicyRepository<Effect> {
    readStaffPeopleResults(companyId: string, now: number, now2: number, value: string | null, value2: string | null, value3: string | null, value4: string | null, value5: string | null): Promise<{
        results: (StaffPerson)[];
    }>;
    readRosterStaffPeopleResults(companyId: string, lastDate: string, firstDate: string, now: number, now2: number, departmentId: string): Promise<{
        results: (StaffPerson)[];
    }>;
    readReadActiveStaffResults(companyId: string, now: number, now2: number, userId: string): Promise<{
        results: (StaffPerson)[];
    }>;
    guard4(key: string, step: string, userId: string, companyId: string, userId2: string, companyId2: string, now: number, now2: number): Effect;
    readCompanyAuthorityEffect(assignmentId: string, companyId: string, value: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    guard6(key: string, value: "company-authority", assignmentId: string, companyId: string, value2: string, now: number, now2: number): Effect;
    readEligibleRoleUsersResults(company_id: string, role: "hr_admin" | "holding_owner", now: number, now2: number, role2: "hr_admin" | "holding_owner", branch_id: string | null, department_id: string | null): Promise<{
        results: ({
            user_id: string;
        })[];
    }>;
    readRequestRouteEffect(user_id: string, companyId: string, now: number, now2: number, now3: number, branch_id: string | null, department_id: string | null): Promise<Record<string, unknown> | null>;
    readRequestRouteHead(companyId: string, department_id: string): Promise<{
        person_id: string;
        version: number;
    } | null>;
    stageNotificationEffect(value: string, company_id: string, userId: string, type: string, id: string, version: number, message: string, now: number): Effect;
}
export interface StaffPolicyPorts<Effect> {
    repository: StaffPolicyRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OperationsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    routeChecks(people: StaffPerson[], companyId: string, head?: {
        departmentId: string;
        version: number;
    }): ApprovalRoute['checks'];
}
export function createStaffPolicyApplication<Effect>(ports: StaffPolicyPorts<Effect>) {
    const r = ports.repository;
    async function staffPeople(companyId: string, filters: {
        departmentId?: string;
        levelId?: string;
    } = {}) {
        const now = ports.clock.now();
        const { results } = await r.readStaffPeopleResults(companyId, now, now, filters.departmentId ?? null, filters.departmentId ?? null, filters.levelId ?? null, filters.levelId ?? null, filters.levelId ?? null);
        return results;
    }
    async function rosterStaffPeople(companyId: string, departmentId: string, firstDate: string, lastDate: string) {
        const now = ports.clock.now();
        const { results } = await r.readRosterStaffPeopleResults(companyId, lastDate, firstDate, now, now, departmentId);
        return results;
    }
    async function activeStaff(companyId: string, userId: string): Promise<StaffPerson> {
        return ports.policyRead(JSON.stringify(["active-staff", companyId, userId]), () => readActiveStaff(companyId, userId));
    }
    async function readActiveStaff(companyId: string, userId: string): Promise<StaffPerson> {
        const now = ports.clock.now();
        const { results } = await r.readReadActiveStaffResults(companyId, now, now, userId);
        if (results.length !== 1)
            throw new BusinessRuleError(409, "STAFF_LINK_REQUIRED", results.length > 1
                ? "Multiple active engagements need correction before this action."
                : "An active employee or contractor engagement and account are required in this company.");
        return results[0];
    }
    function staffGuard(key: string, companyId: string, userId: string, step = `staff-${userId}`) {
        const now = ports.clock.now();
        return r.guard4(key, step, userId, companyId, userId, companyId, now, now);
    }
    async function companyAuthority(actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">, roles: string[] | string = "organization.manage") {
        return ports.policyRead(JSON.stringify(["company-authority", actor, roles]), async () => {
            const now = ports.policyNow();
            return !!(await r.readCompanyAuthorityEffect(actor.assignmentId, actor.companyId, (typeof roles === "string" ? roles : roles.includes("it_admin") ? "ai.manage" : roles.length === 2 && roles.includes("hr_admin") ? "organization.manage" : permissionForRoles(roles)), now, now));
        });
    }
    function authorityGuard(key: string, actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">, roles: string[] | string = "organization.manage") {
        const now = ports.clock.now();
        return r.guard6(key, "company-authority", actor.assignmentId, actor.companyId, (typeof roles === "string" ? roles : roles.includes("it_admin") ? "ai.manage" : roles.length === 2 && roles.includes("hr_admin") ? "organization.manage" : permissionForRoles(roles)), now, now);
    }
    async function requireCompanyAuthority(actor: StaffActor, roles?: string[] | string) {
        if (!(await companyAuthority(actor, roles)))
            throw new BusinessRuleError(403, "COMPANY_AUTHORITY_REQUIRED", "A company-authorized HR administrator or owner is required.");
    }
    async function eligibleRoleUsers(scope: RecordScope, role: "hr_admin" | "holding_owner") {
        const now = ports.clock.now();
        const { results } = await r.readEligibleRoleUsersResults(scope.company_id, role, now, now, role, scope.branch_id, scope.department_id);
        return results.map((x) => x.user_id);
    }
    async function requestRoute(companyId: string, userId: string, kind: "expense" | "leave", excluded: string[] = []): Promise<ApprovalRoute> {
        let scope: RecordScope = {
            company_id: companyId,
            branch_id: null,
            department_id: null,
        };
        try {
            const staff = await activeStaff(companyId, userId);
            scope = staff;
            if (kind === "leave") {
                const people = await staffPeople(companyId);
                const chain = reportingChain(people, staff.person_id);
                const forbidden = new Set([userId, staff.user_id, ...excluded]);
                let manager = chain[0];
                let authority: RouteStep["authority"] = "named";
                if (!manager) {
                    if (!staff.self_service_top_level)
                        throw new BusinessRuleError(409, "MANAGER_REQUIRED", "Ask HR to set your reporting manager, or explicitly confirm a top-level engagement in People settings.");
                    const owners = await eligibleRoleUsers(scope, "holding_owner");
                    const owner = owners.find((id) => !forbidden.has(id) && people.filter((p) => p.user_id === id).length === 1);
                    if (!owner)
                        throw new BusinessRuleError(409, "OWNER_REQUIRED", "Configure an independent company owner for this top-level staff member.");
                    manager = people.find((p) => p.user_id === owner)!;
                    authority = "holding_owner";
                }
                if (forbidden.has(manager.user_id))
                    throw new BusinessRuleError(409, "SELF_APPROVAL", "An independent reporting manager is required.");
                if (authority === 'named' && !await r.readRequestRouteEffect(manager.user_id, companyId, ports.clock.now(), ports.clock.now(), ports.clock.now(), scope.branch_id, scope.department_id))
                    throw new BusinessRuleError(409, 'MANAGER_AUTHORITY_REQUIRED', 'Ask HR to give the reporting manager approval authority for your Company scope.');
                forbidden.add(manager.user_id);
                const hrUsers = await eligibleRoleUsers(scope, "hr_admin");
                const hrUser = hrUsers.find((id) => !forbidden.has(id) && people.filter((p) => p.user_id === id).length === 1);
                if (!hrUser)
                    throw new BusinessRuleError(409, "HR_APPROVER_REQUIRED", "Configure an eligible HR reviewer independent of the requester and reporting manager.");
                return {
                    steps: [
                        { userId: manager.user_id, authority },
                        { userId: hrUser, authority: "hr_admin" },
                    ],
                    blockedReason: null,
                    scope,
                    checks: ports.routeChecks([staff, manager], companyId),
                };
            }
            if (!staff.department_id)
                throw new BusinessRuleError(409, "DEPARTMENT_REQUIRED", "Set the requester's department before submission.");
            const head = await r.readRequestRouteHead(companyId, staff.department_id);
            if (!head)
                throw new BusinessRuleError(409, "DEPARTMENT_HEAD_REQUIRED", "Configure a named head for the requester's department.");
            const people = await staffPeople(companyId);
            const heads = people.filter((p) => p.person_id === head.person_id);
            if (heads.length !== 1)
                throw new BusinessRuleError(409, "HEAD_UNAVAILABLE", "The department head needs one active engagement and account.");
            const chain = reportingChain(people, head.person_id);
            const forbidden = new Set([userId, ...excluded]);
            let reviewer = heads[0], authority: RouteStep["authority"] = "department";
            if (head.person_id === staff.person_id) {
                if (chain.length)
                    reviewer = chain[0];
                else {
                    const owners = await eligibleRoleUsers(scope, "holding_owner");
                    const owner = owners.find((id) => !forbidden.has(id) &&
                        people.filter((p) => p.user_id === id).length === 1);
                    if (!owner)
                        throw new BusinessRuleError(409, "OWNER_REQUIRED", "Configure an independent company owner at the top of the reporting chain.");
                    reviewer = people.find((p) => p.user_id === owner)!;
                    authority = "holding_owner";
                }
            }
            if (forbidden.has(reviewer.user_id))
                throw new BusinessRuleError(409, "SELF_APPROVAL", "An independent department approver is required.");
            const steps: RouteStep[] = [{ userId: reviewer.user_id, authority }];
            return {
                steps,
                blockedReason: null,
                scope,
                checks: ports.routeChecks([...new Map([staff, heads[0], ...chain].map(p => [p.engagement_id, p])).values()], companyId, { departmentId: staff.department_id, version: head.version }),
            };
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError))
                throw error;
            return { steps: [], blockedReason: error.message, scope };
        }
    }
    function notification(scope: RecordScope, userId: string, type: string, id: string, version: number, message: string) {
        return r.stageNotificationEffect(ports.identifiers.next(), scope.company_id, userId, type, id, version, message, ports.clock.now());
    }
    return { staffPeople, rosterStaffPeople, activeStaff, staffGuard, companyAuthority, authorityGuard, requireCompanyAuthority, scopeActor, reportingChain, eligibleRoleUsers, requestRoute, notification };
}
