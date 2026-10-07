import { z } from 'zod';
import type { levelChangeStatements } from "@/server/modules/operations/public/staff-settings";
import type { requireCompanyAuthority, authorityGuard } from "@/server/modules/operations/public/staff-policy";
import type { organizationGuard, validateStaffOrganization } from "@/server/modules/organization/public/organization";
import type { rosterPositionExitStatements } from "../public/roster-position-exit";
import type { recommendationStatements } from "@/server/modules/organization/public/staff-defaults";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock } from '../../shared';
import type { RosterCommands } from './people-roster';
import { directorySchema, directoryUpdateSchema, directoryStatusSchema, canManagePeople } from '../domain/people-directory';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface PeopleDirectoryRepository<Effect> {
    readDirectoryOptionsNodes(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readDirectoryPersonPerson(value: string | null, value2: string | null, personId: string, companyId: string, value3: string | null, value4: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Record<string, string | number | null> | null>;
    guard3(id: string, value: "branch", branchId: string, companyId: string): Effect;
    guard4(id: string, value: "department", departmentId: string | null, departmentId2: string | null, companyId: string, branchId: string): Effect;
    guard5(id: string, value: "manager", managerPersonId: string | null, managerPersonId2: string | null, companyId: string): Effect;
    readSaveDirectoryPersonAccount(linkedMemberUserId: string): Promise<{
        id: string;
        email: string;
    } | null>;
    guard7(commandId: string, value: "member-link", id: string, companyId: string, now: number, now2: number, id2: string, companyId2: string, id3: string): Effect;
    guard8(commandId: string, value: "engagement-version", engagement: string, value2: number): Effect;
    guard9(commandId: string, value: "email", companyId: string, email: string, id: string): Effect;
    guard10(commandId: string, value: "version", id: string, expectedVersion: number): Effect;
    stageSaveDirectoryPersonEffect(legalName: string, value: string | null, email: string, value2: string | null, value3: string | number | null, value4: string | number | null, countryCode: string, now: number, id: string): Effect;
    stageSaveDirectoryPersonEffect2(branchId: string, departmentId: string | null, jobTitle: string, workerType: "employee" | "contractor", employmentType: "shift" | "contractor" | "salaried" | "daily" | "hourly", startDate: string, currency: "USD" | "EUR" | "GBP" | "THB" | "SGD" | "CAD", managerPersonId: string | null, now: number, engagement: string): Effect;
    stageSaveDirectoryPersonEffect3(legalName: string, branchId: string, departmentId: string | null, id: string): Effect;
    stageSaveDirectoryPersonEffect4(id: string, companyId: string, legalName: string, value: string | null, email: string, value2: string | null, value3: string | null, value4: string | null, countryCode: string, userId: string, now: number, now2: number): Effect;
    stageSaveDirectoryPersonEffect5(engagement: string, id: string, companyId: string, branchId: string, departmentId: string | null, workerType: "employee" | "contractor", employmentType: "shift" | "contractor" | "salaried" | "daily" | "hourly", startDate: string, jobTitle: string, currency: "USD" | "EUR" | "GBP" | "THB" | "SGD" | "CAD", managerPersonId: string | null, userId: string, now: number, now2: number): Effect;
    stageSaveDirectoryPersonEffect6(value: string, id: string, companyId: string, branchId: string, departmentId: string | null, legalName: string, now: number): Effect;
    stageSaveDirectoryPersonEffect7(levelId: string, engagement: string): Effect;
    stageSaveDirectoryPersonEffect8(linkedMemberUserId: string, id: string): Effect;
    stageSaveDirectoryPersonEffect9(linkedMemberUserId: string, id: string): Effect;
    stageSaveDirectoryPersonEffect10(positionId: string | null, engagement: string): Effect;
    stageSaveDirectoryPersonEffect11(value: string, companyId: string, engagement: string, value2: string, value3: string, userId: string, reason: string, commandId: string, now: number): Effect;
    readSaveDirectoryPersonPrevious(companyId: string, position_id: string | number): Promise<{
        roster_required: number;
    } | null>;
    readSaveDirectoryPersonNext(companyId: string, positionId: string): Promise<{
        roster_required: number;
    } | null>;
    readSetDirectoryStatusEffect(companyId: string, personId: string): Promise<Record<string, unknown> | null>;
    guard25(commandId: string, value: "ended-engagements", companyId: string, personId: string): Effect;
    guard26(commandId: string, value: "version", personId: string, expectedVersion: number): Effect;
    guard27(commandId: string, value: "active-level", personId: string): Effect;
    stageSetDirectoryStatusEffect(status: "active" | "inactive" | "archived", now: number, personId: string): Effect;
    stageSetDirectoryStatusEffect2(value: "active" | "suspended", now: number, personId: string): Effect;
    stageSetDirectoryStatusEffect3(value: "active" | "inactive", personId: string): Effect;
    stageSetDirectoryStatusEffect4(now: number, user_id: string | number, companyId: string): Effect;
    readPersonAccessAccount(user_id: string | number): Promise<Record<string, unknown> | null>;
    readPersonAccessAssignments(user_id: string | number, companyId: string): Promise<{
        results: (Record<string, string | number | null>)[];
    }>;
}
export interface PeopleDirectoryPorts<Effect> {
    repository: PeopleDirectoryRepository<Effect>;
    clock: Clock;
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: RosterCommands<Effect>;
    levelChangeStatements(...args: Tail<Parameters<typeof levelChangeStatements>>): PortResult<ReturnType<typeof levelChangeStatements>, Effect>;
    requireCompanyAuthority(...args: Tail<Parameters<typeof requireCompanyAuthority>>): PortResult<ReturnType<typeof requireCompanyAuthority>, Effect>;
    authorityGuard(...args: Tail<Parameters<typeof authorityGuard>>): PortResult<ReturnType<typeof authorityGuard>, Effect>;
    organizationGuard(...args: Tail<Parameters<typeof organizationGuard>>): PortResult<ReturnType<typeof organizationGuard>, Effect>;
    validateStaffOrganization(...args: Tail<Parameters<typeof validateStaffOrganization>>): PortResult<ReturnType<typeof validateStaffOrganization>, Effect>;
    rosterPositionExitStatements(...args: Tail<Parameters<typeof rosterPositionExitStatements>>): PortResult<ReturnType<typeof rosterPositionExitStatements>, Effect>;
    recommendationStatements(...args: Tail<Parameters<typeof recommendationStatements>>): PortResult<ReturnType<typeof recommendationStatements>, Effect>;
}
export function createPeopleDirectoryApplication<Effect>(ports: PeopleDirectoryPorts<Effect>) {
    const repository = ports.repository;
    function manage(actor: WorkContext) {
        return ports.authorize(actor, "people.manage");
    }
    async function directoryOptions(actor: WorkContext) {
        await ports.prepare(actor.holdingId);
        const nodes = await repository.readDirectoryOptionsNodes(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        return nodes.results;
    }
    async function directoryPerson(actor: WorkContext, personId: string, engagementId?: string) {
        await ports.prepare(actor.holdingId);
        const person = await repository.readDirectoryPersonPerson(engagementId ?? null, engagementId ?? null, personId, actor.companyId, engagementId ?? null, engagementId ?? null, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!person)
            throw new BusinessRuleError(404, "PERSON_NOT_FOUND", "Person not found in this workspace.");
        return person;
    }
    function placement(actor: WorkContext, id: string, input: z.infer<typeof directorySchema>) {
        if ((actor.branchId && input.branchId !== actor.branchId) ||
            (actor.departmentId && input.departmentId !== actor.departmentId))
            throw new BusinessRuleError(403, "SCOPE_FORBIDDEN", "Choose a placement in your assigned workspace.");
        if ((input.workerType === "contractor") !==
            (input.employmentType === "contractor"))
            throw new BusinessRuleError(422, "WORKER_TYPE_INVALID", "Contractor worker and employment types must agree.");
        return [
            repository.guard3(id, "branch", input.branchId, actor.companyId),
            repository.guard4(id, "department", input.departmentId, input.departmentId, actor.companyId, input.branchId),
            repository.guard5(id, "manager", input.managerPersonId, input.managerPersonId, actor.companyId),
        ];
    }
    async function saveDirectoryPerson(actor: WorkContext, personId: string | null, input: z.infer<typeof directoryUpdateSchema>, commandId: string) {
        actor = manage(actor);
        await ports.prepare(actor.holdingId);
        const current = personId ? await directoryPerson(actor, personId, input.engagementId) : null;
        // Stable IDs make a lost-response retry target the same aggregate.
        const id = personId ?? `person-${commandId}`;
        const engagement = String(current?.engagement_id ?? `eng-${commandId}`);
        const version = current ? input.expectedVersion + 1 : 1;
        const now = ports.clock.now();
        const steps = [await ports.organizationGuard(actor.companyId, commandId), ...placement(actor, commandId, input)];
        if (input.linkedMemberUserId) {
            await ports.requireCompanyAuthority(actor);
            const account = await repository.readSaveDirectoryPersonAccount(input.linkedMemberUserId);
            if (!account ||
                account.email.toLowerCase() !== input.email.toLowerCase() ||
                (current?.user_id && current.user_id !== account.id))
                throw new BusinessRuleError(422, "MEMBER_LINK_INVALID", "Choose the existing company member and their verified account email.");
            steps.push(ports.authorityGuard(commandId, actor), repository.guard7(commandId, "member-link", account.id, actor.companyId, now, now, account.id, actor.companyId, id));
        }
        const levelId = input.levelId ?? (current?.level_id ? String(current.level_id) : null);
        const positionId = input.positionId === undefined ? (current?.position_id ? String(current.position_id) : null) : input.positionId;
        await ports.validateStaffOrganization(actor.companyId, { person_id: id, branch_id: input.branchId, department_id: input.departmentId, position_id: positionId, level_id: levelId, manager_person_id: input.managerPersonId }, current?.engagement_status === "active");
        if (current?.engagement_status === "active" && !levelId)
            throw new BusinessRuleError(422, "LEVEL_REQUIRED", "Assign a staff level before updating an active engagement.");
        if (current)
            steps.push(repository.guard8(commandId, "engagement-version", engagement, input.expectedEngagementVersion ?? Number(current.engagement_version)));
        steps.push(repository.guard9(commandId, "email", actor.companyId, input.email, id));
        if (input.managerPersonId === id)
            throw new BusinessRuleError(422, "SELF_MANAGER", "A person cannot report to themselves.");
        if (current) {
            steps.push(repository.guard10(commandId, "version", id, input.expectedVersion));
            steps.push(repository.stageSaveDirectoryPersonEffect(input.legalName, input.preferredName || null, input.email, input.phone || null, input.workEmail === undefined ? current.work_email : input.workEmail || null, input.workPhone === undefined ? current.work_phone : input.workPhone || null, input.countryCode, now, id));
            steps.push(repository.stageSaveDirectoryPersonEffect2(input.branchId, input.departmentId, input.jobTitle, input.workerType, input.employmentType, input.startDate, input.currency, input.managerPersonId, now, engagement));
            steps.push(repository.stageSaveDirectoryPersonEffect3(input.legalName, input.branchId, input.departmentId, id));
        }
        else {
            steps.push(repository.stageSaveDirectoryPersonEffect4(id, actor.companyId, input.legalName, input.preferredName || null, input.email, input.phone || null, input.workEmail || null, input.workPhone || null, input.countryCode, actor.userId, now, now));
            steps.push(repository.stageSaveDirectoryPersonEffect5(engagement, id, actor.companyId, input.branchId, input.departmentId, input.workerType, input.employmentType, input.startDate, input.jobTitle, input.currency, input.managerPersonId, actor.userId, now, now));
            steps.push(repository.stageSaveDirectoryPersonEffect6(`employee-${commandId}`, id, actor.companyId, input.branchId, input.departmentId, input.legalName, now));
        }
        if (levelId) {
            steps.push(...ports.levelChangeStatements(commandId, actor, engagement, levelId, current?.level_id ? String(current.level_id) : null, "Engagement details updated"), repository.stageSaveDirectoryPersonEffect7(levelId, engagement));
        }
        if (input.linkedMemberUserId)
            steps.push(repository.stageSaveDirectoryPersonEffect8(input.linkedMemberUserId, id), repository.stageSaveDirectoryPersonEffect9(input.linkedMemberUserId, id));
        steps.push(repository.stageSaveDirectoryPersonEffect10(positionId, engagement), repository.stageSaveDirectoryPersonEffect11(`org-history-${commandId}`, actor.companyId, engagement, JSON.stringify({ positionId: current?.position_id ?? null, levelId: current?.level_id ?? null, managerPersonId: current?.manager_person_id ?? null, branchId: current?.branch_id ?? null, departmentId: current?.department_id ?? null }), JSON.stringify({ positionId, levelId, managerPersonId: input.managerPersonId, branchId: input.branchId, departmentId: input.departmentId }), actor.userId, input.reason, commandId, now));
        if (current?.position_id && current.position_id !== positionId) {
            const previous = await repository.readSaveDirectoryPersonPrevious(actor.companyId, current.position_id);
            const next = positionId ? await repository.readSaveDirectoryPersonNext(actor.companyId, positionId) : null;
            if (previous?.roster_required === 1 && next?.roster_required !== 1)
                steps.push(...ports.rosterPositionExitStatements(actor, [engagement], commandId, input.reason));
        }
        steps.push(...await ports.recommendationStatements(actor, {
            engagementId: engagement, positionId, levelId,
            priorLevelId: current?.level_id ? String(current.level_id) : null,
            startDate: input.startDate, currency: input.currency,
            acceptPay: input.acceptRecommendedPay, acceptLeave: input.acceptRecommendedLeave,
            commandId,
        }));
        return ports.commands.commit(actor, commandId, current ? "people.directory.update" : "people.directory.create", input, id, version, { id, version }, steps);
    }
    async function setDirectoryStatus(actor: WorkContext, personId: string, input: z.infer<typeof directoryStatusSchema>, commandId: string) {
        actor = manage(actor);
        await ports.prepare(actor.holdingId);
        const replay = await ports.commands.replay(actor, commandId, "people.directory.status", personId, input);
        if (replay)
            return replay;
        const person = await directoryPerson(actor, personId);
        const graphGuard = await ports.organizationGuard(actor.companyId, commandId);
        if (!actor.beta) {
            if (input.status === "inactive")
                throw new BusinessRuleError(409, "LIFECYCLE_REQUIRED", "Use Suspend access or End employment to review all linked accounts and the effective time.");
            if (input.status === "archived" && person.engagement_status !== "ended")
                throw new BusinessRuleError(409, "ENGAGEMENT_NOT_ENDED", "End employment before archiving its history.");
            if (input.status === "active" && person.status !== "archived" && await repository.readSetDirectoryStatusEffect(actor.companyId, personId))
                throw new BusinessRuleError(409, "LIFECYCLE_REQUIRED", "Request new access through the lifecycle workflow before returning to work.");
        }
        if (input.status === "active" && person.engagement_status !== "ended")
            await ports.validateStaffOrganization(actor.companyId, { person_id: personId, branch_id: person.branch_id as string | null, department_id: person.department_id as string | null, position_id: person.position_id as string | null, level_id: person.level_id as string | null, manager_person_id: person.manager_person_id as string | null }, true);
        if (person.user_id === actor.userId && input.status !== "active")
            throw new BusinessRuleError(409, "SELF_DEACTIVATION", "Another HR administrator must deactivate your employee record.");
        return ports.commands.commit(actor, commandId, "people.directory.status", input, personId, input.expectedVersion + 1, { id: personId, status: input.status, version: input.expectedVersion + 1 }, [
            graphGuard,
            ...(!actor.beta && input.status === "archived" ? [repository.guard25(commandId, "ended-engagements", actor.companyId, personId)] : []),
            repository.guard26(commandId, "version", personId, input.expectedVersion),
            ...(input.status === "active"
                ? [
                    repository.guard27(commandId, "active-level", personId),
                ]
                : []),
            repository.stageSetDirectoryStatusEffect(input.status, ports.clock.now(), personId),
            repository.stageSetDirectoryStatusEffect2(input.status === "active" ? "active" : "suspended", ports.clock.now(), personId),
            repository.stageSetDirectoryStatusEffect3(input.status === "active" && person.engagement_status !== "ended" ? "active" : "inactive", personId),
            // Revoke this company's access; restoration always requires an explicit new assignment.
            ...(input.status === "active" || !person.user_id
                ? []
                : [
                    repository.stageSetDirectoryStatusEffect4(ports.clock.now(), person.user_id, actor.companyId),
                ]),
        ]);
    }
    async function personAccess(actor: WorkContext, personId: string) {
        const person = await directoryPerson(actor, personId);
        if (!canManagePeople(actor) && person.user_id !== actor.userId)
            return null;
        if (!person.user_id)
            return { account: null, assignments: [] };
        await ports.prepare(actor.holdingId);
        const account = await repository.readPersonAccessAccount(person.user_id);
        const assignments = await repository.readPersonAccessAssignments(person.user_id, actor.companyId);
        return {
            account,
            assignments: assignments.results.map((a): Record<string, string | number | null> => ({
                ...a,
                access_status: a.revoked_at
                    ? "revoked"
                    : Number(a.valid_until) <= ports.clock.now()
                        ? "expired"
                        : "active",
            })),
        };
    }
    return { directoryOptions, directoryPerson, saveDirectoryPerson, setDirectoryStatus, personAccess };
}
