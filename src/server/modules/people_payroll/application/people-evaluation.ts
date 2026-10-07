import { z } from 'zod';
import { hasPermission } from '@/lib/permissions';
import { assessmentContentSchema, bandSchema, basisSchema, benchmarkSchema, compensationMetrics, cycleSchema, defaultEvaluationPolicy, definitionSchemas, emptyAssessment, evaluationDate, evaluationMetrics, monthlyFtePay, policySchema, roleSchema, type Compensation, type DefinitionKind, type RoleProfile } from "@/lib/people-evaluation";
import { decimalToMinor } from "@/lib/finance-money";
import type { iamMutationGuards } from '../../identity_access/public/iam-store';
import type { snapshotMoney } from '../../finance/public/company-money';
import type { attachmentLinkStatus } from '../../files/public/attachments/service';
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock } from '../../shared';
import type { RosterCommands } from './people-roster';
import { type Row, type AssessmentRow, type EvaluationEmployee, type EvaluationRow, reason, idSchema, version, createEvaluationQuerySchema } from '../domain/people-evaluation';
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
export interface PeopleEvaluationRepository<Effect> {
    stageEvaluationAuthorityEffect(key: string): Effect;
    readEvaluationEmployeesRows(companyId: string, userId: string, companyId2: string, companyId3: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, role: string): Promise<{
        results: (Row)[];
    }>;
    readEvaluationDefinitionRow(id: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", companyId: string, value: number[], v: number | undefined): Promise<{
        id: string;
        kind: DefinitionKind;
        version: number;
        payload_json: string;
        updated_at: number;
    } | null>;
    readEvaluationTodayGroup(engagementId: string): Promise<{
        timezone: string;
    } | null>;
    readSaveEvaluationDefinitionPosition(positionId: string, companyId: string, levelId: string): Promise<Record<string, unknown> | null>;
    readSaveEvaluationDefinitionEffect(levelId: string, companyId: string): Promise<Record<string, unknown> | null>;
    guard7(key: string, value: "unique-cycle", companyId: string, id: string, year: number): Effect;
    stageSaveEvaluationDefinitionEffect(value: string): Effect;
    guard9(key: string, value: "criteria-version", id: string, companyId: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", expectedVersion: number): Effect;
    stageSaveEvaluationDefinitionEffect2(value: string, userId: string, reason: string, now: number, id: string): Effect;
    stageSaveEvaluationDefinitionEffect3(id: string, companyId: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", value: string, userId: string, userId2: string, reason: string, now: number, now2: number): Effect;
    readEvaluationWorkspaceDefinitions(companyId: string): Promise<{
        results: ({
            id: string;
            kind: DefinitionKind;
            version: number;
            payload_json: string;
            updated_at: number;
        })[];
    }>;
    readEvaluationWorkspaceAssessments(companyId: string, id: string, date: string): Promise<{
        results: (AssessmentRow)[];
    }>;
    readEvaluationWorkspaceWages(id: string, date: string, date2: string): Promise<{
        results: (Compensation)[];
    }>;
    readEvaluationWorkspaceBasisRow(id: string, companyId: string, date: string): Promise<{
        version: number;
        payload_json: string;
    } | null>;
    readEvaluationWorkspacePositions(companyId: string): Promise<{
        results: ({
            id: string;
            title: string;
            level_id: string;
        })[];
    }>;
    readEvaluationWorkspaceLevels(companyId: string): Promise<{
        results: ({
            id: string;
            name: string;
        })[];
    }>;
    readImportEvaluationMarketLevels(companyId: string): Promise<{
        results: ({
            id: string;
        })[];
    }>;
    stageImportEvaluationMarketEffect(value: string, companyId: string, value2: string, userId: string, userId2: string, reason: string, now: number, now2: number): Effect;
    readEvaluationAssessmentRow(assessmentId: string, companyId: string): Promise<AssessmentRow | null>;
    readValidateEvaluationRolePeriodHistory(companyId: string, id: string): Promise<{
        results: ({
            before_json: string;
            after_json: string;
            created_at: number;
        })[];
    }>;
    readValidateEvaluationRolePeriodZone(id: string): Promise<{
        timezone: string;
    } | null>;
    stageCreateEvaluationAssessmentEffect(id: string): Effect;
    guard24(key: string, value: "engagement-version", id: string, companyId: string, version: number): Effect;
    stageCreateEvaluationAssessmentEffect2(id: string): Effect;
    guard26(key: string, value: "cycle-open", cycleId: string, version: number): Effect;
    guard27(key: string, value: "period-overlap", id: string, cycleId: string, periodEnd: string, periodStart: string): Effect;
    stageCreateEvaluationAssessmentEffect3(id: string, companyId: string, id2: string, cycleId: string, id3: string, version: number, periodStart: string, periodEnd: string, value: string, userId: string, reason: string, now: number, now2: number): Effect;
    readSaveEvaluationAssessmentEvidence(assessmentId: string, companyId: string): Promise<{
        results: ({
            id: string;
            observed_on: string;
        })[];
    }>;
    stageSaveEvaluationAssessmentGuards(id: string): Effect;
    guard31(key: string, value: "engagement-version", id: string, version: number): Effect;
    guard32(key: string, value: "period-overlap", id: string, cycle_id: string, assessmentId: string, periodEnd: string, periodStart: string): Effect;
    stageSaveEvaluationAssessmentGuards2(assessmentId: string): Effect;
    stageSaveEvaluationAssessmentGuards3(id: string): Effect;
    guard35(key: string, value: "cycle-open", cycle_id: string, version: number): Effect;
    guard36(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string): Effect;
    stageSaveEvaluationAssessmentEffect(periodStart: string, periodEnd: string, value: string, userId: string, userId2: string, reason: string, now: number, assessmentId: string): Effect;
    readTransitionEvaluationAssessmentEffect(userId: string, created_by: string, companyId: string): Promise<boolean | null>;
    readTransitionEvaluationAssessmentEffect2(userId: string, reviewed_by: string, companyId: string): Promise<boolean | null>;
    stageTransitionEvaluationAssessmentEffect(id: string): Effect;
    guard41(key: string, value: "engagement-version", id: string, version: number): Effect;
    stageTransitionEvaluationAssessmentEffect2(assessmentId: string): Effect;
    guard43(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string): Effect;
    stageTransitionEvaluationAssessmentEffect3(id: string): Effect;
    guard45(key: string, value: "cycle-open", cycle_id: string, version: number): Effect;
    guard46(key: string, value: "no-pay-draft", assessmentId: string): Effect;
    stageTransitionEvaluationAssessmentEffect4(value: string, userId: string, value2: string | null, value3: string, reason: string, now: number, assessmentId: string): Effect;
    readEvaluationDetailEvidence(assessmentId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readEvaluationDetailHistory(assessmentId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readEvaluationDetailSalary(id: string): Promise<{
        results: (Compensation)[];
    }>;
    readEvaluationDetailSalary2(assessmentId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readEvaluationDetailFiles(companyId: string, assessmentId: string): Promise<{
        results: ({
            id: string;
            filename: string;
        })[];
    }>;
    readEvaluationEvidenceSourcesTasksprojectslearning(companyId: string, userId: string | null, sourceId: string | null, sourceId2: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: (Row)[];
    }>;
    readEvaluationEvidenceSourcesTasksprojectslearning2(companyId: string, userId: string | null, sourceId: string | null, sourceId2: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: (Row)[];
    }>;
    readEvaluationEvidenceSourcesTasksprojectslearning3(companyId: string, userId: string | null, sourceId: string | null, sourceId2: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: (Row)[];
    }>;
    readAddEvaluationEvidenceLink(sourceId: string | null, assessmentId: string, companyId: string): Promise<Record<string, unknown> | null>;
    stageAddEvaluationEvidenceEffect(id: string): Effect;
    guard58(key: string, value: "engagement-version", id: string, version: number): Effect;
    stageAddEvaluationEvidenceEffect2(assessmentId: string): Effect;
    guard60(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string): Effect;
    stageAddEvaluationEvidenceEffect3(id: string): Effect;
    guard62(key: string, value: "cycle-open", cycle_id: string, version: number): Effect;
    stageAddEvaluationEvidenceEffect4(id: string, companyId: string, assessmentId: string, sourceType: "file" | "kpi" | "task" | "project" | "learning", sourceId: string | null, description: string, observedOn: string, value: string, userId: string, now: number): Effect;
    stageAddEvaluationEvidenceEffect5(userId: string, userId2: string, now: number, assessmentId: string): Effect;
    readReadEvaluationEvidenceEvidence(evidenceId: string, companyId: string): Promise<Row | null>;
    stageConfirmEvaluationPayBasisEffect(engagementId: string): Effect;
    guard67(key: string, value: "engagement-version", engagementId: string, version: number): Effect;
    guard68(key: string, value: "basis-version", engagementId: string, expectedVersion: number): Effect;
    stageConfirmEvaluationPayBasisEffect2(engagementId: string, companyId: string, value: number, effectiveFrom: string, value2: string, userId: string, now: number): Effect;
    readCreateEvaluationCompensationDraftE(id: string, companyId: string): Promise<{
        pay_group_id: string | null;
        status: string;
    } | null>;
    readCreateEvaluationCompensationDraftLastRun(companyId: string, pay_group_id: string): Promise<{
        period_end: string;
    } | null>;
    readCreateEvaluationCompensationDraftWages(id: string, effectiveFrom: string, effectiveFrom2: string): Promise<{
        results: (Compensation & {
            components_json: string;
        })[];
    }>;
    readCreateEvaluationCompensationDraftBasis(id: string, companyId: string, effectiveFrom: string): Promise<{
        version: number;
        payload_json: string;
    } | null>;
    stageCreateEvaluationCompensationDraftEffect(id: string): Effect;
    guard75(key: string, value: "assessment-confirmed", assessmentId: string, expectedVersion: number): Effect;
    guard76(key: string, value: "wage-version", id: string, value2: number): Effect;
    guard77(key: string, value: "engagement-version", id: string, version: number): Effect;
    stageCreateEvaluationCompensationDraftEffect2(compId: string, id: string, effectiveFrom: string, effective_to: string | null, pay_frequency: string, base: number, hourly_rate: number, daily_rate: number, standard_daily_hours: number, expected_hours: number, currency: string, components_json: string, userId: string, now: number, now2: number): Effect;
    stageCreateEvaluationCompensationDraftEffect3(id: string, companyId: string, assessmentId: string, expectedVersion: number, compId: string, id2: string, version: number, version2: number, id3: string, version3: number, version4: number, reason: string, userId: string, now: number, now2: number, value: string, value2: string): Effect;
    readApproveEvaluationCompensationDraft(draftId: string, companyId: string): Promise<Row | null>;
    readApproveEvaluationCompensationEffect(userId: string, created_by: unknown, companyId: string): Promise<boolean | null>;
    stageApproveEvaluationCompensationEffect(id: string): Effect;
    stageApproveEvaluationCompensationEffect2(id: string): Effect;
    guard84(key: string, value: "pay-draft-version", draftId: string, expectedVersion: number): Effect;
    guard85(key: string, value: "assessment-version", id: string, value2: number): Effect;
    guard86(key: string, value: "engagement-version", id: string, value2: number): Effect;
    guard87(key: string, value: "basis-version", id: string, effective: string, value2: number): Effect;
    guard88(key: string, value: "policy-version", value2: string, companyId: string, value3: number): Effect;
    guard89(key: string, value: "previous-version", value2: string, value3: number, effective: string, effective2: string): Effect;
    guard90(key: string, value: "new-wage-version", value2: string, value3: 1): Effect;
    guard91(key: string, value: "proposed-pay-snapshot", draftId: string): Effect;
    guard92(key: string, value: "no-published-payroll", id: string, effective: string): Effect;
    guard93(key: string, value: "no-other-wage", id: string, value2: string, value3: string, effective: string): Effect;
    stageApproveEvaluationCompensationEffect3(value: string, now: number, previous_id: unknown): Effect;
    stageApproveEvaluationCompensationEffect4(userId: string, now: number, compensation_id: unknown): Effect;
    stageApproveEvaluationCompensationEffect5(userId: string, reason: string, now: number, draftId: string): Effect;
    stageApproveEvaluationCompensationEffect6(value: string, key: string, draftId: string, op: "people.evaluation.compensation.approve", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number): Effect;
    readCancelEvaluationCompensationDraft(draftId: string, companyId: string): Promise<Row | null>;
    stageCancelEvaluationCompensationEffect(id: string): Effect;
    guard100(key: string, value: "engagement-version", id: string, version: number): Effect;
    stageCancelEvaluationCompensationEffect2(draftId: string): Effect;
    guard102(key: string, value: "pay-draft-version", draftId: string, expectedVersion: number): Effect;
    guard103(key: string, value: "unapproved-pay", value2: string): Effect;
    stageCancelEvaluationCompensationEffect3(now: number, compensation_id: unknown): Effect;
    stageCancelEvaluationCompensationEffect4(reason: string, now: number, draftId: string): Effect;
}
export interface PeopleEvaluationPorts<Effect> {
    repository: PeopleEvaluationRepository<Effect>;
    clock: Clock;
    ids: {
        newId(): string;
    };
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: RosterCommands<Effect>;
    querySchema: ReturnType<typeof createEvaluationQuerySchema>;
    batch(effects: Effect[]): Promise<unknown>;
    captureAuthority(actor: WorkContext): Promise<WorkContext>;
    authorityGuards(...args: Tail<Parameters<typeof iamMutationGuards>>): Effect[];
    snapshotMoney(...args: Tail<Parameters<typeof snapshotMoney>>): PortResult<ReturnType<typeof snapshotMoney>, Effect>;
    attachmentLinkStatus(...args: Tail<Parameters<typeof attachmentLinkStatus>>): ReturnType<typeof attachmentLinkStatus>;
}
export function createPeopleEvaluationApplication<Effect>(ports: PeopleEvaluationPorts<Effect>) {
    const repository = ports.repository;
    async function evaluationAuthority(actor: WorkContext, permission = "people.read") {
        if (!actor.iam?.queuedAssignmentVersion)
            actor = await ports.captureAuthority(actor);
        ports.authorize(actor, permission);
        const key = ports.ids.newId();
        try {
            await ports.batch([...ports.authorityGuards(actor, key), repository.stageEvaluationAuthorityEffect(key)]);
        }
        catch {
            throw new BusinessRuleError(403, "EVALUATION_ACCESS_CHANGED", "Your evaluation access changed. Reload your work role.");
        }
        return actor;
    }
    async function evaluationEmployees(actor: WorkContext) {
        const rows = (await repository.readEvaluationEmployeesRows(actor.companyId, actor.userId, actor.companyId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.role)).results;
        return rows.map(r => ({ id: String(r.id), personId: String(r.person_id), name: String(r.legal_name), userId: r.user_id ? String(r.user_id) : null, positionId: r.position_id ? String(r.position_id) : null, levelId: r.level_id ? String(r.level_id) : null, jobTitle: String(r.job_title), branchId: r.branch_id ? String(r.branch_id) : null, departmentId: r.department_id ? String(r.department_id) : null, department: r.department ? String(r.department) : null, currency: String(r.currency), startDate: String(r.start_date), endDate: r.end_date ? String(r.end_date) : null, version: Number(r.version) })) as EvaluationEmployee[];
    }
    async function evaluationEmployee(actor: WorkContext, engagementId: string) {
        const employee = (await evaluationEmployees(actor)).find(e => e.id === engagementId);
        if (!employee)
            throw new BusinessRuleError(404, "EVALUATION_EMPLOYEE_NOT_FOUND", "Employee not found in your evaluation scope.");
        return employee;
    }
    async function evaluationDefinition(actor: WorkContext, id: string, kind: DefinitionKind, v?: number) {
        const row = await repository.readEvaluationDefinitionRow(id, kind, actor.companyId, (v ? [v] : []), v);
        if (!row)
            throw new BusinessRuleError(404, "EVALUATION_DEFINITION_NOT_FOUND", "Evaluation criteria are unavailable in this Company.");
        return { id: row.id, kind: row.kind, version: Number(row.version), data: definitionSchemas[kind].parse(JSON.parse(row.payload_json)) as Record<string, unknown>, updatedAt: Number(row.updated_at) };
    }
    async function frozenPolicy(actor: WorkContext, cycleId: string) {
        const cycle = await evaluationDefinition(actor, cycleId, "cycle"), data = cycleSchema.parse(cycle.data);
        const policy = await evaluationDefinition(actor, data.policy.id, "policy", data.policy.version);
        return { cycle, data, policy, policyData: policySchema.parse(policy.data) };
    }
    async function evaluationToday(engagementId: string) {
        const group = await repository.readEvaluationTodayGroup(engagementId);
        return new Intl.DateTimeFormat("en-CA", { timeZone: group?.timezone ?? "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(ports.clock.now()));
    }
    function confirmed(data: {
        confirmed: boolean;
    }) { if (!data.confirmed)
        throw new BusinessRuleError(422, "CRITERIA_NOT_CONFIRMED", "HR must confirm the Company's criteria before use."); }
    function fullHr(actor: WorkContext) { ports.authorize(actor, "organization.manage"); if (actor.branchId || actor.departmentId)
        throw new BusinessRuleError(403, "COMPANY_EVALUATION_REQUIRED", "Company HR manages evaluation criteria and annual cycles."); }
    async function saveEvaluationDefinition(actor: WorkContext, raw: unknown, key: string) {
        const input = z.object({ id: idSchema.optional(), kind: z.enum(["skill", "role", "policy", "cycle", "band", "benchmark"]), expectedVersion: z.number().int().nonnegative(), data: z.unknown(), reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        fullHr(actor);
        if (["band", "benchmark", "policy"].includes(input.kind))
            ports.authorize(actor, "people.compensation.read");
        const payload = definitionSchemas[input.kind].parse(input.data), id = input.id ?? `evaluation-${input.kind}-${key}`, op = "people.evaluation.criteria.save";
        const previous = await ports.commands.replay<{id:string;version:number}>(actor, key, op, id, input);
        if (previous)
            return previous;
        const old = input.expectedVersion ? await evaluationDefinition(actor, id, input.kind) : null;
        if (old && old.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "Criteria changed. Reload before saving.");
        const checks: Effect[] = [];
        if (input.kind === "role") {
            const role = roleSchema.parse(payload);
            const position = await repository.readSaveEvaluationDefinitionPosition(role.positionId, actor.companyId, role.levelId);
            if (!position)
                throw new BusinessRuleError(422, "ROLE_POSITION_MISMATCH", "Choose an active Company position and its staff level.");
            for (const r of role.requirements) {
                const skill = await evaluationDefinition(actor, r.skill.id, "skill", r.skill.version);
                if (role.confirmed)
                    confirmed(skill.data as {
                        confirmed: boolean;
                    });
            }
        }
        if (input.kind === "band" || input.kind === "benchmark") {
            const band = payload as z.infer<typeof bandSchema>;
            if (!await repository.readSaveEvaluationDefinitionEffect(band.levelId, actor.companyId))
                throw new BusinessRuleError(422, "LEVEL_NOT_FOUND", "Choose an active Company staff level.");
            if (input.kind === "band" && band.benchmark) {
                const market = benchmarkSchema.parse((await evaluationDefinition(actor, band.benchmark.id, "benchmark", band.benchmark.version)).data);
                if (market.family !== band.family || market.levelId !== band.levelId || market.location !== band.location || market.currency !== band.currency)
                    throw new BusinessRuleError(422, "BENCHMARK_MISMATCH", "Market source must use the same job family, level, location and currency.");
                if (band.confirmed)
                    confirmed(market);
            }
        }
        if (input.kind === "cycle") {
            const cycle = cycleSchema.parse(payload), policy = await evaluationDefinition(actor, cycle.policy.id, "policy", cycle.policy.version);
            if (cycle.status !== "draft") {
                confirmed(cycle);
                confirmed(policy.data as {
                    confirmed: boolean;
                });
            }
            if (old) {
                const before = cycleSchema.parse(old.data);
                if (before.status !== "draft" && (JSON.stringify({ ...before, status: cycle.status }) !== JSON.stringify(cycle) || cycle.status !== "closed"))
                    throw new BusinessRuleError(409, "CYCLE_FROZEN", "An opened cycle keeps its year, criteria and policy. Only closing it is allowed.");
            }
            checks.push(repository.guard7(key, "unique-cycle", actor.companyId, id, cycle.year));
        }
        const now = ports.clock.now(), next = input.expectedVersion + 1;
        return ports.commands.commit(actor, key, op, input, id, next, { id, version: next }, [
            repository.stageSaveEvaluationDefinitionEffect(`evaluation-criteria:${actor.companyId}`),
            ...checks,
            ...(old ? [repository.guard9(key, "criteria-version", id, actor.companyId, input.kind, input.expectedVersion), repository.stageSaveEvaluationDefinitionEffect2(JSON.stringify(payload), actor.userId, input.reason, now, id)] : [repository.stageSaveEvaluationDefinitionEffect3(id, actor.companyId, input.kind, JSON.stringify(payload), actor.userId, actor.userId, input.reason, now, now)]),
        ]);
    }
    async function evaluationWorkspace(actor: WorkContext, raw: unknown = {}) {
        const query = ports.querySchema.parse(raw), date = query.date ?? `${query.year}-12-31`;
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor);
        const canCompensation = hasPermission(actor, "people.compensation.read");
        const definitions = (await repository.readEvaluationWorkspaceDefinitions(actor.companyId)).results.map(r => ({ id: r.id, kind: r.kind, version: Number(r.version), data: JSON.parse(r.payload_json) as Record<string, unknown>, updatedAt: Number(r.updated_at) }));
        const cycles = definitions.filter(d => d.kind === "cycle" && cycleSchema.parse(d.data).year === query.year);
        const cycle = query.cycleId ? cycles.find(d => d.id === query.cycleId) : cycles[0];
        if (query.cycleId && !cycle)
            throw new BusinessRuleError(404, "CYCLE_NOT_FOUND", "Annual cycle not found for this year.");
        const policy = cycle ? (await frozenPolicy(actor, cycle.id)).policyData : defaultEvaluationPolicy;
        const employees = (await evaluationEmployees(actor)).filter(e => e.startDate <= date && (!e.endDate || e.endDate >= `${query.year}-01-01`) && (!query.departmentId || e.departmentId === query.departmentId) && (!query.levelId || e.levelId === query.levelId) && (!query.engagementId || e.id === query.engagementId));
        const assessments = cycle ? (await repository.readEvaluationWorkspaceAssessments(actor.companyId, cycle.id, date)).results : [];
        const rows: EvaluationRow[] = [];
        for (const employee of employees) {
            const segments = assessments.filter(a => a.engagement_id === employee.id);
            for (const assessment of segments.length ? segments : [null]) {
                const definition = assessment ? await evaluationDefinition(actor, assessment.role_id, "role", Number(assessment.role_version)) : definitions.find(d => d.kind === "role" && d.data.positionId === employee.positionId && d.data.confirmed === true) ?? null;
                const role = definition ? roleSchema.parse(definition.data) : null;
                if ((query.family && role?.family !== query.family) || (query.location && role?.location !== query.location))
                    continue;
                const content = assessment ? assessmentContentSchema.parse(JSON.parse(assessment.content_json)) : emptyAssessment;
                const target = content.targetRole ? roleSchema.parse((await evaluationDefinition(actor, content.targetRole.id, "role", content.targetRole.version)).data) : null;
                const row: EvaluationRow = { key: assessment?.id ?? employee.id, employee, role, roleRef: definition ? { id: definition.id, version: definition.version } : null, assessment: assessment ? { id: assessment.id, version: Number(assessment.version), cycleId: assessment.cycle_id, status: assessment.status, periodStart: assessment.period_start, periodEnd: assessment.period_end, content, createdBy: assessment.created_by, reviewedBy: assessment.reviewed_by, confirmedBy: assessment.confirmed_by } : null, metrics: evaluationMetrics(role, target, content, policy) };
                if (canCompensation) {
                    const wages = (await repository.readEvaluationWorkspaceWages(employee.id, date, date)).results;
                    const wage = wages.length === 1 ? wages[0] : null;
                    const basisRow = await repository.readEvaluationWorkspaceBasisRow(employee.id, actor.companyId, date);
                    const basis = basisRow ? basisSchema.parse(JSON.parse(basisRow.payload_json)) : null;
                    const bands = definitions.filter(d => d.kind === "band" && d.data.family === role?.family && d.data.levelId === role?.levelId && d.data.location === role?.location && d.data.currency === employee.currency && d.data.confirmed && String(d.data.effectiveFrom) <= date && String(d.data.effectiveTo) >= date);
                    const band = bands.length === 1 ? bandSchema.parse(bands[0].data) : null;
                    const market = band?.benchmark ? benchmarkSchema.parse((await evaluationDefinition(actor, band.benchmark.id, "benchmark", band.benchmark.version)).data) : null;
                    row.compensation = { wage, band, basis, basisVersion: basisRow ? Number(basisRow.version) : null, metrics: compensationMetrics(wage, basis, band, market, date, policy) };
                    if (wages.length > 1)
                        row.compensation.metrics.missing = "Overlapping approved wages require HR review";
                    if (bands.length > 1)
                        row.compensation.metrics.missing = "Overlapping pay bands require HR review";
                }
                rows.push(row);
            }
        }
        const skillRefs = new Map<string, {
            id: string;
            version: number;
        }>();
        for (const profile of [...definitions.filter(d => d.kind === "role").map(d => roleSchema.parse(d.data)), ...rows.flatMap(r => r.role ? [r.role] : [])])
            for (const requirement of profile.requirements)
                skillRefs.set(`${requirement.skill.id}:${requirement.skill.version}`, requirement.skill);
        for (const row of rows)
            if (row.assessment?.content.targetRole) {
                const ref = row.assessment.content.targetRole, target = roleSchema.parse((await evaluationDefinition(actor, ref.id, "role", ref.version)).data);
                for (const requirement of target.requirements)
                    skillRefs.set(`${requirement.skill.id}:${requirement.skill.version}`, requirement.skill);
            }
        const skillVersions = await Promise.all([...skillRefs.values()].map(ref => evaluationDefinition(actor, ref.id, "skill", ref.version)));
        const publicDefinitions = definitions.filter(d => canCompensation || !["band", "benchmark"].includes(d.kind)).map(d => !canCompensation && d.kind === "policy" ? { ...d, data: { ...d.data, maxRaisePercent: undefined, compensationRule: undefined } } : d);
        const positions = hasPermission(actor, "organization.manage") ? (await repository.readEvaluationWorkspacePositions(actor.companyId)).results : [];
        const levels = (await repository.readEvaluationWorkspaceLevels(actor.companyId)).results;
        return { rows, skillVersions, definitions: publicDefinitions, positions, levels, query: { ...query, date }, cycleId: cycle?.id ?? null, policy: canCompensation ? policy : { ...policy, maxRaisePercent: undefined, compensationRule: undefined }, actorId: actor.userId, canWrite: hasPermission(actor, "people.manage"), canConfigure: hasPermission(actor, "organization.manage") && !actor.branchId && !actor.departmentId, canConfirm: hasPermission(actor, "people.manage"), canCompensation, canEvidenceFiles: canCompensation && hasPermission(actor, "people.documents.read"), canApproveCompensation: hasPermission(actor, "people.compensation.approve") && canCompensation };
    }
    async function importEvaluationMarket(actor: WorkContext, raw: unknown, key: string) {
        const input = z.object({ rows: z.array(benchmarkSchema).min(1).max(100), reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        fullHr(actor);
        ports.authorize(actor, "people.compensation.read");
        const id = `evaluation-import-${key}`, op = "people.evaluation.market.import", prior = await ports.commands.replay<{id:string;imported:number;confirmed:boolean}>(actor, key, op, id, input);
        if (prior)
            return prior;
        const levels = (await repository.readImportEvaluationMarketLevels(actor.companyId)).results;
        if (input.rows.some(r => !levels.some(l => l.id === r.levelId)))
            throw new BusinessRuleError(422, "IMPORT_LEVEL_UNAVAILABLE", "Every market row must refer to an active Company staff level.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, op, input, id, 1, { id, imported: input.rows.length, confirmed: false }, input.rows.map((row, i) => repository.stageImportEvaluationMarketEffect(`${id}-${i}`, actor.companyId, JSON.stringify({ ...row, confirmed: false }), actor.userId, actor.userId, input.reason, now, now)));
    }
    async function evaluationAssessment(actor: WorkContext, assessmentId: string) {
        const row = await repository.readEvaluationAssessmentRow(assessmentId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "ASSESSMENT_NOT_FOUND", "Assessment not found.");
        const employee = await evaluationEmployee(actor, row.engagement_id);
        return { row, employee };
    }
    async function validateEvaluationRolePeriod(actor: WorkContext, employee: EvaluationEmployee, role: RoleProfile, periodStart: string, periodEnd: string) {
        // Respect dated changes in position/level, independently of profile revisions and actual-work evidence.
        const history = (await repository.readValidateEvaluationRolePeriodHistory(actor.companyId, employee.id)).results;
        const zone = await repository.readValidateEvaluationRolePeriodZone(employee.id);
        const changes = history.map(h => {
            const before = JSON.parse(h.before_json), after = JSON.parse(h.after_json);
            const position = (r: Row) => r.positionId ?? r.position_id ?? null, level = (r: Row) => r.levelId ?? r.level_id ?? null;
            return { before: { positionId: position(before), levelId: level(before) }, after: { positionId: position(after), levelId: level(after) }, date: new Intl.DateTimeFormat("en-CA", { timeZone: zone?.timezone ?? "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(Number(after.effectiveAt ?? h.created_at))) };
        }).filter(h => h.before.positionId !== h.after.positionId || h.before.levelId !== h.after.levelId).sort((a, b) => a.date.localeCompare(b.date));
        if (changes.some(h => h.date > periodStart && h.date <= periodEnd))
            throw new BusinessRuleError(422, "ROLE_PERIOD_SPLIT_REQUIRED", "Split this assessment at each recorded position or staff-level change.");
        const priorChange = changes.filter(h => h.date <= periodStart).at(-1), futureChange = changes.find(h => h.date > periodStart);
        const assigned = priorChange?.after ?? futureChange?.before ?? { positionId: employee.positionId, levelId: employee.levelId };
        if (assigned.positionId !== role.positionId || assigned.levelId !== role.levelId)
            throw new BusinessRuleError(422, "ROLE_ASSIGNMENT_UNVERIFIED", "Use the position and staff level recorded for this role period.");
    }
    async function createEvaluationAssessment(actor: WorkContext, raw: unknown, key: string) {
        const input = z.object({ engagementId: idSchema, cycleId: idSchema, role: z.object({ id: idSchema, version }).strict(), periodStart: evaluationDate, periodEnd: evaluationDate, reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        const id = `assessment-${key}`, op = "people.evaluation.assessment.create", prior = await ports.commands.replay<{id:string;version:number;status:string}>(actor, key, op, id, input);
        if (prior)
            return prior;
        const employee = await evaluationEmployee(actor, input.engagementId), cycle = await frozenPolicy(actor, input.cycleId), role = roleSchema.parse((await evaluationDefinition(actor, input.role.id, "role", input.role.version)).data);
        confirmed(role);
        if (cycle.data.status !== "open" || !cycle.policyData.confirmed)
            throw new BusinessRuleError(409, "CYCLE_NOT_OPEN", "Confirm the Company's policy and open the annual cycle first.");
        if (input.periodStart > input.periodEnd || input.periodStart < `${cycle.data.year}-01-01` || input.periodEnd > `${cycle.data.year}-12-31` || input.periodStart < employee.startDate || (employee.endDate && input.periodEnd > employee.endDate))
            throw new BusinessRuleError(422, "ASSESSMENT_PERIOD_INVALID", "Evaluation period must fall within the annual cycle and employment.");
        await validateEvaluationRolePeriod(actor, employee, role, input.periodStart, input.periodEnd);
        return ports.commands.commit(actor, key, op, input, id, 1, { id, status: "draft", version: 1 }, [
            repository.stageCreateEvaluationAssessmentEffect(employee.id),
            repository.guard24(key, "engagement-version", employee.id, actor.companyId, employee.version),
            repository.stageCreateEvaluationAssessmentEffect2(cycle.cycle.id), repository.guard26(key, "cycle-open", input.cycleId, cycle.cycle.version),
            repository.guard27(key, "period-overlap", employee.id, input.cycleId, input.periodEnd, input.periodStart),
            repository.stageCreateEvaluationAssessmentEffect3(id, actor.companyId, employee.id, input.cycleId, input.role.id, input.role.version, input.periodStart, input.periodEnd, JSON.stringify(emptyAssessment), actor.userId, input.reason, ports.clock.now(), ports.clock.now()),
        ]);
    }
    async function saveEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: version, content: assessmentContentSchema, periodStart: evaluationDate.optional(), periodEnd: evaluationDate.optional(), reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        const prior = await ports.commands.replay(actor, key, "people.evaluation.assessment.save", assessmentId, input);
        if (prior)
            return prior;
        const { row, employee } = await evaluationAssessment(actor, assessmentId), cycle = await frozenPolicy(actor, row.cycle_id);
        if (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open")
            throw new BusinessRuleError(409, "ASSESSMENT_LOCKED", "Reopen the assessment before changing scores; closed cycles retain their results.");
        const role = roleSchema.parse((await evaluationDefinition(actor, row.role_id, "role", row.role_version)).data);
        const periodStart = input.periodStart ?? row.period_start, periodEnd = input.periodEnd ?? row.period_end;
        if (periodStart > periodEnd || periodStart < `${cycle.data.year}-01-01` || periodEnd > `${cycle.data.year}-12-31` || periodStart < employee.startDate || (employee.endDate && periodEnd > employee.endDate))
            throw new BusinessRuleError(422, "ASSESSMENT_PERIOD_INVALID", "The revised period must remain within the cycle and employment.");
        await validateEvaluationRolePeriod(actor, employee, role, periodStart, periodEnd);
        const target = input.content.targetRole ? roleSchema.parse((await evaluationDefinition(actor, input.content.targetRole.id, "role", input.content.targetRole.version)).data) : null;
        if (target) {
            confirmed(target);
            if (target.positionId === role.positionId)
                throw new BusinessRuleError(422, "TARGET_ROLE_REQUIRED", "Select a distinct target role for readiness.");
        }
        const allowed = new Set([...role.requirements, ...(target?.requirements ?? [])].map(r => `${r.skill.id}:${r.skill.version}`));
        if (input.content.skills.some(s => !allowed.has(`${s.skillId}:${s.skillVersion}`)))
            throw new BusinessRuleError(422, "SKILL_NOT_IN_ROLE", "Rate only skills in the current or selected target profile.");
        const evidence = (await repository.readSaveEvaluationAssessmentEvidence(assessmentId, actor.companyId)).results;
        const cited = [...input.content.skills.flatMap(s => s.evidenceIds), ...[input.content.performance.results, input.content.performance.quality, input.content.performance.collaboration].flatMap(p => p?.evidenceIds ?? []), ...(input.content.actualWork?.evidenceIds ?? [])];
        if (cited.some(id => !evidence.some(e => e.id === id && e.observed_on >= periodStart && e.observed_on <= periodEnd)))
            throw new BusinessRuleError(422, "EVIDENCE_REFERENCE_INVALID", "Every score must cite evidence from this assessment.");
        const guards = [repository.stageSaveEvaluationAssessmentGuards(employee.id), repository.guard31(key, "engagement-version", employee.id, employee.version), repository.guard32(key, "period-overlap", employee.id, row.cycle_id, assessmentId, periodEnd, periodStart), repository.stageSaveEvaluationAssessmentGuards2(assessmentId), repository.stageSaveEvaluationAssessmentGuards3(cycle.cycle.id), repository.guard35(key, "cycle-open", row.cycle_id, cycle.cycle.version), repository.guard36(key, "assessment-version", assessmentId, input.expectedVersion, row.status)];
        return ports.commands.commit(actor, key, "people.evaluation.assessment.save", input, assessmentId, input.expectedVersion + 1, { id: assessmentId, version: input.expectedVersion + 1, status: row.status }, [...guards, repository.stageSaveEvaluationAssessmentEffect(periodStart, periodEnd, JSON.stringify(input.content), actor.userId, actor.userId, input.reason, ports.clock.now(), assessmentId)]);
    }
    async function transitionEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
        const input = z.object({ action: z.enum(["submit", "calibrate", "confirm", "reopen"]), expectedVersion: version, reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        const prior = await ports.commands.replay(actor, key, "people.evaluation.assessment.transition", assessmentId, input);
        if (prior)
            return prior;
        const { row, employee } = await evaluationAssessment(actor, assessmentId), cycle = await frozenPolicy(actor, row.cycle_id);
        const states = { submit: ["draft", "submitted"], calibrate: ["submitted", "calibrating"], confirm: ["calibrating", "confirmed"], reopen: [row.status, "draft"] };
        if (row.status !== states[input.action][0] || (input.action === "reopen" && row.status === "draft") || cycle.data.status !== "open")
            throw new BusinessRuleError(409, "INVALID_EVALUATION_TRANSITION", "The assessment cannot take this step in the current annual cycle.");
        if (input.action === "submit" || input.action === "confirm")
            await validateEvaluationRolePeriod(actor, employee, roleSchema.parse((await evaluationDefinition(actor, row.role_id, "role", row.role_version)).data), row.period_start, row.period_end);
        if (input.action === "confirm") {
            if (await repository.readTransitionEvaluationAssessmentEffect(actor.userId, row.created_by, actor.companyId) || (row.reviewed_by && await repository.readTransitionEvaluationAssessmentEffect2(actor.userId, row.reviewed_by, actor.companyId)))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must confirm the scores after calibration.");
        }
        return ports.commands.commit(actor, key, "people.evaluation.assessment.transition", input, assessmentId, input.expectedVersion + 1, { id: assessmentId, status: states[input.action][1], version: input.expectedVersion + 1 }, [repository.stageTransitionEvaluationAssessmentEffect(employee.id), repository.guard41(key, "engagement-version", employee.id, employee.version), repository.stageTransitionEvaluationAssessmentEffect2(assessmentId),
            repository.guard43(key, "assessment-version", assessmentId, input.expectedVersion, row.status),
            repository.stageTransitionEvaluationAssessmentEffect3(cycle.cycle.id), repository.guard45(key, "cycle-open", row.cycle_id, cycle.cycle.version),
            repository.guard46(key, "no-pay-draft", assessmentId),
            repository.stageTransitionEvaluationAssessmentEffect4(states[input.action][1], actor.userId, input.action === "confirm" ? actor.userId : null, states[input.action][1], input.reason, ports.clock.now(), assessmentId),
        ]);
    }
    async function evaluationDetail(actor: WorkContext, assessmentId: string) {
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor);
        const { employee } = await evaluationAssessment(actor, assessmentId);
        let evidence = (await repository.readEvaluationDetailEvidence(assessmentId, actor.companyId)).results;
        if (!hasPermission(actor, "people.compensation.read") || !hasPermission(actor, "people.documents.read"))
            evidence = evidence.filter(e => e.source_type !== "file");
        const history = (await repository.readEvaluationDetailHistory(assessmentId, actor.companyId)).results;
        const sources = await evaluationEvidenceSources(actor, employee);
        const salary = hasPermission(actor, "people.compensation.read") ? { wages: (await repository.readEvaluationDetailSalary(employee.id)).results, drafts: (await repository.readEvaluationDetailSalary2(assessmentId, actor.companyId)).results } : undefined;
        const files = hasPermission(actor, "people.compensation.read") && hasPermission(actor, "people.documents.read") ? (await repository.readEvaluationDetailFiles(actor.companyId, assessmentId)).results : [];
        return { evidence, history, sources, salary, files };
    }
    async function evaluationEvidenceSources(actor: WorkContext, employee: EvaluationEmployee, sourceId: string | null = null): Promise<(Row & {
        type: string;
    })[]> {
        const [tasks, projects, learning] = await Promise.all([
            repository.readEvaluationEvidenceSourcesTasksprojectslearning(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            repository.readEvaluationEvidenceSourcesTasksprojectslearning2(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            repository.readEvaluationEvidenceSourcesTasksprojectslearning3(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
        ]);
        return [...tasks.results.map(r => ({ type: "task", ...r })), ...projects.results.map(r => ({ type: "project", ...r })), ...learning.results.map(r => ({ type: "learning", ...r }))];
    }
    async function addEvaluationEvidence(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: version, sourceType: z.enum(["kpi", "task", "project", "learning", "file"]), sourceId: idSchema.nullable().default(null), description: reason, observedOn: evaluationDate }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        const id = `evaluation-evidence-${key}`, prior = await ports.commands.replay(actor, key, "people.evaluation.evidence.add", id, input);
        if (prior)
            return prior;
        const { row, employee } = await evaluationAssessment(actor, assessmentId), cycle = await frozenPolicy(actor, row.cycle_id);
        if (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open")
            throw new BusinessRuleError(409, "ASSESSMENT_LOCKED", "Add evidence to a draft or calibration in an open cycle.");
        if (input.observedOn < row.period_start || input.observedOn > row.period_end)
            throw new BusinessRuleError(422, "EVIDENCE_DATE_INVALID", "Evidence date must be within this role period.");
        let source: unknown = { description: input.description };
        if (["task", "project", "learning"].includes(input.sourceType)) {
            source = (await evaluationEvidenceSources(actor, employee, input.sourceId)).find(s => s.type === input.sourceType && "id" in s && s.id === input.sourceId);
            if (!source)
                throw new BusinessRuleError(404, "EVIDENCE_SOURCE_UNAVAILABLE", "Source must belong to this employee and current team scope.");
        }
        if (input.sourceType === "file") {
            ports.authorize(actor, "people.compensation.read");
            ports.authorize(actor, "people.documents.read");
            await ports.attachmentLinkStatus(actor, input.sourceId ?? "");
            const link = await repository.readAddEvaluationEvidenceLink(input.sourceId, assessmentId, actor.companyId);
            if (!link)
                throw new BusinessRuleError(404, "EVIDENCE_FILE_UNAVAILABLE", "Choose a scanned file attached to this assessment.");
            source = { linkId: input.sourceId };
        }
        return ports.commands.commit(actor, key, "people.evaluation.evidence.add", input, id, 1, { id, assessmentVersion: input.expectedVersion + 1 }, [repository.stageAddEvaluationEvidenceEffect(employee.id), repository.guard58(key, "engagement-version", employee.id, employee.version), repository.stageAddEvaluationEvidenceEffect2(assessmentId),
            repository.guard60(key, "assessment-version", assessmentId, input.expectedVersion, row.status),
            repository.stageAddEvaluationEvidenceEffect3(cycle.cycle.id), repository.guard62(key, "cycle-open", row.cycle_id, cycle.cycle.version),
            repository.stageAddEvaluationEvidenceEffect4(id, actor.companyId, assessmentId, input.sourceType, input.sourceId, input.description, input.observedOn, JSON.stringify(source), actor.userId, ports.clock.now()),
            repository.stageAddEvaluationEvidenceEffect5(actor.userId, actor.userId, ports.clock.now(), assessmentId),
        ]);
    }
    async function readEvaluationEvidence(actor: WorkContext, evidenceId: string) {
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor);
        const evidence = await repository.readReadEvaluationEvidenceEvidence(evidenceId, actor.companyId);
        if (!evidence)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "Evidence unavailable.");
        const { employee } = await evaluationAssessment(actor, String(evidence.assessment_id));
        if (evidence.source_type === "file") {
            ports.authorize(actor, "people.compensation.read");
            ports.authorize(actor, "people.documents.read");
        }
        if (["task", "project", "learning"].includes(String(evidence.source_type)) && !(await evaluationEvidenceSources(actor, employee, String(evidence.source_id))).some(s => s.type === evidence.source_type && "id" in s && s.id === evidence.source_id))
            throw new BusinessRuleError(404, "EVIDENCE_SOURCE_UNAVAILABLE", "The current source is unavailable in your scope.");
        return evidence;
    }
    async function confirmEvaluationPayBasis(actor: WorkContext, engagementId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: z.number().int().nonnegative(), effectiveFrom: evaluationDate, basis: basisSchema }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        ports.authorize(actor, "people.compensation.read");
        const prior = await ports.commands.replay(actor, key, "people.evaluation.basis.confirm", engagementId, input);
        if (prior)
            return prior;
        const employee = await evaluationEmployee(actor, engagementId);
        if (input.effectiveFrom < employee.startDate)
            throw new BusinessRuleError(422, "BASIS_DATE_INVALID", "Basis date must be within employment.");
        return ports.commands.commit(actor, key, "people.evaluation.basis.confirm", input, engagementId, input.expectedVersion + 1, { id: engagementId, version: input.expectedVersion + 1 }, [
            repository.stageConfirmEvaluationPayBasisEffect(engagementId),
            repository.guard67(key, "engagement-version", engagementId, employee.version),
            repository.guard68(key, "basis-version", engagementId, input.expectedVersion),
            repository.stageConfirmEvaluationPayBasisEffect2(engagementId, actor.companyId, input.expectedVersion + 1, input.effectiveFrom, JSON.stringify(input.basis), actor.userId, ports.clock.now()),
        ]);
    }
    async function createEvaluationCompensationDraft(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: version, effectiveFrom: evaluationDate, amountMinor: z.number().int().positive().max(Number.MAX_SAFE_INTEGER), reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        ports.authorize(actor, "people.compensation.read");
        const id = `evaluation-pay-${key}`, compId = `evaluation-comp-${key}`, op = "people.evaluation.compensation.create", prior = await ports.commands.replay(actor, key, op, assessmentId, input);
        if (prior)
            return prior;
        const { row, employee } = await evaluationAssessment(actor, assessmentId), cycle = await frozenPolicy(actor, row.cycle_id);
        if (row.status !== "confirmed" || row.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "CONFIRMED_ASSESSMENT_REQUIRED", "Use the current independently confirmed assessment.");
        if (!cycle.policyData.confirmed || cycle.policyData.maxRaisePercent <= 0)
            throw new BusinessRuleError(422, "PAY_POLICY_REQUIRED", "HR must confirm a policy that permits salary drafts.");
        const today = await evaluationToday(employee.id);
        const e = await repository.readCreateEvaluationCompensationDraftE(employee.id, actor.companyId);
        if (e?.status !== "active" || !e.pay_group_id)
            throw new BusinessRuleError(422, "PAY_GROUP_REQUIRED", "An active engagement and pay group are required.");
        const lastRun = await repository.readCreateEvaluationCompensationDraftLastRun(actor.companyId, e.pay_group_id);
        if (!lastRun)
            throw new BusinessRuleError(422, "NEXT_PAY_PERIOD_REQUIRED", "Confirm the next payroll period after an existing payroll run first.");
        const nextDate = new Date(`${lastRun.period_end}T00:00:00Z`);
        nextDate.setUTCDate(nextDate.getUTCDate() + 1);
        if (input.effectiveFrom !== nextDate.toISOString().slice(0, 10) || input.effectiveFrom <= today)
            throw new BusinessRuleError(422, "NEXT_PAY_PERIOD_REQUIRED", "New pay must start at the next future payroll period boundary.");
        const wages = (await repository.readCreateEvaluationCompensationDraftWages(employee.id, input.effectiveFrom, input.effectiveFrom)).results;
        const previous = wages.length === 1 ? wages[0] : null;
        const basis = await repository.readCreateEvaluationCompensationDraftBasis(employee.id, actor.companyId, input.effectiveFrom);
        if (!previous || !basis || previous.effective_from >= input.effectiveFrom)
            throw new BusinessRuleError(422, "PAY_BASELINE_REQUIRED", "Review one approved wage and an HR-confirmed comparison basis.");
        const b = basisSchema.parse(JSON.parse(basis.payload_json));
        if (monthlyFtePay(previous, b).value === null)
            throw new BusinessRuleError(422, "PAY_BASIS_MISMATCH", "The confirmed basis must match the approved wage frequency and hours.");
        const field = b.type === "monthly" ? "base_amount_minor" : b.type === "daily" ? "daily_rate_minor" : "hourly_rate_minor", oldAmount = Number(previous[field]);
        if (oldAmount <= 0 || input.amountMinor < oldAmount || input.amountMinor > oldAmount * (1 + cycle.policyData.maxRaisePercent / 100))
            throw new BusinessRuleError(422, "PAY_POLICY_LIMIT", "Proposed pay must be within the confirmed Company increase limit.");
        const amounts = { base: previous.base_amount_minor, hourly_rate: previous.hourly_rate_minor, daily_rate: previous.daily_rate_minor };
        amounts[b.type === "monthly" ? "base" : b.type === "daily" ? "daily_rate" : "hourly_rate"] = input.amountMinor;
        const components = JSON.parse(previous.components_json) as {
            code: string;
            amount: string;
        }[];
        const snapshotAmounts = [...Object.entries(amounts).map(([amountKind, amount]) => ({ amountKind, amount })), ...components.map(c => ({ amountKind: `component:${c.code}`, amount: decimalToMinor(c.amount) }))];
        const money = await Promise.all(snapshotAmounts.map(({ amountKind, amount }) => ports.snapshotMoney(actor, { sourceType: "hr_compensation_revision", sourceId: compId, amountKind, transactionDate: input.effectiveFrom, currency: employee.currency, currencyScale: 2, amountUnits: String(amount) })));
        return ports.commands.commit(actor, key, op, input, assessmentId, 1, { id, compensationId: compId, status: "draft", version: 1 }, [
            repository.stageCreateEvaluationCompensationDraftEffect(employee.id),
            repository.guard75(key, "assessment-confirmed", assessmentId, input.expectedVersion),
            repository.guard76(key, "wage-version", previous.id, Number(previous.version)),
            repository.guard77(key, "engagement-version", employee.id, employee.version),
            repository.stageCreateEvaluationCompensationDraftEffect2(compId, employee.id, input.effectiveFrom, previous.effective_to, previous.pay_frequency, amounts.base, amounts.hourly_rate, amounts.daily_rate, previous.standard_daily_hours, previous.expected_hours, employee.currency, previous.components_json, actor.userId, ports.clock.now(), ports.clock.now()),
            ...money.map(m => m.statement),
            repository.stageCreateEvaluationCompensationDraftEffect3(id, actor.companyId, assessmentId, input.expectedVersion, compId, previous.id, previous.version, employee.version, cycle.policy.id, cycle.policy.version, basis.version, input.reason, actor.userId, ports.clock.now(), ports.clock.now(), JSON.stringify(previous), JSON.stringify({ ...previous, id: compId, status: "draft", approved_by: null, created_by: actor.userId, created_at: ports.clock.now(), updated_at: ports.clock.now(), effective_from: input.effectiveFrom, base_amount_minor: amounts.base, hourly_rate_minor: amounts.hourly_rate, daily_rate_minor: amounts.daily_rate, version: 1 })),
        ]);
    }
    async function approveEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: version, reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.compensation.approve");
        ports.authorize(actor, "people.compensation.read");
        const op = "people.evaluation.compensation.approve", prior = await ports.commands.replay(actor, key, op, draftId, input);
        if (prior)
            return prior;
        const draft = await repository.readApproveEvaluationCompensationDraft(draftId, actor.companyId);
        if (!draft)
            throw new BusinessRuleError(404, "PAY_DRAFT_NOT_FOUND", "Compensation draft not found.");
        const { row, employee } = await evaluationAssessment(actor, String(draft.assessment_id));
        if (await repository.readApproveEvaluationCompensationEffect(actor.userId, draft.created_by, actor.companyId))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the pay change.");
        const effective = String(draft.effective_from), end = new Date(`${effective}T00:00:00Z`);
        end.setUTCDate(end.getUTCDate() - 1);
        if (effective <= await evaluationToday(employee.id))
            throw new BusinessRuleError(409, "PAY_EFFECTIVE_DATE_PASSED", "Review a future period before approving.");
        return ports.commands.commit(actor, key, op, input, draftId, input.expectedVersion + 1, { id: draftId, status: "approved", version: input.expectedVersion + 1 }, [
            repository.stageApproveEvaluationCompensationEffect(employee.id),
            repository.stageApproveEvaluationCompensationEffect2(employee.id),
            repository.guard84(key, "pay-draft-version", draftId, input.expectedVersion),
            repository.guard85(key, "assessment-version", row.id, Number(draft.assessment_version)),
            repository.guard86(key, "engagement-version", employee.id, Number(draft.engagement_version)),
            repository.guard87(key, "basis-version", employee.id, effective, Number(draft.basis_version)),
            repository.guard88(key, "policy-version", String(draft.policy_id), actor.companyId, Number(draft.policy_version)),
            repository.guard89(key, "previous-version", String(draft.previous_id), Number(draft.previous_version), effective, effective),
            repository.guard90(key, "new-wage-version", String(draft.compensation_id), 1),
            repository.guard91(key, "proposed-pay-snapshot", draftId),
            repository.guard92(key, "no-published-payroll", employee.id, effective),
            repository.guard93(key, "no-other-wage", employee.id, String(draft.previous_id), String(draft.compensation_id), effective),
            repository.stageApproveEvaluationCompensationEffect3(end.toISOString().slice(0, 10), ports.clock.now(), draft.previous_id),
            repository.stageApproveEvaluationCompensationEffect4(actor.userId, ports.clock.now(), draft.compensation_id),
            repository.stageApproveEvaluationCompensationEffect5(actor.userId, input.reason, ports.clock.now(), draftId),
            repository.stageApproveEvaluationCompensationEffect6(ports.ids.newId(), key, draftId, op, actor.userId, actor.assignmentId, input.expectedVersion, JSON.stringify(input), ports.clock.now()),
        ]);
    }
    async function cancelEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) {
        const input = z.object({ expectedVersion: version, reason }).strict().parse(raw);
        await ports.prepare(actor.holdingId);
        actor = await evaluationAuthority(actor, "people.manage");
        ports.authorize(actor, "people.compensation.read");
        const op = "people.evaluation.compensation.cancel", prior = await ports.commands.replay(actor, key, op, draftId, input);
        if (prior)
            return prior;
        const draft = await repository.readCancelEvaluationCompensationDraft(draftId, actor.companyId);
        if (!draft)
            throw new BusinessRuleError(404, "PAY_DRAFT_NOT_FOUND", "Compensation draft not found.");
        const { employee } = await evaluationAssessment(actor, String(draft.assessment_id));
        return ports.commands.commit(actor, key, op, input, draftId, input.expectedVersion + 1, { id: draftId, status: "cancelled", version: input.expectedVersion + 1 }, [
            repository.stageCancelEvaluationCompensationEffect(employee.id),
            repository.guard100(key, "engagement-version", employee.id, employee.version),
            repository.stageCancelEvaluationCompensationEffect2(draftId),
            repository.guard102(key, "pay-draft-version", draftId, input.expectedVersion),
            repository.guard103(key, "unapproved-pay", String(draft.compensation_id)),
            repository.stageCancelEvaluationCompensationEffect3(ports.clock.now(), draft.compensation_id),
            repository.stageCancelEvaluationCompensationEffect4(input.reason, ports.clock.now(), draftId),
        ]);
    }
    return { evaluationAuthority, evaluationEmployees, evaluationEmployee, evaluationDefinition, saveEvaluationDefinition, evaluationWorkspace, importEvaluationMarket, evaluationAssessment, createEvaluationAssessment, saveEvaluationAssessment, transitionEvaluationAssessment, evaluationDetail, evaluationEvidenceSources, addEvaluationEvidence, readEvaluationEvidence, confirmEvaluationPayBasis, createEvaluationCompensationDraft, approveEvaluationCompensation, cancelEvaluationCompensation };
}
export type EvaluationWorkspace = Awaited<ReturnType<ReturnType<typeof createPeopleEvaluationApplication>['evaluationWorkspace']>>;
