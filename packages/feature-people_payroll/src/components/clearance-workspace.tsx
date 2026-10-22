"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";

import Link from "@erpx/workplace/components/workspace-link";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import type { ClearanceKind } from "@erpx/feature-people_payroll/lib/retirement";
import type { clearanceCase } from "@erpx/server-people_payroll/public/retirement-clearance";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";
import { RecordDocumentPanel } from "@erpx/feature-files/components/record-document-panel";
import { Card, FormLayout } from "@erpx/ui/components/foundation";

type Data = Awaited<ReturnType<typeof clearanceCase>>;

export function ClearanceWorkspace({
  base,
  data,
  owners,
}: {
  base: string;
  data: Data;
  owners: { id: string; name: string; kinds: ClearanceKind[] }[];
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [attachmentSets, setAttachmentSets] = useState<Record<string, AttachmentPanelChange>>({});
  const attempt = useRef<{ body: string; key: string } | null>(null);

  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/calendar`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body,
      });
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to save clearance.");
      attempt.current = null;
      router.refresh();
      window.dispatchEvent(new Event("erpx:work-changed"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save.");
    } finally {
      setBusy(false);
    }
  }

  async function prepareFinalPay(task: Data["tasks"][number], form: FormData) {
    const body = JSON.stringify({
      taskId: task.id,
      expectedTaskVersion: task.version,
      payGroupId: String(form.get("payGroupId")),
      periodStart: String(form.get("periodStart")),
      periodEnd: String(form.get("periodEnd")),
      payDate: String(form.get("payDate")),
      currency: String(form.get("currency")),
      ...(form.get("countryPackId")?{countryPackId:String(form.get("countryPackId"))}:{}),
      reason: String(form.get("reason")),
    });
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
        `/api/v1${base}/people/lifecycle/tasks/${encodeURIComponent(task.id)}/final-pay`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": crypto.randomUUID(),
          },
          body,
        },
      );
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to prepare final pay.");
      router.refresh();
      window.dispatchEvent(new Event("erpx:work-changed"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to prepare final pay.");
    } finally {
      setBusy(false);
    }
  }

  const pending = data.tasks.filter((task) => task.status === "pending").length;
  return (
    <>
      <Card as="section" appearance="standard" className="ui-panel">
        <p>
          Retirement:{" "}
          {data.case.retirementEffective
            ? "Effective"
            : data.case.status === "cancelled"
              ? "Cancelled"
              : "Scheduled"}{" "}
          · ERPX access: {data.case.accessEnded ? "Ended" : "Not ended"} · Directory:{" "}
          {data.case.directory}
        </p>
        <p>Last working day / Engagement end: {data.case.lastWorkingDate}</p>
        <h2>{pending ? `${pending} clearance tasks remaining` : "Clearance complete"}</h2>
        <p>
          Access cutoff: <TimeText appearance="standard" value={data.case.effectiveAt} /> {data.case.timezone}. Pending
          tasks do not extend access.
        </p>
      </Card>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data.tasks.map((task) => {
        const taskFiles = attachmentSets[task.id];
        return (
          <Card as="section" appearance="standard" className="ui-panel" key={`${task.id}:${task.version}`}>
            <header className="ui-section-header">
              <h2>{task.title}</h2>
              <StatusBadge appearance="standard" status={task.status} />
            </header>
            <p>
              Owner:{" "}
              {owners.find((owner) => owner.id === task.assignee_id)?.name ??
                task.assignee_id ??
                `Unassigned ${task.clearance_type} queue`}{" "}
              · Due: {task.due_date ?? "Not set"}
            </p>
            {!task.canComplete && task.status === "pending" && (
              <p className="muted">
                {task.assignee_id
                  ? "The assigned colleague must use their current department role to close this task."
                  : "Assign an eligible colleague to continue."}
              </p>
            )}
            {task.clearance_type === "payroll" &&
              task.canComplete &&
              !task.finalPay &&
              task.finalPayOptions && (
                <FormLayout
                  className="ui-form-grid"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void prepareFinalPay(task, new FormData(event.currentTarget));
                  }}
                >
                  <h3>Prepare final pay</h3>
                  <p className="muted">
                    Creates final pay through the last working day using the selected calculation rules. It must
                    be calculated and reviewed before this clearance is completed.
                  </p>
                  <Field appearance="standard">
                    Calculation rules
                    <Select appearance="standard" name="countryPackId" defaultValue="">
                      <option value="">Gross-pay preview</option>
                      {data.countryPacks.map(pack=><option key={pack.id} value={pack.id}>Thai statutory · {pack.version} · {pack.effective_from} to {pack.effective_to??"open"}</option>)}
                    </Select>
                  </Field>
                  <TextInput appearance="standard"
                    type="hidden"
                    name="payGroupId"
                    value={String(task.finalPayOptions.pay_group_id ?? "")}
                  />
                  <TextInput appearance="standard"
                    type="hidden"
                    name="currency"
                    value={String(task.finalPayOptions.currency ?? "")}
                  />
                  <Field appearance="standard">
                    Period start
                    <TextInput appearance="standard"
                      type="date"
                      name="periodStart"
                      required
                      defaultValue={data.case.lastWorkingDate}
                    />
                  </Field>
                  <Field appearance="standard">
                    Period end
                    <TextInput appearance="standard"
                      type="date"
                      name="periodEnd"
                      required
                      defaultValue={data.case.lastWorkingDate}
                    />
                  </Field>
                  <Field appearance="standard">
                    Pay date
                    <TextInput appearance="standard"
                      type="date"
                      name="payDate"
                      required
                      defaultValue={data.case.lastWorkingDate}
                    />
                  </Field>
                  <Field appearance="standard">
                    Preparation reason
                    <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000}>
                      Final pay for the approved retirement clearance
                    </TextArea>
                  </Field>
                  <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                    Create final-pay preview
                  </Button>
                </FormLayout>
              )}
            {task.finalPay && (
              <>
                <p className="muted">
                  Final-pay preview prepared:{" "}
                  <Link
                    href={canonicalUiHref(`${base}/people/payroll/runs/${encodeURIComponent(task.finalPay.run_id)}`)}
                  >
                    {String(task.finalPay.run_id)}
                  </Link>{" "}
                  · {String(task.finalPay.run_status)}
                </p>
                <RecordDocumentPanel appearance="standard"
                  base={base}
                  targetType="payroll_final_pay"
                  targetId={String(task.finalPay.preparation_id)}
                  expectedTargetVersion={Number(task.finalPay.version)}
                  editable={task.canComplete}
                  title="Final-pay support"
                  purposes={[
                    {
                      key: "final_pay_support",
                      label: "Final-pay support",
                      description:
                        "Attach the controlled final-pay worksheet or supporting document.",
                      visibility:
                        "Limited to the assigned Payroll clearance owner or recorded completer with current Payroll document access.",
                    },
                  ]}
                />
              </>
            )}
            {task.canPlan && (
              <details>
                <summary>Assign owner or due date</summary>
                <FormLayout
                  className="ui-form-grid"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void send({
                      action: "task.plan",
                      id: task.id,
                      expectedVersion: task.version,
                      assigneeId: String(form.get("owner") ?? "") || null,
                      dueDate: String(form.get("due") ?? "") || null,
                      responsibility: task.clearance_type,
                    });
                  }}
                >
                  <Field appearance="standard">
                    Owner
                    <Select appearance="standard" name="owner" defaultValue={task.assignee_id ?? ""}>
                      <option value="">Unassigned</option>
                      {owners
                        .filter((owner) => owner.kinds.includes(task.clearance_type!))
                        .map((owner) => (
                          <option key={owner.id} value={owner.id}>
                            {owner.name}
                          </option>
                        ))}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Due date
                    <TextInput appearance="standard" type="date" name="due" required defaultValue={task.due_date ?? ""} />
                  </Field>
                  <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                    Save assignment
                  </Button>
                </FormLayout>
              </details>
            )}
            {task.canComplete && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget);
                  const sourceId = String(form.get("document") ?? "").trim();
                  void send({
                    action: "task.complete",
                    id: task.id,
                    expectedVersion: task.version,
                    status: form.get("status"),
                    reason: form.get("reason"),
                    references: sourceId ? [{ kind: form.get("kind"), id: sourceId }] : [],
                    attachmentSetIds:
                      taskFiles?.setId && taskFiles.readyCount > 0 && taskFiles.bindable
                        ? [taskFiles.setId]
                        : [],
                  });
                }}
              >
                <Field appearance="standard">
                  Result
                  <Select appearance="standard" name="status">
                    <option value="completed">Completed</option>
                    <option value="not_required">Not required</option>
                  </Select>
                </Field>
                <Field appearance="standard">
                  Review note / reason
                  <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
                </Field>
                <AttachmentPanel appearance="standard"
                  base={base}
                  targetType="people_clearance_task"
                  targetId={task.id}
                  expectedTargetVersion={Number(task.version)}
                  componentKind="action"
                  componentRef={String(task.clearance_type ?? "clearance")}
                  purposeKey="clearance_evidence"
                  title="Clearance evidence"
                  description="Attach task-specific handover, return, access, payroll, or finance evidence. Clean files are locked with this completion."
                  visibility="Only the assigned responsible colleague, or the recorded completer with the required current permission, can access these files."
                  onChange={(change) =>
                    setAttachmentSets((current) => ({
                      ...current,
                      [task.id]: change,
                    }))
                  }
                />
                <Field appearance="standard">
                  Source document type
                  <Select appearance="standard" name="kind">
                    {(task.clearance_type === "payroll"
                      ? ["payroll"]
                      : ["expense", "invoice", "journal", "payroll"]
                    ).map((kind) => (
                      <option key={kind}>{kind}</option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Source document ID
                  <TextInput appearance="standard" name="document" placeholder="ID from the document URL" />
                </Field>
                <p className="muted">
                  Finance and Payroll require source evidence when completed. This confirmation does
                  not approve, post or pay the document.
                </p>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Confirm clearance result
                </Button>
              </FormLayout>
            )}
            {!task.canComplete && task.evidenceReadable && (
              <AttachmentPanel appearance="standard"
                base={base}
                targetType="people_clearance_task"
                targetId={task.id}
                expectedTargetVersion={Number(task.version)}
                componentKind="action"
                componentRef={String(task.clearance_type ?? "clearance")}
                purposeKey="clearance_evidence"
                title="Clearance evidence"
                readOnly
                showHistory
              />
            )}
            {task.completion_reason && <p>Review: {task.completion_reason}</p>}
            {task.evidence.map((item, index) => (
              <p key={index}>
                {item.href ? <Link href={`${base}${item.href}`}>{item.label}</Link> : item.label} ·{" "}
                {item.status}
              </p>
            ))}
            {task.completed_at && (
              <small className="muted">
                Recorded by {task.completed_by} · <TimeText appearance="standard" value={task.completed_at} />
              </small>
            )}
          </Card>
        );
      })}
    </>
  );
}
