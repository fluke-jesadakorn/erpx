import { nextOccurrence } from "@erpx/feature-work_projection/lib/routine-calendar";
import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { notification } from "@erpx/server-operations/public/staff-policy";
import type {taskRoute} from '@erpx/server-workflow/public/approval-planning';
import type {openApproval} from '@erpx/server-workflow/public/approval-planning';
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import type { productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import { type TemplateRow, type ScheduleRow, eventScope } from '@erpx/server-operations/domain/routines';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
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
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    occurrenceUnit<Result extends {next:number|null;generated:boolean}>(schedule:ScheduleRow,at:number,run:()=>Promise<Result>):Promise<Result|undefined>;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
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
    /**
     * Retained TypeScript scheduled-occurrence scheduler. The manual run is
     * native; this scheduler stays until the native occurrence generator owns
     * the schedule cursor, execution lineage and purchase/shadow fences.
     * Removal condition: native `routine.occurrence` generation passes the
     * scheduler PostgreSQL suites and the retained generation oracle migrates.
     */
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
    return { listTemplates, getTemplate, canEditTemplate, generateOccurrences };
}
