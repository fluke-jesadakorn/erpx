import { beforeAll, expect, it, vi } from "vitest";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { defaultEvaluationPolicy, emptyAssessment, evaluationMetrics, compensationMetrics, monthlyFtePay, type AssessmentContent, type Compensation, type RoleProfile } from "../src/lib/people-evaluation";
import { addEvaluationEvidence, approveEvaluationCompensation, cancelEvaluationCompensation, confirmEvaluationPayBasis, createEvaluationAssessment, createEvaluationCompensationDraft, evaluationDetail, evaluationWorkspace, importEvaluationMarket, readEvaluationEvidence, saveEvaluationAssessment, saveEvaluationDefinition, transitionEvaluationAssessment } from "../src/server/people-evaluation";
import { parseEvaluationMarketCsv } from "../src/lib/evaluation-market-import";
import { queueEvaluationAi, runEvaluationAi, evaluationAiStatus } from "../src/server/people-evaluation-ai";
import { transitionCompensation } from "../src/server/people-payroll";
import { activateAiConnection, aiConnectionSchema, aiSelectionSchema, saveAiConnection, selectAiProviders, testAiConnection } from "../src/server/ai-providers";
import { authorizeAttachmentTarget, attachmentPolicy } from "../src/server/attachments/policy";
import { applyMigrations, env } from "./runtime-env";
const db = env.H001_DB, key = () => crypto.randomUUID();
const aiEnv = { AI_CREDENTIAL_KEY_ID: "v1", AI_CREDENTIAL_KEYS: JSON.stringify({ v1: btoa(String.fromCharCode(...new Uint8Array(32).fill(23))) }) };
vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => aiEnv }));
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
const role: RoleProfile = { name: "Engineer", positionId: "position", levelId: "level", family: "Engineering", location: "Bangkok", requirements: [1, 2, 3, 4, 5].map(i => ({ skill: { id: `skill-${i}`, version: 1 }, weight: 1, level: 3, critical: i === 1 })), difficulty: 3, autonomy: 3, impact: 3, responsibility: 3, description: "Build reliable applications", confirmed: true };
const content = (levels: number[], score = 4): AssessmentContent => ({ ...structuredClone(emptyAssessment), skills: levels.map((level, i) => ({ skillId: `skill-${i + 1}`, skillVersion: 1, level, evidenceIds: ["evidence"], rationale: "Observed behavior" })), performance: { goals: [{ goal: "Deliver work", target: "Meets quality criteria", outcome: "Delivered on time" }], results: { score, evidenceIds: ["evidence"], rationale: "Delivered expected results" }, quality: { score, evidenceIds: ["evidence"], rationale: "Reviewed quality" }, collaboration: { score, evidenceIds: ["evidence"], rationale: "Team review" } } });
const pay: Compensation = { id: "pay", version: 1, currency: "THB", pay_frequency: "monthly", base_amount_minor: 1500000, hourly_rate_minor: 0, daily_rate_minor: 0, expected_hours: 88, standard_daily_hours: 8, effective_from: "2026-01-01", effective_to: null };
const basis = { type: "monthly" as const, fte: 0.5, fullTimeMonthlyHours: 176, fullTimeMonthlyDays: 22, reason: "Confirmed employment basis" };
const band = { name: "Engineering", family: "Engineering", levelId: "level", location: "Bangkok", currency: "THB", basis: "monthly_fte" as const, minimumMinor: 2500000, midpointMinor: 4000000, maximumMinor: 5000000, effectiveFrom: "2026-01-01", effectiveTo: "2026-12-31", benchmark: null, confirmed: true };
it("separates high performance/low pay, high skills/low performance, gaps and target readiness", () => {
  const high = evaluationMetrics(role, null, content([4, 4, 4, 4, 4]), defaultEvaluationPolicy);
  expect(high.outperform).toBe(true); expect(high.fit).toBe(100); expect(high.targetReady).toBe(false);
  expect(compensationMetrics(pay, basis, band, null, "2026-10-05", defaultEvaluationPolicy).compaRatio).toBe(75);
  expect(evaluationMetrics(role, null, content([5, 5, 5, 5, 5], 2), defaultEvaluationPolicy)).toMatchObject({ fit: 100, performance: 2, outperform: false });
  expect(evaluationMetrics(role, null, content([2, 4, 4, 4, 4]), defaultEvaluationPolicy)).toMatchObject({ fit: 80, coverage: 100, conclusion: "confirmed_gap", criticalGaps: ["skill-1"] });
  expect(evaluationMetrics(role, null, content([4, 4]), defaultEvaluationPolicy)).toMatchObject({ coverage: 40, conclusion: "insufficient_evidence" });
  const unknownCritical = content([4, 4, 4, 4, 4]); unknownCritical.skills.shift();
  expect(evaluationMetrics(role, null, unknownCritical, defaultEvaluationPolicy).conclusion).toBe("insufficient_evidence");
  expect(evaluationMetrics(role, role, { ...content([5, 5, 5, 5, 5]), targetConfirmed: true }, defaultEvaluationPolicy).targetReady).toBe(true);
  const revisedTarget = { ...role, requirements: role.requirements.map(r => ({ ...r, skill: { ...r.skill, version: 2 } })) };
  expect(evaluationMetrics(role, revisedTarget, { ...content([5, 5, 5, 5, 5]), targetConfirmed: true }, defaultEvaluationPolicy)).toMatchObject({ fit: 100, targetReady: false, targetFit: { conclusion: "insufficient_evidence" } });
  expect(evaluationMetrics(role, null, emptyAssessment, defaultEvaluationPolicy).performance).toBeNull();
  expect(evaluationMetrics(role, null, emptyAssessment, defaultEvaluationPolicy).fit).toBeNull();
});
it("normalizes monthly, daily, hourly and part-time pay without inventing a basis or mixing currency", () => {
  expect(monthlyFtePay(pay, basis).value).toBe(3000000);
  expect(monthlyFtePay({ ...pay, hourly_rate_minor: 20000 }, { ...basis, type: "hourly" }).value).toBe(3520000);
  expect(monthlyFtePay({ ...pay, daily_rate_minor: 100000 }, { ...basis, type: "daily" }).value).toBe(2200000);
  expect(monthlyFtePay({ ...pay, daily_rate_minor: 100000, standard_daily_hours: 6 }, { ...basis, type: "daily" }).value).toBeNull();
  expect(monthlyFtePay(pay, null).value).toBeNull();
  expect(monthlyFtePay({ ...pay, pay_frequency: "weekly" }, basis).value).toBeNull();
  expect(compensationMetrics(pay, basis, { ...band, currency: "USD" }, null, "2026-10-05", defaultEvaluationPolicy).compaRatio).toBeNull();
  expect(compensationMetrics(pay, basis, null, null, "2026-10-05", defaultEvaluationPolicy).compaRatio).toBeNull();
  const market = { ...band, source: "Imported salary survey", observedOn: "2024-01-01", validUntil: "2025-01-01" };
  expect(compensationMetrics(pay, basis, band, market, "2026-10-05", defaultEvaluationPolicy).marketStatus).toBe("expired");
});
async function setup(aiEnabled = false) {
  const f = await seedExpenseCompany(db, env.H001_FILES), positionId = `${f.company}-position`, engagementId = `eng-${f.claimant.userId}`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) VALUES(?,?,?,?,'Engineer',?,'fixture',1,1)").bind(positionId, f.company, f.branch, f.department, `${f.company}-level-1`).run();
  await db.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?").bind(positionId, engagementId).run();
  const save = (kind: string, data: unknown) => saveEvaluationDefinition(f.owner, { kind, expectedVersion: 0, data, reason: "Company reviewed fixture criteria" }, key());
  const skill = await save("skill", { name: "Problem solving", category: "professional", behaviors: ["Needs close support", "Solves routine tasks", "Works independently", "Solves complex work", "Guides the organization"], confirmed: true });
  const policy = await save("policy", { ...defaultEvaluationPolicy, maxRaisePercent: 20, compensationRule: "Allow up to twenty percent after independent evidence review", confirmed: true, aiEnabled });
  const profile = await save("role", { ...role, positionId, levelId: `${f.company}-level-1`, requirements: [{ skill: { id: skill.id, version: 1 }, level: 3, weight: 1, critical: true }] });
  const cycle = await save("cycle", { name: "Annual 2026", year: 2026, policy: { id: policy.id, version: 1 }, status: "open", confirmed: true });
  const reviewerGrant = await f.grant(f.reviewer, "holding_owner");
  const reviewer = { ...f.reviewer, assignmentId: reviewerGrant, role: "holding_owner" };
  const input = { engagementId, cycleId: cycle.id, role: { id: profile.id, version: 1 }, periodStart: "2026-01-01", periodEnd: "2026-12-31", reason: "Confirmed current role assignment" };
  const assessment = await createEvaluationAssessment(f.owner, input, key());
  return { f, reviewer, skill, policy, profile, cycle, input, assessment, engagementId };
}
async function rated(s: Awaited<ReturnType<typeof setup>>) {
  const evidence = await addEvaluationEvidence(s.f.owner, String(s.assessment.id), { expectedVersion: 1, sourceType: "kpi", description: "Delivered the reviewed quality milestone", observedOn: "2026-06-01" }, key());
  const c = content([4]); c.skills[0].skillId = String(s.skill.id); c.skills[0].evidenceIds = [String(evidence.id)];
  for (const k of ["results", "quality", "collaboration"] as const) c.performance[k]!.evidenceIds = [String(evidence.id)];
  await saveEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 2, content: c, reason: "Confirmed evidence supports each score" }, key());
  return { evidence, c };
}
async function confirm(s: Awaited<ReturnType<typeof setup>>) {
  await rated(s);
  await transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 3, action: "submit", reason: "Ready for independent evaluation" }, key());
  await transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 4, action: "calibrate", reason: "Calibrated against role expectations" }, key());
  await transitionEvaluationAssessment(s.reviewer, String(s.assessment.id), { expectedVersion: 5, action: "confirm", reason: "Independently reviewed the evidence" }, key());
}
it("retains independent workflow/history, rejects unowned citations and protects frozen criteria", async () => {
  const s = await setup(), { c } = await rated(s);
  const invalid = structuredClone(c); invalid.skills[0].evidenceIds = ["other-person-evidence"];
  await expect(saveEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 3, content: invalid, reason: "Invalid evidence reference" }, key())).rejects.toMatchObject({ code: "EVIDENCE_REFERENCE_INVALID" });
  await transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 3, action: "submit", reason: "Submit review" }, key());
  await transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 4, action: "calibrate", reason: "Review calibration" }, key());
  await expect(transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 5, action: "confirm", reason: "Self confirmation" }, key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await transitionEvaluationAssessment(s.reviewer, String(s.assessment.id), { expectedVersion: 5, action: "confirm", reason: "Independent confirmation" }, key());
  expect((await evaluationDetail(s.f.owner, String(s.assessment.id))).history).toHaveLength(6);
  await expect(db.prepare("UPDATE hr_evaluation_evidence SET description='rewrite' WHERE assessment_id=?").bind(s.assessment.id).run()).rejects.toThrow("EVALUATION_EVIDENCE_IMMUTABLE");
  await expect(saveEvaluationDefinition(s.f.owner, { id: s.cycle.id, kind: "cycle", expectedVersion: 1, data: { name: "Changed cycle", year: 2026, policy: { id: s.policy.id, version: 1 }, status: "open", confirmed: true }, reason: "Change open cycle" }, key())).rejects.toMatchObject({ code: "CYCLE_FROZEN" });
  await transitionEvaluationAssessment(s.reviewer, String(s.assessment.id), { expectedVersion: 6, action: "reopen", reason: "Independent reviewer requests further evidence" }, key());
  expect((await evaluationDetail(s.f.owner, String(s.assessment.id))).history[0].actor_id).toBe(s.reviewer.userId);
});
it("rejects overlapping role periods, stale edits, duplicate commands and cross-company evidence", async () => {
  const s = await setup();
  await expect(createEvaluationAssessment(s.f.owner, s.input, key())).rejects.toMatchObject({ status: 409 });
  const evidenceKey = key(), input = { expectedVersion: 1, sourceType: "kpi", description: "Recorded measurable result", observedOn: "2026-03-01" };
  const first = await addEvaluationEvidence(s.f.owner, String(s.assessment.id), input, evidenceKey);
  expect(await addEvaluationEvidence(s.f.owner, String(s.assessment.id), input, evidenceKey)).toEqual(first);
  await expect(addEvaluationEvidence(s.f.owner, String(s.assessment.id), input, key())).rejects.toMatchObject({ status: 409 });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(readEvaluationEvidence(other.owner, String(first.id))).rejects.toMatchObject({ status: 404 });
  await expect(evaluationWorkspace(s.f.owner, { companyId: other.company })).rejects.toThrow();
});
it("keeps assessment behaviors and evidence pinned when Company skill and role criteria are revised", async () => {
  const s = await setup(); await rated(s);
  await saveEvaluationDefinition(s.f.owner, { id: s.skill.id, kind: "skill", expectedVersion: 1, data: { name: "Problem solving revised", category: "professional", behaviors: ["Revised novice behavior", "Revised routine behavior", "Revised independent behavior", "Revised complex behavior", "Revised leadership behavior"], confirmed: true }, reason: "Company revises observable criteria" }, key());
  await saveEvaluationDefinition(s.f.owner, { id: s.profile.id, kind: "role", expectedVersion: 1, data: { ...role, positionId: `${s.f.company}-position`, levelId: `${s.f.company}-level-1`, requirements: [{ skill: { id: s.skill.id, version: 2 }, level: 3, weight: 1, critical: true }] }, reason: "Company revises future profile" }, key());
  const workspace = await evaluationWorkspace(s.f.owner, { year: 2026, engagementId: s.engagementId });
  expect(workspace.rows[0]).toMatchObject({ roleRef: { version: 1 }, metrics: { fit: 100 } });
  expect(workspace.skillVersions.find(d => d.version === 1)?.data.behaviors).toContain("Works independently");
  const changedRating = structuredClone(workspace.rows[0].assessment!.content); changedRating.skills[0].skillVersion = 2;
  await expect(saveEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 3, content: changedRating, reason: "Attempt to substitute revised criteria" }, key())).rejects.toMatchObject({ code: "SKILL_NOT_IN_ROLE" });
});
it("redacts salary and inferred salary fields, limits managers to reporting teams, and rejects revoked access", async () => {
  const s = await setup(); await rated(s);
  await confirmEvaluationPayBasis(s.f.owner, s.engagementId, { expectedVersion: 0, effectiveFrom: "2026-01-01", basis }, key());
  const manager = await evaluationWorkspace(s.f.head, { year: 2026 });
  expect(manager.rows.map(r => r.employee.id)).toEqual([s.engagementId]);
  const json = JSON.stringify(manager);
  for (const field of ["base_amount_minor", "compaRatio", "monthlyFteMinor", "maxRaisePercent", "compensationRule"]) expect(json).not.toContain(field);
  expect((await evaluationDetail(s.f.head, String(s.assessment.id))).salary).toBeUndefined();
  await expect(evaluationDetail(s.f.tie, String(s.assessment.id))).rejects.toMatchObject({ status: 404 });
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), s.f.head.assignmentId).run();
  await expect(evaluationWorkspace(s.f.head, {})).rejects.toMatchObject({ status: 403 });
});
async function payroll(s: Awaited<ReturnType<typeof setup>>) {
  const group = `${s.f.company}-paygroup`, wage = `${s.f.company}-wage`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Monthly','TH','THB','monthly','Asia/Bangkok','fixture',1,1)").bind(group, s.f.company).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=? WHERE id=?").bind(group, s.engagementId).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,currency,status,created_by,created_at,updated_at) VALUES(?,?,'2026-01-01','monthly',3000000,'THB','approved','fixture',1,1)").bind(wage, s.engagementId).run();
  await db.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,'2026-10-01','2026-10-31','2026-10-31','THB','approved','fixture',1,1)").bind(`${group}-run`, s.f.company, group).run();
  await confirmEvaluationPayBasis(s.f.owner, s.engagementId, { expectedVersion: 0, effectiveFrom: "2026-01-01", basis: { ...basis, fte: 1 } }, key());
  return { group, wage };
}
it("creates one draft, forbids self/generic approval, and atomically replaces the next-period wage", async () => {
  const s = await setup(); await confirm(s); const p = await payroll(s);
  const input = { expectedVersion: 6, effectiveFrom: "2026-11-01", amountMinor: 3300000, reason: "Policy-backed pay review for demonstrated outcomes" }, draftKey = key();
  const draft = await createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), input, draftKey);
  expect(await createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), input, draftKey)).toEqual(draft);
  await expect(createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), input, key())).rejects.toMatchObject({ status: 409 });
  await expect(approveEvaluationCompensation(s.f.owner, String(draft.id), { expectedVersion: 1, reason: "Self approval" }, key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await expect(transitionCompensation(s.reviewer, String(draft.compensationId), { expectedVersion: 1, status: "approved" }, key())).rejects.toMatchObject({ code: "EVALUATION_COMPENSATION_WORKFLOW_REQUIRED" });
  const approvalKey = key(), approval = { expectedVersion: 1, reason: "Independently accepted the next-period pay change" };
  await approveEvaluationCompensation(s.reviewer, String(draft.id), approval, approvalKey);
  expect((await db.prepare("SELECT effective_to,status FROM hr_compensation_revisions WHERE id=?").bind(p.wage).first())).toEqual({ effective_to: "2026-10-31", status: "approved" });
  expect(await db.prepare("SELECT status FROM hr_compensation_revisions WHERE id=?").bind(draft.compensationId).first("status")).toBe("approved");
  expect(await db.prepare("SELECT reason,review_reason FROM hr_evaluation_compensation_drafts WHERE id=?").bind(draft.id).first()).toEqual({ reason: input.reason, review_reason: approval.reason });
  expect(await approveEvaluationCompensation(s.reviewer, String(draft.id), approval, approvalKey)).toMatchObject({ status: "approved" });
});
it("rejects an altered wage or a date inside the payroll period while preserving original pay", async () => {
  const s = await setup(); await confirm(s); const p = await payroll(s);
  const input = { expectedVersion: 6, effectiveFrom: "2026-11-01", amountMinor: 3300000, reason: "Reviewed salary recommendation" };
  await expect(createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), { ...input, effectiveFrom: "2026-11-15" }, key())).rejects.toMatchObject({ code: "NEXT_PAY_PERIOD_REQUIRED" });
  const draft = await createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), input, key());
  await db.prepare("UPDATE hr_compensation_revisions SET version=version+1,base_amount_minor=3100000 WHERE id=?").bind(p.wage).run();
  await expect(approveEvaluationCompensation(s.reviewer, String(draft.id), { expectedVersion: 1, reason: "Independent review" }, key())).rejects.toMatchObject({ status: 409 });
  expect(await db.prepare("SELECT effective_to FROM hr_compensation_revisions WHERE id=?").bind(p.wage).first("effective_to")).toBeNull();
  expect(await db.prepare("SELECT status FROM hr_compensation_revisions WHERE id=?").bind(draft.compensationId).first("status")).toBe("draft");
  await cancelEvaluationCompensation(s.f.owner, String(draft.id), { expectedVersion: 1, reason: "Withdraw stale proposal for renewed human review" }, key());
  await transitionEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 6, action: "reopen", reason: "Review changed compensation baseline" }, key());
  expect(await db.prepare("SELECT status FROM hr_evaluation_compensation_drafts WHERE id=?").bind(draft.id).first("status")).toBe("cancelled");
  expect(await db.prepare("SELECT base_amount_minor FROM hr_compensation_revisions WHERE id=?").bind(p.wage).first("base_amount_minor")).toBe(3100000);
});
it("applies the same scope to protected evaluation files", async () => {
  const s = await setup(), policy = attachmentPolicy("hr_evaluation_assessment", "evaluation_evidence", "record");
  expect(await authorizeAttachmentTarget(db, s.f.owner, policy, String(s.assessment.id), "read")).toMatchObject({ companyId: s.f.company });
  await expect(authorizeAttachmentTarget(db, s.f.head, policy, String(s.assessment.id), "read")).rejects.toMatchObject({ status: 404 });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(authorizeAttachmentTarget(db, other.owner, policy, String(s.assessment.id), "read")).rejects.toMatchObject({ status: 404 });
});
it("imports quoted market CSV atomically for HR confirmation, with strict ranges and Company levels", async () => {
  const s = await setup(), header = "name,family,levelId,location,currency,minimum,midpoint,maximum,source,observedOn,validUntil";
  const rows = parseEvaluationMarketCsv(`${header}\nEngineering survey,Engineering,${s.f.company}-level-1,Bangkok,THB,25000,40000.50,50000,"Survey, company-owned source",2026-01-01,2026-12-31`);
  expect(rows[0]).toMatchObject({ midpointMinor: 4000050, source: "Survey, company-owned source", confirmed: false });
  expect(() => parseEvaluationMarketCsv(`${header}\nBad range,Engineering,level,Bangkok,THB,50000,40000,20000,Survey,2026-01-01,2026-12-31`)).toThrow();
  expect(() => parseEvaluationMarketCsv(`${header}\n"Unclosed cell`)).toThrow();
  const input = { rows: [...rows, { ...rows[0], levelId: "foreign-company-level" }], reason: "Reviewed imported survey source" };
  await expect(importEvaluationMarket(s.f.owner, input, key())).rejects.toMatchObject({ code: "IMPORT_LEVEL_UNAVAILABLE" });
  expect(await db.prepare("SELECT count(*) n FROM hr_evaluation_definitions WHERE company_id=? AND kind='benchmark'").bind(s.f.company).first("n")).toBe(0);
  await expect(importEvaluationMarket(s.f.head, { ...input, rows }, key())).rejects.toMatchObject({ status: 403 });
  const importKey = key(), imported = await importEvaluationMarket(s.f.owner, { ...input, rows }, importKey);
  expect(await importEvaluationMarket(s.f.owner, { ...input, rows }, importKey)).toEqual(imported);
  expect((await evaluationWorkspace(s.f.owner, {})).definitions.find(d => d.kind === "benchmark")?.data.confirmed).toBe(false);
});
it("splits role changes into dated periods while retaining original periods and frozen criteria", async () => {
  const s = await setup(), nextPosition = `${s.f.company}-next-position`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) VALUES(?,?,?,?,'Senior Engineer',?,'fixture',1,1)").bind(nextPosition, s.f.company, s.f.branch, s.f.department, `${s.f.company}-level-1`).run();
  const nextRole = await saveEvaluationDefinition(s.f.owner, { kind: "role", expectedVersion: 0, data: { ...role, name: "Senior Engineer", positionId: nextPosition, levelId: `${s.f.company}-level-1`, requirements: [{ skill: { id: s.skill.id, version: 1 }, level: 4, weight: 1, critical: true }] }, reason: "Confirmed target role requirements" }, key());
  const change = { positionId: nextPosition, levelId: `${s.f.company}-level-1`, effectiveAt: Date.parse("2026-07-01T00:00:00+07:00") };
  await db.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,'fixture','Role change',?,?)").bind(key(), s.f.company, s.engagementId, JSON.stringify({ positionId: s.input.role.id ? `${s.f.company}-position` : null, levelId: change.levelId }), JSON.stringify(change), key(), change.effectiveAt).run();
  await db.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE id=?").bind(nextPosition, s.engagementId).run();
  await expect(saveEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 1, content: emptyAssessment, reason: "Invalid unsplit period" }, key())).rejects.toMatchObject({ code: "ROLE_PERIOD_SPLIT_REQUIRED" });
  await saveEvaluationAssessment(s.f.owner, String(s.assessment.id), { expectedVersion: 1, content: emptyAssessment, periodEnd: "2026-06-30", reason: "Separate work in the former role" }, key());
  const history = (await evaluationDetail(s.f.owner, String(s.assessment.id))).history;
  expect(history.map(h => h.period_end)).toEqual(["2026-06-30", "2026-12-31"]);
  await expect(createEvaluationAssessment(s.f.owner, { ...s.input, periodStart: "2026-07-01" }, key())).rejects.toMatchObject({ code: "ROLE_ASSIGNMENT_UNVERIFIED" });
  await createEvaluationAssessment(s.f.owner, { ...s.input, periodStart: "2026-07-01", role: { id: nextRole.id, version: 1 } }, key());
  expect((await evaluationWorkspace(s.f.owner, { year: 2026, engagementId: s.engagementId })).rows).toHaveLength(2);
});
it("rejects changed proposed hours or components and retains the approved wage and copied money snapshots", async () => {
  const s = await setup(); await confirm(s); const p = await payroll(s);
  await db.prepare("UPDATE hr_compensation_revisions SET components_json=? WHERE id=?").bind(JSON.stringify([{ code: "ALLOWANCE", kind: "earning", amount: "1000.00" }]), p.wage).run();
  const draft = await createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), { expectedVersion: 6, effectiveFrom: "2026-11-01", amountMinor: 3300000, reason: "Reviewed policy-backed recommendation" }, key());
  expect(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE source_type='hr_compensation_revision' AND source_id=?").bind(draft.compensationId).first("n")).toBe(4);
  await db.prepare("UPDATE hr_compensation_revisions SET expected_hours=80 WHERE id=?").bind(draft.compensationId).run();
  await expect(approveEvaluationCompensation(s.reviewer, String(draft.id), { expectedVersion: 1, reason: "Review altered hours" }, key())).rejects.toMatchObject({ status: 409 });
  expect(await db.prepare("SELECT effective_to FROM hr_compensation_revisions WHERE id=?").bind(p.wage).first("effective_to")).toBeNull();
});
it("preserves issued payroll and refuses a pending draft after its proposed period has been issued", async () => {
  const s = await setup(); await confirm(s); const p = await payroll(s);
  const draft = await createEvaluationCompensationDraft(s.f.owner, String(s.assessment.id), { expectedVersion: 6, effectiveFrom: "2026-11-01", amountMinor: 3300000, reason: "Reviewed next-period salary" }, key());
  const run = `${p.group}-issued-next`, item = `${run}-item`;
  await db.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,'2026-11-01','2026-11-30','2026-11-30','THB','approved','fixture',1,1)").bind(run, s.f.company, p.group).run();
  await db.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,net_minor,currency,status,created_at) VALUES(?,?,?,'employee',?,?,3000000,3000000,'THB','approved',1)").bind(item, run, s.engagementId, JSON.stringify({ compensationId: p.wage, amount: 3000000 }), JSON.stringify({ net: 3000000 })).run();
  const before = await db.prepare("SELECT * FROM hr_payroll_items WHERE id=?").bind(item).first();
  await expect(approveEvaluationCompensation(s.reviewer, String(draft.id), { expectedVersion: 1, reason: "Cannot amend already-issued period" }, key())).rejects.toMatchObject({ status: 409 });
  expect(await db.prepare("SELECT * FROM hr_payroll_items WHERE id=?").bind(item).first()).toEqual(before);
  expect(await db.prepare("SELECT effective_to FROM hr_compensation_revisions WHERE id=?").bind(p.wage).first("effective_to")).toBeNull();
});
async function aiProvider(s: Awaited<ReturnType<typeof setup>>) {
  const c = await saveAiConnection(db, s.f.owner, aiEnv, null, aiConnectionSchema.parse({ name: "Evaluation AI", baseUrl: "https://opencode.ai/zen/v1", apiKey: "synthetic-private-key", textModel: "gpt-5.6-luna" }), key());
  await testAiConnection(db, s.f.owner, aiEnv, c.id, 1, key(), async () => ({ result: { ok: true }, inputTokens: 1, outputTokens: 1 })); await activateAiConnection(db, s.f.owner, c.id, 2, key());
  await selectAiProviders(db, s.f.owner, aiSelectionSchema.parse({ expectedVersion: 0, mode: "custom", textConnectionId: c.id }), key());
}
it("returns cited advisory AI proposals and discards results when human evidence changes during generation", async () => {
  const s = await setup(true), { evidence } = await rated(s); await aiProvider(s);
  const input = { query: { year: 2026, engagementId: s.engagementId }, engagementIds: [s.engagementId] }, queued = await queueEvaluationAi(s.f.owner, aiEnv, input, key());
  const result = { people: [{ engagementId: s.engagementId, summary: "Evidence supports development review", supportingEvidenceIds: [String(evidence.id)], contradictions: [], unknowns: ["No target role assessment"], suggestions: [{ kind: "development", text: "Review the demonstrated complex-work behavior", evidenceIds: [String(evidence.id)] }] }] };
  await runEvaluationAi(db, aiEnv, String(queued.id), async () => ({ result, inputTokens: 5, outputTokens: 5 }));
  expect((await evaluationAiStatus(s.f.owner, String(queued.id))).state).toBe("ready");
  const changed = await queueEvaluationAi(s.f.owner, aiEnv, input, key());
  await runEvaluationAi(db, aiEnv, String(changed.id), async () => {
    await addEvaluationEvidence(s.f.owner, String(s.assessment.id), { expectedVersion: 3, sourceType: "kpi", observedOn: "2026-07-01", description: "New human evidence arrived during generation" }, key());
    return { result, inputTokens: 5, outputTokens: 5 };
  });
  expect((await evaluationAiStatus(s.f.owner, String(changed.id))).state).toBe("stale");
  expect((await evaluationAiStatus(s.f.owner, String(changed.id))).result).toBeNull();
});
it("validates AI citations, preserves manual workflow on failure and rechecks revoked authority", async () => {
  const s = await setup(true); const { evidence } = await rated(s); await aiProvider(s);
  const input = { query: { year: 2026 }, engagementIds: [s.engagementId] };
  const queued = await queueEvaluationAi(s.f.owner, aiEnv, input, key());
  await runEvaluationAi(db, aiEnv, String(queued.id), async () => ({ result: { people: [{ engagementId: s.engagementId, summary: "Review demonstrated outcomes", supportingEvidenceIds: ["invented"], contradictions: [], unknowns: [], suggestions: [] }] }, inputTokens: 1, outputTokens: 1 }));
  expect((await evaluationAiStatus(s.f.owner, String(queued.id))).state).toBe("failed");
  const second = await queueEvaluationAi(s.f.owner, aiEnv, input, key());
  await runEvaluationAi(db, aiEnv, String(second.id), async () => { await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), s.f.owner.assignmentId).run(); return { result: { people: [{ engagementId: s.engagementId, summary: "Evidence supports review", supportingEvidenceIds: [String(evidence.id)], contradictions: [], unknowns: ["No target role"], suggestions: [] }] }, inputTokens: 1, outputTokens: 1 }; });
  expect(await db.prepare("SELECT state FROM hr_evaluation_ai_jobs WHERE id=?").bind(second.id).first("state")).toBe("stale");
  expect(await db.prepare("SELECT status FROM hr_evaluation_assessments WHERE id=?").bind(s.assessment.id).first("status")).toBe("draft");
});
