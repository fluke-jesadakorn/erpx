import type { getProject } from "@erpx/server-projects/public/staff-projects";
import { z } from "zod";
import { nextOccurrence, type RoutineRule } from "@erpx/feature-work_projection/lib/routine-calendar";
import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type {validateTaskConfiguration} from '@erpx/server-operations/public/staff-tasks';
import type {configGuards} from '@erpx/server-operations/public/staff-tasks';
import type { activeStaff } from "@erpx/server-operations/public/staff-policy";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { staffGuard } from "@erpx/server-operations/public/staff-policy";
import type { notification } from "@erpx/server-operations/public/staff-policy";
import type {taskRoute} from '@erpx/server-workflow/public/approval-planning';
import type {openApproval} from '@erpx/server-workflow/public/approval-planning';
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import type { productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import { templateSchema, scheduleSchema, type TemplateRow, type ScheduleRow, eventScope } from '@erpx/server-operations/domain/routines';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { OperationsCommands } from '@erpx/server-operations/domain/commands';
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
export interface RoutinesRepository<Effect> {
    readListTemplatesEffect(companyId: string, userId: string, value: 0 | 1): Promise<{
        results: (TemplateRow)[];
    }>;
    readGetTemplateRow(id: string, companyId: string): Promise<TemplateRow | null>;
    guard3(key: string, value: "template-authority", id: string, companyId: string, userId: string, assignmentId: string, now: number, now2: number): Effect;
    guard4(key: string, value: "template-version", recordId: string, expectedVersion: number): Effect;
    stageSaveTemplateEffect(title: string, description: string, assigneeUserId: string, projectId: string | null, value: string, value2: string, value3: 0 | 1, now: number, recordId: string): Effect;
    stageSaveTemplateEffect2(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, assigneeUserId: string, projectId: string | null, value: string, value2: string, value3: 0 | 1, now: number, now2: number): Effect;
    guard7(key: string, value: "schedule-version", recordId: string, templateId: string, companyId: string, expectedVersion: number): Effect;
    stageSaveScheduleEffect(frequency: "daily" | "weekly" | "monthly", timezone: string, occurrence_time: string, start_date: string, end_date: string | null, weekday: number | null, month_day: number | null, dueOffsetHours: number, next: number | null, now: number, recordId: string): Effect;
    stageSaveScheduleEffect2(recordId: string, templateId: string, companyId: string, frequency: "daily" | "weekly" | "monthly", timezone: string, occurrence_time: string, start_date: string, end_date: string | null, weekday: number | null, month_day: number | null, dueOffsetHours: number, next: number | null, now: number, now2: number, now3: number): Effect;
    stageSaveScheduleEffect3(key: string, recordId: string, now: number): Effect;
    readChangeScheduleStateRow(id: string, companyId: string): Promise<ScheduleRow | null>;
    guard12(key: string, value: "schedule-version", id: string, version: number, value2: "paused" | "active"): Effect;
    stageChangeScheduleStateEffect(value: "paused" | "active", value2: number | null, now: number, id: string): Effect;
    stageChangeScheduleStateEffect2(now: number, id: string): Effect;
    stageChangeScheduleStateEffect3(key: string, id: string, now: number): Effect;
    stageEndScheduleEffect(now:number,id:string):Effect;
    readOccurrenceStatementsEffect(project_id: string, company_id: string): Promise<Record<string, unknown> | null>;
    guard17(key: string, value: "template-version", id: string, version: number): Effect;
    stageOccurrenceStatementsSteps(id: string, company_id: string, branch_id: string | null, department_id: string | null, title: string, description: string, creator: string, assignee_user_id: string, dueAt: number, project_id: string | null, start_approvers_json: string, completion_approvers_json: string, state: "draft" | "start_approval" | "ready", blocked: string | null, id2: string, version: number, value: string | null, occurrenceAt: number | null, now: number, now2: number): Effect;
    readGenerateOccurrencesSchedules(now: number, value: number): Promise<{
        results: (ScheduleRow)[];
    }>;
    readGenerateOccurrencesPause(id: string, at: number, at2: number): Promise<{
        ended_at: number | null;
    } | null>;
    readGenerateOccurrencesTemplate(template_id: string, company_id: string): Promise<TemplateRow | null>;
    readGenerateOccurrencesPrior(id: string, at: number): Promise<Record<string, unknown> | null>;
    guard23(key: string, value: "schedule-cursor", id: string, version: number, at: number): Effect;
    stageGenerateOccurrencesEffect(next: number | null, next2: number | null, value: 0 | 1, at: number, now: number, id: string): Effect;
    stageGenerateOccurrencesEffect2(key: string, key2: string, value: "system", value2: "scheduler", value3: "routine.occurrence", value4: "staff_task", id: string, value5: 1, value6: "Generate scheduled occurrence", now: number): Effect;
    stageGenerateOccurrencesEffect3(key: string, key2: string, value: "task.generated", value2: "staff_task", id: string, value3: 1, value4: string, now: number): Effect;
    stageGenerateOccurrencesEffect4(key: string): Effect;
    readGenerateOccurrencesLatest(id: string): Promise<Pick<ScheduleRow, "next_occurrence_at" | "version" | "state"> | null>;
    stageGenerateOccurrencesEffect5(value: string, now: number, id: string, version: number): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readGenerateOccurrencesLag(now: number): Promise<{
        oldest: number | null;
    } | null>;
}
export interface RoutinesPorts<Effect> {
    repository: RoutinesRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OperationsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    occurrenceUnit<Result extends {next:number|null;generated:boolean}>(schedule:ScheduleRow,at:number,run:()=>Promise<Result>):Promise<Result|undefined>;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    getProject(...args: Tail<Parameters<typeof getProject>>): PortResult<ReturnType<typeof getProject>, Effect>;
    validateTaskConfiguration(...args: Tail<Parameters<typeof validateTaskConfiguration>>): PortResult<ReturnType<typeof validateTaskConfiguration>, Effect>;
    configGuards(...args: Tail<Parameters<typeof configGuards>>): PortResult<ReturnType<typeof configGuards>, Effect>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
    staffGuard(...args: Tail<Parameters<typeof staffGuard>>): PortResult<ReturnType<typeof staffGuard>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    taskRoute(...args: Tail<Parameters<typeof taskRoute>>): PortResult<ReturnType<typeof taskRoute>, Effect>;
    openApproval(...args: Tail<Parameters<typeof openApproval>>): PortResult<ReturnType<typeof openApproval>, Effect>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): PortResult<ReturnType<typeof assertProductionModuleEnabled>, Effect>;
    productionFeatureGuard(...args: Tail<Parameters<typeof productionFeatureGuard>>): PortResult<ReturnType<typeof productionFeatureGuard>, Effect>;
}
export function createRoutinesApplication<Effect>(ports: RoutinesPorts<Effect>) {
    const r = ports.repository;
    async function listTemplates(actor: StaffActor) {
        const owner = await ports.companyAuthority(actor, ["holding_owner"]);
        return (await r.readListTemplatesEffect(actor.companyId, actor.userId, owner ? 1 : 0)).results;
    }
    async function getTemplate(actor: StaffActor, id: string) {
        const row = await r.readGetTemplateRow(id, actor.companyId);
        if (!row ||
            (!row.published &&
                row.creator_user_id !== actor.userId &&
                !(await ports.companyAuthority(actor, ["holding_owner"]))))
            throw new BusinessRuleError(404, "TEMPLATE_NOT_FOUND", "Routine template not found.");
        return row;
    }
    async function canEditTemplate(actor: StaffActor, row: TemplateRow) {
        return (row.creator_user_id === actor.userId ||
            (await ports.companyAuthority(actor, ["holding_owner"])));
    }
    function templateAuthorityGuard(key: string, actor: StaffActor, id: string) {
        const now = ports.clock.now();
        return r.guard3(key, "template-authority", id, actor.companyId, actor.userId, actor.assignmentId, now, now);
    }
    async function saveTemplate(actor: StaffActor, id: string | null, input: z.infer<typeof templateSchema>, key: string, cloneId?: string) {
        const recordId = id ?? `routine-${key}`, operation = cloneId
            ? "routine.clone"
            : id
                ? "routine.update"
                : "routine.create", commandInput = { ...input, cloneId: cloneId ?? null };
        const prior = await ports.commands.replay(actor, key, operation, recordId, commandInput);
        if (prior)
            return prior;
        const staff = await ports.activeStaff(actor.companyId, actor.userId), current = id ? await getTemplate(actor, id) : null;
        if (cloneId)
            await getTemplate(actor, cloneId);
        if (current && !(await canEditTemplate(actor, current)))
            throw new BusinessRuleError(403, "TEMPLATE_OWNER_REQUIRED", "Only the creator or company owner edits this source. Clone it to make your own version.");
        await ports.validateTaskConfiguration(actor, input, current?.creator_user_id ?? actor.userId);
        const now = ports.clock.now(), version = current ? input.expectedVersion + 1 : 1;
        return ports.commands.commit(actor, key, operation, commandInput, recordId, version, { id: recordId, version }, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            ...ports.configGuards(key, actor, input),
            ...(current
                ? [
                    templateAuthorityGuard(key, actor, recordId),
                    r.guard4(key, "template-version", recordId, input.expectedVersion),
                ]
                : []),
            current
                ? r.stageSaveTemplateEffect(input.title, input.description, input.assigneeUserId, input.projectId, JSON.stringify(input.startApprovers), JSON.stringify(input.completionApprovers), input.published ? 1 : 0, now, recordId)
                : r.stageSaveTemplateEffect2(recordId, actor.companyId, staff.branch_id, staff.department_id, input.title, input.description, actor.userId, input.assigneeUserId, input.projectId, JSON.stringify(input.startApprovers), JSON.stringify(input.completionApprovers), input.published ? 1 : 0, now, now),
        ], eventScope(current ?? staff));
    }
    async function saveSchedule(actor: StaffActor, templateId: string, id: string | null, input: z.infer<typeof scheduleSchema>, key: string) {
        const recordId = id ?? `schedule-${key}`, operation = id ? "routine.schedule.update" : "routine.schedule.create";
        const prior = await ports.commands.replay(actor, key, operation, recordId, input);
        if (prior)
            return prior;
        const template = await getTemplate(actor, templateId);
        if (!(await canEditTemplate(actor, template)))
            throw new BusinessRuleError(403, "TEMPLATE_OWNER_REQUIRED", "The source owner manages its schedules.");
        const rule: RoutineRule = {
            frequency: input.frequency,
            timezone: input.timezone,
            occurrence_time: input.occurrenceTime,
            start_date: input.startDate,
            end_date: input.endDate,
            weekday: input.weekday,
            month_day: input.monthDay,
        }, now = ports.clock.now(), next = nextOccurrence(rule, now - 1), version = id ? input.expectedVersion + 1 : 1;
        return ports.commands.commit(actor, key, operation, input, recordId, version, { id: recordId, state: "paused", version }, [
            templateAuthorityGuard(key, actor, templateId),
            ...(id
                ? [
                    r.guard7(key, "schedule-version", recordId, templateId, actor.companyId, input.expectedVersion),
                ]
                : []),
            id
                ? r.stageSaveScheduleEffect(rule.frequency, rule.timezone, rule.occurrence_time, rule.start_date, rule.end_date, rule.weekday, rule.month_day, input.dueOffsetHours, next, now, recordId)
                : r.stageSaveScheduleEffect2(recordId, templateId, actor.companyId, rule.frequency, rule.timezone, rule.occurrence_time, rule.start_date, rule.end_date, rule.weekday, rule.month_day, input.dueOffsetHours, next, now, now, now),
            ...(!id
                ? [
                    r.stageSaveScheduleEffect3(key, recordId, now),
                ]
                : []),
        ], eventScope(template));
    }
    async function changeScheduleState(actor: StaffActor, id: string, version: number, resume: boolean, key: string) {
        const input = { expectedVersion: version, resume }, operation = resume ? "routine.schedule.resume" : "routine.schedule.pause";
        const prior = await ports.commands.replay(actor, key, operation, id, input);
        if (prior)
            return prior;
        const row = await r.readChangeScheduleStateRow(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "SCHEDULE_NOT_FOUND", "Schedule not found.");
        const template = await getTemplate(actor, row.template_id);
        if (!(await canEditTemplate(actor, template)))
            throw new BusinessRuleError(403, "TEMPLATE_OWNER_REQUIRED", "The source owner manages its schedules.");
        if (resume && !template.published)
            throw new BusinessRuleError(409, "TEMPLATE_UNPUBLISHED", "Publish the template before enabling its schedule.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, operation, input, id, version + 1, { id, state: resume ? "active" : "paused", version: version + 1 }, [
            templateAuthorityGuard(key, actor, template.id),
            r.guard12(key, "schedule-version", id, version, resume ? "paused" : "active"),
            r.stageChangeScheduleStateEffect(resume ? "active" : "paused", resume ? null : now, now, id),
            resume
                ? r.stageChangeScheduleStateEffect2(now, id)
                : r.stageChangeScheduleStateEffect3(key, id, now),
        ], eventScope(template));
    }
    async function endSchedule(actor:StaffActor,id:string,version:number,key:string){
        const input={expectedVersion:version},operation='routine.schedule.end',prior=await ports.commands.replay(actor,key,operation,id,input);if(prior)return prior;
        const row=await r.readChangeScheduleStateRow(id,actor.companyId);if(!row)throw new BusinessRuleError(404,'SCHEDULE_NOT_FOUND','Schedule not found.');
        const template=await getTemplate(actor,row.template_id);if(!await canEditTemplate(actor,template))throw new BusinessRuleError(403,'TEMPLATE_OWNER_REQUIRED','The source owner manages its schedules.');
        if(row.state==='ended')throw new BusinessRuleError(409,'SCHEDULE_ENDED','This schedule has already ended.');const now=ports.clock.now();
        return ports.commands.commit(actor,key,operation,input,id,version+1,{id,state:'ended',version:version+1},[templateAuthorityGuard(key,actor,template.id),r.guard12(key,'schedule-version',id,version,row.state),r.stageEndScheduleEffect(now,id),r.stageChangeScheduleStateEffect2(now,id)],eventScope(template));
    }
    async function occurrenceStatements(template: TemplateRow, id: string, key: string, dueAt: number, creator: string, schedule: ScheduleRow | null, occurrenceAt: number | null) {
        const now = ports.clock.now(), start: string[] = JSON.parse(template.start_approvers_json), completion: string[] = JSON.parse(template.completion_approvers_json);
        const startRoute = await ports.taskRoute(template, creator, template.assignee_user_id, "start", start), completionRoute = await ports.taskRoute(template, creator, template.assignee_user_id, "completion", completion);
        let blocked = startRoute.blockedReason ?? completionRoute.blockedReason;
        if (template.project_id &&
            !(await r.readOccurrenceStatementsEffect(template.project_id, template.company_id)))
            blocked ??= "The project is unavailable.";
        const state = blocked ? "draft" : start.length ? "start_approval" : "ready";
        const steps: Effect[] = [
            r.guard17(key, "template-version", template.id, template.version),
            r.stageOccurrenceStatementsSteps(id, template.company_id, template.branch_id, template.department_id, template.title, template.description, creator, template.assignee_user_id, dueAt, template.project_id, template.start_approvers_json, template.completion_approvers_json, state, blocked, template.id, template.version, schedule?.id ?? null, occurrenceAt, now, now),
            ports.notification(template, template.assignee_user_id, "task", id, 1, blocked
                ? "Routine task blocked: correction required"
                : "Routine task assigned to you"),
        ];
        if (!blocked && start.length)
            steps.push(...(await ports.openApproval(key, "task", id, "start", creator, template.assignee_user_id, startRoute, 1)).statements);
        if (blocked) {
            steps.push(ports.notification(template, creator, "task", id, 1, `Routine blocked: ${blocked}`));
            console.warn(JSON.stringify({
                event: "routine_blocked",
                taskId: id,
                templateId: template.id,
                companyId: template.company_id,
                reason: blocked,
            }));
        }
        return { steps, state };
    }
    async function runTemplate(actor: StaffActor, id: string, dueAt: number, key: string) {
        const taskId = `task-${key}`, input = { templateId: id, dueAt }, prior = await ports.commands.replay(actor, key, "routine.run", taskId, input);
        if (prior)
            return prior;
        await ports.activeStaff(actor.companyId, actor.userId);
        const template = await getTemplate(actor, id);
        if (!template.published)
            throw new BusinessRuleError(409, "TEMPLATE_UNPUBLISHED", "Publish the template before running it.");
        // A runner is the creator of the occurrence and must have access to the chosen project.
        if (template.project_id) {
            await ports.getProject(actor, template.project_id);
        }
        const prepared = await occurrenceStatements(template, taskId, key, dueAt, actor.userId, null, null);
        return ports.commands.commit(actor, key, "routine.run", input, taskId, 1, { id: taskId, state: prepared.state, version: 1 }, [ports.staffGuard(key, actor.companyId, actor.userId), ...prepared.steps], eventScope(template));
    }
    async function generateOccurrences(now = ports.clock.now(), limit = 20) {
        let generated = 0, examined = 0;
        const schedules = (await r.readGenerateOccurrencesSchedules(now, Math.min(limit, 20))).results;
        for (const schedule of schedules) {
            while (schedule.next_occurrence_at !== null &&
                schedule.next_occurrence_at <= now &&
                examined < limit) {
                examined++;
                const at = schedule.next_occurrence_at;
                try {
                    const outcome=await ports.occurrenceUnit(schedule,at,async()=>{
                    const pause = await r.readGenerateOccurrencesPause(schedule.id, at, at);
                const next = nextOccurrence(schedule, pause?.ended_at ?? at), key = `occurrence-${schedule.id}-${at}`, id = `task-${schedule.id}-${at}`;
                    const template = await r.readGenerateOccurrencesTemplate(schedule.template_id, schedule.company_id);
                    if (!template)
                        throw new Error("TEMPLATE_UNAVAILABLE");
                    const prior = await r.readGenerateOccurrencesPrior(schedule.id, at);
                    const prepared = pause || prior
                        ? null
                        : await occurrenceStatements(template, id, key, at + schedule.due_offset_hours * 3600000, template.creator_user_id, schedule, at);
                    await ports.batch([
                        ...ports.productionFeatureGuard(key, schedule.company_id, "operations"),
                        r.guard23(key, "schedule-cursor", schedule.id, schedule.version, at),
                        ...(prepared?.steps ?? []),
                        r.stageGenerateOccurrencesEffect(next, next, prepared ? 1 : 0, at, now, schedule.id),
                        ...(prepared
                            ? [
                                r.stageGenerateOccurrencesEffect2(key, key, "system", "scheduler", "routine.occurrence", "staff_task", id, 1, "Generate scheduled occurrence", now),
                                r.stageGenerateOccurrencesEffect3(key, key, "task.generated", "staff_task", id, 1, JSON.stringify({
                                    id,
                                    state: prepared.state,
                                    version: 1,
                                    ...eventScope(template),
                                }), now),
                            ]
                            : []),
                        r.stageGenerateOccurrencesEffect4(key),
                    ]);
                    return {next,generated:!!prepared};
                    });
                    if(!outcome)break;
                    if(outcome.generated)generated++;
                    schedule.next_occurrence_at=outcome.next;
                }
                catch (error) {
                    if (ports.production) {
                        try {
                            await ports.assertProductionModuleEnabled(schedule.company_id, "operations");
                        }
                        catch {
                            break;
                        }
                    }
                    // A competing generator can win the cursor without making this a generation failure.
                    const latest = await r.readGenerateOccurrencesLatest(schedule.id);
                    if (latest?.next_occurrence_at !== at ||
                        latest?.version !== schedule.version ||
                        latest?.state !== "active")
                        break;
                    const message = error instanceof Error ? error.message : "Generation failed";
                    await r.stageGenerateOccurrencesEffect5(message.slice(0, 500), now, schedule.id, schedule.version);
                    console.error(JSON.stringify({
                        event: "routine_generation_failed",
                        scheduleId: schedule.id,
                        companyId: schedule.company_id,
                        occurrenceAt: at,
                        lagMs: now - at,
                        error: message,
                    }));
                    break;
                }
            }
        }
        const lag = await r.readGenerateOccurrencesLag(now);
        console.log(JSON.stringify({
            event: "routine_generation",
            generated,
            examined,
            lagMs: lag?.oldest == null ? 0 : now - lag.oldest,
        }));
        return { generated, examined };
    }
    return { listTemplates, getTemplate, canEditTemplate, saveTemplate, saveSchedule, changeScheduleState, endSchedule, runTemplate, generateOccurrences };
}
