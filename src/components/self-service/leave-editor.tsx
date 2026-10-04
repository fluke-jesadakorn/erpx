"use client";
import { RequestValidation } from "@/components/workplace/request-validation";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useId, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { RequestSteps } from "@/components/workplace/request-experience";
import { ReviewDecisionForm } from "@/components/workplace/review-decision";
import type { ReviewDecision } from "@/lib/work-review";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
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
    try {
      snapshot = JSON.parse(snapshot);
    } catch {
      return null;
    }
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
  returnTo = "",
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
  returnTo?: string;
}) {
  const router = useRouter();
  const recovered = useRecoverableDraft<Draft>(`erpx:leave-draft:${base}:${companyId}:${userId}:${record?.id ?? "new"}`, {
    leaveTypeId: record?.leave_type_id ?? "",
    startDate: record?.start_date ?? initialDate,
    endDate: record?.end_date ?? record?.start_date ?? initialDate,
    startSegment: record?.start_segment ?? "full",
    endSegment: record?.end_segment ?? "full",
    handoverNote: record?.handover_note ?? "",
  }, "leave request");
  const draft = recovered.fields, setDraft = recovered.update;
  const [createdId,setCreatedId] = useState<string|null>(null);
  const submitAttempt=useRef<{body:string;key:string}|null>(null);
  const [guided, setGuided] = useState<Record<string, string> | null>(null);
  useEffect(() => {
    if (record || !companyId || !userId || !assignmentId) return;
    const restore = () =>
      setGuided(readGuidedIntakeHandoff(companyId, userId, assignmentId, base, "leave"));
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [record, companyId, userId, assignmentId, base]);
  function applyGuided() {
    if (!guided || !companyId || !userId || !assignmentId) return;
    const namedType = types.find(
      (item) => item.type.name.toLocaleLowerCase() === guided.leaveType?.toLocaleLowerCase(),
    );
    setDraft((current) => ({
      ...current,
      leaveTypeId: namedType?.type.id ?? current.leaveTypeId,
      startDate: /^\d{4}-\d{2}-\d{2}$/.test(guided.startDate ?? "")
        ? guided.startDate
        : current.startDate,
      endDate: /^\d{4}-\d{2}-\d{2}$/.test(guided.endDate ?? "") ? guided.endDate : current.endDate,
    }));
    setPreview(null);
    clearGuidedIntakeHandoff(companyId, userId, assignmentId, base, "leave");
    setGuided(null);
  }
  const [preview, setPreview] = useState<Preview | null>(null);
  const formId=useId(),reviewHeading=useRef<HTMLHeadingElement>(null);
  useEffect(()=>{if(preview)reviewHeading.current?.focus();},[preview]);
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
    if(createdId && !record){router.replace(base+"/tasks/people_payroll/"+createdId+"?from=leave"+(returnTo?"&return="+encodeURIComponent(returnTo):""));setBusy(false);return;}
    const body = JSON.stringify({
      ...draft,
      saveOnly: file ? true : saveOnly,
      ...(record ? { expectedVersion: record.version } : {}),
    });
    const storage = "erpx:leave-command:" + base + ":" + (record?.id ?? "new");
    try {
      if (!attempt.current) {
        try {
          attempt.current = JSON.parse(sessionStorage.getItem(storage) ?? "null");
        } catch {
          /* Start a new command. */
        }
      }
      if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
      sessionStorage.setItem(storage, JSON.stringify(attempt.current));
      const response = await workFetch(
        "/api/v1" + base + "/leave-requests" + (record ? "/" + record.id : createdId ? "/" + createdId : ""),
        {
          method: record || createdId ? "PATCH" : "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        },
      );
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to save this request.");
      const savedId = record?.id ?? result.data.id;
      if(!record)setCreatedId(savedId);
      if(file){
        const signature = file.name+":"+file.size+":"+file.lastModified;
        if(uploadAttempt.current?.signature!==signature)uploadAttempt.current={signature,key:crypto.randomUUID()};
        if(!uploadAttempt.current.url){
          const info=await workFetch(`/api/v1${base}/leave-requests/${savedId}`,{cache:"no-store"});const infoBody=await info.json();if(!info.ok)throw Error(infoBody.error?.message??"Unable to read the saved draft.");
          const uploadResponse=await workFetch(`/api/v1${base}/uploads`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":uploadAttempt.current.key},body:JSON.stringify({ownerType:"leave_request",ownerId:savedId,purpose:preview?.evidence.purposeKey??snapshotEvidencePurpose(infoBody.data)??"leave_supporting_photo",filename:file.name,mediaType:file.type,bytes:file.size})});
          const uploadBody=await uploadResponse.json();if(!uploadResponse.ok)throw Error(uploadBody.error?.message??"Unable to upload evidence.");uploadAttempt.current.url=uploadBody.data.uploadUrl;
        }
        const uploadResponse=await workFetch(uploadAttempt.current.url!,{method:"PUT",headers:{"Content-Type":file.type},body:file});if(!uploadResponse.ok)throw Error("Evidence upload failed. Your draft is saved; open it to retry.");
        setHasEvidence(true);setFile(null);uploadAttempt.current=null;
        if(!saveOnly){
          const latest=await workFetch(`/api/v1${base}/leave-requests/${savedId}`,{cache:"no-store"});const latestBody=await latest.json();if(!latest.ok)throw Error("Unable to reload the saved draft.");
          const sendBody=JSON.stringify({...draft,saveOnly:false,expectedVersion:latestBody.data.version});
          if(submitAttempt.current?.body!==sendBody)submitAttempt.current={body:sendBody,key:crypto.randomUUID()};
          const sent=await workFetch(`/api/v1${base}/leave-requests/${savedId}`,{method:"PATCH",headers:{"Content-Type":"application/json","Idempotency-Key":submitAttempt.current.key},body:sendBody});const sentBody=await sent.json();if(!sent.ok)throw Error(sentBody.error?.message??"Evidence is saved. Review the draft before submitting again.");
        }
      }
      recovered.clear();
      sessionStorage.removeItem(storage);
      attempt.current = null;
      setSaved(true);
      setPreview(null);
      window.dispatchEvent(new Event("erpx:work-changed"));
      if (!record) router.replace(base + "/tasks/people_payroll/" + result.data.id + "?from=leave" + (returnTo ? "&return=" + encodeURIComponent(returnTo) : ""));
      else router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Connection interrupted. Your entered values are retained.",
      );
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
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": uploadAttempt.current.key,
          },
          body: JSON.stringify({
            ownerType: "leave_request",
            ownerId: record.id,
            purpose:
              preview?.evidence.purposeKey ??
              snapshotEvidencePurpose(record) ??
              "leave_supporting_photo",
            filename: file.name,
            mediaType: file.type,
            bytes: file.size,
          }),
        });
        const result = await response.json();
        if (!response.ok)
          throw new Error(result.error?.message ?? "Unable to start the evidence upload.");
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
      setError(
        cause instanceof Error ? cause.message : "Evidence upload interrupted. Retry this file.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={styles.formShell}>
      <RequestSteps reviewing={!!preview} />
      {recovered.notice}

      <RequestValidation><form id={formId} hidden={!!preview}
        className={styles.leaveForm}
        onSubmit={(event) => {
          event.preventDefault();
          void calculate();
        }}
      >
        <div className={styles.formIntro}>
          <h2>Leave details</h2>
          <p>Working days and balance use your assigned Company policy.</p>
        </div>
        {guided && (
          <InlineNotice tone="info" className="notice" role="status">
            <p>
              AI prepared dates from your message. Review the leave type, balance, and dates before
              continuing.
            </p>
            <Button appearance="standard" variant="secondary" type="button" onClick={applyGuided}>
              Use prepared details
            </Button>{" "}
            <Button appearance="standard"
              variant="ghost"
              type="button"
              onClick={() => {
                if (companyId && userId && assignmentId)
                  clearGuidedIntakeHandoff(companyId, userId, assignmentId, base, "leave");
                setGuided(null);
              }}
            >
              Discard suggestion
            </Button>
          </InlineNotice>
        )}
        <Field appearance="standard" className={styles.field}>
          Leave type
          <Select appearance="standard"
            required
            value={draft.leaveTypeId}
            onChange={(event) => set("leaveTypeId", event.target.value)}
          >
            <option value="">Choose a leave type</option>
            {types.map((item) => (
              <option key={item.type.id} value={item.type.id}>
                {item.type.name}
              </option>
            ))}
          </Select>
        </Field>
        {selectedType && (
          <InlineNotice tone="info" className={styles.selectedBalance} role="status">
            <span>{selectedType.type.name}</span>
            <strong>
              {selectedType.type.tracked
                ? formatUiNumber(Number(selectedType.available), { maximumFractionDigits: 1 }) +
                  " days available"
                : "Untracked entitlement"}
            </strong>
          </InlineNotice>
        )}
        <div className={styles.dateGroup}>
          <div className={styles.groupHeading}>
            <h3>Dates and day portions</h3>
            <p>Select half days at the beginning or end of your request.</p>
          </div>
          <div className={styles.dateFields}>
            <Field appearance="standard" className={styles.field}>
              First day
              <TextInput appearance="standard"
                required
                type="date"
                value={draft.startDate}
                onChange={(event) => set("startDate", event.target.value)}
              />
            </Field>
            <Field appearance="standard" className={styles.field}>
              Portion
              <Select appearance="standard"
                value={draft.startSegment}
                onChange={(event) => set("startSegment", event.target.value as LeaveSegment)}
              >
                <option value="full">Full day</option>
                <option value="first_half">First half</option>
                <option value="second_half">Second half</option>
              </Select>
            </Field>
            <Field appearance="standard" className={styles.field}>
              Last day
              <TextInput appearance="standard"
                required
                type="date"
                value={draft.endDate}
                onChange={(event) => set("endDate", event.target.value)}
              />
            </Field>
            <Field appearance="standard" className={styles.field}>
              Portion
              <Select appearance="standard"
                value={draft.endSegment}
                onChange={(event) => set("endSegment", event.target.value as LeaveSegment)}
              >
                <option value="full">Full day</option>
                <option value="first_half">First half</option>
                <option value="second_half">Second half</option>
              </Select>
            </Field>
          </div>
          <p className={styles.fieldHint}>
            Non-working days and assigned holidays are excluded from the total.
          </p>
        </div>
        <Field appearance="standard" className={styles.field}>
          Handover note
          <TextArea appearance="standard"
            maxLength={2048}
            value={draft.handoverNote}
            onChange={(event) => set("handoverNote", event.target.value)}
          />
          <span className={styles.fieldHint}>
            Share anything your team needs to keep work moving.
          </span>
        </Field>
      </form></RequestValidation>

      {!preview && !externalEvidence &&
        (record ? (
          <section className={styles.evidenceSection}>
            <div>
              <h3>Supporting evidence</h3>
              <p>
                Files stay private with this request. Attach evidence before submitting when policy
                requires it.
              </p>
            </div>
            <Field appearance="standard" className={styles.fileField}>
              Choose a private file
              <TextInput appearance="standard"
                type="file"
                accept="image/jpeg,image/png,image/webp,application/pdf"
                onChange={(event) => setFile(event.target.files?.[0] ?? null)}
              />
            </Field>
            {file && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy}
                onClick={() => void upload()}
              >
                {busy ? "Uploading…" : "Upload / retry " + file.name}
              </Button>
            )}
            {uploaded && (
              <InlineNotice tone="info" role="status" className={styles.savedMessage}>
                {uploaded}
              </InlineNotice>
            )}
            {!hasEvidence && (
              <p className={styles.fieldHint}>
                No evidence is attached yet. Save this request as a draft, attach a file, then
                review it again.
              </p>
            )}
          </section>
        ) : (
          <section className={styles.evidenceSection}><h3>Supporting evidence</h3><p>Optional unless your policy requires it. Files remain private with this request.</p><Field appearance="standard">Choose a private file<TextInput appearance="standard" type="file" accept="image/jpeg,image/png,image/webp,application/pdf" disabled={busy} onChange={e=>setFile(e.target.files?.[0]??null)} /></Field>{file&&<p>{file.name} · uploaded only when you save or send</p>}</section>
        ))}

      {!preview && <>        <div className={`request-footer ${styles.formActions}`}>
          <Button appearance="standard"
            clientManaged variant="secondary"
            type="submit" form={formId}
            disabled={busy || recovered.conflict || !draft.leaveTypeId || !draft.startDate || !draft.endDate}
          >
            {busy ? "Working…" : "Review request"}
          </Button>
          <Button appearance="standard"
            variant="ghost"
            type="button"
            disabled={busy || recovered.conflict || !draft.leaveTypeId || !draft.startDate || !draft.endDate}
            onClick={() => void save(true)}
          >
            Save draft
          </Button>
          {saved && (
            <span role="status" className={styles.savedMessage}>
              Saved to server.
            </span>
          )}
        </div>
</>}

      {preview && (
        <section className={styles.reviewSection} aria-labelledby="leave-review-title">
          <div className={styles.reviewHeading}>
            <div>
              <p className={styles.eyebrow}>Step 2 · Review</p>
              <h2 ref={reviewHeading} tabIndex={-1} id="leave-review-title">Check the calculation</h2>
            </div>
            <strong>
              {preview.units} working day{preview.units === 1 ? "" : "s"}
            </strong>
          </div>
          <p className={styles.reviewPolicy}>
            {preview.leaveType.name} · {preview.policy.name} version {preview.policy.version} ·{" "}
            {preview.policy.timezone}
          </p>
          <div className={styles.reviewFacts}>
            {preview.accounts.map((account) => (
              <div key={account.year}>
                <span>{account.year}</span>
                <strong>{account.units} requested</strong>
                <small>
                  {preview.leaveType.tracked
                    ? formatUiNumber(Number(account.available), { maximumFractionDigits: 1 }) +
                      " available → " +
                      formatUiNumber(Number(account.available - account.units), {
                        maximumFractionDigits: 1,
                      }) +
                      " remaining"
                    : "Untracked entitlement"}
                </small>
              </div>
            ))}
          </div>
          <details className={styles.calculationDetails}>
            <summary>Working dates and portions · {preview.days.length}</summary>
            <ul>
              {preview.days.map((day) => (
                <li key={day.date}>
                  {day.date} · {day.segment.replaceAll("_", " ")} ·{" "}
                  {day.source.replaceAll("_", " ")}
                </li>
              ))}
            </ul>
          </details>
          {!!preview.approverRoute.length && (
            <div className={styles.approvalRoute}>
              <h3>Review route</h3>
              <ol>
                {preview.approverRoute.map((step) => (
                  <li key={step.userId}>
                    <span>{step.stage}</span>
                    <strong>{step.name}</strong>
                  </li>
                ))}
              </ol>
            </div>
          )}
          {preview.evidenceRequired && (
            <div className={styles.evidenceNotice}>
              <strong>Evidence required</strong>
              <span>
                {preview.evidenceNote ||
                  (preview.evidence.dueStage === "before_final_approval"
                    ? "You may submit now. Final approval waits for the required clean evidence."
                    : "Save a draft and attach supporting evidence before submitting.")}
              </span>
            </div>
          )}
          {!!preview.blockers.length && (
            <InlineNotice tone="danger" className={styles.blockers} role="alert">
              <strong>This request needs attention</strong>
              {preview.blockers.map((blocker) => (
                <p key={blocker}>{blocker}</p>
              ))}
            </InlineNotice>
          )}
          {file && <p>Evidence selected: {file.name} · scanned and locked before submission.</p>}
          <div className={`request-footer ${styles.reviewActions}`}>
            <Button appearance="standard" type="button" disabled={busy} onClick={()=>setPreview(null)}>Edit details</Button>
            {preview.canSubmit && maySubmit && (!evidenceRequiredBeforeSubmit || hasEvidence || !!file) && (
              <Button appearance="standard"
                variant="primary"
                type="button"
                disabled={busy}
                onClick={() => void save(false)}
              >
                {busy ? "Submitting…" : "Submit leave request"}
              </Button>
            )}
            {evidenceRequiredBeforeSubmit && !hasEvidence && !file && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy}
                onClick={() => void save(true)}
              >
                Save draft to attach evidence
              </Button>
            )}
            {!preview.canSubmit && (
              <span className={styles.reviewHelp}>
                Resolve the blockers above, then review the request again.
              </span>
            )}
            {preview.canSubmit && !maySubmit && (
              <span className={styles.reviewHelp}>
                This request is not currently open for submission.
              </span>
            )}
          </div>
        </section>
      )}

      {createdId && error && <p><a href={`${base}/tasks/people_payroll/${createdId}?from=leave`}>Open saved draft and retry evidence / submission</a></p>}
      {error && (
        <InlineNotice tone="danger" className={styles.errorBox} role="alert">
          <p>{error}</p>
          {record && error.toLowerCase().includes("changed") && (
            <Button appearance="standard"
              variant="secondary"
              type="button"
              onClick={() => {
                router.refresh();
                setPreview(null);
                setError(
                  "The latest version was requested. Your edits are retained; review the calculation again before submitting.",
                );
              }}
            >
              Load latest version and keep edits
            </Button>
          )}
        </InlineNotice>
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
      ...(cancellation
        ? { decision: action === "approve_cancellation" ? "approve" : "reject" }
        : action === "approve"
          ? {}
          : { action }),
    });
    const signature = action + body;
    if (attempt.current?.body !== signature)
      attempt.current = { body: signature, key: crypto.randomUUID() };
    try {
      const response = await workFetch(
        "/api/v1" +
          base +
          "/leave-requests/" +
          record.id +
          "/" +
          (cancellation ? "cancellation-review" : action === "approve" ? "approve" : "transition"),
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body,
        },
      );
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to record this decision.");
      attempt.current = null;
      window.dispatchEvent(new Event("erpx:work-changed"));
      router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Connection interrupted. Retry this action.",
      );
    } finally {
      setBusy(false);
    }
  }

  const actions = record.permittedActions.filter((action) => !["save", "submit"].includes(action));
  if (!actions.length) return null;
  return (
    <section className={styles.decisionSection}>
      <div>
        <p className={styles.eyebrow}>Available action</p>
        <h2>Review this request</h2>
        <p>Choose an action permitted for your current role and approval stage.</p>
      </div>
      <Field appearance="standard" className={styles.field}>
        Decision or cancellation reason
        <TextArea appearance="standard"
          className="input"
          value={note}
          maxLength={2048}
          onChange={(event) => setNote(event.target.value)}
        />
      </Field>
      <div className={styles.formActions}>
        {actions.filter(a=>!["approve","reject"].includes(a)).map((action) => {
          const requiresNote =
            ["reject", "reject_cancellation"].includes(action) ||
            (action === "cancel" && record.state === "approved");
          const label =
            action === "cancel"
              ? record.state === "approved"
                ? "Request cancellation review"
                : record.state === "pending_approval"
                  ? "Withdraw request"
                  : "Cancel request"
              : action.replaceAll("_", " ").replace(/^./, (letter) => letter.toUpperCase());
          return (
            <Button appearance="standard"
              variant="secondary"
              key={action}
              type="button"
              disabled={busy || (requiresNote && !note.trim())}
              onClick={() => void send(action)}
            >
              {busy ? "Saving…" : label}
            </Button>
          );
        })}
      </div>
      {actions.some(a=>["approve","reject"].includes(a)) && <ReviewDecisionForm actions={actions.filter((a):a is ReviewDecision=>["approve","reject"].includes(a))} busy={busy} note={note} onNote={setNote} minimum={a=>a === "approve" ? 0 : 2} onConfirm={a=>void send(a)} />}
      {error && (
        <InlineNotice tone="danger" role="alert" className={styles.errorBox}>
          {error}
        </InlineNotice>
      )}
    </section>
  );
}
