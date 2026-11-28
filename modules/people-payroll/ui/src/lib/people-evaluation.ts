import { z } from "zod";

const id = z.string().trim().min(1).max(180);
const text = z.string().trim().min(3).max(4000);
export const evaluationDate = z.string().date();
const score = z.number().int().min(1).max(5);
const reference = z.object({ id, version: z.number().int().positive() }).strict();
export const skillSchema = z.object({ name: text, category: z.enum(["professional", "knowledge", "collaboration", "leadership"]), behaviors: z.array(text).length(5), confirmed: z.boolean() }).strict();
export const roleSchema = z.object({ name: text, positionId: id, levelId: id, family: text, location: text, requirements: z.array(z.object({ skill: reference, level: score, weight: z.number().positive().max(100), critical: z.boolean() }).strict()).min(1).max(100), difficulty: score, autonomy: score, impact: score, responsibility: score, description: text, confirmed: z.boolean() }).strict().refine(x => new Set(x.requirements.map(r => r.skill.id)).size === x.requirements.length, "Each skill must appear once.");
export const policySchema = z.object({ name: text, weights: z.object({ results: z.number().min(0).max(100), quality: z.number().min(0).max(100), collaboration: z.number().min(0).max(100) }).strict(), fitThreshold: z.number().min(1).max(100), coverageThreshold: z.number().min(1).max(100), outperformThreshold: score, marketMaxAgeDays: z.number().int().min(1).max(1095), maxRaisePercent: z.number().min(0).max(100), compensationRule: text, aiEnabled: z.boolean().default(false), confirmed: z.boolean() }).strict().refine(p => Math.abs(p.weights.results + p.weights.quality + p.weights.collaboration - 100) < 0.001, "Performance weights must total 100%.");
export const defaultEvaluationPolicy: z.infer<typeof policySchema> = { name: "Annual evaluation", weights: { results: 50, quality: 30, collaboration: 20 }, fitThreshold: 80, coverageThreshold: 80, outperformThreshold: 4, marketMaxAgeDays: 365, maxRaisePercent: 0, compensationRule: "HR must define and confirm the Company's compensation policy before pay recommendations.", aiEnabled: false, confirmed: false };
export const benchmarkSchema = z.object({ name: text, family: text, levelId: id, location: text, currency: z.string().regex(/^[A-Z]{3}$/), source: text, observedOn: evaluationDate, validUntil: evaluationDate, basis: z.literal("monthly_fte"), minimumMinor: z.number().int().nonnegative(), midpointMinor: z.number().int().positive(), maximumMinor: z.number().int().positive(), confirmed: z.boolean() }).strict().refine(p => p.minimumMinor <= p.midpointMinor && p.midpointMinor <= p.maximumMinor && p.observedOn <= p.validUntil, "Review the salary range and market dates.");
export const bandSchema = z.object({ name: text, family: text, levelId: id, location: text, currency: z.string().regex(/^[A-Z]{3}$/), basis: z.literal("monthly_fte"), minimumMinor: z.number().int().nonnegative(), midpointMinor: z.number().int().positive(), maximumMinor: z.number().int().positive(), effectiveFrom: evaluationDate, effectiveTo: evaluationDate, benchmark: reference.nullable(), confirmed: z.boolean() }).strict().refine(p => p.minimumMinor <= p.midpointMinor && p.midpointMinor <= p.maximumMinor && p.effectiveFrom <= p.effectiveTo, "Review salary range and effective dates.");
export const cycleSchema = z.object({ name: text, year: z.number().int().min(2020).max(2200), policy: reference, status: z.enum(["draft", "open", "closed"]), confirmed: z.boolean() }).strict();
export const definitionSchemas = { skill: skillSchema, role: roleSchema, policy: policySchema, benchmark: benchmarkSchema, band: bandSchema, cycle: cycleSchema };
export type DefinitionKind = keyof typeof definitionSchemas;
export type EvaluationDefinition = { id: string; kind: DefinitionKind; version: number; data: Record<string, unknown>; updatedAt: number };
export type RoleProfile = z.infer<typeof roleSchema>;
export type EvaluationPolicy = z.infer<typeof policySchema>;
const rating = z.object({ score, evidenceIds: z.array(id).min(1).max(40), rationale: text }).strict();
export const assessmentContentSchema = z.object({ skills: z.array(z.object({ skillId: id, skillVersion: z.number().int().positive().default(1), level: score, evidenceIds: z.array(id).min(1).max(40), rationale: text }).strict()).max(100), performance: z.object({ results: rating.nullable(), quality: rating.nullable(), collaboration: rating.nullable(), goals: z.array(z.object({ goal: text, target: text, outcome: z.string().trim().max(4000) }).strict()).max(30) }).strict(), actualWork: z.object({ description: text, difficulty: score, autonomy: score, impact: score, responsibility: score, evidenceIds: z.array(id).min(1).max(40) }).strict().nullable(), targetRole: reference.nullable(), targetConfirmed: z.boolean(), recommendation: text.nullable() }).strict().refine(p => new Set(p.skills.map(s => `${s.skillId}:${s.skillVersion}`)).size === p.skills.length, "Each skill version needs one rating.");
export type AssessmentContent = z.infer<typeof assessmentContentSchema>;
export const emptyAssessment: AssessmentContent = { skills: [], performance: { results: null, quality: null, collaboration: null, goals: [] }, actualWork: null, targetRole: null, targetConfirmed: false, recommendation: null };
export const basisSchema = z.object({ type: z.enum(["monthly", "daily", "hourly"]), fte: z.number().positive().max(1), fullTimeMonthlyHours: z.number().positive().max(744), fullTimeMonthlyDays: z.number().positive().max(31), reason: text }).strict();
export type PayBasis = z.infer<typeof basisSchema>;
export type Compensation = { id: string; version: number; currency: string; pay_frequency: string; base_amount_minor: number; hourly_rate_minor: number; daily_rate_minor: number; standard_daily_hours: number; expected_hours: number; effective_from: string; effective_to: string | null };

/** Unknown is never coerced to zero, including absent critical skills. */
export function skillFit(role: RoleProfile | null, content: AssessmentContent, policy: EvaluationPolicy) {
  if (!role) return { fit: null, coverage: 0, conclusion: "unassessed", criticalGaps: [] as string[], unknown: [] as string[] };
  const requirements = role.requirements;
  const assessed = requirements.filter(r => content.skills.some(s => s.skillId === r.skill.id && s.skillVersion === r.skill.version));
  const total = requirements.reduce((n, r) => n + r.weight, 0);
  const pass = assessed.filter(r => content.skills.find(s => s.skillId === r.skill.id && s.skillVersion === r.skill.version)!.level >= r.level);
  const fit = assessed.length ? pass.reduce((n, r) => n + r.weight, 0) / total * 100 : null;
  const coverage = assessed.length / requirements.length * 100;
  const criticalGaps = assessed.filter(r => r.critical && !pass.includes(r)).map(r => r.skill.id);
  const unknown = requirements.filter(r => !assessed.includes(r)).map(r => r.skill.id);
  const criticalUnknown = requirements.some(r => r.critical && unknown.includes(r.skill.id));
  const conclusion = coverage < policy.coverageThreshold || criticalUnknown ? "insufficient_evidence" : fit !== null && fit >= policy.fitThreshold && !criticalGaps.length ? "meets_role" : "confirmed_gap";
  return { fit, coverage, conclusion, criticalGaps, unknown };
}
export function evaluationMetrics(role: RoleProfile | null, target: RoleProfile | null, content: AssessmentContent, policy: EvaluationPolicy) {
  const p = content.performance;
  const performance = (Object.keys(policy.weights) as (keyof typeof policy.weights)[]).some(k => policy.weights[k] > 0 && !p[k]) ? null : (Object.keys(policy.weights) as (keyof typeof policy.weights)[]).reduce((sum, k) => sum + (p[k]?.score ?? 0) * policy.weights[k] / 100, 0);
  const current = skillFit(role, content, policy), targetFit = skillFit(target, content, policy);
  return { ...current, performance, outperform: performance === null ? null : performance >= policy.outperformThreshold, targetFit: target ? targetFit : null, targetReady: !!target && content.targetConfirmed && targetFit.conclusion === "meets_role", difficulty: content.actualWork?.difficulty ?? null, responsibility: content.actualWork?.responsibility ?? null };
}
/** The pay period/FTE basis is an HR attestation, not inferred from attendance. */
export function monthlyFtePay(pay: Compensation | null, basis: PayBasis | null): { value: number | null; missing: string | null } {
  if (!pay) return { value: null, missing: "No approved wage on comparison date" };
  if (!basis) return { value: null, missing: "HR has not confirmed the FTE/hour basis" };
  if (basis.type === "monthly") return pay.pay_frequency === "monthly" && pay.base_amount_minor > 0 ? { value: pay.base_amount_minor / basis.fte, missing: null } : { value: null, missing: "Pay period or monthly base does not match the confirmed basis" };
  if (basis.type === "daily") return pay.daily_rate_minor > 0 && Math.abs(basis.fullTimeMonthlyDays * pay.standard_daily_hours - basis.fullTimeMonthlyHours) < 0.01 ? { value: pay.daily_rate_minor * basis.fullTimeMonthlyDays, missing: null } : { value: null, missing: "Daily rate or confirmed hours do not match" };
  return pay.hourly_rate_minor > 0 ? { value: pay.hourly_rate_minor * basis.fullTimeMonthlyHours, missing: null } : { value: null, missing: "No hourly rate for the confirmed basis" };
}
export function compensationMetrics(pay: Compensation | null, basis: PayBasis | null, band: z.infer<typeof bandSchema> | null, market: z.infer<typeof benchmarkSchema> | null, date: string, policy: EvaluationPolicy) {
  const normalized = monthlyFtePay(pay, basis);
  let missing = normalized.missing;
  if (!missing && (!band || !band.confirmed || date < band.effectiveFrom || date > band.effectiveTo)) missing = "No confirmed pay band on comparison date";
  if (!missing && pay!.currency !== band!.currency) missing = "Currency/basis mismatch";
  const age = market ? (Date.parse(date) - Date.parse(market.observedOn)) / 86400000 : null;
  const marketStatus = !market ? "unavailable" : !market.confirmed || age! < 0 || age! > policy.marketMaxAgeDays || market.validUntil < date ? "expired" : "current";
  return { monthlyFteMinor: normalized.value, compaRatio: missing ? null : normalized.value! / band!.midpointMinor * 100, missing, marketStatus };
}
