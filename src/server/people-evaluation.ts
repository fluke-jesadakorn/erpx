import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { assessmentContentSchema, bandSchema, basisSchema, benchmarkSchema, compensationMetrics, cycleSchema, defaultEvaluationPolicy, definitionSchemas, emptyAssessment, evaluationDate, evaluationMetrics, monthlyFtePay, policySchema, roleSchema, type AssessmentContent, type Compensation, type DefinitionKind, type PayBasis, type RoleProfile } from "@/lib/people-evaluation";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { captureJobAuthority } from "./iam-jobs";
import { requirePermission } from "./iam-permissions";
import { iamMutationGuards } from "./iam-store";
import { holdingResources } from "./runtime";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { snapshotMoney } from "./company-money";
import { decimalToMinor } from "@/lib/finance-money";

type Row = Record<string, unknown>;
const reason = z.string().trim().min(3).max(4000), idSchema = z.string().min(1).max(180), version = z.number().int().positive();
export const evaluationQuerySchema = z.object({ year: z.coerce.number().int().min(2020).max(2200).default(new Date().getUTCFullYear()), date: evaluationDate.optional(), cycleId: idSchema.optional(), departmentId: idSchema.optional(), family: z.string().max(160).optional(), levelId: idSchema.optional(), location: z.string().max(160).optional(), engagementId: idSchema.optional() }).strict();
export type EvaluationQuery = z.infer<typeof evaluationQuerySchema>;
type AssessmentRow = { id: string; company_id: string; engagement_id: string; cycle_id: string; role_id: string; role_version: number; period_start: string; period_end: string; status: string; content_json: string; version: number; created_by: string; reviewed_by: string | null; confirmed_by: string | null; updated_at: number };
export type EvaluationEmployee = { id: string; personId: string; name: string; userId: string | null; positionId: string | null; levelId: string | null; jobTitle: string; branchId: string | null; departmentId: string | null; department: string | null; currency: string; startDate: string; endDate: string | null; version: number };
export type EvaluationRow = { key: string; employee: EvaluationEmployee; assessment: { id: string; version: number; cycleId: string; status: string; periodStart: string; periodEnd: string; content: AssessmentContent; createdBy: string; reviewedBy: string | null; confirmedBy: string | null } | null; role: RoleProfile | null; roleRef: { id: string; version: number } | null; metrics: ReturnType<typeof evaluationMetrics>; compensation?: { wage: Compensation | null; band: z.infer<typeof bandSchema> | null; basis: PayBasis | null; basisVersion: number | null; metrics: ReturnType<typeof compensationMetrics> } };

/** Also used for evidence and workers; no client-supplied Company or team expands this boundary. */
export async function evaluationAuthority(db: SqlDatabase, actor: WorkContext, permission = "people.read") {
  if (!actor.iam?.queuedAssignmentVersion) actor = await captureJobAuthority(db, actor);
  requirePermission(actor, permission);
  const key = crypto.randomUUID();
  try { await db.batch([...iamMutationGuards(db, actor, key), db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]); }
  catch { throw new ApiError(403, "EVALUATION_ACCESS_CHANGED", "Your evaluation access changed. Reload your work role."); }
  return actor;
}
export async function evaluationEmployees(db: SqlDatabase, actor: WorkContext) {
  const rows = (await db.prepare(`WITH RECURSIVE team AS (
    SELECT e.person_id FROM hr_engagements e JOIN hr_people manager ON manager.id=e.manager_person_id AND manager.company_id=e.company_id WHERE e.company_id=? AND manager.user_id=? AND e.status IN ('active','suspended','ended')
    UNION SELECT e.person_id FROM hr_engagements e JOIN team t ON t.person_id=e.manager_person_id WHERE e.company_id=? AND e.status IN ('active','suspended','ended')
  ) SELECT e.id,e.person_id,p.legal_name,p.user_id,e.position_id,e.level_id,e.job_title,e.branch_id,e.department_id,n.display_name department,e.currency,e.start_date,e.end_date,e.version
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id LEFT JOIN organization_nodes n ON n.id=e.department_id AND n.company_id=e.company_id
    WHERE e.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND e.status IN ('active','suspended','ended')
    AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
    AND (?<>'people_manager' OR e.person_id IN (SELECT person_id FROM team)) ORDER BY p.legal_name,e.id`)
    .bind(actor.companyId, actor.userId, actor.companyId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.role).all<Row>()).results;
  return rows.map(r => ({ id: String(r.id), personId: String(r.person_id), name: String(r.legal_name), userId: r.user_id ? String(r.user_id) : null, positionId: r.position_id ? String(r.position_id) : null, levelId: r.level_id ? String(r.level_id) : null, jobTitle: String(r.job_title), branchId: r.branch_id ? String(r.branch_id) : null, departmentId: r.department_id ? String(r.department_id) : null, department: r.department ? String(r.department) : null, currency: String(r.currency), startDate: String(r.start_date), endDate: r.end_date ? String(r.end_date) : null, version: Number(r.version) })) as EvaluationEmployee[];
}
export async function evaluationEmployee(db: SqlDatabase, actor: WorkContext, engagementId: string) {
  const employee = (await evaluationEmployees(db, actor)).find(e => e.id === engagementId);
  if (!employee) throw new ApiError(404, "EVALUATION_EMPLOYEE_NOT_FOUND", "Employee not found in your evaluation scope.");
  return employee;
}
export async function evaluationDefinition(db: SqlDatabase, actor: WorkContext, id: string, kind: DefinitionKind, v?: number) {
  const row = await db.prepare(v ? "SELECT id,kind,version,payload_json,created_at updated_at FROM hr_evaluation_definition_history WHERE id=? AND kind=? AND company_id=? AND version=?" : "SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE id=? AND kind=? AND company_id=?")
    .bind(id, kind, actor.companyId, ...(v ? [v] : [])).first<{ id: string; kind: DefinitionKind; version: number; payload_json: string; updated_at: number }>();
  if (!row) throw new ApiError(404, "EVALUATION_DEFINITION_NOT_FOUND", "Evaluation criteria are unavailable in this Company.");
  return { id: row.id, kind: row.kind, version: Number(row.version), data: definitionSchemas[kind].parse(JSON.parse(row.payload_json)) as Record<string, unknown>, updatedAt: Number(row.updated_at) };
}
async function frozenPolicy(db: SqlDatabase, actor: WorkContext, cycleId: string) {
  const cycle = await evaluationDefinition(db, actor, cycleId, "cycle"), data = cycleSchema.parse(cycle.data);
  const policy = await evaluationDefinition(db, actor, data.policy.id, "policy", data.policy.version);
  return { cycle, data, policy, policyData: policySchema.parse(policy.data) };
}
async function evaluationToday(db: SqlDatabase, engagementId: string) {
  const group = await db.prepare("SELECT g.timezone FROM hr_engagements e JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.id=?").bind(engagementId).first<{timezone:string}>();
  return new Intl.DateTimeFormat("en-CA", { timeZone: group?.timezone ?? "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date());
}
function confirmed(data: { confirmed: boolean }) { if (!data.confirmed) throw new ApiError(422, "CRITERIA_NOT_CONFIRMED", "HR must confirm the Company's criteria before use."); }
function fullHr(actor: WorkContext) { requirePermission(actor, "organization.manage"); if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_EVALUATION_REQUIRED", "Company HR manages evaluation criteria and annual cycles."); }
export async function saveEvaluationDefinition(actor: WorkContext, raw: unknown, key: string) {
  const input = z.object({ id: idSchema.optional(), kind: z.enum(["skill", "role", "policy", "cycle", "band", "benchmark"]), expectedVersion: z.number().int().nonnegative(), data: z.unknown(), reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId);
  actor = await evaluationAuthority(db, actor, "people.manage"); fullHr(actor);
  if (["band", "benchmark", "policy"].includes(input.kind)) requirePermission(actor, "people.compensation.read");
  const payload = definitionSchemas[input.kind].parse(input.data), id = input.id ?? `evaluation-${input.kind}-${key}`, op = "people.evaluation.criteria.save";
  const previous = await replayCommand(db, actor, key, op, id, input); if (previous) return previous;
  const old = input.expectedVersion ? await evaluationDefinition(db, actor, id, input.kind) : null;
  if (old && old.version !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "Criteria changed. Reload before saving.");
  const checks: SqlStatement[] = [];
  if (input.kind === "role") {
    const role = roleSchema.parse(payload);
    const position = await db.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND level_id=? AND active=1").bind(role.positionId, actor.companyId, role.levelId).first();
    if (!position) throw new ApiError(422, "ROLE_POSITION_MISMATCH", "Choose an active Company position and its staff level.");
    for (const r of role.requirements) { const skill = await evaluationDefinition(db, actor, r.skill.id, "skill", r.skill.version); if (role.confirmed) confirmed(skill.data as { confirmed: boolean }); }
  }
  if (input.kind === "band" || input.kind === "benchmark") {
    const band = payload as z.infer<typeof bandSchema>;
    if (!await db.prepare("SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1").bind(band.levelId, actor.companyId).first()) throw new ApiError(422, "LEVEL_NOT_FOUND", "Choose an active Company staff level.");
    if (input.kind === "band" && band.benchmark) {
      const market = benchmarkSchema.parse((await evaluationDefinition(db, actor, band.benchmark.id, "benchmark", band.benchmark.version)).data);
      if (market.family !== band.family || market.levelId !== band.levelId || market.location !== band.location || market.currency !== band.currency) throw new ApiError(422, "BENCHMARK_MISMATCH", "Market source must use the same job family, level, location and currency.");
      if (band.confirmed) confirmed(market);
    }
  }
  if (input.kind === "cycle") {
    const cycle = cycleSchema.parse(payload), policy = await evaluationDefinition(db, actor, cycle.policy.id, "policy", cycle.policy.version);
    if (cycle.status !== "draft") { confirmed(cycle); confirmed(policy.data as { confirmed: boolean }); }
    if (old) {
      const before = cycleSchema.parse(old.data);
      if (before.status !== "draft" && (JSON.stringify({ ...before, status: cycle.status }) !== JSON.stringify(cycle) || cycle.status !== "closed")) throw new ApiError(409, "CYCLE_FROZEN", "An opened cycle keeps its year, criteria and policy. Only closing it is allowed.");
    }
    checks.push(assertion(db, key, "unique-cycle", "NOT EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE company_id=? AND kind='cycle' AND id<>? AND (payload_json::jsonb->>'year')::integer=?)", [actor.companyId, id, cycle.year]));
  }
  const now = Date.now(), next = input.expectedVersion + 1;
  return executeCommand(db, actor, key, op, input, id, next, { id, version: next }, [
    db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`evaluation-criteria:${actor.companyId}`),
    ...checks,
    ...(old ? [assertion(db, key, "criteria-version", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND kind=? AND version=?)", [id, actor.companyId, input.kind, input.expectedVersion]), db.prepare("UPDATE hr_evaluation_definitions SET payload_json=?,version=version+1,updated_by=?,reason=?,updated_at=? WHERE id=?").bind(JSON.stringify(payload), actor.userId, input.reason, now, id)] : [db.prepare("INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, input.kind, JSON.stringify(payload), actor.userId, actor.userId, input.reason, now, now)]),
  ]);
}
export async function evaluationWorkspace(actor: WorkContext, raw: unknown = {}) {
  const query = evaluationQuerySchema.parse(raw), date = query.date ?? `${query.year}-12-31`;
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor);
  const canCompensation = hasPermission(actor, "people.compensation.read");
  const definitions = (await db.prepare("SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE company_id=? ORDER BY kind,id").bind(actor.companyId).all<{ id: string; kind: DefinitionKind; version: number; payload_json: string; updated_at: number }>()).results.map(r => ({ id: r.id, kind: r.kind, version: Number(r.version), data: JSON.parse(r.payload_json) as Record<string, unknown>, updatedAt: Number(r.updated_at) }));
  const cycles = definitions.filter(d => d.kind === "cycle" && cycleSchema.parse(d.data).year === query.year);
  const cycle = query.cycleId ? cycles.find(d => d.id === query.cycleId) : cycles[0];
  if (query.cycleId && !cycle) throw new ApiError(404, "CYCLE_NOT_FOUND", "Annual cycle not found for this year.");
  const policy = cycle ? (await frozenPolicy(db, actor, cycle.id)).policyData : defaultEvaluationPolicy;
  const employees = (await evaluationEmployees(db, actor)).filter(e => e.startDate <= date && (!e.endDate || e.endDate >= `${query.year}-01-01`) && (!query.departmentId || e.departmentId === query.departmentId) && (!query.levelId || e.levelId === query.levelId) && (!query.engagementId || e.id === query.engagementId));
  const assessments = cycle ? (await db.prepare("SELECT * FROM hr_evaluation_assessments WHERE company_id=? AND cycle_id=? AND period_start<=? ORDER BY period_start,id").bind(actor.companyId, cycle.id, date).all<AssessmentRow>()).results : [];
  const rows: EvaluationRow[] = [];
  for (const employee of employees) {
    const segments = assessments.filter(a => a.engagement_id === employee.id);
    for (const assessment of segments.length ? segments : [null]) {
      const definition = assessment ? await evaluationDefinition(db, actor, assessment.role_id, "role", Number(assessment.role_version)) : definitions.find(d => d.kind === "role" && d.data.positionId === employee.positionId && d.data.confirmed === true) ?? null;
      const role = definition ? roleSchema.parse(definition.data) : null;
      if ((query.family && role?.family !== query.family) || (query.location && role?.location !== query.location)) continue;
      const content = assessment ? assessmentContentSchema.parse(JSON.parse(assessment.content_json)) : emptyAssessment;
      const target = content.targetRole ? roleSchema.parse((await evaluationDefinition(db, actor, content.targetRole.id, "role", content.targetRole.version)).data) : null;
      const row: EvaluationRow = { key: assessment?.id ?? employee.id, employee, role, roleRef: definition ? { id: definition.id, version: definition.version } : null, assessment: assessment ? { id: assessment.id, version: Number(assessment.version), cycleId: assessment.cycle_id, status: assessment.status, periodStart: assessment.period_start, periodEnd: assessment.period_end, content, createdBy: assessment.created_by, reviewedBy: assessment.reviewed_by, confirmedBy: assessment.confirmed_by } : null, metrics: evaluationMetrics(role, target, content, policy) };
      if (canCompensation) {
        const wages = (await db.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC").bind(employee.id, date, date).all<Compensation>()).results;
        const wage = wages.length === 1 ? wages[0] : null;
        const basisRow = await db.prepare("SELECT version,payload_json FROM hr_evaluation_pay_basis WHERE engagement_id=? AND company_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1").bind(employee.id, actor.companyId, date).first<{ version: number; payload_json: string }>();
        const basis = basisRow ? basisSchema.parse(JSON.parse(basisRow.payload_json)) : null;
        const bands = definitions.filter(d => d.kind === "band" && d.data.family === role?.family && d.data.levelId === role?.levelId && d.data.location === role?.location && d.data.currency === employee.currency && d.data.confirmed && String(d.data.effectiveFrom) <= date && String(d.data.effectiveTo) >= date);
        const band = bands.length === 1 ? bandSchema.parse(bands[0].data) : null;
        const market = band?.benchmark ? benchmarkSchema.parse((await evaluationDefinition(db, actor, band.benchmark.id, "benchmark", band.benchmark.version)).data) : null;
        row.compensation = { wage, band, basis, basisVersion: basisRow ? Number(basisRow.version) : null, metrics: compensationMetrics(wage, basis, band, market, date, policy) };
        if (wages.length > 1) row.compensation.metrics.missing = "Overlapping approved wages require HR review";
        if (bands.length > 1) row.compensation.metrics.missing = "Overlapping pay bands require HR review";
      }
      rows.push(row);
    }
  }
  const skillRefs = new Map<string, {id:string;version:number}>();
  for (const profile of [...definitions.filter(d => d.kind === "role").map(d => roleSchema.parse(d.data)), ...rows.flatMap(r => r.role ? [r.role] : [])]) for (const requirement of profile.requirements) skillRefs.set(`${requirement.skill.id}:${requirement.skill.version}`, requirement.skill);
  for (const row of rows) if (row.assessment?.content.targetRole) {
    const ref = row.assessment.content.targetRole, target = roleSchema.parse((await evaluationDefinition(db, actor, ref.id, "role", ref.version)).data);
    for (const requirement of target.requirements) skillRefs.set(`${requirement.skill.id}:${requirement.skill.version}`, requirement.skill);
  }
  const skillVersions = await Promise.all([...skillRefs.values()].map(ref => evaluationDefinition(db, actor, ref.id, "skill", ref.version)));
  const publicDefinitions = definitions.filter(d => canCompensation || !["band", "benchmark"].includes(d.kind)).map(d => !canCompensation && d.kind === "policy" ? { ...d, data: { ...d.data, maxRaisePercent: undefined, compensationRule: undefined } } : d);
  const positions = hasPermission(actor, "organization.manage") ? (await db.prepare("SELECT id,title,level_id FROM organization_positions WHERE company_id=? AND active=1 ORDER BY title").bind(actor.companyId).all<{ id: string; title: string; level_id: string }>()).results : [];
  const levels = (await db.prepare("SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ORDER BY rank").bind(actor.companyId).all<{ id: string; name: string }>()).results;
  return { rows, skillVersions, definitions: publicDefinitions, positions, levels, query: { ...query, date }, cycleId: cycle?.id ?? null, policy: canCompensation ? policy : { ...policy, maxRaisePercent: undefined, compensationRule: undefined }, actorId: actor.userId, canWrite: hasPermission(actor, "people.manage"), canConfigure: hasPermission(actor, "organization.manage") && !actor.branchId && !actor.departmentId, canConfirm: hasPermission(actor, "people.manage"), canCompensation, canEvidenceFiles: canCompensation && hasPermission(actor, "people.documents.read"), canApproveCompensation: hasPermission(actor, "people.compensation.approve") && canCompensation };
}
export type EvaluationWorkspace = Awaited<ReturnType<typeof evaluationWorkspace>>;
export async function importEvaluationMarket(actor: WorkContext, raw: unknown, key: string) {
  const input = z.object({ rows: z.array(benchmarkSchema).min(1).max(100), reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId);
  actor = await evaluationAuthority(db, actor, "people.manage"); fullHr(actor); requirePermission(actor, "people.compensation.read");
  const id = `evaluation-import-${key}`, op = "people.evaluation.market.import", prior = await replayCommand(db, actor, key, op, id, input); if (prior) return prior;
  const levels = (await db.prepare("SELECT id FROM staff_levels WHERE company_id=? AND active=1").bind(actor.companyId).all<{id:string}>()).results;
  if (input.rows.some(r => !levels.some(l => l.id === r.levelId))) throw new ApiError(422, "IMPORT_LEVEL_UNAVAILABLE", "Every market row must refer to an active Company staff level.");
  const now = Date.now();
  return executeCommand(db, actor, key, op, input, id, 1, { id, imported: input.rows.length, confirmed: false }, input.rows.map((row, i) => db.prepare("INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES(?,?,'benchmark',?,?,?,?,?,?)").bind(`${id}-${i}`, actor.companyId, JSON.stringify({...row, confirmed:false}), actor.userId, actor.userId, input.reason, now, now)));
}
export async function evaluationAssessment(db: SqlDatabase, actor: WorkContext, assessmentId: string) {
  const row = await db.prepare("SELECT * FROM hr_evaluation_assessments WHERE id=? AND company_id=?").bind(assessmentId, actor.companyId).first<AssessmentRow>();
  if (!row) throw new ApiError(404, "ASSESSMENT_NOT_FOUND", "Assessment not found.");
  const employee = await evaluationEmployee(db, actor, row.engagement_id);
  return { row, employee };
}
async function validateEvaluationRolePeriod(db: SqlDatabase, actor: WorkContext, employee: EvaluationEmployee, role: RoleProfile, periodStart: string, periodEnd: string) {
  // Respect dated changes in position/level, independently of profile revisions and actual-work evidence.
  const history = (await db.prepare("SELECT before_json,after_json,created_at FROM staff_organization_history WHERE company_id=? AND engagement_id=? ORDER BY created_at,id").bind(actor.companyId, employee.id).all<{ before_json: string; after_json: string; created_at: number }>()).results;
  const zone = await db.prepare("SELECT g.timezone FROM hr_engagements e JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.id=?").bind(employee.id).first<{timezone:string}>();
  const changes = history.map(h => {
    const before = JSON.parse(h.before_json), after = JSON.parse(h.after_json);
    const position = (r: Row) => r.positionId ?? r.position_id ?? null, level = (r: Row) => r.levelId ?? r.level_id ?? null;
    return { before: { positionId: position(before), levelId: level(before) }, after: { positionId: position(after), levelId: level(after) }, date: new Intl.DateTimeFormat("en-CA", { timeZone: zone?.timezone ?? "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(Number(after.effectiveAt ?? h.created_at))) };
  }).filter(h => h.before.positionId !== h.after.positionId || h.before.levelId !== h.after.levelId).sort((a,b) => a.date.localeCompare(b.date));
  if (changes.some(h => h.date > periodStart && h.date <= periodEnd)) throw new ApiError(422, "ROLE_PERIOD_SPLIT_REQUIRED", "Split this assessment at each recorded position or staff-level change.");
  const priorChange = changes.filter(h => h.date <= periodStart).at(-1), futureChange = changes.find(h => h.date > periodStart);
  const assigned = priorChange?.after ?? futureChange?.before ?? { positionId: employee.positionId, levelId: employee.levelId };
  if (assigned.positionId !== role.positionId || assigned.levelId !== role.levelId) throw new ApiError(422, "ROLE_ASSIGNMENT_UNVERIFIED", "Use the position and staff level recorded for this role period.");
}
export async function createEvaluationAssessment(actor: WorkContext, raw: unknown, key: string) {
  const input = z.object({ engagementId: idSchema, cycleId: idSchema, role: z.object({ id: idSchema, version }).strict(), periodStart: evaluationDate, periodEnd: evaluationDate, reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage");
  const id = `assessment-${key}`, op = "people.evaluation.assessment.create", prior = await replayCommand(db, actor, key, op, id, input); if (prior) return prior;
  const employee = await evaluationEmployee(db, actor, input.engagementId), cycle = await frozenPolicy(db, actor, input.cycleId), role = roleSchema.parse((await evaluationDefinition(db, actor, input.role.id, "role", input.role.version)).data);
  confirmed(role);
  if (cycle.data.status !== "open" || !cycle.policyData.confirmed) throw new ApiError(409, "CYCLE_NOT_OPEN", "Confirm the Company's policy and open the annual cycle first.");
  if (input.periodStart > input.periodEnd || input.periodStart < `${cycle.data.year}-01-01` || input.periodEnd > `${cycle.data.year}-12-31` || input.periodStart < employee.startDate || (employee.endDate && input.periodEnd > employee.endDate)) throw new ApiError(422, "ASSESSMENT_PERIOD_INVALID", "Evaluation period must fall within the annual cycle and employment.");
  await validateEvaluationRolePeriod(db, actor, employee, role, input.periodStart, input.periodEnd);
  return executeCommand(db, actor, key, op, input, id, 1, { id, status: "draft", version: 1 }, [
    db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id),
    assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)", [employee.id, actor.companyId, employee.version]),
    db.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(cycle.cycle.id), assertion(db, key, "cycle-open", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [input.cycleId, cycle.cycle.version]),
    assertion(db, key, "period-overlap", "NOT EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE engagement_id=? AND cycle_id=? AND period_start<=? AND period_end>=?)", [employee.id, input.cycleId, input.periodEnd, input.periodStart]),
    db.prepare("INSERT INTO hr_evaluation_assessments(id,company_id,engagement_id,cycle_id,role_id,role_version,period_start,period_end,content_json,created_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, employee.id, input.cycleId, input.role.id, input.role.version, input.periodStart, input.periodEnd, JSON.stringify(emptyAssessment), actor.userId, input.reason, Date.now(), Date.now()),
  ]);
}
export async function saveEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: version, content: assessmentContentSchema, periodStart: evaluationDate.optional(), periodEnd: evaluationDate.optional(), reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage");
  const prior = await replayCommand(db, actor, key, "people.evaluation.assessment.save", assessmentId, input); if (prior) return prior;
  const { row, employee } = await evaluationAssessment(db, actor, assessmentId), cycle = await frozenPolicy(db, actor, row.cycle_id);
  if (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open") throw new ApiError(409, "ASSESSMENT_LOCKED", "Reopen the assessment before changing scores; closed cycles retain their results.");
  const role = roleSchema.parse((await evaluationDefinition(db, actor, row.role_id, "role", row.role_version)).data);
  const periodStart = input.periodStart ?? row.period_start, periodEnd = input.periodEnd ?? row.period_end;
  if (periodStart > periodEnd || periodStart < `${cycle.data.year}-01-01` || periodEnd > `${cycle.data.year}-12-31` || periodStart < employee.startDate || (employee.endDate && periodEnd > employee.endDate)) throw new ApiError(422, "ASSESSMENT_PERIOD_INVALID", "The revised period must remain within the cycle and employment.");
  await validateEvaluationRolePeriod(db, actor, employee, role, periodStart, periodEnd);
  const target = input.content.targetRole ? roleSchema.parse((await evaluationDefinition(db, actor, input.content.targetRole.id, "role", input.content.targetRole.version)).data) : null;
  if (target) { confirmed(target); if (target.positionId === role.positionId) throw new ApiError(422, "TARGET_ROLE_REQUIRED", "Select a distinct target role for readiness."); }
  const allowed = new Set([...role.requirements, ...(target?.requirements ?? [])].map(r => `${r.skill.id}:${r.skill.version}`));
  if (input.content.skills.some(s => !allowed.has(`${s.skillId}:${s.skillVersion}`))) throw new ApiError(422, "SKILL_NOT_IN_ROLE", "Rate only skills in the current or selected target profile.");
  const evidence = (await db.prepare("SELECT id,observed_on FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=?").bind(assessmentId, actor.companyId).all<{ id: string; observed_on: string }>()).results;
  const cited = [...input.content.skills.flatMap(s => s.evidenceIds), ...[input.content.performance.results, input.content.performance.quality, input.content.performance.collaboration].flatMap(p => p?.evidenceIds ?? []), ...(input.content.actualWork?.evidenceIds ?? [])];
  if (cited.some(id => !evidence.some(e => e.id === id && e.observed_on >= periodStart && e.observed_on <= periodEnd))) throw new ApiError(422, "EVIDENCE_REFERENCE_INVALID", "Every score must cite evidence from this assessment.");
  const guards = [db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id), assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [employee.id, employee.version]), assertion(db, key, "period-overlap", "NOT EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE engagement_id=? AND cycle_id=? AND id<>? AND period_start<=? AND period_end>=?)", [employee.id, row.cycle_id, assessmentId, periodEnd, periodStart]), db.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId), db.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(cycle.cycle.id), assertion(db, key, "cycle-open", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [row.cycle_id, cycle.cycle.version]), assertion(db, key, "assessment-version", "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, input.expectedVersion, row.status])];
  return executeCommand(db, actor, key, "people.evaluation.assessment.save", input, assessmentId, input.expectedVersion + 1, { id: assessmentId, version: input.expectedVersion + 1, status: row.status }, [...guards, db.prepare("UPDATE hr_evaluation_assessments SET period_start=?,period_end=?,content_json=?,reviewed_by=?,updated_by=?,reason=?,version=version+1,updated_at=? WHERE id=?").bind(periodStart, periodEnd, JSON.stringify(input.content), actor.userId, actor.userId, input.reason, Date.now(), assessmentId)]);
}
export async function transitionEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
  const input = z.object({ action: z.enum(["submit", "calibrate", "confirm", "reopen"]), expectedVersion: version, reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage");
  const prior = await replayCommand(db, actor, key, "people.evaluation.assessment.transition", assessmentId, input); if (prior) return prior;
  const { row, employee } = await evaluationAssessment(db, actor, assessmentId), cycle = await frozenPolicy(db, actor, row.cycle_id);
  const states = { submit: ["draft", "submitted"], calibrate: ["submitted", "calibrating"], confirm: ["calibrating", "confirmed"], reopen: [row.status, "draft"] };
  if (row.status !== states[input.action][0] || (input.action === "reopen" && row.status === "draft") || cycle.data.status !== "open") throw new ApiError(409, "INVALID_EVALUATION_TRANSITION", "The assessment cannot take this step in the current annual cycle.");
  if (input.action === "submit" || input.action === "confirm") await validateEvaluationRolePeriod(db, actor, employee, roleSchema.parse((await evaluationDefinition(db, actor, row.role_id, "role", row.role_version)).data), row.period_start, row.period_end);
  if (input.action === "confirm") {
    if (await db.prepare("SELECT iam_same_person(?,?,?) same").bind(actor.userId, row.created_by, actor.companyId).first<boolean>("same") || (row.reviewed_by && await db.prepare("SELECT iam_same_person(?,?,?) same").bind(actor.userId, row.reviewed_by, actor.companyId).first<boolean>("same"))) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different person must confirm the scores after calibration.");
  }
  return executeCommand(db, actor, key, "people.evaluation.assessment.transition", input, assessmentId, input.expectedVersion + 1, { id: assessmentId, status: states[input.action][1], version: input.expectedVersion + 1 }, [db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id), assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [employee.id, employee.version]), db.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId),
    assertion(db, key, "assessment-version", "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, input.expectedVersion, row.status]),
    db.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(cycle.cycle.id), assertion(db, key, "cycle-open", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [row.cycle_id, cycle.cycle.version]),
    assertion(db, key, "no-pay-draft", "NOT EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE assessment_id=? AND status='draft')", [assessmentId]),
    db.prepare("UPDATE hr_evaluation_assessments SET status=?,updated_by=?,confirmed_by=?,reviewed_by=CASE WHEN ?='draft' THEN NULL ELSE reviewed_by END,reason=?,version=version+1,updated_at=? WHERE id=?").bind(states[input.action][1], actor.userId, input.action === "confirm" ? actor.userId : null, states[input.action][1], input.reason, Date.now(), assessmentId),
  ]);
}
export async function evaluationDetail(actor: WorkContext, assessmentId: string) {
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor);
  const { employee } = await evaluationAssessment(db, actor, assessmentId);
  let evidence = (await db.prepare("SELECT id,source_type,source_id,description,observed_on,created_by,created_at FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=? ORDER BY created_at,id").bind(assessmentId, actor.companyId).all<Row>()).results;
  if (!hasPermission(actor, "people.compensation.read") || !hasPermission(actor, "people.documents.read")) evidence = evidence.filter(e => e.source_type !== "file");
  const history = (await db.prepare("SELECT version,status,content_json,actor_id,reason,created_at,period_start,period_end,role_id,role_version FROM hr_evaluation_assessment_history WHERE id=? AND company_id=? ORDER BY version DESC").bind(assessmentId, actor.companyId).all<Row>()).results;
  const sources = await evaluationEvidenceSources(db, actor, employee);
  const salary = hasPermission(actor, "people.compensation.read") ? { wages: (await db.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('approved','superseded') ORDER BY effective_from DESC").bind(employee.id).all<Compensation>()).results, drafts: (await db.prepare("SELECT d.id,d.compensation_id,d.version,d.status,d.reason,d.review_reason,d.created_by,d.approved_by,d.previous_json,d.proposed_json,c.effective_from,c.base_amount_minor,c.hourly_rate_minor,c.daily_rate_minor FROM hr_evaluation_compensation_drafts d JOIN hr_compensation_revisions c ON c.id=d.compensation_id WHERE d.assessment_id=? AND d.company_id=? ORDER BY d.created_at DESC").bind(assessmentId, actor.companyId).all<Row>()).results } : undefined;
  const files = hasPermission(actor, "people.compensation.read") && hasPermission(actor, "people.documents.read") ? (await db.prepare("SELECT l.id,a.original_name filename FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=? AND s.target_type='hr_evaluation_assessment' AND s.target_id=? AND l.state='bound' AND a.source_state='available'").bind(actor.companyId, assessmentId).all<{id:string;filename:string}>()).results : [];
  return { evidence, history, sources, salary, files };
}
export async function evaluationEvidenceSources(db: SqlDatabase, actor: WorkContext, employee: EvaluationEmployee, sourceId: string | null = null): Promise<(Row & {type:string})[]> {
  const [tasks, projects, learning] = await Promise.all([
    db.prepare("SELECT id,title,state,completion_note,version,updated_at FROM staff_tasks WHERE company_id=? AND assignee_user_id=? AND state='completed' AND (?::text IS NULL OR id=?) AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC LIMIT 100").bind(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>(),
    db.prepare("SELECT p.id,p.title,p.version,p.updated_at FROM staff_projects p JOIN staff_project_members m ON m.project_id=p.id WHERE p.company_id=? AND m.user_id=? AND (?::text IS NULL OR p.id=?) AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY p.updated_at DESC LIMIT 100").bind(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>(),
    db.prepare("SELECT a.id,c.title,a.status,a.version,a.updated_at FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.company_id=? AND a.learner_id=? AND a.status='completed' AND (?::text IS NULL OR a.id=?) AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) ORDER BY a.updated_at DESC LIMIT 100").bind(actor.companyId, employee.userId, sourceId, sourceId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>(),
  ]);
  return [...tasks.results.map(r => ({ type: "task", ...r })), ...projects.results.map(r => ({ type: "project", ...r })), ...learning.results.map(r => ({ type: "learning", ...r }))];
}
export async function addEvaluationEvidence(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: version, sourceType: z.enum(["kpi", "task", "project", "learning", "file"]), sourceId: idSchema.nullable().default(null), description: reason, observedOn: evaluationDate }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage");
  const id = `evaluation-evidence-${key}`, prior = await replayCommand(db, actor, key, "people.evaluation.evidence.add", id, input); if (prior) return prior;
  const { row, employee } = await evaluationAssessment(db, actor, assessmentId), cycle = await frozenPolicy(db, actor, row.cycle_id);
  if (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open") throw new ApiError(409, "ASSESSMENT_LOCKED", "Add evidence to a draft or calibration in an open cycle.");
  if (input.observedOn < row.period_start || input.observedOn > row.period_end) throw new ApiError(422, "EVIDENCE_DATE_INVALID", "Evidence date must be within this role period.");
  let source: unknown = { description: input.description };
  if (["task", "project", "learning"].includes(input.sourceType)) {
    source = (await evaluationEvidenceSources(db, actor, employee, input.sourceId)).find(s => s.type === input.sourceType && "id" in s && s.id === input.sourceId);
    if (!source) throw new ApiError(404, "EVIDENCE_SOURCE_UNAVAILABLE", "Source must belong to this employee and current team scope.");
  }
  if (input.sourceType === "file") {
    requirePermission(actor, "people.compensation.read"); requirePermission(actor, "people.documents.read");
    const { attachmentLinkStatus } = await import("./attachments/service");
    await attachmentLinkStatus(db, actor, input.sourceId ?? "");
    const link = await db.prepare("SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments f ON f.id=l.attachment_id WHERE l.id=? AND s.target_type='hr_evaluation_assessment' AND s.target_id=? AND s.company_id=? AND l.state='bound' AND f.source_state='available'").bind(input.sourceId, assessmentId, actor.companyId).first();
    if (!link) throw new ApiError(404, "EVIDENCE_FILE_UNAVAILABLE", "Choose a scanned file attached to this assessment.");
    source = { linkId: input.sourceId };
  }
  return executeCommand(db, actor, key, "people.evaluation.evidence.add", input, id, 1, { id, assessmentVersion: input.expectedVersion + 1 }, [db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id), assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [employee.id, employee.version]), db.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId),
    assertion(db, key, "assessment-version", "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, input.expectedVersion, row.status]),
    db.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(cycle.cycle.id), assertion(db, key, "cycle-open", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [row.cycle_id, cycle.cycle.version]),
    db.prepare("INSERT INTO hr_evaluation_evidence(id,company_id,assessment_id,source_type,source_id,description,observed_on,source_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, assessmentId, input.sourceType, input.sourceId, input.description, input.observedOn, JSON.stringify(source), actor.userId, Date.now()),
    db.prepare("UPDATE hr_evaluation_assessments SET version=version+1,reviewed_by=?,updated_by=?,reason='Evidence added',updated_at=? WHERE id=?").bind(actor.userId, actor.userId, Date.now(), assessmentId),
  ]);
}
export async function readEvaluationEvidence(actor: WorkContext, evidenceId: string) {
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor);
  const evidence = await db.prepare("SELECT * FROM hr_evaluation_evidence WHERE id=? AND company_id=?").bind(evidenceId, actor.companyId).first<Row>();
  if (!evidence) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "Evidence unavailable.");
  const { employee } = await evaluationAssessment(db, actor, String(evidence.assessment_id));
  if (evidence.source_type === "file") { requirePermission(actor, "people.compensation.read"); requirePermission(actor, "people.documents.read"); }
  if (["task", "project", "learning"].includes(String(evidence.source_type)) && !(await evaluationEvidenceSources(db, actor, employee, String(evidence.source_id))).some(s => s.type === evidence.source_type && "id" in s && s.id === evidence.source_id)) throw new ApiError(404, "EVIDENCE_SOURCE_UNAVAILABLE", "The current source is unavailable in your scope.");
  return evidence;
}
export async function confirmEvaluationPayBasis(actor: WorkContext, engagementId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: z.number().int().nonnegative(), effectiveFrom: evaluationDate, basis: basisSchema }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage"); requirePermission(actor, "people.compensation.read");
  const prior = await replayCommand(db, actor, key, "people.evaluation.basis.confirm", engagementId, input); if (prior) return prior;
  const employee = await evaluationEmployee(db, actor, engagementId);
  if (input.effectiveFrom < employee.startDate) throw new ApiError(422, "BASIS_DATE_INVALID", "Basis date must be within employment.");
  return executeCommand(db, actor, key, "people.evaluation.basis.confirm", input, engagementId, input.expectedVersion + 1, { id: engagementId, version: input.expectedVersion + 1 }, [
    db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(engagementId),
    assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [engagementId, employee.version]),
    assertion(db, key, "basis-version", "COALESCE((SELECT max(version) FROM hr_evaluation_pay_basis WHERE engagement_id=?),0)=?", [engagementId, input.expectedVersion]),
    db.prepare("INSERT INTO hr_evaluation_pay_basis(engagement_id,company_id,version,effective_from,payload_json,confirmed_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(engagementId, actor.companyId, input.expectedVersion + 1, input.effectiveFrom, JSON.stringify(input.basis), actor.userId, Date.now()),
  ]);
}
export async function createEvaluationCompensationDraft(actor: WorkContext, assessmentId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: version, effectiveFrom: evaluationDate, amountMinor: z.number().int().positive().max(Number.MAX_SAFE_INTEGER), reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage"); requirePermission(actor, "people.compensation.read");
  const id = `evaluation-pay-${key}`, compId = `evaluation-comp-${key}`, op = "people.evaluation.compensation.create", prior = await replayCommand(db, actor, key, op, assessmentId, input); if (prior) return prior;
  const { row, employee } = await evaluationAssessment(db, actor, assessmentId), cycle = await frozenPolicy(db, actor, row.cycle_id);
  if (row.status !== "confirmed" || row.version !== input.expectedVersion) throw new ApiError(409, "CONFIRMED_ASSESSMENT_REQUIRED", "Use the current independently confirmed assessment.");
  if (!cycle.policyData.confirmed || cycle.policyData.maxRaisePercent <= 0) throw new ApiError(422, "PAY_POLICY_REQUIRED", "HR must confirm a policy that permits salary drafts.");
  const today = await evaluationToday(db, employee.id);
  const e = await db.prepare("SELECT pay_group_id,status FROM hr_engagements WHERE id=? AND company_id=?").bind(employee.id, actor.companyId).first<{ pay_group_id: string | null; status: string }>();
  if (e?.status !== "active" || !e.pay_group_id) throw new ApiError(422, "PAY_GROUP_REQUIRED", "An active engagement and pay group are required.");
  const lastRun = await db.prepare("SELECT period_end FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND status IN ('approved','posted') ORDER BY period_end DESC LIMIT 1").bind(actor.companyId, e.pay_group_id).first<{ period_end: string }>();
  if (!lastRun) throw new ApiError(422, "NEXT_PAY_PERIOD_REQUIRED", "Confirm the next payroll period after an existing payroll run first.");
  const nextDate = new Date(`${lastRun.period_end}T00:00:00Z`); nextDate.setUTCDate(nextDate.getUTCDate() + 1);
  if (input.effectiveFrom !== nextDate.toISOString().slice(0, 10) || input.effectiveFrom <= today) throw new ApiError(422, "NEXT_PAY_PERIOD_REQUIRED", "New pay must start at the next future payroll period boundary.");
  const wages = (await db.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)").bind(employee.id, input.effectiveFrom, input.effectiveFrom).all<Compensation & { components_json: string }>()).results;
  const previous = wages.length === 1 ? wages[0] : null;
  const basis = await db.prepare("SELECT version,payload_json FROM hr_evaluation_pay_basis WHERE engagement_id=? AND company_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1").bind(employee.id, actor.companyId, input.effectiveFrom).first<{ version: number; payload_json: string }>();
  if (!previous || !basis || previous.effective_from >= input.effectiveFrom) throw new ApiError(422, "PAY_BASELINE_REQUIRED", "Review one approved wage and an HR-confirmed comparison basis.");
  const b = basisSchema.parse(JSON.parse(basis.payload_json));
  if (monthlyFtePay(previous, b).value === null) throw new ApiError(422, "PAY_BASIS_MISMATCH", "The confirmed basis must match the approved wage frequency and hours.");
  const field = b.type === "monthly" ? "base_amount_minor" : b.type === "daily" ? "daily_rate_minor" : "hourly_rate_minor", oldAmount = Number(previous[field]);
  if (oldAmount <= 0 || input.amountMinor < oldAmount || input.amountMinor > oldAmount * (1 + cycle.policyData.maxRaisePercent / 100)) throw new ApiError(422, "PAY_POLICY_LIMIT", "Proposed pay must be within the confirmed Company increase limit.");
  const amounts = { base: previous.base_amount_minor, hourly_rate: previous.hourly_rate_minor, daily_rate: previous.daily_rate_minor }; amounts[b.type === "monthly" ? "base" : b.type === "daily" ? "daily_rate" : "hourly_rate"] = input.amountMinor;
  const components = JSON.parse(previous.components_json) as { code: string; amount: string }[];
  const snapshotAmounts = [...Object.entries(amounts).map(([amountKind, amount]) => ({ amountKind, amount })), ...components.map(c => ({ amountKind: `component:${c.code}`, amount: decimalToMinor(c.amount) }))];
  const money = await Promise.all(snapshotAmounts.map(({amountKind, amount}) => snapshotMoney(db, actor, { sourceType: "hr_compensation_revision", sourceId: compId, amountKind, transactionDate: input.effectiveFrom, currency: employee.currency, currencyScale: 2, amountUnits: String(amount) })));
  return executeCommand(db, actor, key, op, input, assessmentId, 1, { id, compensationId: compId, status: "draft", version: 1 }, [
    db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id),
    assertion(db, key, "assessment-confirmed", "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status='confirmed')", [assessmentId, input.expectedVersion]),
    assertion(db, key, "wage-version", "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='approved')", [previous.id, Number(previous.version)]),
    assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", [employee.id, employee.version]),
    db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(compId, employee.id, input.effectiveFrom, previous.effective_to, previous.pay_frequency, amounts.base, amounts.hourly_rate, amounts.daily_rate, previous.standard_daily_hours, previous.expected_hours, employee.currency, previous.components_json, actor.userId, Date.now(), Date.now()),
    ...money.map(m => m.statement),
    db.prepare("INSERT INTO hr_evaluation_compensation_drafts(id,company_id,assessment_id,assessment_version,compensation_id,previous_id,previous_version,engagement_version,policy_id,policy_version,basis_version,reason,created_by,created_at,updated_at,previous_json,proposed_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, assessmentId, input.expectedVersion, compId, previous.id, previous.version, employee.version, cycle.policy.id, cycle.policy.version, basis.version, input.reason, actor.userId, Date.now(), Date.now(), JSON.stringify(previous), JSON.stringify({...previous, id:compId, status:"draft", approved_by:null, created_by:actor.userId, created_at:Date.now(), updated_at:Date.now(), effective_from:input.effectiveFrom, base_amount_minor:amounts.base, hourly_rate_minor:amounts.hourly_rate, daily_rate_minor:amounts.daily_rate, version:1})),
  ]);
}
export async function approveEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: version, reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.compensation.approve"); requirePermission(actor, "people.compensation.read");
  const op = "people.evaluation.compensation.approve", prior = await replayCommand(db, actor, key, op, draftId, input); if (prior) return prior;
  const draft = await db.prepare("SELECT d.*,c.engagement_id,c.effective_from,c.version compensation_version FROM hr_evaluation_compensation_drafts d JOIN hr_compensation_revisions c ON c.id=d.compensation_id WHERE d.id=? AND d.company_id=?").bind(draftId, actor.companyId).first<Row>();
  if (!draft) throw new ApiError(404, "PAY_DRAFT_NOT_FOUND", "Compensation draft not found.");
  const { row, employee } = await evaluationAssessment(db, actor, String(draft.assessment_id));
  if (await db.prepare("SELECT iam_same_person(?,?,?) same").bind(actor.userId, draft.created_by, actor.companyId).first<boolean>("same")) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the pay change.");
  const effective = String(draft.effective_from), end = new Date(`${effective}T00:00:00Z`); end.setUTCDate(end.getUTCDate() - 1);
  if (effective <= await evaluationToday(db, employee.id)) throw new ApiError(409, "PAY_EFFECTIVE_DATE_PASSED", "Review a future period before approving.");
  return executeCommand(db, actor, key, op, input, draftId, input.expectedVersion + 1, { id: draftId, status: "approved", version: input.expectedVersion + 1 }, [
    db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id),
    db.prepare("SELECT id FROM hr_compensation_revisions WHERE engagement_id=? FOR UPDATE").bind(employee.id),
    assertion(db, key, "pay-draft-version", "EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE id=? AND version=? AND status='draft')", [draftId, input.expectedVersion]),
    assertion(db, key, "assessment-version", "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status='confirmed')", [row.id, Number(draft.assessment_version)]),
    assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", [employee.id, Number(draft.engagement_version)]),
    assertion(db, key, "basis-version", "(SELECT version FROM hr_evaluation_pay_basis WHERE engagement_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1)=?", [employee.id, effective, Number(draft.basis_version)]),
    assertion(db, key, "policy-version", "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND version=? AND payload_json::jsonb->>'confirmed'='true')", [String(draft.policy_id), actor.companyId, Number(draft.policy_version)]),
    assertion(db, key, "previous-version", "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='approved' AND effective_from<? AND (effective_to IS NULL OR effective_to>=?))", [String(draft.previous_id), Number(draft.previous_version), effective, effective]),
    assertion(db, key, "new-wage-version", "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='draft')", [String(draft.compensation_id), 1]),
    assertion(db, key, "proposed-pay-snapshot", "EXISTS(SELECT 1 FROM hr_compensation_revisions c JOIN hr_evaluation_compensation_drafts d ON d.compensation_id=c.id WHERE d.id=? AND c.base_amount_minor=(d.proposed_json::jsonb->>'base_amount_minor')::bigint AND c.hourly_rate_minor=(d.proposed_json::jsonb->>'hourly_rate_minor')::bigint AND c.daily_rate_minor=(d.proposed_json::jsonb->>'daily_rate_minor')::bigint AND c.standard_daily_hours=(d.proposed_json::jsonb->>'standard_daily_hours')::numeric AND c.expected_hours=(d.proposed_json::jsonb->>'expected_hours')::numeric AND c.effective_to IS NOT DISTINCT FROM (d.proposed_json::jsonb->>'effective_to') AND c.components_json=d.proposed_json::jsonb->>'components_json' AND c.currency=d.proposed_json::jsonb->>'currency' AND c.pay_frequency=d.proposed_json::jsonb->>'pay_frequency' AND c.effective_from=d.proposed_json::jsonb->>'effective_from')", [draftId]),
    assertion(db, key, "no-published-payroll", "NOT EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE i.engagement_id=? AND r.period_end>=? AND r.status IN ('approved','posted'))", [employee.id, effective]),
    assertion(db, key, "no-other-wage", "NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id NOT IN (?,?) AND status='approved' AND (effective_to IS NULL OR effective_to>=?))", [employee.id, String(draft.previous_id), String(draft.compensation_id), effective]),
    db.prepare("UPDATE hr_compensation_revisions SET effective_to=?,version=version+1,updated_at=? WHERE id=?").bind(end.toISOString().slice(0, 10), Date.now(), draft.previous_id),
    db.prepare("UPDATE hr_compensation_revisions SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId, Date.now(), draft.compensation_id),
    db.prepare("UPDATE hr_evaluation_compensation_drafts SET status='approved',approved_by=?,review_reason=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId, input.reason, Date.now(), draftId),
    db.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), key, draftId, op, actor.userId, actor.assignmentId, input.expectedVersion, JSON.stringify(input), Date.now()),
  ]);
}
export async function cancelEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) {
  const input = z.object({ expectedVersion: version, reason }).strict().parse(raw);
  const { write: db } = await holdingResources(actor.holdingId); actor = await evaluationAuthority(db, actor, "people.manage"); requirePermission(actor, "people.compensation.read");
  const op = "people.evaluation.compensation.cancel", prior = await replayCommand(db, actor, key, op, draftId, input); if (prior) return prior;
  const draft = await db.prepare("SELECT * FROM hr_evaluation_compensation_drafts WHERE id=? AND company_id=?").bind(draftId, actor.companyId).first<Row>();
  if (!draft) throw new ApiError(404, "PAY_DRAFT_NOT_FOUND", "Compensation draft not found.");
  const { employee } = await evaluationAssessment(db, actor, String(draft.assessment_id));
  return executeCommand(db, actor, key, op, input, draftId, input.expectedVersion + 1, { id: draftId, status: "cancelled", version: input.expectedVersion + 1 }, [
    db.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(employee.id),
    assertion(db, key, "engagement-version", "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [employee.id, employee.version]),
    db.prepare("SELECT id FROM hr_evaluation_compensation_drafts WHERE id=? FOR UPDATE").bind(draftId),
    assertion(db, key, "pay-draft-version", "EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE id=? AND version=? AND status='draft')", [draftId, input.expectedVersion]),
    assertion(db, key, "unapproved-pay", "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND status='draft')", [String(draft.compensation_id)]),
    db.prepare("UPDATE hr_compensation_revisions SET status='superseded',version=version+1,updated_at=? WHERE id=?").bind(Date.now(), draft.compensation_id),
    db.prepare("UPDATE hr_evaluation_compensation_drafts SET status='cancelled',review_reason=?,version=version+1,updated_at=? WHERE id=?").bind(input.reason, Date.now(), draftId),
  ]);
}
