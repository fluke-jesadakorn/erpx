"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { LeaveRequest } from "@/server/leave";
import type { leaveSummary, previewLeave } from "@/server/leave-self-service";
import type { LeaveSegment } from "@/lib/leave-self-service";
import styles from "@/components/leave/leave-flow.module.css";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/ai/guided-intake";
import { formatUiNumber } from "@/lib/ui-format";

type Summary = Awaited<ReturnType<typeof leaveSummary>>;
type Preview = Awaited<ReturnType<typeof previewLeave>>;
type Draft = {
  leaveTypeId: string;
  startDate: string;
  endDate: string;
  startSegment: LeaveSegment;
  endSegment: LeaveSegment;
  handoverNote: string;
};

function snapshotEvidencePurpose(record?: LeaveRequest) {
  let snapshot = record?.evidence_policy_snapshot;
  if (typeof snapshot === "string") {
    try { snapshot = JSON.parse(snapshot); } catch { return null; }
  }
  if (!snapshot || typeof snapshot !== "object") return null;
  const purpose = (snapshot as { purposeKey?: unknown }).purposeKey;
  return purpose === "medical_certificate" || purpose === "leave_supporting_photo" ? purpose : null;
}

export function LeaveEditor({
  base,
  companyId,
  userId,
  assignmentId,
  record,
  types,
  initialDate = "",
  evidencePresent = false,
  externalEvidence = false,
}: {
  base: string;
  companyId?: string;
  userId?: string;
  assignmentId?: string;
  record?: LeaveRequest;
  types: Summary["balances"];
  initialDate?: string;
  evidencePresent?: boolean;
  externalEvidence?: boolean;
}) {
  const router = useRouter();
  const [draft, setDraft] = useState<Draft>({
    leaveTypeId: record?.leave_type_id ?? "",
    startDate: record?.start_date ?? initialDate,
    endDate: record?.end_date ?? record?.start_date ?? initialDate,
    startSegment: record?.start_segment ?? "full",
    endSegment: record?.end_segment ?? "full",
    handoverNote: record?.handover_note ?? "",
  });
  const [guided,setGuided] = useState<Record<string,string>|null>(null);
  useEffect(()=>{
    if (record || !companyId || !userId || !assignmentId) return;
    const restore=()=>setGuided(readGuidedIntakeHandoff(companyId,userId,assignmentId,base,"leave"));
    restore();
    window.addEventListener("erpx:guided-intake-handoff",restore);
    return()=>window.removeEventListener("erpx:guided-intake-handoff",restore);
  },[record,companyId,userId,assignmentId,base]);
  function applyGuided() {
    if (!guided || !companyId || !userId || !assignmentId) return;
    const namedType = types.find(item=>item.type.name.toLocaleLowerCase()===guided.leaveType?.toLocaleLowerCase());
    setDraft(current=>({...current,
      leaveTypeId:namedType?.type.id??current.leaveTypeId,
      startDate:/^\d{4}-\d{2}-\d{2}$/.test(guided.startDate??"")?guided.startDate:current.startDate,
      endDate:/^\d{4}-\d{2}-\d{2}$/.test(guided.endDate??"")?guided.endDate:current.endDate,
    }));
    setPreview(null);
    clearGuidedIntakeHandoff(companyId,userId,assignmentId,base,"leave");
    setGuided(null);
  }
  const [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [uploaded, setUploaded] = useState("");
  const [hasEvidence, setHasEvidence] = useState(evidencePresent);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const uploadAttempt = useRef<{ signature: string; key: string; url?: string } | null>(null);
  const selectedType = types.find((item) => item.type.id === draft.leaveTypeId);
  const maySubmit = !record || record.permittedActions.includes("submit");
  const evidenceRequiredBeforeSubmit = Boolean(
    preview?.evidenceRequired && preview.evidence.dueStage === "before_submit",
  );

  function set<K extends keyof Draft>(key: K, value: Draft[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
    setPreview(null);
    setSaved(false);
  }

  async function calculate() {
    setBusy(true);
    setError("");
    try {
      const response = await workFetch("/api/v1" + base + "/leave-requests/preview", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...draft, id: record?.id }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to calculate this request.");
      setPreview(body.data as Preview);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to calculate this request.");
    } finally {
      setBusy(false);
    }
  }

  async function save(saveOnly: boolean) {
    setBusy(true);
    setError("");
    const body = JSON.stringify({ ...draft, saveOnly, ...(record ? { expectedVersion: record.version } : {}) });
    const storage = "erpx:leave-command:" + base + ":" + (record?.id ?? "new");
    try {
      if (!attempt.current) {
        try { attempt.current = JSON.parse(sessionStorage.getItem(storage) ?? "null"); } catch { /* Start a new command. */ }
      }
      if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
      sessionStorage.setItem(storage, JSON.stringify(attempt.current));
      const response = await workFetch("/api/v1" + base + "/leave-requests" + (record ? "/" + record.id : ""), {
        method: record ? "PATCH" : "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to save this request.");
      sessionStorage.removeItem(storage);
      attempt.current = null;
      setSaved(true);
      setPreview(null);
      window.dispatchEvent(new Event("erpx:work-changed"));
      if (!record) router.replace(base + "/tasks/people_payroll/" + result.data.id + "?from=leave");
      else router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Connection interrupted. Your entered values are retained.");
    } finally {
      setBusy(false);
    }
  }

  async function upload() {
    if (!file || !record) return;
    setBusy(true);
    setError("");
    try {
      const signature = file.name + ":" + file.size + ":" + file.lastModified;
      if (uploadAttempt.current?.signature !== signature)
        uploadAttempt.current = { signature, key: crypto.randomUUID() };
      if (!uploadAttempt.current.url) {
        const response = await workFetch("/api/v1" + base + "/uploads", {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": uploadAttempt.current.key },
          body: JSON.stringify({
            ownerType: "leave_request",
            ownerId: record.id,
            purpose: preview?.evidence.purposeKey ?? snapshotEvidencePurpose(record) ?? "leave_supporting_photo",
            filename: file.name,
            mediaType: file.type,
            bytes: file.size,
          }),
        });
        const result = await response.json();
        if (!response.ok) throw new Error(result.error?.message ?? "Unable to start the evidence upload.");
        uploadAttempt.current.url = result.data.uploadUrl;
      }
      const response = await workFetch(uploadAttempt.current.url!, {
        method: "PUT",
        headers: { "Content-Type": file.type },
        body: file,
      });
      if (!response.ok) {
        const result = await response.json();
        if (response.status === 410) uploadAttempt.current = null;
        throw new Error(result.error?.message ?? "Evidence upload interrupted. Retry this file.");
      }
      setUploaded(file.name + " · uploaded and verified");
      setHasEvidence(true);
      setFile(null);
      uploadAttempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Evidence upload interrupted. Retry this file.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={styles.formShell}>
      <div className={styles.stepHeader}>
        <div className={styles.step} data-active={!preview}><span>1</span><strong>Details</strong></div>
        <span className={styles.stepLine} />
        <div className={styles.step} data-active={!!preview}><span>2</span><strong>Review</strong></div>
        <span className={styles.stepLine} />
        <div className={styles.step} data-active={false}><span>3</span><strong>Submit</strong></div>
      </div>

      <form
        className={styles.leaveForm}
        onSubmit={(event) => { event.preventDefault(); void calculate(); }}
      >
        <div className={styles.formIntro}>
          <h2>Leave details</h2>
          <p>Working days and balance use your assigned Company policy.</p>
        </div>
        {guided && <div className="notice" role="status"><p>AI prepared dates from your message. Review the leave type, balance, and dates before continuing.</p><button type="button" className="button secondary" onClick={applyGuided}>Use prepared details</button> <button type="button" className="button ghost" onClick={()=>{if(companyId&&userId&&assignmentId)clearGuidedIntakeHandoff(companyId,userId,assignmentId,base,"leave");setGuided(null);}}>Discard suggestion</button></div>}
        <label className={styles.field}>
          Leave type
          <select required value={draft.leaveTypeId} onChange={(event) => set("leaveTypeId", event.target.value)}>
            <option value="">Choose a leave type</option>
            {types.map((item) => <option key={item.type.id} value={item.type.id}>{item.type.name}</option>)}
          </select>
        </label>
        {selectedType && (
          <div className={styles.selectedBalance} role="status">
            <span>{selectedType.type.name}</span>
            <strong>{selectedType.type.tracked ? formatUiNumber(Number(selectedType.available), { maximumFractionDigits: 1 }) + " days available" : "Untracked entitlement"}</strong>
          </div>
        )}
        <div className={styles.dateGroup}>
          <div className={styles.groupHeading}><h3>Dates and day portions</h3><p>Select half days at the beginning or end of your request.</p></div>
          <div className={styles.dateFields}>
            <label className={styles.field}>
              First day
              <input required type="date" value={draft.startDate} onChange={(event) => set("startDate", event.target.value)} />
            </label>
            <label className={styles.field}>
              Portion
              <select value={draft.startSegment} onChange={(event) => set("startSegment", event.target.value as LeaveSegment)}>
                <option value="full">Full day</option><option value="first_half">First half</option><option value="second_half">Second half</option>
              </select>
            </label>
            <label className={styles.field}>
              Last day
              <input required type="date" value={draft.endDate} onChange={(event) => set("endDate", event.target.value)} />
            </label>
            <label className={styles.field}>
              Portion
              <select value={draft.endSegment} onChange={(event) => set("endSegment", event.target.value as LeaveSegment)}>
                <option value="full">Full day</option><option value="first_half">First half</option><option value="second_half">Second half</option>
              </select>
            </label>
          </div>
          <p className={styles.fieldHint}>Non-working days and assigned holidays are excluded from the total.</p>
        </div>
        <label className={styles.field}>
          Handover note
          <textarea maxLength={2048} value={draft.handoverNote} onChange={(event) => set("handoverNote", event.target.value)} />
          <span className={styles.fieldHint}>Share anything your team needs to keep work moving.</span>
        </label>
        <div className={styles.formActions}>
          <button type="submit" className="button secondary" disabled={busy || !draft.leaveTypeId || !draft.startDate || !draft.endDate}>
            {busy ? "Working…" : "Review request"}
          </button>
          <button type="button" className="button ghost" disabled={busy || !draft.leaveTypeId || !draft.startDate || !draft.endDate} onClick={() => void save(true)}>
            Save draft
          </button>
          {saved && <span role="status" className={styles.savedMessage}>Saved to server.</span>}
        </div>
      </form>

      {!externalEvidence && (record ? (
        <section className={styles.evidenceSection}>
          <div><h3>Supporting evidence</h3><p>Files stay private with this request. Attach evidence before submitting when policy requires it.</p></div>
          <label className={styles.fileField}>Choose a private file<input type="file" accept="image/jpeg,image/png,image/webp,application/pdf" onChange={(event) => setFile(event.target.files?.[0] ?? null)} /></label>
          {file && <button type="button" className="button secondary" disabled={busy} onClick={() => void upload()}>{busy ? "Uploading…" : "Upload / retry " + file.name}</button>}
          {uploaded && <p role="status" className={styles.savedMessage}>{uploaded}</p>}
          {!hasEvidence && <p className={styles.fieldHint}>No evidence is attached yet. Save this request as a draft, attach a file, then review it again.</p>}
        </section>
      ) : (
        <p className={styles.formHint}>If policy requires evidence, save a draft first. You can upload a private file from the draft before submission.</p>
      ))}

      {preview && (
        <section className={styles.reviewSection} aria-labelledby="leave-review-title">
          <div className={styles.reviewHeading}>
            <div><p className={styles.eyebrow}>Step 2 · Review</p><h2 id="leave-review-title">Check the calculation</h2></div>
            <strong>{preview.units} working day{preview.units === 1 ? "" : "s"}</strong>
          </div>
          <p className={styles.reviewPolicy}>{preview.leaveType.name} · {preview.policy.name} version {preview.policy.version} · {preview.policy.timezone}</p>
          <div className={styles.reviewFacts}>
            {preview.accounts.map((account) => (
              <div key={account.year}><span>{account.year}</span><strong>{account.units} requested</strong><small>{preview.leaveType.tracked ? formatUiNumber(Number(account.available), { maximumFractionDigits: 1 }) + " available → " + formatUiNumber(Number(account.available - account.units), { maximumFractionDigits: 1 }) + " remaining" : "Untracked entitlement"}</small></div>
            ))}
          </div>
          <details className={styles.calculationDetails}>
            <summary>Working dates and portions · {preview.days.length}</summary>
            <ul>{preview.days.map((day) => <li key={day.date}>{day.date} · {day.segment.replaceAll("_", " ")} · {day.source.replaceAll("_", " ")}</li>)}</ul>
          </details>
          {!!preview.approverRoute.length && <div className={styles.approvalRoute}><h3>Review route</h3><ol>{preview.approverRoute.map((step) => <li key={step.userId}><span>{step.stage}</span><strong>{step.name}</strong></li>)}</ol></div>}
          {preview.evidenceRequired && <div className={styles.evidenceNotice}><strong>Evidence required</strong><span>{preview.evidenceNote || (preview.evidence.dueStage === "before_final_approval" ? "You may submit now. Final approval waits for the required clean evidence." : "Save a draft and attach supporting evidence before submitting.")}</span></div>}
          {!!preview.blockers.length && <div className={styles.blockers} role="alert"><strong>This request needs attention</strong>{preview.blockers.map((blocker) => <p key={blocker}>{blocker}</p>)}</div>}
          <div className={styles.reviewActions}>
            {(preview.canSubmit && maySubmit && (!evidenceRequiredBeforeSubmit || hasEvidence)) && (
              <button type="button" className="button" disabled={busy} onClick={() => void save(false)}>{busy ? "Submitting…" : "Submit leave request"}</button>
            )}
            {evidenceRequiredBeforeSubmit && !hasEvidence && (
              <button type="button" className="button secondary" disabled={busy} onClick={() => void save(true)}>Save draft to attach evidence</button>
            )}
            {!preview.canSubmit && <span className={styles.reviewHelp}>Resolve the blockers above, then review the request again.</span>}
            {preview.canSubmit && !maySubmit && <span className={styles.reviewHelp}>This request is not currently open for submission.</span>}
          </div>
        </section>
      )}

      {error && (
        <div className={styles.errorBox} role="alert">
          <p>{error}</p>
          {record && error.toLowerCase().includes("changed") && <button type="button" className="button secondary" onClick={() => { router.refresh(); setPreview(null); setError("The latest version was requested. Your edits are retained; review the calculation again before submitting."); }}>Load latest version and keep edits</button>}
        </div>
      )}
    </section>
  );
}

export function LeaveDecisions({ base, record }: { base: string; record: LeaveRequest }) {
  const router = useRouter();
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const attempt = useRef<{ body: string; key: string } | null>(null);

  async function send(action: string) {
    setBusy(true);
    setError("");
    const cancellation = action.endsWith("_cancellation");
    const body = JSON.stringify({
      expectedVersion: record.version,
      note,
      ...(cancellation ? { decision: action === "approve_cancellation" ? "approve" : "reject" } : action === "approve" ? {} : { action }),
    });
    const signature = action + body;
    if (attempt.current?.body !== signature) attempt.current = { body: signature, key: crypto.randomUUID() };
    try {
      const response = await workFetch("/api/v1" + base + "/leave-requests/" + record.id + "/" + (cancellation ? "cancellation-review" : action === "approve" ? "approve" : "transition"), {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to record this decision.");
      attempt.current = null;
      window.dispatchEvent(new Event("erpx:work-changed"));
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Connection interrupted. Retry this action.");
    } finally {
      setBusy(false);
    }
  }

  const actions = record.permittedActions.filter((action) => !["save", "submit"].includes(action));
  if (!actions.length) return null;
  return (
    <section className={styles.decisionSection}>
      <div><p className={styles.eyebrow}>Available action</p><h2>Review this request</h2><p>Choose an action permitted for your current role and approval stage.</p></div>
      <label className={styles.field}>Decision or cancellation reason<textarea className="input" value={note} maxLength={2048} onChange={(event) => setNote(event.target.value)} /></label>
      <div className={styles.formActions}>
        {actions.map((action) => {
          const requiresNote = ["reject", "reject_cancellation"].includes(action) || action === "cancel" && record.state === "approved";
          const label = action === "cancel"
            ? record.state === "approved" ? "Request cancellation review" : record.state === "pending_approval" ? "Withdraw request" : "Cancel request"
            : action.replaceAll("_", " ").replace(/^./, (letter) => letter.toUpperCase());
          return <button key={action} type="button" className="button secondary" disabled={busy || requiresNote && !note.trim()} onClick={() => void send(action)}>{busy ? "Saving…" : label}</button>;
        })}
      </div>
      {error && <p role="alert" className={styles.errorBox}>{error}</p>}
    </section>
  );
}
