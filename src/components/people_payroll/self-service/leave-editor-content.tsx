"use client";
import WorkspaceLink from '@/components/workplace/workspace-link';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RequestValidation } from "@/components/workplace/request-validation";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useId, useMemo, useRef, useState } from "react";
import {createLeaveEvidenceClient} from "@/lib/people_payroll/workforce-evidence-client";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import {useWorkforceCommand} from "./use-workforce-command";
import {WorkforceDraftBoundary} from "./workforce-draft-boundary";
import type {WorkforcePurchase} from "@/lib/people_payroll/workforce-controls";
import type {WorkforceCommand} from "@/lib/people_payroll/workforce-command-client";
import { useRouter } from "@/lib/shared/destination-router";
import { RequestSteps } from "@/components/workplace/request-experience";
import { ReviewDecisionForm } from "@/components/workplace/review-decision";
import type { ReviewDecision } from "@/lib/work_projection/work-review";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { LeaveRequest } from "@/server/modules/people_payroll/public/leave";
import type { leaveSummary, previewLeave } from "@/server/modules/people_payroll/public/leave-self-service";
import type { LeaveSegment } from "@/lib/people_payroll/leave-self-service";
import styles from "@/components/people_payroll/leave/leave-flow.module.css";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/shared/ai/guided-intake";
import { formatUiNumber } from "@/lib/shared/ui-format";

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
  workContext,
  purchase,
  drawer = false,
}: {
  base: string;
  workContext:string;
  purchase:WorkforcePurchase|undefined;
  drawer?:boolean;
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
  const recovered = useRecoverableDraft<Draft>(`erpx:leave-draft:${workContext}:${record?.id ?? "new"}`, {
    leaveTypeId: record?.leave_type_id ?? "",
    startDate: record?.start_date ?? initialDate,
    endDate: record?.end_date ?? record?.start_date ?? initialDate,
    startSegment: record?.start_segment ?? "full",
    endSegment: record?.end_segment ?? "full",
    handoverNote: record?.handover_note ?? "",
  }, "leave request");
  const draft = recovered.fields, setDraft = recovered.update;
  const [createdId,setCreatedId] = useState<string|null>(null);
  const command=useWorkforceCommand(base,`${workContext}:leave:${record?.id??"new"}:editor`,purchase);
  const draftRef=useRef(draft),previewSequence=useRef(0);
  useEffect(()=>{draftRef.current=draft;},[draft]);
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
  const [working, setBusy] = useState(false);
  const busy=working||command.busy;
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [uploaded, setUploaded] = useState("");
  const [hasEvidence, setHasEvidence] = useState(evidencePresent);

  const evidenceClient=useMemo(()=>createLeaveEvidenceClient(base,`${workContext}:leave:evidence`),[base,workContext]);
  const pendingEvidence=evidenceClient.pending();
  const selectedType = types.find((item) => item.type.id === draft.leaveTypeId);
  const maySubmit = !record || record.permittedActions.includes("submit");
  const evidenceRequiredBeforeSubmit = Boolean(
    preview?.evidenceRequired && preview.evidence.dueStage === "before_submit",
  );

  function set<K extends keyof Draft>(key: K, value: Draft[K]) {
    previewSequence.current++;draftRef.current={...draftRef.current,[key]:value};
    setDraft((current) => ({ ...current, [key]: value }));
    setPreview(null);
    setSaved(false);
  }

  async function calculate() {
    if(!command.allowed("save")||recovered.conflict)return;const sequence=++previewSequence.current;
    setBusy(true);
    setError("");
    try {
      const response = await workFetch("/api/v1" + base + "/leave-requests/preview", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...draft, id: record?.id }),
      },undefined,"caller");
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to calculate this request.");
      if(sequence===previewSequence.current)setPreview(body.data as Preview);
    } catch (cause) {
      if(sequence===previewSequence.current)setError(cause instanceof Error ? cause.message : "Unable to calculate this request.");
    } finally {
      setBusy(false);
    }
  }

  const destination=(id:string)=>canonicalUiHref(base+"/tasks/people_payroll/"+id+"?from=leave"+(returnTo?"&return="+encodeURIComponent(returnTo):""));
  async function complete(request:WorkforceCommand,id:string){
    const submitted=request.input,unchanged=Object.keys(draftRef.current).every(key=>draftRef.current[key as keyof Draft]===submitted[key]);
    if(!record)setCreatedId(id);
    if(unchanged){recovered.clear();setSaved(true);setPreview(null);if(!record)router.replace(destination(id));else router.refresh();}
    else command.setNotice("The submitted leave action was saved. Your newer draft is retained; review the saved version before submitting it.");
  }
  async function save(saveOnly: boolean) {
    if(busy||recovered.conflict||!command.allowed("save"))return;
    if(createdId&&!record){router.replace(destination(createdId));return;}
    setBusy(true);setError("");const submitted={...draftRef.current},selectedFile=file;
    try {
      const result=await command.send({kind:"leave",action:record?(saveOnly||selectedFile?"save":"submit"):"create",...(record?{id:record.id}:{}),input:{...submitted,saveOnly:selectedFile?true:saveOnly,...(record?{expectedVersion:record.version}:{})}});
      if(!result)return;
      const savedId=result.receipt.id;if(!record)setCreatedId(savedId);
      if(selectedFile){
        await uploadFile(savedId,selectedFile);
        if(!saveOnly){
          const latest=await workFetch(`/api/v1${base}/leave-requests/${savedId}`,{cache:"no-store"}),body=await latest.json();
          if(!latest.ok||body.data?.id!==savedId||!Number.isInteger(body.data?.version))throw Error("The draft and evidence were saved. Open the saved draft to review submission.");
          const sent=await command.send({kind:"leave",action:"submit",id:savedId,input:{...submitted,saveOnly:false,expectedVersion:body.data.version}});if(!sent)return;
        }
      }
      await complete(result.request,savedId);
    }catch(cause){setError(cause instanceof Error?cause.message:"Connection interrupted. Your draft is retained.");}
    finally{setBusy(false);}
  }
  async function uploadFile(id:string,selectedFile:File){
    await evidenceClient.upload(id,selectedFile,preview?.evidence.purposeKey??snapshotEvidencePurpose(record)??"leave_supporting_photo");
    setHasEvidence(true);setUploaded(selectedFile.name+" · uploaded and verified");setFile(null);
  }

  async function upload() {
    if(!file||!record||busy||!command.allowed("save"))return;
    setBusy(true);setError("");try{await uploadFile(record.id,file);router.refresh();}
    catch(cause){setError(cause instanceof Error?cause.message:"Evidence upload interrupted. Retry the same file.");}finally{setBusy(false);}
  }
  const contents=(

    <WorkforceDraftBoundary dirty={Boolean(command.pending)||Boolean(file)||evidenceClient.pending()}><section className={styles.formShell}>
      {command.notice&&<InlineNotice role="status">{command.notice}</InlineNotice>}
      {command.pending&&<InlineNotice role="status"><p>The original leave action has an uncertain result. Your newer draft is retained.</p><Button appearance="standard" disabled={busy} onClick={async()=>{const result=await command.recover();if(result){if(file||evidenceClient.pending()){setCreatedId(result.receipt.id);command.setNotice("The original draft was saved. Your selected evidence is retained here; open the saved request to finish evidence and submission.");}else await complete(result.request,result.receipt.id);}}}>Check original action</Button></InlineNotice>}
      <RequestSteps reviewing={!!preview} />
      {recovered.notice}
      {pendingEvidence&&<InlineNotice role="status"><p>The original evidence upload is still pending. Choose the same private file to check it; private bytes are kept only in memory.</p>{evidenceClient.target()&&<WorkspaceLink href={destination(evidenceClient.target()!)}>Open the original saved request</WorkspaceLink>}</InlineNotice>}

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
          <Select disabled={busy||!command.allowed("save")} appearance="standard"
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
              <TextInput disabled={busy||!command.allowed("save")} appearance="standard"
                required
                type="date"
                value={draft.startDate}
                onChange={(event) => set("startDate", event.target.value)}
              />
            </Field>
            <Field appearance="standard" className={styles.field}>
              Portion
              <Select disabled={busy||!command.allowed("save")} appearance="standard"
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
              <TextInput disabled={busy||!command.allowed("save")} appearance="standard"
                required
                type="date"
                value={draft.endDate}
                onChange={(event) => set("endDate", event.target.value)}
              />
            </Field>
            <Field appearance="standard" className={styles.field}>
              Portion
              <Select disabled={busy||!command.allowed("save")} appearance="standard"
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
          <TextArea disabled={busy||!command.allowed("save")} appearance="standard"
            maxLength={2048}
            value={draft.handoverNote}
            onChange={(event) => set("handoverNote", event.target.value)}
          />
          <span className={styles.fieldHint}>
            Share anything your team needs to keep work moving.
          </span>
        </Field>
      </form></RequestValidation>

      {!preview && (!externalEvidence||pendingEvidence) &&
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
              <TextInput disabled={busy||!command.allowed("save")} appearance="standard"
                type="file"
                accept="image/jpeg,image/png,image/webp,application/pdf"
                onChange={(event) => setFile(event.target.files?.[0] ?? null)}
              />
            </Field>
            {file && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy||!command.allowed("save")}
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
          <section className={styles.evidenceSection}><h3>Supporting evidence</h3><p>Optional unless your policy requires it. Files remain private with this request.</p><Field appearance="standard">Choose a private file<TextInput disabled={busy||!command.allowed("save")} appearance="standard" type="file" accept="image/jpeg,image/png,image/webp,application/pdf" onChange={e=>setFile(e.target.files?.[0]??null)} /></Field>{file&&<p>{file.name} · uploaded only when you save or send</p>}</section>
        ))}

      {!preview && <>        <div className={`request-footer ${styles.formActions}`}>
          <Button appearance="standard"
            clientManaged variant="secondary"
            type="submit" form={formId}
            disabled={busy || !command.allowed("save") || recovered.conflict || !draft.leaveTypeId || !draft.startDate || !draft.endDate}
          >
            {busy ? "Working…" : "Review request"}
          </Button>
          <Button appearance="standard"
            variant="ghost"
            type="button"
            disabled={busy || !command.allowed("save") || recovered.conflict || !draft.leaveTypeId || !draft.startDate || !draft.endDate}
            form={formId} onClick={() => void save(true)}
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
            <Button appearance="standard" type="button" disabled={busy||!command.allowed("save")} onClick={()=>setPreview(null)}>Edit details</Button>
            {preview.canSubmit && maySubmit && (!evidenceRequiredBeforeSubmit || hasEvidence || !!file) && (
              <Button appearance="standard"
                variant="primary"
                type="button"
                disabled={busy||!command.allowed("save")}
                form={formId} onClick={() => void save(false)}
              >
                {busy ? "Submitting…" : "Submit leave request"}
              </Button>
            )}
            {evidenceRequiredBeforeSubmit && !hasEvidence && !file && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy||!command.allowed("save")}
                form={formId} onClick={() => void save(true)}
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

      {createdId && (error||command.notice) && <p><WorkspaceLink href={canonicalUiHref(`${base}/tasks/people_payroll/${createdId}?from=leave`)}>Open saved draft and retry evidence / submission</WorkspaceLink></p>}
      {(error||command.error) && (
        <InlineNotice tone="danger" className={styles.errorBox} role="alert">
          <p>{error||command.error}</p>
          {record && (error||command.error).toLowerCase().includes("changed") && (
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
    </section></WorkforceDraftBoundary>
  );
  return drawer?<FormDisclosure appearance="standard" drawerTitle="Edit leave request" dismissDisabled={busy} error={error||command.error}><summary>Edit leave request</summary>{contents}</FormDisclosure>:contents;
}

export function LeaveDecisions({ base, record, workContext, purchase }: { base: string; record: LeaveRequest;workContext:string;purchase:WorkforcePurchase|undefined }) {
  const router=useRouter(),command=useWorkforceCommand(base,`${workContext}:leave:${record.id}:decisions`,purchase);
  const [note,setNote]=useState("");const noteRef=useRef(note);const {busy,error}=command;
  async function complete(request:WorkforceCommand){if(request.input.note===noteRef.current){setNote("");noteRef.current="";router.refresh();}else command.setNotice("The decision was saved. Your newer note is retained.");}
  async function send(action:string){const result=await command.send({kind:"leave",action,id:record.id,input:{expectedVersion:record.version,note,...(action.endsWith("_cancellation")?{decision:action==="approve_cancellation"?"approve":"reject"}:action==="approve"?{}:{action})}});if(result)await complete(result.request);}
  function updateNote(value:string){noteRef.current=value;setNote(value);}
  const actions = record.permittedActions.filter((action) => !["save", "submit"].includes(action));
  if (!actions.length&&!command.pending) return null;
  return (
    <WorkforceDraftBoundary dirty={Boolean(note)||Boolean(command.pending)}><section className={styles.decisionSection}>
      {command.notice&&<InlineNotice role="status">{command.notice}</InlineNotice>}
      {command.pending&&<InlineNotice role="status"><p>Check the original leave action before another decision.</p><Button appearance="standard" disabled={busy} onClick={async()=>{const result=await command.recover();if(result)await complete(result.request);}}>Check original action</Button></InlineNotice>}
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
          onChange={(event) => updateNote(event.target.value)}
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
              disabled={busy || !command.allowed(action) || (requiresNote && !note.trim())}
              onClick={() => void send(action)}
            >
              {busy ? "Saving…" : label}
            </Button>
          );
        })}
      </div>
      {actions.some(a=>["approve","reject"].includes(a)) && <ReviewDecisionForm actions={actions.filter((a):a is ReviewDecision=>["approve","reject"].includes(a))} busy={busy||!command.allowed("approve")} note={note} onNote={updateNote} minimum={a=>a === "approve" ? 0 : 2} onConfirm={a=>void send(a)} />}
      {error && (
        <InlineNotice tone="danger" role="alert" className={styles.errorBox}>
          {error}
        </InlineNotice>
      )}
    </section></WorkforceDraftBoundary>
  );
}
