"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect,useRef, useState } from "react";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import {useWorkforceCommand} from './use-workforce-command';
import {WorkforceDraftBoundary} from './workforce-draft-boundary';
import type {WorkforcePurchase} from '@/lib/people_payroll/workforce-controls';
import type {WorkforceCommand} from '@/lib/people_payroll/workforce-command-client';
import { useRouter } from "@/lib/shared/destination-router";
import Link from "@/components/workplace/workspace-link";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { overtimeLabels, type OvertimeDetails } from "@/lib/people_payroll/overtime";
import type { OvertimeDetail, previewOvertime } from "@/server/modules/people_payroll/public/overtime";
import { ApprovalHistory } from "@/components/workflow/approval-history";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import styles from "@/components/people_payroll/self-service/self-service.module.css";
type Preview = Awaited<ReturnType<typeof previewOvertime>>;
export function OvertimeForm({
  base,
  record,
  initial,
  workContext,
  purchase,
}: {
  base: string;
  record?: OvertimeDetail;
  initial?: Partial<OvertimeDetails>;
  workContext:string;
  purchase:WorkforcePurchase|undefined;
}) {
  const router = useRouter(),
    [details, setDetails] = useState<OvertimeDetails>(
      record?.details ?? {
        mode: "advance",
        startLocal: "",
        endLocal: "",
        timezone: "Asia/Bangkok",
        breakMinutes: 0,
        reason: "",
        lateExplanation: "",
        rosterShiftId: null,
        ...initial,
      },
    );
  const [preview, setPreview] = useState<Preview | null>(null),
    [calculating, setBusy] = useState(false),
    [error, setError] = useState(""),
    [note, setNote] = useState(""),
    [saved, setSaved] = useState(false);
  const [attachments, setAttachments] = useState<Record<string, AttachmentPanelChange>>({});
  const command=useWorkforceCommand(base,`${workContext}:overtime:${record?.id??"new"}`,purchase),busy=calculating||command.busy;
  const detailsRef=useRef(details),noteRef=useRef(note),previewSequence=useRef(0),[dirty,setDirty]=useState(false);
  useEffect(()=>{detailsRef.current=details;noteRef.current=note;},[details,note]);
  const
    actions = record?.permittedActions ?? ["save"],
    editable = !record || actions.includes("save"),
    actual = record && ["authorized", "returned"].includes(record.state);
  const canChange=editable&&command.allowed(actual?'actuals':'save');
  const submissionPurposes =
    record && editable
      ? actual
        ? ["actual_evidence", "exception_evidence"]
        : details.mode === "advance"
          ? ["planned_evidence"]
          : ["actual_evidence", "exception_evidence"]
      : [];
  const attachmentPending = submissionPurposes.some((purpose) => {
    const change = attachments[purpose];
    return Boolean(
      change?.links.some((link) => !["withdrawn", "expired"].includes(link.linkState)) &&
      !change.bindable,
    );
  });
  const attachmentSetIds = submissionPurposes.flatMap((purpose) => {
    const change = attachments[purpose];
    return change?.setId && change.readyCount > 0 && change.bindable ? [change.setId] : [];
  });
  const set = <K extends keyof OvertimeDetails>(key: K, value: OvertimeDetails[K]) => {
    previewSequence.current++;setDirty(true);detailsRef.current={...detailsRef.current,[key]:value};
    setDetails((d) => ({ ...d, [key]: value }));
    setPreview(null);
    setSaved(false);
  };
  async function calculate() {
    if(!command.allowed(actual?'actuals':'save'))return;const sequence=++previewSequence.current;
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/overtime-requests/preview`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...details, actual: !!actual, id: record?.id }),
      },undefined,'caller');
      const b = await r.json();
      if (!r.ok) throw Error(b.error?.message);
      if(sequence===previewSequence.current)setPreview(b.data);
    } catch (e) {
      if(sequence===previewSequence.current)setError(e instanceof Error ? e.message : "Unable to calculate hours.");
    } finally {
      setBusy(false);
    }
  }
  async function send(action: string) {
    setError("");
    const input =
      record
        ? {
            action,
            expectedVersion: record.version,
            details: editable ? details : undefined,
            note,
            attachmentSetIds: ["submit", "actuals"].includes(action) ? attachmentSetIds : [],
          }
        : details;
    const result=await command.send({kind:'overtime',action:record?action:'create',...(record?{id:record.id}:{}),input});if(result)await acknowledged(result.request,result.receipt.id);
  }
  async function acknowledged(request:WorkforceCommand,id:string){
    const input=request.input,unchanged=request.id?(!input.details||JSON.stringify(input.details)===JSON.stringify(detailsRef.current))&&input.note===noteRef.current:JSON.stringify(input)===JSON.stringify(detailsRef.current);
    if(unchanged){
      setSaved(true);
      setDirty(false);
      setPreview(null);
      setAttachments({});
      if (!record) router.replace(canonicalUiHref(`${base}/overtime/${id}`));
      else router.refresh();
    }else command.setNotice('The submitted overtime action was saved. Your newer draft is retained; load the saved version before submitting it.');
  }
  const editor=(
      <Card as="section" appearance="standard" className={styles.panel}>
        <h2>{actual ? "Submit actual hours" : editable ? "1. Enter details" : "Worked time"}</h2>
        <p className={styles.muted}>
          {actual
            ? "Enter what you actually worked. Both reviews restart when actuals are submitted."
            : "An extra roster shift does not authorize overtime. Planned work and worked hours are reviewed separately."}
        </p>
        <form
          className={styles.form}
          onSubmit={(e) => {
            e.preventDefault();
            void calculate();
          }}
        >
          <div className={styles.fields}>
            <Field appearance="standard">
              Request path
              <Select appearance="standard"
                disabled={!!record || busy||!canChange}
                value={details.mode}
                onChange={(e) => set("mode", e.target.value as OvertimeDetails["mode"])}
              >
                <option value="advance">Advance authorization</option>
                <option value="retrospective">Retrospective claim</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Timezone
              <TextInput appearance="standard"
                required
                disabled={!canChange || busy}
                value={details.timezone}
                onChange={(e) => set("timezone", e.target.value)}
                list="overtime-timezones"
              />
              <datalist id="overtime-timezones">
                <option>Asia/Bangkok</option>
                <option>UTC</option>
                <option>America/New_York</option>
              </datalist>
            </Field>
            <Field appearance="standard">
              {actual || details.mode === "retrospective" ? "Actual start" : "Planned start"}
              <TextInput appearance="standard"
                type="datetime-local"
                required
                disabled={!canChange || busy}
                value={details.startLocal}
                onChange={(e) => set("startLocal", e.target.value)}
              />
            </Field>
            <Field appearance="standard">
              {actual || details.mode === "retrospective" ? "Actual end" : "Planned end"}
              <TextInput appearance="standard"
                type="datetime-local"
                required
                disabled={!canChange || busy}
                value={details.endLocal}
                onChange={(e) => set("endLocal", e.target.value)}
              />
            </Field>
          </div>
          <Field appearance="standard">
            Unpaid break (minutes)
            <TextInput appearance="standard"
              type="number"
              required
              min="0"
              max="1439"
              disabled={!canChange || busy}
              value={details.breakMinutes}
              onChange={(e) => set("breakMinutes", Number(e.target.value))}
            />
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard"
              required
              minLength={3}
              maxLength={2048}
              disabled={!canChange || busy}
              value={details.reason}
              onChange={(e) => set("reason", e.target.value)}
            />
          </Field>
          {details.mode === "retrospective" && (
            <Field appearance="standard">
              Late-submission explanation
              <TextArea appearance="standard"
                required
                maxLength={2048}
                disabled={!canChange || busy}
                value={details.lateExplanation}
                onChange={(e) => set("lateExplanation", e.target.value)}
              />
            </Field>
          )}
          {details.rosterShiftId && (
            <p className={styles.muted}>
              Linked to your roster shift.{" "}
              <Link href={canonicalUiHref(`${base}/people/roster?view=mine`)}>View schedule</Link>
            </p>
          )}
          {editable && (
            <div className={styles.actions}>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy||!canChange}>
                Review calculated hours
              </Button>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={
                  busy ||
                  !command.allowed('save')||
                  !details.startLocal ||
                  !details.endLocal ||
                  details.reason.trim().length < 3
                }
                onClick={() => void send("save")}
              >
                Save draft
              </Button>
              <span role="status">{saved ? "Saved to server" : ""}</span>
            </div>
          )}
        </form>
        {record &&
          submissionPurposes.map((purpose) => (
            <AttachmentPanel appearance="standard"
              disabled={!canChange||busy}
              readOnly={!canChange}
              key={`${record.id}:${record.version}:${purpose}`}
              base={base}
              targetType="overtime_request"
              targetId={record.id}
              expectedTargetVersion={record.version}
              componentKind="action"
              componentRef={actual ? "actual-hours" : "request-submission"}
              purposeKey={purpose}
              title={
                {
                  planned_evidence: "Planned overtime evidence",
                  actual_evidence: "Actual-hours evidence",
                  exception_evidence: "Exception evidence",
                }[purpose]
              }
              description={
                {
                  planned_evidence:
                    "Optionally attach the roster, work order, or other document supporting the planned overtime.",
                  actual_evidence:
                    "Optionally attach a timesheet, work record, or photo supporting the actual hours.",
                  exception_evidence:
                    "Optionally attach evidence explaining an exception or retrospective submission.",
                }[purpose]
              }
              visibility="Visible only to the requester and current eligible overtime reviewers."
              onChange={(change) =>
                setAttachments((current) => ({ ...current, [purpose]: change }))
              }
            />
          ))}
        {preview && (
          <section className={styles.preview}>
            <h2>2. Review & submit</h2>
            <p className={styles.metric}>
              {preview.hours.toLocaleString(undefined, { maximumFractionDigits: 2 })} hours
            </p>
            <p>
              {preview.breakMinutes} minutes unpaid break · {preview.timezone}
            </p>
            <ol>
              {preview.reviewers.map((r) => (
                <li key={r.userId}>
                  {r.stage}: {r.name}
                </li>
              ))}
            </ol>
            {preview.blockers.map((b) => (
              <p className={styles.error} key={b}>
                {b}
              </p>
            ))}
            <p className={styles.muted}>
              HR can resolve eligibility and reporting setup in People settings.
            </p>
            {attachmentPending && (
              <p className={styles.error}>
                Wait for every selected evidence file to pass security scanning before submitting.
              </p>
            )}
            {!record ? (
              <Button appearance="standard"
                type="submit"
                variant="primary"
                disabled={busy||!command.allowed('create')}
                onClick={() => void send("save")}
              >
                Save draft to continue
              </Button>
            ) : (
              <Button appearance="standard"
                type="submit"
                variant="primary"
                disabled={busy || !command.allowed(actual?'actuals':'submit')|| !preview.canSubmit || attachmentPending}
                onClick={() => void send(actual ? "actuals" : "submit")}
              >
                {actual
                  ? "Submit actual hours"
                  : details.mode === "advance"
                    ? "Request authorization"
                    : "Submit claim"}
              </Button>
            )}
          </section>
        )}
        {(error||command.error) && (
          <InlineNotice tone="danger" role="alert" className={styles.error}>
            <p>{error||command.error}</p>
            {record && (error||command.error).includes("changed") && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() => {
                  router.refresh();
                  setPreview(null);
                  setError(
                    "Latest version requested. Your edits are retained; calculate again before submitting.",
                  );
                }}
              >
                Load latest version and keep edits
              </Button>
            )}
          </InlineNotice>
        )}
      </Card>
  );
  return (
    <WorkforceDraftBoundary dirty={dirty||Boolean(command.pending)||attachmentSetIds.length>0}>
      {command.notice&&<InlineNotice role="status">{command.notice}</InlineNotice>}
      {command.pending&&<InlineNotice role="status"><p>The original overtime action has an uncertain result. Your newer draft is retained.</p><Button appearance="standard" disabled={busy} onClick={async()=>{const result=await command.recover();if(result)await acknowledged(result.request,result.receipt.id);}}>Check original action</Button></InlineNotice>}
      {record && (
        <Card as="section" appearance="standard" className={styles.panel}>
          <h2>{overtimeLabels[record.state]}</h2>
          <p>
            Responsible:{" "}
            {record.approval.currentApprover?.approver_name ??
              (record.state === "approved" ? "Payroll preview" : "Requester")}
          </p>
          {record.approval.blockedReason && (
            <InlineNotice tone="danger" role="alert">
              {record.approval.blockedReason} Ask HR to review{" "}
              <Link href={canonicalUiHref(`${base}/people/settings`)}>People settings</Link>.
            </InlineNotice>
          )}
          <div className={styles.progress}>
            {(record.mode === "advance"
              ? [
                  "Plan",
                  "Manager authorization",
                  "Actual hours",
                  "Manager verification",
                  "Independent HR",
                  "Payroll handoff",
                ]
              : [
                  "Actual hours & explanation",
                  "Manager verification",
                  "Independent HR",
                  "Payroll handoff",
                ]
            ).map((s) => (
              <span key={s}>{s}</span>
            ))}
          </div>
          {record.decision_note && (
            <p>
              <strong>Reviewer’s note:</strong> {record.decision_note}
            </p>
          )}
          <ApprovalHistory approval={record.approval} />
          {record.handoff && (
            <p>
              Approved hours have been handed off to payroll preview. Payment is handled separately.
            </p>
          )}
        </Card>
      )}
      {record&&editable?<FormDisclosure appearance="standard" drawerTitle={actual?"Record actual overtime hours":"Edit overtime request"} dismissDisabled={busy} error={error||command.error} onDiscard={()=>{setDetails(record.details);detailsRef.current=record.details;setPreview(null);setAttachments({});setDirty(Boolean(noteRef.current));previewSequence.current++;}}><summary>{actual?"Record actual hours":"Edit overtime request"}</summary>{editor}</FormDisclosure>:editor}
      {record && actions.some((a) => !["save", "submit", "actuals"].includes(a)) && (
        <Card as="section" appearance="standard" className={styles.panel}>
          <Field appearance="standard">
            Decision note
            <TextArea appearance="standard"
              className="input"
              value={note}
              onChange={(e) => {noteRef.current=e.target.value;setDirty(true);setNote(e.target.value);}}
              maxLength={2048}
            />
          </Field>
          <div className={styles.actions}>
            {actions
              .filter((a) => !["save", "submit", "actuals"].includes(a))
              .map((a) => (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  key={a}
                  disabled={busy || !command.allowed(a)|| (["return", "reject"].includes(a) && !note.trim())}
                  onClick={() => void send(a)}
                >
                  {
                    {
                      authorize: "Authorize planned work",
                      verify: "Verify actual hours",
                      approve: "Approve hours & hand off",
                      return: "Return for correction",
                      reject: "Reject request",
                      withdraw: "Withdraw request",
                    }[a]
                  }
                </Button>
              ))}
          </div>
        </Card>
      )}
    </WorkforceDraftBoundary>
  );
}
