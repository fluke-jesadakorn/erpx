"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import Link from "@erpx/workplace/components/workspace-link";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { useRef, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { leaveAdministration } from "@erpx/server-people_payroll/public/leave-self-service";
import styles from "@erpx/feature-people_payroll/components/self-service/leave-policy-wizard.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

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
type Draft = {
  policyKey: string;
  version: string;
  name: string;
  effectiveFrom: string;
  effectiveTo: string;
  timezone: string;
  workweek: number[];
  allowNegative: boolean;
  rules: Rule[];
};
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

export function LeavePolicyWizard({ appearance = "legacy",
  base,
  types,
  onSaved,
  onCancel,
  cancelAction,
}: ({
  base: string;
  types: Awaited<ReturnType<typeof leaveAdministration>>["types"];
  onSaved?: () => Promise<void>;
  onCancel?: () => void;
  cancelAction?: ReactNode;
}) & { appearance?: UiAppearance }) {
  const router = useRouter();
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>({
    policyKey: "",
    version: "1",
    name: "",
    effectiveFrom: "",
    effectiveTo: "",
    timezone: "Asia/Bangkok",
    workweek: [1, 2, 3, 4, 5],
    allowNegative: false,
    rules: [newRule()],
  });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const ready = useSyncExternalStore(subscribeHydration, clientReady, serverReady);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const update = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));
  const updateRule = <K extends keyof Rule>(index: number, key: K, value: Rule[K]) =>
    setDraft((current) => ({
      ...current,
      rules: current.rules.map((rule, i) => (i === index ? { ...rule, [key]: value } : rule)),
    }));
  const typeName = (id: string) => types.find((type) => type.id === id)?.name ?? "Choose a type";
  function next(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    if (step === 0 && draft.effectiveTo && draft.effectiveTo < draft.effectiveFrom) {
      setError("Effective end date must be on or after the start date.");
      return;
    }
    if (step === 1 && draft.workweek.length === 0) {
      setError("Choose at least one working day.");
      return;
    }
    if (
      step === 2 &&
      new Set(draft.rules.map((rule) => rule.leaveTypeId)).size !== draft.rules.length
    ) {
      setError("Use each leave type once in this policy.");
      return;
    }
    if (
      step === 2 &&
      draft.rules.some(
        (rule) =>
          rule.grantKind === "accrual" && (!rule.accrualPeriod || Number(rule.accrualUnits) <= 0),
      )
    ) {
      setError("Accrual rules need a positive amount and a period.");
      return;
    }
    if (
      step === 2 &&
      draft.rules.some(
        (rule) =>
          rule.evidencePurpose &&
          (!rule.evidenceMinUnits ||
            Number(rule.evidenceMinUnits) <= 0 ||
            Number(rule.evidenceMinCount) < 1),
      )
    ) {
      setError("Evidence rules need a minimum leave duration and at least one required file.");
      return;
    }
    setStep((current) => Math.min(current + 1, 3));
  }
  async function save() {
    const input = {
      resource: "policy",
      policyKey: draft.policyKey,
      version: Number(draft.version),
      name: draft.name,
      effectiveFrom: draft.effectiveFrom,
      effectiveTo: draft.effectiveTo || null,
      timezone: draft.timezone,
      workweek: draft.workweek,
      allowNegative: draft.allowNegative,
      rules: draft.rules.map((rule) => ({
        leaveTypeId: rule.leaveTypeId,
        grantKind: rule.grantKind,
        grantUnits: Number(rule.grantUnits),
        accrualUnits: Number(rule.accrualUnits),
        accrualPeriod: rule.accrualPeriod || null,
        carryoverLimit: rule.carryoverLimit ? Number(rule.carryoverLimit) : null,
        employeeEligible: rule.employeeEligible,
        contractorEligible: rule.contractorEligible,
        evidencePurpose: rule.evidencePurpose || null,
        evidenceMinUnits: rule.evidencePurpose ? Number(rule.evidenceMinUnits) : null,
        evidenceMinCount: rule.evidencePurpose ? Number(rule.evidenceMinCount) : 0,
        evidenceDueStage: rule.evidenceDueStage,
        evidenceGraceDays: Number(rule.evidenceGraceDays),
        evidenceNote: rule.evidenceNote,
      })),
    };
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/leave/admin`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to create policy.");
      attempt.current = null;
      if (onSaved) await onSaved();
      else {
        router.push(canonicalUiHref(`${base}/people/settings?section=leave`));
        router.refresh();
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to create policy.");
    } finally {
      setBusy(false);
    }
  }
  if (!ready)
    return (
      <div data-ui-appearance={appearance} className={styles.wizard}>
        <section data-ui-appearance={appearance} className={styles.panel} role="status">
          Loading policy editor…
        </section>
      </div>
    );
  return (
    <div data-ui-appearance={appearance} className={styles.wizard}>
      <ol data-ui-appearance={appearance} className={styles.progress} aria-label="Policy creation steps">
        {steps.map((label, index) => (
          <li data-ui-appearance={appearance} key={label} aria-current={step === index ? "step" : undefined}>
            <span data-ui-appearance={appearance}>{index + 1}</span>
            {label}
          </li>
        ))}
      </ol>
      <section data-ui-appearance={appearance} className={styles.panel}>
        <div data-ui-appearance={appearance} className={styles.heading}>
          <p data-ui-appearance={appearance}>
            Step {step + 1} of {steps.length}
          </p>
          <h2 data-ui-appearance={appearance}>{steps[step]}</h2>
        </div>
        {step === 0 && (
          <form data-ui-appearance={appearance} className={styles.form} onSubmit={next}>
            <div data-ui-appearance={appearance} className={styles.fields}>
              <Field appearance={appearance}>
                Policy key
                <TextInput appearance={appearance}
                  value={draft.policyKey}
                  onChange={(e) => update("policyKey", e.target.value)}
                  required
                  maxLength={80}
                />
              </Field>
              <Field appearance={appearance}>
                Version
                <TextInput appearance={appearance}
                  type="number"
                  min="1"
                  value={draft.version}
                  onChange={(e) => update("version", e.target.value)}
                  required
                />
              </Field>
            </div>
            <Field appearance={appearance}>
              Policy name
              <TextInput appearance={appearance}
                value={draft.name}
                onChange={(e) => update("name", e.target.value)}
                required
                maxLength={100}
              />
            </Field>
            <div data-ui-appearance={appearance} className={styles.fields}>
              <Field appearance={appearance}>
                Effective from
                <TextInput appearance={appearance}
                  type="date"
                  value={draft.effectiveFrom}
                  onChange={(e) => update("effectiveFrom", e.target.value)}
                  required
                />
              </Field>
              <Field appearance={appearance}>
                Effective to
                <TextInput appearance={appearance}
                  type="date"
                  value={draft.effectiveTo}
                  onChange={(e) => update("effectiveTo", e.target.value)}
                />
              </Field>
            </div>
            <Field appearance={appearance}>
              Timezone
              <TextInput appearance={appearance}
                value={draft.timezone}
                onChange={(e) => update("timezone", e.target.value)}
                required
                maxLength={80}
              />
            </Field>
            <div data-ui-appearance={appearance} className={styles.actions}>
              {cancelAction ?? (onCancel ? (
                <Button appearance={appearance} variant="secondary" type="button" onClick={onCancel}>
                  Cancel
                </Button>
              ) : (
                <UiActionLink appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/people/settings?section=leave`)}>
                  Cancel
                </UiActionLink>
              ))}
              <Button appearance={appearance} clientManaged variant="primary" type="submit">
                Continue
              </Button>
            </div>
          </form>
        )}
        {step === 1 && (
          <form data-ui-appearance={appearance} className={styles.form} onSubmit={next}>
            <fieldset data-ui-appearance={appearance}>
              <legend data-ui-appearance={appearance}>Working weekdays</legend>
              <div data-ui-appearance={appearance} className={styles.weekdays}>
                {weekdays.map((day, index) => (
                  <Field appearance={appearance} orientation="inline" key={day}>
                    <TextInput appearance={appearance}
                      type="checkbox"
                      checked={draft.workweek.includes(index + 1)}
                      onChange={(e) =>
                        update(
                          "workweek",
                          e.target.checked
                            ? [...draft.workweek, index + 1].sort()
                            : draft.workweek.filter((value) => value !== index + 1),
                        )
                      }
                    />
                    {day}
                  </Field>
                ))}
              </div>
            </fieldset>
            <Field appearance={appearance} orientation="inline" className={styles.check}>
              <TextInput appearance={appearance}
                type="checkbox"
                checked={draft.allowNegative}
                onChange={(e) => update("allowNegative", e.target.checked)}
              />
              Allow negative entitlement
            </Field>
            <div data-ui-appearance={appearance} className={styles.actions}>
              <Button appearance={appearance} variant="secondary" type="button" onClick={() => setStep(0)}>
                Back
              </Button>
              <Button appearance={appearance} clientManaged variant="primary" type="submit">
                Continue
              </Button>
            </div>
          </form>
        )}
        {step === 2 && (
          <form data-ui-appearance={appearance} className={styles.form} onSubmit={next}>
            {types.length === 0 && (
              <p data-ui-appearance={appearance}>
                No leave types exist yet.{" "}
                <Link href={canonicalUiHref(`${base}/people/settings?section=leave`)}>
                  Create a leave type first.
                </Link>
              </p>
            )}
            {draft.rules.map((rule, index) => (
              <fieldset data-ui-appearance={appearance} key={index}>
                <legend data-ui-appearance={appearance}>Leave rule {index + 1}</legend>
                <Field appearance={appearance}>
                  Leave type
                  <Select appearance={appearance}
                    value={rule.leaveTypeId}
                    onChange={(e) => updateRule(index, "leaveTypeId", e.target.value)}
                    required
                  >
                    <option data-ui-appearance={appearance} value="">Choose a type</option>
                    {types.map((type) => (
                      <option data-ui-appearance={appearance} key={type.id} value={type.id}>
                        {type.name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance={appearance}>
                  Entitlement method
                  <Select appearance={appearance}
                    value={rule.grantKind}
                    onChange={(e) =>
                      updateRule(index, "grantKind", e.target.value as Rule["grantKind"])
                    }
                  >
                    <option data-ui-appearance={appearance} value="annual">Annual grant</option>
                    <option data-ui-appearance={appearance} value="period">One grant per assignment</option>
                    <option data-ui-appearance={appearance} value="accrual">Periodic accrual</option>
                    <option data-ui-appearance={appearance} value="untracked">Untracked</option>
                  </Select>
                </Field>
                <div data-ui-appearance={appearance} className={styles.fields}>
                  <Field appearance={appearance}>
                    Grant units
                    <TextInput appearance={appearance}
                      type="number"
                      min="0"
                      step="0.5"
                      value={rule.grantUnits}
                      onChange={(e) => updateRule(index, "grantUnits", e.target.value)}
                      required
                    />
                  </Field>
                  <Field appearance={appearance}>
                    Accrual units
                    <TextInput appearance={appearance}
                      type="number"
                      min="0"
                      step="0.5"
                      value={rule.accrualUnits}
                      onChange={(e) => updateRule(index, "accrualUnits", e.target.value)}
                      required
                    />
                  </Field>
                  <Field appearance={appearance}>
                    Accrual period
                    <Select appearance={appearance}
                      value={rule.accrualPeriod}
                      onChange={(e) =>
                        updateRule(index, "accrualPeriod", e.target.value as Rule["accrualPeriod"])
                      }
                    >
                      <option data-ui-appearance={appearance} value="">None</option>
                      <option data-ui-appearance={appearance} value="monthly">Monthly</option>
                      <option data-ui-appearance={appearance} value="quarterly">Quarterly</option>
                      <option data-ui-appearance={appearance} value="annual">Annual</option>
                    </Select>
                  </Field>
                  <Field appearance={appearance}>
                    Carryover limit
                    <TextInput appearance={appearance}
                      type="number"
                      min="0"
                      step="0.5"
                      value={rule.carryoverLimit}
                      onChange={(e) => updateRule(index, "carryoverLimit", e.target.value)}
                    />
                  </Field>
                </div>
                <Field appearance={appearance} orientation="inline" className={styles.check}>
                  <TextInput appearance={appearance}
                    type="checkbox"
                    checked={rule.employeeEligible}
                    onChange={(e) => updateRule(index, "employeeEligible", e.target.checked)}
                  />
                  Eligible employees
                </Field>
                <Field appearance={appearance} orientation="inline" className={styles.check}>
                  <TextInput appearance={appearance}
                    type="checkbox"
                    checked={rule.contractorEligible}
                    onChange={(e) => updateRule(index, "contractorEligible", e.target.checked)}
                  />
                  Eligible contractors
                </Field>
                <Field appearance={appearance}>
                  Required evidence
                  <Select appearance={appearance}
                    value={rule.evidencePurpose}
                    onChange={(e) =>
                      updateRule(
                        index,
                        "evidencePurpose",
                        e.target.value as Rule["evidencePurpose"],
                      )
                    }
                  >
                    <option data-ui-appearance={appearance} value="">Not required</option>
                    <option data-ui-appearance={appearance} value="medical_certificate">Medical certificate</option>
                    <option data-ui-appearance={appearance} value="leave_supporting_photo">Supporting photo or document</option>
                  </Select>
                </Field>
                {rule.evidencePurpose && (
                  <>
                    <div data-ui-appearance={appearance} className={styles.fields}>
                      <Field appearance={appearance}>
                        Required from leave units
                        <TextInput appearance={appearance}
                          type="number"
                          min="0.5"
                          max="365"
                          step="0.5"
                          required
                          value={rule.evidenceMinUnits}
                          onChange={(e) => updateRule(index, "evidenceMinUnits", e.target.value)}
                        />
                      </Field>
                      <Field appearance={appearance}>
                        Minimum clean files
                        <TextInput appearance={appearance}
                          type="number"
                          min="1"
                          max="100"
                          required
                          value={rule.evidenceMinCount}
                          onChange={(e) => updateRule(index, "evidenceMinCount", e.target.value)}
                        />
                      </Field>
                      <Field appearance={appearance}>
                        Due stage
                        <Select appearance={appearance}
                          value={rule.evidenceDueStage}
                          onChange={(e) =>
                            updateRule(
                              index,
                              "evidenceDueStage",
                              e.target.value as Rule["evidenceDueStage"],
                            )
                          }
                        >
                          <option data-ui-appearance={appearance} value="before_final_approval">Before final approval</option>
                          <option data-ui-appearance={appearance} value="before_submit">Before submission</option>
                        </Select>
                      </Field>
                      <Field appearance={appearance}>
                        Grace days
                        <TextInput appearance={appearance}
                          type="number"
                          min="0"
                          max="365"
                          value={rule.evidenceGraceDays}
                          onChange={(e) => updateRule(index, "evidenceGraceDays", e.target.value)}
                        />
                      </Field>
                    </div>
                    <Field appearance={appearance}>
                      Employee guidance
                      <TextArea appearance={appearance}
                        maxLength={500}
                        value={rule.evidenceNote}
                        onChange={(e) => updateRule(index, "evidenceNote", e.target.value)}
                        placeholder="Explain which document is required and when it is due."
                      />
                    </Field>
                    <p data-ui-appearance={appearance}>
                      Visibility is restricted to the requester, current reviewer, recorded decision
                      makers, and authorized HR.
                    </p>
                  </>
                )}
                {draft.rules.length > 1 && (
                  <Button appearance={appearance}
                    variant="secondary"
                    type="button"
                    onClick={() =>
                      update(
                        "rules",
                        draft.rules.filter((_, i) => i !== index),
                      )
                    }
                  >
                    Remove rule
                  </Button>
                )}
              </fieldset>
            ))}
            <Button appearance={appearance}
              variant="secondary"
              type="button"
              onClick={() => update("rules", [...draft.rules, newRule()])}
            >
              Add leave type rule
            </Button>
            <div data-ui-appearance={appearance} className={styles.actions}>
              <Button appearance={appearance} variant="secondary" type="button" onClick={() => setStep(1)}>
                Back
              </Button>
              <Button appearance={appearance} clientManaged variant="primary" type="submit" disabled={types.length === 0}>
                Continue
              </Button>
            </div>
          </form>
        )}
        {step === 3 && (
          <div data-ui-appearance={appearance} className={styles.review}>
            <dl data-ui-appearance={appearance}>
              <div data-ui-appearance={appearance}>
                <dt data-ui-appearance={appearance}>Policy</dt>
                <dd data-ui-appearance={appearance}>
                  {draft.name} · {draft.policyKey} v{draft.version}
                </dd>
              </div>
              <div data-ui-appearance={appearance}>
                <dt data-ui-appearance={appearance}>Effective dates</dt>
                <dd data-ui-appearance={appearance}>
                  {draft.effectiveFrom} — {draft.effectiveTo || "ongoing"}
                </dd>
              </div>
              <div data-ui-appearance={appearance}>
                <dt data-ui-appearance={appearance}>Timezone</dt>
                <dd data-ui-appearance={appearance}>{draft.timezone}</dd>
              </div>
              <div data-ui-appearance={appearance}>
                <dt data-ui-appearance={appearance}>Working days</dt>
                <dd data-ui-appearance={appearance}>{draft.workweek.map((day) => weekdays[day - 1]).join(", ")}</dd>
              </div>
              <div data-ui-appearance={appearance}>
                <dt data-ui-appearance={appearance}>Negative entitlement</dt>
                <dd data-ui-appearance={appearance}>{draft.allowNegative ? "Allowed" : "Not allowed"}</dd>
              </div>
            </dl>
            <h3 data-ui-appearance={appearance}>Leave rules</h3>
            <ul data-ui-appearance={appearance}>
              {draft.rules.map((rule, index) => (
                <li data-ui-appearance={appearance} key={index}>
                  <strong data-ui-appearance={appearance}>{typeName(rule.leaveTypeId)}</strong> · {rule.grantKind} ·{" "}
                  {rule.grantUnits} grant units
                  {rule.grantKind === "accrual"
                    ? ` · ${rule.accrualUnits} ${rule.accrualPeriod}`
                    : ""}{" "}
                  · {rule.employeeEligible ? "Employees" : ""}
                  {rule.employeeEligible && rule.contractorEligible ? " and " : ""}
                  {rule.contractorEligible ? "Contractors" : ""}
                  {rule.evidencePurpose
                    ? ` · ${rule.evidenceMinCount} ${rule.evidencePurpose.replaceAll("_", " ")} file(s) from ${rule.evidenceMinUnits} unit(s), ${rule.evidenceDueStage.replaceAll("_", " ")}`
                    : " · No required evidence"}
                </li>
              ))}
            </ul>
            <div data-ui-appearance={appearance} className={styles.actions}>
              <Button appearance={appearance} variant="secondary" type="button" onClick={() => setStep(2)} disabled={busy}>
                Back
              </Button>
              <Button appearance={appearance} variant="primary" type="button" onClick={() => void save()} disabled={busy}>
                {busy ? "Creating…" : "Create policy version"}
              </Button>
            </div>
          </div>
        )}
        {error && (
          <p data-ui-appearance={appearance} className={styles.error} role="alert">
            {error}
          </p>
        )}
      </section>
    </div>
  );
}
