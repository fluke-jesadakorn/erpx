"use client";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef, useState, useSyncExternalStore, type FormEvent } from "react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import type { leaveAdministration } from "@/server/leave-self-service";
import styles from "./self-service.module.css";

type Data = Awaited<ReturnType<typeof leaveAdministration>>;
type Action = "type" | "calendar" | "holiday" | "assignment" | "adjustment" | "eligibility" | "assignment_end";
const pageSize = 10;
const subscribeHydration = () => () => {};
const clientReady = () => true;
const serverReady = () => false;

export function SelfServiceSettings({ base, data }: { base: string; data: Data }) {
  const router = useRouter();
  const [action, setAction] = useState<Action | null>(null);
  const [endingId, setEndingId] = useState("");
  const [eligibilityId, setEligibilityId] = useState("");
  const [page, setPage] = useState(0);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const ready = useSyncExternalStore(subscribeHydration, clientReady, serverReady);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const activeAssignments = data.assignments.filter(a => !a.effective_to).length;
  const selectedAssignment = data.assignments.find(a => a.id === endingId);
  const unavailable = (id: Action | "policy") =>
    (id === "assignment" && (!data.policies.length || !data.engagements.length)) ||
    (id === "adjustment" && (!data.types.length || !data.engagements.length)) ||
    (id === "eligibility" && !data.engagements.length);
  function open(next: Action, id = "") { setAction(next); setEndingId(id); setError(""); setMessage(""); }
  function close() { if (!busy) { setAction(null); setError(""); } }
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!action) return;
    const f = new FormData(event.currentTarget);
    const value = (k: string) => String(f.get(k) ?? "");
    const number = (k: string) => Number(f.get(k) ?? 0);
    let input: unknown;
    switch (action) {
      case "type": input = { resource: "type", code: value("code"), name: value("name"), description: value("description"), tracked: f.has("tracked"), evidenceAfterUnits: value("evidence") ? number("evidence") : null, evidenceNote: value("evidenceNote") }; break;
      case "calendar": input = { resource: "calendar", name: value("name"), timezone: value("timezone") }; break;
      case "holiday": input = { resource: "holiday", calendarId: value("calendar"), date: value("date"), name: value("name") }; break;
      case "assignment": input = { resource: "assignment", engagementId: value("engagement"), policyVersionId: value("policy"), calendarId: value("calendar") || null, effectiveFrom: value("from"), effectiveTo: value("to") || null }; break;
      case "adjustment": input = { resource: "adjustment", engagementId: value("engagement"), leaveTypeId: value("type"), units: number("units"), effectiveDate: value("date"), note: value("note") }; break;
      case "assignment_end": input = { resource: "assignment_end", id: endingId, expectedVersion: selectedAssignment?.version ?? 0, effectiveTo: value("to") }; break;
      case "eligibility": input = { engagementId: value("engagement"), expectedVersion: data.eligibilities.find(e => e.engagement_id === value("engagement"))?.version ?? 0, eligible: value("eligible") === "true", topLevel: f.has("topLevel"), reason: value("note") }; break;
    }
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true); setError("");
    try {
      const response = await workFetch(`/api/v1${base}/${action === "eligibility" ? "overtime-requests/eligibility" : "leave/admin"}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to save settings.");
      attempt.current = null;
      setAction(null);
      setMessage("Saved to server.");
      router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Unable to save settings."); }
    finally { setBusy(false); }
  }
  const people = <label>Engagement<select name="engagement" required value={action === "eligibility" ? eligibilityId : undefined} onChange={action === "eligibility" ? e => setEligibilityId(e.target.value) : undefined}><option value="">Choose staff</option>{data.engagements.map(e => <option key={e.id} value={e.id}>{e.name} · {e.worker_type}</option>)}</select></label>;
  const types = <label>Leave type<select name="type" required><option value="">Choose a type</option>{data.types.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}</select></label>;
  const calendars = <label>Holiday calendar<select name="calendar"><option value="">No calendar</option>{data.calendars.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label>;
  const actions: { id: Action | "policy"; title: string; detail: string; count: string; label: string }[] = [
    { id: "type", title: "Leave types & evidence", detail: "Define available leave and the evidence required.", count: `${data.types.length} types`, label: "Create leave type" },
    { id: "policy", title: "Policy versions", detail: "Set workdays, entitlement grants, and accruals.", count: `${data.policies.length} versions`, label: "Create policy" },
    { id: "calendar", title: "Holiday calendars", detail: data.calendars.length ? "Maintain calendars and dated holidays." : "Create a calendar before adding holidays.", count: `${data.calendars.length} calendars`, label: "Create calendar" },
    { id: "assignment", title: "Policy assignments", detail: "Assign a policy version to an active engagement.", count: `${activeAssignments} active`, label: "Assign policy" },
    { id: "adjustment", title: "Entitlement adjustments", detail: "Record a dated adjustment and reason.", count: "Manual entry", label: "Record adjustment" },
    { id: "eligibility", title: "Overtime eligibility", detail: "Review who may request overtime.", count: `${data.eligibilities.length} decisions`, label: "Set eligibility" },
  ];
  return <div className={styles.settingsStack}>
    <aside className={styles.policyNote}><strong>Policy model</strong><span>Policies belong to the Company and are assigned to engagements. Positions organize work; they do not grant entitlement or approval authority.</span></aside>
    {message && <p className={styles.feedback} role="status">{message}</p>}
    <div className={styles.settingsList}>{actions.map(item => <section className={styles.settingsRow} key={item.id}><div><h2>{item.title}</h2><p>{item.detail}</p><small>{item.count}</small></div>{item.id === "policy" ? <Link className="button secondary" href={`${base}/people/settings/leave-policy/new`}>{item.label}</Link> : <button className="button secondary" type="button" disabled={!ready || unavailable(item.id)} onClick={() => open(item.id as Action)}>{item.label}</button>}{item.id === "calendar" && <button className="button secondary" type="button" disabled={!ready || !data.calendars.length} onClick={() => open("holiday")}>Add holiday</button>}</section>)}</div>
    <section className={styles.assignmentList} aria-labelledby="policy-assignment-heading"><div className={styles.assignmentHead}><div><h2 id="policy-assignment-heading">Current policy assignments</h2><p>{data.assignments.length} total · {activeAssignments} active</p></div></div>{data.assignments.length ? data.assignments.slice(page * pageSize, (page + 1) * pageSize).map(a => <div className={styles.assignmentRow} data-assignment-row key={a.id}><div><strong>{a.name}</strong><span>{a.policy} · {a.effective_from} — {a.effective_to ?? "ongoing"}</span></div>{!a.effective_to && <button className="button secondary" type="button" disabled={!ready} onClick={() => open("assignment_end", a.id)}>End assignment</button>}</div>) : <p>No policy assignments yet.</p>}{data.assignments.length > pageSize && <nav className={styles.pagination} aria-label="Policy assignment pages"><button type="button" className="button secondary" disabled={!ready || page === 0} onClick={() => setPage(p => p - 1)}>Previous</button><span>Page {page + 1} of {Math.ceil(data.assignments.length / pageSize)}</span><button type="button" className="button secondary" disabled={!ready || (page + 1) * pageSize >= data.assignments.length} onClick={() => setPage(p => p + 1)}>Next</button></nav>}</section>
    <Dialog open={!!action} onClose={close} title={{ type: "Create leave type", calendar: "Create holiday calendar", holiday: "Add holiday", assignment: "Assign policy to staff", adjustment: "Record entitlement adjustment", eligibility: "Set overtime eligibility", assignment_end: "End policy assignment" }[action ?? "type"]} presentation="responsive-panel">
      <form className={styles.form} onSubmit={e => void save(e)}>
        {action === "type" && <><label>Code<input name="code" required /></label><label>Name<input name="name" required /></label><label>Description<textarea name="description" /></label><label><input type="checkbox" name="tracked" defaultChecked />Track entitlement</label><label>Evidence required after (days)<input type="number" name="evidence" min="0.5" step="0.5" /></label><label>Evidence guidance<textarea name="evidenceNote" /></label></>}
        {action === "calendar" && <><label>Calendar name<input name="name" required /></label><label>Timezone<input name="timezone" defaultValue="Asia/Bangkok" required /></label></>}
        {action === "holiday" && <><label>Holiday calendar<select name="calendar" required><option value="">Choose a calendar</option>{data.calendars.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label><label>Holiday date<input name="date" type="date" required /></label><label>Holiday name<input name="name" required /></label></>}
        {action === "assignment" && <>{people}<label>Policy version<select name="policy" required><option value="">Choose a policy</option>{data.policies.map(p => <option key={p.id} value={p.id}>{p.name} · v{p.version} · {p.effective_from}</option>)}</select></label>{calendars}<div className={styles.fields}><label>Effective from<input type="date" name="from" required /></label><label>Effective to<input type="date" name="to" /></label></div></>}
        {action === "adjustment" && <>{people}{types}<label>Adjustment units<input type="number" step="0.5" name="units" required /></label><label>Effective date<input name="date" type="date" required /></label><label>Reason<textarea name="note" required /></label></>}
        {action === "eligibility" && <>{people}<div key={eligibilityId} className={styles.form}><label>Eligibility<select name="eligible" defaultValue={String(data.eligibilities.find(e => e.engagement_id === eligibilityId)?.eligible ?? true)}><option value="true">Eligible</option><option value="false">Ineligible</option></select></label><label>Reason<textarea name="note" required minLength={3} /></label><label><input type="checkbox" name="topLevel" defaultChecked={data.eligibilities.find(e => e.engagement_id === eligibilityId)?.top_level ?? false} />Confirmed top-level engagement (no reporting manager)</label><p className={styles.muted}>Contractors require an explicit eligible decision. Manager and independent HR reviews still apply.</p></div></>}
        {action === "assignment_end" && selectedAssignment && <><p>{selectedAssignment.name} · {selectedAssignment.policy}</p><label>End assignment on<input type="date" name="to" min={selectedAssignment.effective_from} required /></label></>}
        {error && <p className={styles.error} role="alert">{error}</p>}
        <div className={styles.actions}><button type="button" className="button secondary" onClick={close}>Cancel</button><button className="button" disabled={busy}>{busy ? "Saving…" : "Save"}</button></div>
      </form>
    </Dialog>
  </div>;
}
