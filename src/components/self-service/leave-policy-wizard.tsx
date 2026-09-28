"use client";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef, useState, useSyncExternalStore, type FormEvent } from "react";
import { workFetch } from "@/lib/work-fetch";
import type { leaveAdministration } from "@/server/leave-self-service";
import styles from "./leave-policy-wizard.module.css";

type Rule = {
  leaveTypeId: string;
  grantKind: "annual" | "period" | "accrual" | "untracked";
  grantUnits: string;
  accrualUnits: string;
  accrualPeriod: "" | "monthly" | "quarterly" | "annual";
  carryoverLimit: string;
  employeeEligible: boolean;
  contractorEligible: boolean;
  evidencePurpose: "" | "medical_certificate" | "leave_supporting_photo";
  evidenceMinUnits: string;
  evidenceMinCount: string;
  evidenceDueStage: "before_submit" | "before_final_approval";
  evidenceGraceDays: string;
  evidenceNote: string;
};
type Draft = { policyKey: string; version: string; name: string; effectiveFrom: string; effectiveTo: string; timezone: string; workweek: number[]; allowNegative: boolean; rules: Rule[] };
const weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const steps = ["Basics", "Working days", "Leave rules", "Review"];
const newRule = (): Rule => ({
  leaveTypeId: "",
  grantKind: "annual",
  grantUnits: "0",
  accrualUnits: "0",
  accrualPeriod: "",
  carryoverLimit: "",
  employeeEligible: true,
  contractorEligible: false,
  evidencePurpose: "",
  evidenceMinUnits: "",
  evidenceMinCount: "0",
  evidenceDueStage: "before_final_approval",
  evidenceGraceDays: "0",
  evidenceNote: "",
});
const subscribeHydration = () => () => {};
const clientReady = () => true;
const serverReady = () => false;

export function LeavePolicyWizard({ base, types }: { base: string; types: Awaited<ReturnType<typeof leaveAdministration>>["types"] }) {
  const router = useRouter();
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>({ policyKey: "", version: "1", name: "", effectiveFrom: "", effectiveTo: "", timezone: "Asia/Bangkok", workweek: [1, 2, 3, 4, 5], allowNegative: false, rules: [newRule()] });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const ready = useSyncExternalStore(subscribeHydration, clientReady, serverReady);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const update = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft(current => ({ ...current, [key]: value }));
  const updateRule = <K extends keyof Rule>(index: number, key: K, value: Rule[K]) => setDraft(current => ({ ...current, rules: current.rules.map((rule, i) => i === index ? { ...rule, [key]: value } : rule) }));
  const typeName = (id: string) => types.find(type => type.id === id)?.name ?? "Choose a type";
  function next(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    if (step === 0 && draft.effectiveTo && draft.effectiveTo < draft.effectiveFrom) { setError("Effective end date must be on or after the start date."); return; }
    if (step === 1 && draft.workweek.length === 0) { setError("Choose at least one working day."); return; }
    if (step === 2 && new Set(draft.rules.map(rule => rule.leaveTypeId)).size !== draft.rules.length) { setError("Use each leave type once in this policy."); return; }
    if (step === 2 && draft.rules.some(rule => rule.grantKind === "accrual" && (!rule.accrualPeriod || Number(rule.accrualUnits) <= 0))) { setError("Accrual rules need a positive amount and a period."); return; }
    if (step === 2 && draft.rules.some(rule => rule.evidencePurpose && (!rule.evidenceMinUnits || Number(rule.evidenceMinUnits) <= 0 || Number(rule.evidenceMinCount) < 1))) { setError("Evidence rules need a minimum leave duration and at least one required file."); return; }
    setStep(current => Math.min(current + 1, 3));
  }
  async function save() {
    const input = { resource: "policy", policyKey: draft.policyKey, version: Number(draft.version), name: draft.name, effectiveFrom: draft.effectiveFrom, effectiveTo: draft.effectiveTo || null, timezone: draft.timezone, workweek: draft.workweek, allowNegative: draft.allowNegative, rules: draft.rules.map(rule => ({ leaveTypeId: rule.leaveTypeId, grantKind: rule.grantKind, grantUnits: Number(rule.grantUnits), accrualUnits: Number(rule.accrualUnits), accrualPeriod: rule.accrualPeriod || null, carryoverLimit: rule.carryoverLimit ? Number(rule.carryoverLimit) : null, employeeEligible: rule.employeeEligible, contractorEligible: rule.contractorEligible, evidencePurpose: rule.evidencePurpose || null, evidenceMinUnits: rule.evidencePurpose ? Number(rule.evidenceMinUnits) : null, evidenceMinCount: rule.evidencePurpose ? Number(rule.evidenceMinCount) : 0, evidenceDueStage: rule.evidenceDueStage, evidenceGraceDays: Number(rule.evidenceGraceDays), evidenceNote: rule.evidenceNote })) };
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true); setError("");
    try {
      const response = await workFetch(`/api/v1${base}/leave/admin`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to create policy.");
      attempt.current = null;
      router.push(`${base}/people/settings?section=leave`);
      router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Unable to create policy."); }
    finally { setBusy(false); }
  }
  if (!ready) return <div className={styles.wizard}><section className={styles.panel} role="status">Loading policy editor…</section></div>;
  return <div className={styles.wizard}>
    <ol className={styles.progress} aria-label="Policy creation steps">{steps.map((label, index) => <li key={label} aria-current={step === index ? "step" : undefined}><span>{index + 1}</span>{label}</li>)}</ol>
    <section className={styles.panel}>
      <div className={styles.heading}><p>Step {step + 1} of {steps.length}</p><h2>{steps[step]}</h2></div>
      {step === 0 && <form className={styles.form} onSubmit={next}><div className={styles.fields}><label>Policy key<input value={draft.policyKey} onChange={e => update("policyKey", e.target.value)} required maxLength={80} /></label><label>Version<input type="number" min="1" value={draft.version} onChange={e => update("version", e.target.value)} required /></label></div><label>Policy name<input value={draft.name} onChange={e => update("name", e.target.value)} required maxLength={100} /></label><div className={styles.fields}><label>Effective from<input type="date" value={draft.effectiveFrom} onChange={e => update("effectiveFrom", e.target.value)} required /></label><label>Effective to<input type="date" value={draft.effectiveTo} onChange={e => update("effectiveTo", e.target.value)} /></label></div><label>Timezone<input value={draft.timezone} onChange={e => update("timezone", e.target.value)} required maxLength={80} /></label><div className={styles.actions}><Link className="button secondary" href={`${base}/people/settings?section=leave`}>Cancel</Link><button className="button" type="submit">Continue</button></div></form>}
      {step === 1 && <form className={styles.form} onSubmit={next}><fieldset><legend>Working weekdays</legend><div className={styles.weekdays}>{weekdays.map((day, index) => <label key={day}><input type="checkbox" checked={draft.workweek.includes(index + 1)} onChange={e => update("workweek", e.target.checked ? [...draft.workweek, index + 1].sort() : draft.workweek.filter(value => value !== index + 1))} />{day}</label>)}</div></fieldset><label className={styles.check}><input type="checkbox" checked={draft.allowNegative} onChange={e => update("allowNegative", e.target.checked)} />Allow negative entitlement</label><div className={styles.actions}><button className="button secondary" type="button" onClick={() => setStep(0)}>Back</button><button className="button" type="submit">Continue</button></div></form>}
      {step === 2 && <form className={styles.form} onSubmit={next}>
        {types.length === 0 && <p>No leave types exist yet. <Link href={`${base}/people/settings?section=leave`}>Create a leave type first.</Link></p>}
        {draft.rules.map((rule, index) => <fieldset key={index}>
          <legend>Leave rule {index + 1}</legend>
          <label>Leave type<select value={rule.leaveTypeId} onChange={e => updateRule(index, "leaveTypeId", e.target.value)} required><option value="">Choose a type</option>{types.map(type => <option key={type.id} value={type.id}>{type.name}</option>)}</select></label>
          <label>Entitlement method<select value={rule.grantKind} onChange={e => updateRule(index, "grantKind", e.target.value as Rule["grantKind"])}><option value="annual">Annual grant</option><option value="period">One grant per assignment</option><option value="accrual">Periodic accrual</option><option value="untracked">Untracked</option></select></label>
          <div className={styles.fields}><label>Grant units<input type="number" min="0" step="0.5" value={rule.grantUnits} onChange={e => updateRule(index, "grantUnits", e.target.value)} required /></label><label>Accrual units<input type="number" min="0" step="0.5" value={rule.accrualUnits} onChange={e => updateRule(index, "accrualUnits", e.target.value)} required /></label><label>Accrual period<select value={rule.accrualPeriod} onChange={e => updateRule(index, "accrualPeriod", e.target.value as Rule["accrualPeriod"])}><option value="">None</option><option value="monthly">Monthly</option><option value="quarterly">Quarterly</option><option value="annual">Annual</option></select></label><label>Carryover limit<input type="number" min="0" step="0.5" value={rule.carryoverLimit} onChange={e => updateRule(index, "carryoverLimit", e.target.value)} /></label></div>
          <label className={styles.check}><input type="checkbox" checked={rule.employeeEligible} onChange={e => updateRule(index, "employeeEligible", e.target.checked)} />Eligible employees</label>
          <label className={styles.check}><input type="checkbox" checked={rule.contractorEligible} onChange={e => updateRule(index, "contractorEligible", e.target.checked)} />Eligible contractors</label>
          <label>Required evidence<select value={rule.evidencePurpose} onChange={e => updateRule(index, "evidencePurpose", e.target.value as Rule["evidencePurpose"])}><option value="">Not required</option><option value="medical_certificate">Medical certificate</option><option value="leave_supporting_photo">Supporting photo or document</option></select></label>
          {rule.evidencePurpose && <>
            <div className={styles.fields}>
              <label>Required from leave units<input type="number" min="0.5" max="365" step="0.5" required value={rule.evidenceMinUnits} onChange={e => updateRule(index, "evidenceMinUnits", e.target.value)} /></label>
              <label>Minimum clean files<input type="number" min="1" max="100" required value={rule.evidenceMinCount} onChange={e => updateRule(index, "evidenceMinCount", e.target.value)} /></label>
              <label>Due stage<select value={rule.evidenceDueStage} onChange={e => updateRule(index, "evidenceDueStage", e.target.value as Rule["evidenceDueStage"])}><option value="before_final_approval">Before final approval</option><option value="before_submit">Before submission</option></select></label>
              <label>Grace days<input type="number" min="0" max="365" value={rule.evidenceGraceDays} onChange={e => updateRule(index, "evidenceGraceDays", e.target.value)} /></label>
            </div>
            <label>Employee guidance<textarea maxLength={500} value={rule.evidenceNote} onChange={e => updateRule(index, "evidenceNote", e.target.value)} placeholder="Explain which document is required and when it is due." /></label>
            <p>Visibility is restricted to the requester, current reviewer, recorded decision makers, and authorized HR.</p>
          </>}
          {draft.rules.length > 1 && <button className="button secondary" type="button" onClick={() => update("rules", draft.rules.filter((_, i) => i !== index))}>Remove rule</button>}
        </fieldset>)}
        <button className="button secondary" type="button" onClick={() => update("rules", [...draft.rules, newRule()])}>Add leave type rule</button>
        <div className={styles.actions}><button className="button secondary" type="button" onClick={() => setStep(1)}>Back</button><button className="button" type="submit" disabled={types.length === 0}>Continue</button></div>
      </form>}
      {step === 3 && <div className={styles.review}><dl><div><dt>Policy</dt><dd>{draft.name} · {draft.policyKey} v{draft.version}</dd></div><div><dt>Effective dates</dt><dd>{draft.effectiveFrom} — {draft.effectiveTo || "ongoing"}</dd></div><div><dt>Timezone</dt><dd>{draft.timezone}</dd></div><div><dt>Working days</dt><dd>{draft.workweek.map(day => weekdays[day - 1]).join(", ")}</dd></div><div><dt>Negative entitlement</dt><dd>{draft.allowNegative ? "Allowed" : "Not allowed"}</dd></div></dl><h3>Leave rules</h3><ul>{draft.rules.map((rule, index) => <li key={index}><strong>{typeName(rule.leaveTypeId)}</strong> · {rule.grantKind} · {rule.grantUnits} grant units{rule.grantKind === "accrual" ? ` · ${rule.accrualUnits} ${rule.accrualPeriod}` : ""} · {rule.employeeEligible ? "Employees" : ""}{rule.employeeEligible && rule.contractorEligible ? " and " : ""}{rule.contractorEligible ? "Contractors" : ""}{rule.evidencePurpose ? ` · ${rule.evidenceMinCount} ${rule.evidencePurpose.replaceAll("_", " ")} file(s) from ${rule.evidenceMinUnits} unit(s), ${rule.evidenceDueStage.replaceAll("_", " ")}` : " · No required evidence"}</li>)}</ul><div className={styles.actions}><button className="button secondary" type="button" onClick={() => setStep(2)} disabled={busy}>Back</button><button className="button" type="button" onClick={() => void save()} disabled={busy}>{busy ? "Creating…" : "Create policy version"}</button></div></div>}
      {error && <p className={styles.error} role="alert">{error}</p>}
    </section>
  </div>;
}
