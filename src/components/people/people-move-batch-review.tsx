"use client";

import { useRef, useState, type FormEvent } from "react";
import Link from "next/link";
import { retirementInstant, retirementLocalTime } from "@/lib/retirement";
import { workFetch } from "@/lib/work-fetch";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import type { OrganizationAccessData } from "@/lib/organization-access";
import { moveTrayKey, useMoveTray, writeMoveTray } from "./people-move-tray-state";
import styles from "./people-move-batch-review.module.css";

type BatchPreview = { ready: boolean; token: string | null; items: { engagementId: string; ready: boolean; blockers: string[]; legacyCaseToSupersede?: { id: string; version: number; kind: string; status: string } | null; affectedPeople?: number }[] };
type Props = { base: string; actorId: string; companyId: string; directory: HoldingPeopleDirectory; organization: OrganizationAccessData };

export function PeopleMoveBatchReview({ base, actorId, companyId, directory, organization }: Props) {
  const key = moveTrayKey(base, companyId, actorId);
  const items = useMoveTray(key);
  const [timezone, setTimezone] = useState("Asia/Bangkok");
  const [effectiveLocal, setEffectiveLocal] = useState(() => retirementLocalTime(Date.now() + 3600000, "Asia/Bangkok"));
  const [reason, setReason] = useState("");
  const [manager, setManager] = useState<Record<string, string>>({});
  const [leave, setLeave] = useState<Record<string, boolean>>({});
  const [legacy, setLegacy] = useState<Record<string, { id: string; expectedVersion: number }>>({});
  const [preview, setPreview] = useState<BatchPreview | null>(null);
  const [submitted, setSubmitted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submitKey = useRef<string | null>(null);

  function makeInput() {
    return {
      effectiveAt: retirementInstant(effectiveLocal, timezone), timezone, reason: reason.trim(),
      items: items.map(item => ({
        engagementId: item.engagementId,
        positionId: item.positionId,
        managerPersonId: manager[item.engagementId] ?? organization.people.find(person => person.engagement_id === item.engagementId)?.manager_person_id ?? null,
        acceptRecommendedLeave: leave[item.engagementId] ?? false,
        ...(legacy[item.engagementId] ? { supersedeLegacyCase: legacy[item.engagementId] } : {}),
      })),
    };
  }

  async function command<T>(suffix: "preview" | "submit", body: unknown, idempotencyKey: string): Promise<T> {
    const response = await workFetch(`/api/v1${base}/people/lifecycle/placements/batch/${suffix}`, {
      method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": idempotencyKey }, body: JSON.stringify(body),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error?.message ?? "The batch could not be reviewed. Refresh and try again.");
    return result.data as T;
  }

  async function check(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setBusy(true); setError(""); setPreview(null); submitKey.current = null;
    try { setPreview(await command<BatchPreview>("preview", makeInput(), crypto.randomUUID())); }
    catch (caught) { setError(caught instanceof Error ? caught.message : "Unable to review the batch."); }
    finally { setBusy(false); }
  }

  async function confirm() {
    if (!preview?.ready || !preview.token) return;
    setBusy(true); setError("");
    try {
      submitKey.current ??= crypto.randomUUID();
      await command("submit", { ...makeInput(), previewToken: preview.token }, submitKey.current);
      writeMoveTray(key, []);
      setSubmitted(true);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "No placements were saved. Review the batch again."); setPreview(null); }
    finally { setBusy(false); }
  }

  if (submitted) return <div className={styles.workspace} role="status"><h2>All moves scheduled</h2><p>{items.length} employees were submitted together. Approved salaries remain unchanged; IT preparation follows each HR change.</p><Link className="button" href={`${base}/people`}>Back to People</Link></div>;
  if (!items.length) return <div className={styles.workspace}><h2>The move tray is empty</h2><p>Add employees from People before reviewing a batch.</p><Link className="button secondary" href={`${base}/people`}>Back to People</Link></div>;

  const invalid = items.some(item => !item.positionId || !directory.people.some(person => person.id === item.engagementId && person.companyId === companyId) || !directory.positions.some(position => position.id === item.positionId && position.companyId === companyId));
  return <div className={styles.workspace} data-testid="people-move-batch-review">
    <div className={styles.heading}><span>1 · Check the batch</span><span>2 · Confirm together</span></div>
    <form onSubmit={event => void check(event)}>
      <h2>{items.length} employees</h2>
      <p>Each employee can have a different destination and manager. One effective date and reason apply to the entire batch.</p>
      <div className={styles.people}>{items.map(item => {
        const person = directory.people.find(value => value.id === item.engagementId && value.companyId === companyId);
        const target = directory.positions.find(value => value.id === item.positionId && value.companyId === companyId);
        const result = preview?.items.find(value => value.engagementId === item.engagementId);
        return <section key={item.engagementId} className={styles.person}><div><strong>{person?.legalName ?? "Employee unavailable"}</strong><small>{directory.positions.find(value => value.id === person?.positionId)?.title ?? "No current position"} → {target?.title ?? "Choose destination in tray"}</small></div>
          <label>Reporting manager<select value={manager[item.engagementId] ?? organization.people.find(value => value.engagement_id === item.engagementId)?.manager_person_id ?? ""} onChange={event => { setManager(previous => ({ ...previous, [item.engagementId]: event.target.value })); setPreview(null); }}><option value="">No manager</option>{organization.people.filter(value => value.status === "active" && value.person_id !== person?.personId).map(value => <option key={value.engagement_id} value={value.person_id}>{value.legal_name}</option>)}</select></label>
          <label className={styles.check}><input type="checkbox" checked={leave[item.engagementId] ?? false} onChange={event => { setLeave(previous => ({ ...previous, [item.engagementId]: event.target.checked })); setPreview(null); }} /> Apply destination Level’s leave default, if configured</label>
          {result?.legacyCaseToSupersede && <label className={styles.check}><input type="checkbox" checked={legacy[item.engagementId]?.id === result.legacyCaseToSupersede.id} onChange={event => { setLegacy(previous => { const next = { ...previous }; if (event.target.checked) next[item.engagementId] = { id: result.legacyCaseToSupersede!.id, expectedVersion: result.legacyCaseToSupersede!.version }; else delete next[item.engagementId]; return next; }); setPreview(null); }} /> Replace the older {result.legacyCaseToSupersede.kind} request ({result.legacyCaseToSupersede.status.replaceAll("_", " ")})</label>}
          {result && <p className={result.ready && (!result.legacyCaseToSupersede || legacy[item.engagementId]?.id === result.legacyCaseToSupersede.id) ? styles.good : styles.error}>{result.blockers.length ? result.blockers.join(" · ") : result.legacyCaseToSupersede && legacy[item.engagementId]?.id !== result.legacyCaseToSupersede.id ? "Confirm the older request and preview again." : "Ready to schedule"}</p>}
        </section>;
      })}</div>
      <div className={styles.fields}><label>Timezone<select value={timezone} onChange={event => { setTimezone(event.target.value); setEffectiveLocal(retirementLocalTime(Date.now() + 3600000, event.target.value)); setPreview(null); }}><option>Asia/Bangkok</option><option>UTC</option><option>Asia/Singapore</option></select></label><label>Effective date and time<input type="datetime-local" required value={effectiveLocal} onChange={event => { setEffectiveLocal(event.target.value); setPreview(null); }} /></label></div>
      <label className={styles.reason}>Reason<textarea required minLength={3} maxLength={1000} value={reason} onChange={event => { setReason(event.target.value); setPreview(null); }} placeholder="Reason recorded for every employee" /></label>
      <p className={styles.assurance}>Approved salaries stay unchanged. HR scheduling does not wait for IT delivery. If one submission fails, no employee in this batch is scheduled.</p>
      {error && <p className={styles.error} role="alert">{error}</p>}
      <div className={styles.actions}><Link className="button secondary" href={`${base}/people`}>Back to tray</Link><button className="button secondary" type="submit" disabled={busy || invalid || reason.trim().length < 3}>{busy ? "Checking…" : preview ? "Review again" : "Preview all moves"}</button><button className="button" type="button" disabled={busy || !preview?.ready} onClick={() => void confirm()}>{busy ? "Submitting…" : "Confirm and schedule all"}</button></div>
    </form>
  </div>;
}
