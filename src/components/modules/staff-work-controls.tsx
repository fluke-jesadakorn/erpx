"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useEffect, useRef, useState, useTransition } from "react";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@/lib/ai/guided-intake";
import { preparedTaskFields } from "@/lib/ai/prepared-work";
import { formatUiDateTime } from "@/lib/ui-format";
import { localMinuteAt } from "@/lib/calendar-timeline";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import { TimeText, useDisplayTimeZone } from "@/components/time/time-preference-context";
import { useRouter } from "next/navigation";
import type { StaffPerson } from "@/server/staff-policy";
import type { TaskDetail, TaskRow, ProjectRow } from "@/server/staff-tasks";
import type { ScheduleRow, TemplateRow } from "@/server/routines";
import type { ApprovalView } from "@/server/approvals";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { type UiAppearance } from "@/components/ui/foundation";

export type StaffOptions = {
  people: StaffPerson[];
  levels: {
    id: string;
    name: string;
    rank: number;
    level_number?: number | null;
    active: number;
    version: number;
    code: string;
  }[];
  departments: {
    id: string;
    display_name: string;
    person_id: string | null;
    version: number | null;
    head_name: string;
  }[];
};
export function useStaffCommand(base: string) {
  const router = useRouter(),
    [refreshing, startRefresh] = useTransition(),
    attempt = useRef<{ signature: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(path: string, input: unknown, destination?: string) {
    const body = JSON.stringify(input),
      signature = path + body;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body,
      });
      const data = (await response.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!response.ok)
        throw new Error(data.error?.message ?? "Unable to save. Your details are still here.");
      attempt.current = null;
      if (destination) router.push(`${base}/${destination}/${data.data.id}`);
      startRefresh(() => router.refresh());
      return data.data;
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Connection interrupted. Retry to finish the same action.",
      );
      return null;
    } finally {
      setBusy(false);
    }
  }
  return { send, busy: busy || refreshing, error };
}
export function StaffPicker({ appearance = "legacy",
  label,
  value,
  onChange,
  options,
  exclude = [],
  required = true,
}: ({
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: StaffOptions;
  exclude?: string[];
  required?: boolean;
}) & { appearance?: UiAppearance }) {
  const [department, setDepartment] = useState(""),
    [level, setLevel] = useState("");
  const seen = new Set<string>(),
    people = options.people.filter((p) => {
      if (seen.has(p.user_id)) return false;
      seen.add(p.user_id);
      return true;
    });
  const visible = people.filter(
    (p) =>
      !exclude.includes(p.user_id) &&
      (!department || p.department_id === department) &&
      (!level || (level === "unset" ? !p.level_id : p.level_id === level)),
  );
  const selected = people.find((p) => p.user_id === value);
  return (
    <fieldset data-ui-appearance={appearance} className="staff-picker">
      <legend data-ui-appearance={appearance}>{label}</legend>
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-grid" : "business-form"}>
        <Field appearance={appearance}>
          Department filter
          <Select appearance={appearance} value={department} onChange={(e) => setDepartment(e.target.value)}>
            <option data-ui-appearance={appearance} value="">All departments</option>
            {options.departments.map((d) => (
              <option data-ui-appearance={appearance} key={d.id} value={d.id}>
                {d.display_name}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance={appearance}>
          Level filter
          <Select appearance={appearance} value={level} onChange={(e) => setLevel(e.target.value)}>
            <option data-ui-appearance={appearance} value="">All levels</option>
            <option data-ui-appearance={appearance} value="unset">Not set</option>
            {options.levels.map((l) => (
              <option data-ui-appearance={appearance} key={l.id} value={l.id}>
                {l.name} · rank {l.rank}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <Field appearance={appearance}>
        {label}
        <Select appearance={appearance} value={value} onChange={(e) => onChange(e.target.value)} required={required}>
          <option data-ui-appearance={appearance} value="">Choose staff</option>
          {value && !visible.some((p) => p.user_id === value) && (
            <option data-ui-appearance={appearance} value={value}>{selected?.legal_name ?? "Unavailable staff"} · selected</option>
          )}
          {visible.map((p) => (
            <option data-ui-appearance={appearance} key={p.user_id} value={p.user_id}>
              {p.legal_name} · {p.level_name ?? "Not set"}
            </option>
          ))}
        </Select>
      </Field>
    </fieldset>
  );
}
function Approvers({ appearance = "legacy",
  label,
  values,
  onChange,
  options,
  exclude,
}: ({
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  options: StaffOptions;
  exclude: string[];
}) & { appearance?: UiAppearance }) {
  return (
    <fieldset data-ui-appearance={appearance}>
      <legend data-ui-appearance={appearance}>{label}</legend>
      {values.map((value, index) => (
        <div data-ui-appearance={appearance} className="staff-step" key={index}>
          <StaffPicker appearance={appearance}
            label={`Step ${index + 1} approver`}
            value={value}
            onChange={(user) => onChange(values.map((v, i) => (i === index ? user : v)))}
            options={options}
            exclude={[...exclude, ...values.filter((_, i) => i !== index)]}
          />
          <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
            <Button appearance={appearance}
              variant="secondary"
              type="button"

              disabled={index === 0}
              onClick={() => {
                const next = [...values];
                [next[index - 1], next[index]] = [next[index], next[index - 1]];
                onChange(next);
              }}
            >
              Move step {index + 1} up
            </Button>
            <Button appearance={appearance}
              variant="ghost"
              type="button"

              onClick={() => onChange(values.filter((_, i) => i !== index))}
            >
              Remove step {index + 1}
            </Button>
          </div>
        </div>
      ))}
      <Button appearance={appearance}
        variant="secondary"
        type="button"

        disabled={values.length >= 20}
        onClick={() => onChange([...values, ""])}
      >
        Add {label.toLowerCase()} step
      </Button>
    </fieldset>
  );
}
export function TaskEditor({ appearance = "legacy",
  base,
  options,
  projects,
  userId,
  companyId,
  assignmentId,
  task,
  template,
  clone = false,
  canRevise = false,
  createAs = "task",
  initialDueDate,
  initialProjectId,
}: ({
  base: string;
  options: StaffOptions;
  projects: ProjectRow[];
  userId: string;
  companyId?: string;
  assignmentId?: string;
  task?: TaskRow;
  template?: TemplateRow;
  clone?: boolean;
  canRevise?: boolean;
  createAs?: "task" | "routine";
  initialDueDate?: string;
  initialProjectId?: string;
}) & { appearance?: UiAppearance }) {
  const { timeZone, ready: timeReady } = useDisplayTimeZone();
  const [defaultDue] = useState(() =>
    initialDueDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDueDate) ? `${initialDueDate}T09:00` : "",
  );
  const source = task ?? template,
    { send, busy, error } = useStaffCommand(base),
    [title, setTitle] = useState(`${clone ? "Copy of " : ""}${source?.title ?? ""}`),
    [description, setDescription] = useState(source?.description ?? ""),
    [guided, setGuided] = useState<Record<string, string> | null>(null),
    [assignee, setAssignee] = useState(source?.assignee_user_id ?? userId),
    [before, setBefore] = useState<string[]>(source ? JSON.parse(source.start_approvers_json) : []),
    [after, setAfter] = useState<string[]>(
      source ? JSON.parse(source.completion_approvers_json) : createAs === "routine" ? [""] : [],
    ),
    [phase, setPhase] = useState(
      source
        ? JSON.parse(source.start_approvers_json).length
          ? JSON.parse(source.completion_approvers_json).length
            ? "both"
            : "start"
          : JSON.parse(source.completion_approvers_json).length
            ? "completion"
            : "none"
        : createAs === "routine"
          ? "completion"
          : "none",
    ),
    [dueOverride, setDue] = useState<string | null>(null);
  const due =
    dueOverride ??
    (task?.due_at != null && timeReady
      ? (() => {
          const value = localMinuteAt(task.due_at, timeZone);
          return `${value.date}T${value.time}`;
        })()
      : defaultDue);
  const creator = clone
    ? userId
    : source
      ? "creator_user_id" in source
        ? source.creator_user_id
        : userId
      : userId;
  const [createKind, setCreateKind] = useState(createAs);
  const templateMode = !!template || createKind === "routine";
  const maySkipReview =
    !templateMode &&
    (!task || (task.start_approvers_json === "[]" && task.completion_approvers_json === "[]"));
  const ordinaryTask = !templateMode && phase === "none";
  useEffect(() => {
    if (source || createAs !== "task" || !companyId || !assignmentId) return;
    const restore = () =>
      setGuided(readGuidedIntakeHandoff(companyId, userId, assignmentId, base, "task"));
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [source, createAs, companyId, userId, assignmentId, base]);
  function clearPrepared() {
    if (companyId && assignmentId)
      clearGuidedIntakeHandoff(companyId, userId, assignmentId, base, "task");
    setGuided(null);
  }
  function usePrepared() {
    if (!guided || createKind !== "task") return;
    const prepared = preparedTaskFields(guided, options.people);
    if (prepared.title) setTitle(prepared.title);
    if (prepared.description) setDescription(prepared.description);
    if (prepared.dueLocal) setDue(prepared.dueLocal);
    if (prepared.assigneeId) setAssignee(prepared.assigneeId);
    clearPrepared();
  }
  return (
    <FormDisclosure appearance={appearance} inline={Boolean(!source || clone)} className={appearance === "standard" ? "ui-panel" : "card"} open={!source || clone}>
      <summary data-ui-appearance={appearance}>
        {templateMode
          ? clone
            ? "Clone routine template"
            : source
              ? "Edit routine template"
              : "New routine template"
          : canRevise
            ? ordinaryTask
              ? "Update task"
              : "Revise task and restart approvals"
            : task
              ? "Edit draft"
              : "New task"}
      </summary>
      {guided && createKind === "task" && (
        <div data-ui-appearance={appearance} className="notice" role="status">
          <p data-ui-appearance={appearance}>
            AI prepared task details. Review the owner, date and text before saving; this will
            replace any text already entered.
          </p>
          {guided.assignee && (
            <p data-ui-appearance={appearance}>
              Suggested owner: {guided.assignee}. Choose the accountable assignee if it is not
              selected automatically.
            </p>
          )}
          <Button appearance={appearance} variant="secondary" type="button" onClick={usePrepared}>
            Use prepared details
          </Button>{" "}
          <Button appearance={appearance} variant="ghost" type="button" onClick={clearPrepared}>
            Discard suggestion
          </Button>
        </div>
      )}
      <form data-ui-appearance={appearance}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),
            routine = f.get("recordKind") === "routine" || templateMode;
          const payload = {
            title: f.get("title"),
            description: f.get("description"),
            assigneeUserId: assignee,
            projectId: f.get("projectId") || null,
            startApprovers: phase === "start" || phase === "both" ? before : [],
            completionApprovers: phase === "completion" || phase === "both" ? after : [],
            expectedVersion: source?.version ?? 1,
            ...(routine
              ? { published: f.get("published") === "on" }
              : {
                  dueAt: f.get("dueAt") || null,
                  reason: f.get("reason") ?? "",
                }),
          };
          const path = routine
            ? `routine-templates${template ? `/${template.id}${clone ? "/clone" : ""}` : ""}`
            : `staff-tasks${task ? `/${task.id}${canRevise ? "/revise" : ""}` : ""}`;
          void send(path, payload, routine ? "routine-templates" : "staff-tasks");
        }}
      >
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-grid" : "business-form"}>
          {!source && (
            <Field appearance={appearance}>
              Create as
              <Select appearance={appearance}
                name="recordKind"
                value={createKind}
                onChange={(e) => {
                  const kind = e.target.value as "task" | "routine";
                  setCreateKind(kind);
                  if (kind === "routine" && phase === "none") {
                    setPhase("completion");
                    setAfter([""]);
                  }
                }}
              >
                <option data-ui-appearance={appearance} value="task">Operational or project task</option>
                <option data-ui-appearance={appearance} value="routine">Reusable routine template</option>
              </Select>
            </Field>
          )}
          <Field appearance={appearance}>
            Title
            <TextInput appearance={appearance}
              name="title"
              required
              minLength={2}
              maxLength={200}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </Field>
          <Field appearance={appearance}>
            Project
            <Select appearance={appearance} name="projectId" defaultValue={source?.project_id ?? initialProjectId ?? ""}>
              <option data-ui-appearance={appearance} value="">Standalone work</option>
              {projects.map((p) => (
                <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                  {p.title}
                </option>
              ))}
            </Select>
          </Field>
          {!templateMode && (
            <PreferredDateTimeField appearance={appearance}
              name="dueAt"
              label={`Due date and time${ordinaryTask ? " (optional)" : ""}`}
              required={!ordinaryTask}
              valueLocal={due}
              onLocalChange={setDue}
            />
          )}
        </div>
        <Field appearance={appearance}>
          Description
          <TextArea appearance={appearance}
            name="description"
            maxLength={10000}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
        <StaffPicker appearance={appearance}
          label="Accountable assignee"
          value={assignee}
          onChange={setAssignee}
          options={options}
        />
        <Field appearance={appearance}>
          Review
          <Select appearance={appearance}
            value={phase}
            onChange={(e) => {
              const next = e.target.value;
              setPhase(next);
              if ((next === "start" || next === "both") && !before.length) setBefore([""]);
              if ((next === "completion" || next === "both") && !after.length) setAfter([""]);
              if (next !== "none" && !due) {
                const value = localMinuteAt(Date.now() + 86400000, timeZone);
                setDue(`${value.date}T${value.time}`);
              }
            }}
          >
            {maySkipReview && <option data-ui-appearance={appearance} value="none">No review needed</option>}
            <option data-ui-appearance={appearance} value="completion">After completion</option>
            <option data-ui-appearance={appearance} value="start">Before work starts</option>
            <option data-ui-appearance={appearance} value="both">Before work and after completion</option>
          </Select>
        </Field>
        {(phase === "start" || phase === "both") && (
          <Approvers appearance={appearance}
            label="Start approval"
            values={before}
            onChange={setBefore}
            options={options}
            exclude={[assignee, creator]}
          />
        )}
        {(phase === "completion" || phase === "both") && (
          <Approvers appearance={appearance}
            label="Completion approval"
            values={after}
            onChange={setAfter}
            options={options}
            exclude={[assignee]}
          />
        )}
        {templateMode && (
          <Field appearance={appearance} orientation="inline" className="staff-check">
            <TextInput appearance={appearance}
              name="published"
              type="checkbox"
              defaultChecked={!template || !!template.published}
            />
            Publish for company staff
          </Field>
        )}
        {canRevise && (
          <Field appearance={appearance}>
            Revision reason
            <TextArea appearance={appearance}
              name="reason"
              required={!ordinaryTask}
              minLength={ordinaryTask ? undefined : 3}
              maxLength={2000}
            />
            <small data-ui-appearance={appearance}>
              {ordinaryTask
                ? "The updated task remains ready to work."
                : "This returns the task to draft. Submit it again to restart approvals. Previous decisions remain in history."}
            </small>
          </Field>
        )}
        {error && (
          <p data-ui-appearance={appearance} className="error" role="alert">
            {error}
          </p>
        )}
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
            {busy
              ? "Saving…"
              : canRevise
                ? "Save revision"
                : clone
                  ? "Create my copy"
                  : templateMode
                    ? "Save routine template"
                    : ordinaryTask
                      ? "Create task"
                      : "Save task draft"}
          </Button>
        </div>
      </form>
    </FormDisclosure>
  );
}
export function TaskActions({ appearance = "legacy", base, detail }: ({ base: string; detail: TaskDetail }) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [note, setNote] = useState(detail.task.completion_note),
    [evidence, setEvidence] = useState<AttachmentPanelChange>({
      setId: null,
      links: [],
      readyCount: 0,
      requiredCount: 0,
      bindable: true,
    });
  const ordinaryTask =
    detail.task.template_id === null &&
    detail.task.schedule_id === null &&
    detail.task.start_approvers_json === "[]" &&
    detail.task.completion_approvers_json === "[]";
  const labels: Record<string, string> = {
    submit: "Submit task",
    start: "Start work",
    "save-progress": "Save progress",
    complete: ordinaryTask ? "Mark complete" : "Submit completion",
    approve: "Approve current step",
    reject: "Reject with reason",
    cancel: "Cancel task",
  };
  const actions = detail.permittedActions.filter((a) => labels[a]);
  const canUpload =
    detail.permittedActions.includes("edit") ||
    detail.permittedActions.includes("save-progress") ||
    (ordinaryTask && detail.task.state === "ready" && detail.permittedActions.includes("complete"));
  const activeEvidence = evidence.links.filter(
    (link) => !["withdrawn", "expired"].includes(link.linkState),
  );
  const hasEvidence = activeEvidence.length > 0,
    evidencePending = hasEvidence && !evidence.bindable;
  const evidenceActions = new Set(["submit", "save-progress", "complete"]);
  return (
    <>
      {(actions.length > 0 || detail.blockedReason) && (
        <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
          <h2 data-ui-appearance={appearance}>Next action</h2>
          {detail.blockedReason && (
            <p data-ui-appearance={appearance} className="notice" role="status">
              Blocked: {detail.blockedReason}
            </p>
          )}
          {actions.length > 0 && (
            <>
              <Field appearance={appearance}>
                {ordinaryTask
                  ? "Completion note (optional; cancellation needs a reason)"
                  : "Completion or decision note"}
                <TextArea appearance={appearance}
                  aria-label="Completion or decision note"
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  maxLength={10000}
                />
              </Field>
              <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
                {actions.map((action) => (
                  <Button appearance={appearance}
                    clientManaged type="submit"
                    variant="plain"
                    key={action}
                    className={`button ${["reject", "cancel", "save-progress"].includes(action) ? "secondary" : ""}`}
                    disabled={
                      busy || evidencePending || (hasEvidence && !evidenceActions.has(action))
                    }
                    onClick={() =>
                      void send(`staff-tasks/${detail.task.id}/transition`, {
                        action,
                        expectedVersion: detail.task.version,
                        note,
                        attachmentSetIds:
                          hasEvidence &&
                          evidence.bindable &&
                          evidence.setId &&
                          evidenceActions.has(action)
                            ? [evidence.setId]
                            : [],
                      })
                    }
                  >
                    {labels[action]}
                  </Button>
                ))}
              </div>
            </>
          )}
          {error && (
            <p data-ui-appearance={appearance} role="alert" className="error">
              {error}
            </p>
          )}
        </section>
      )}
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <AttachmentPanel appearance={appearance}
          key={`${detail.task.id}:${detail.task.version}`}
          base={base}
          targetType="staff_task"
          targetId={detail.task.id}
          expectedTargetVersion={detail.task.version}
          componentKind="action"
          componentRef={`task:${detail.task.state}`}
          purposeKey="task_evidence"
          title="Task evidence"
          description="Attach clean photos or documents to the next submit, progress, or completion action. Bound evidence cannot be replaced in place."
          visibility="Available only to people who may read this task. A revoked delegate loses access immediately."
          readOnly={!canUpload}
          frozen={!canUpload}
          showHistory
          onChange={setEvidence}
        />
        {evidencePending && (
          <p data-ui-appearance={appearance} className="notice" role="status">
            Wait for every selected file to pass security scanning before changing the task.
          </p>
        )}
        {hasEvidence && evidence.bindable && (
          <p data-ui-appearance={appearance} className="notice" role="status">
            Clean evidence will be locked to the next supported task action.
          </p>
        )}
        {detail.attachments.length > 0 && (
          <details data-ui-appearance={appearance}>
            <summary data-ui-appearance={appearance}>Legacy task attachments</summary>
            {detail.attachments.map((file) => (
              <a data-ui-appearance={appearance}
                className={appearance === "standard" ? "ui-record-item" : "row"}
                key={file.id}
                href={`/api/v1${base}/attachments/${file.id}/content`}
              >
                {file.original_name} · {Math.ceil(file.bytes / 1024)} KB
              </a>
            ))}
          </details>
        )}
      </section>
    </>
  );
}
export function TaskDelegationControls({ appearance = "legacy",
  base,
  detail,
  options,
}: ({
  base: string;
  detail: TaskDetail;
  options: StaffOptions;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    canDelegate = detail.permittedActions.includes("delegate"),
    [defaultExpiry] = useState(() => Date.now() + 86_400_000),
    [delegate, setDelegate] = useState(""),
    [reason, setReason] = useState(""),
    [revokeReason, setRevokeReason] = useState("");
  const timestamp = (value: number) => <TimeText appearance={appearance} value={value} />;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Temporary delegation</h2>
      {detail.delegation ? (
        <>
          <p data-ui-appearance={appearance} className="notice">
            This task is delegated to <strong data-ui-appearance={appearance}>{detail.delegation.delegate_name}</strong> until{" "}
            {timestamp(detail.delegation.expires_at)}.
          </p>
          <p data-ui-appearance={appearance} className="muted">Reason: {detail.delegation.reason}</p>
          {canDelegate && (
            <form data-ui-appearance={appearance}
              className={appearance === "standard" ? "ui-form-grid" : "business-form"}
              onSubmit={(event) => {
                event.preventDefault();
                void send(
                  `staff-tasks/${detail.task.id}/delegate/revoke/${detail.delegation!.id}`,
                  {
                    expectedVersion: detail.task.version,
                    reason: revokeReason,
                  },
                ).then((result) => {
                  if (result) setRevokeReason("");
                });
              }}
            >
              <Field appearance={appearance}>
                Reason for revoking
                <TextArea appearance={appearance}
                  value={revokeReason}
                  onChange={(event) => setRevokeReason(event.target.value)}
                  minLength={3}
                  maxLength={2000}
                  required
                />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
                Revoke delegation
              </Button>
            </form>
          )}
        </>
      ) : canDelegate ? (
        <form data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(event) => {
            event.preventDefault();
            void send(`staff-tasks/${detail.task.id}/delegate`, {
              delegateUserId: delegate,
              expiresAt: String(new FormData(event.currentTarget).get("expiry")),
              expectedVersion: detail.task.version,
              reason,
            });
          }}
        >
          <Field appearance={appearance}>
            Delegate to
            <Select appearance={appearance} value={delegate} onChange={(event) => setDelegate(event.target.value)} required>
              <option data-ui-appearance={appearance} value="">Choose active staff</option>
              {options.people
                .filter(
                  (person, index, people) =>
                    people.findIndex((candidate) => candidate.user_id === person.user_id) ===
                      index && person.user_id !== detail.task.assignee_user_id,
                )
                .map((person) => (
                  <option data-ui-appearance={appearance} key={person.user_id} value={person.user_id}>
                    {person.legal_name}
                  </option>
                ))}
            </Select>
          </Field>
          <PreferredDateTimeField appearance={appearance}
            name="expiry"
            label="Expires"
            required
            initialInstant={defaultExpiry}
          />
          <Field appearance={appearance}>
            Reason
            <TextArea appearance={appearance}
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              minLength={3}
              maxLength={2000}
              required
            />
          </Field>
          {error && (
            <p data-ui-appearance={appearance} className="error" role="alert">
              {error}
            </p>
          )}
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy || !delegate}>
            Delegate task
          </Button>
        </form>
      ) : (
        <p data-ui-appearance={appearance} className="muted">
          No active delegation. The creator or project owner can temporarily assign coverage.
        </p>
      )}
      {error && detail.delegation && (
        <p data-ui-appearance={appearance} className="error" role="alert">
          {error}
        </p>
      )}
      {detail.delegationHistory.length > 0 && (
        <details data-ui-appearance={appearance}>
          <summary data-ui-appearance={appearance}>Delegation history</summary>
          {detail.delegationHistory.length === 50 && (
            <p data-ui-appearance={appearance} className="muted">Showing the 50 most recent delegations.</p>
          )}
          <ol data-ui-appearance={appearance}>
            {detail.delegationHistory.map((entry) => (
              <li data-ui-appearance={appearance} key={entry.id}>
                <p data-ui-appearance={appearance}>
                  <strong data-ui-appearance={appearance}>{entry.delegate_name}</strong> · {entry.status}
                </p>
                <p data-ui-appearance={appearance}>
                  Delegated by {entry.delegator_name} from {timestamp(entry.starts_at)} until{" "}
                  {timestamp(entry.expires_at)}.
                </p>
                <p data-ui-appearance={appearance}>Reason: {entry.reason}</p>
                {entry.status === "revoked" && (
                  <p data-ui-appearance={appearance}>
                    Revoked by {entry.revoked_by_name}{" "}
                    {entry.revoked_at ? <>at {timestamp(entry.revoked_at)}</> : ""}. Reason:{" "}
                    {entry.revoke_reason ?? "No reason retained for this earlier revocation."}
                  </p>
                )}
              </li>
            ))}
          </ol>
        </details>
      )}
    </section>
  );
}
export function ProjectEditor({ appearance = "legacy",
  base,
  options,
  project,
  members = [],
}: ({
  base: string;
  options: StaffOptions;
  project?: ProjectRow;
  members?: string[];
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [selected, setSelected] = useState(members),
    [person, setPerson] = useState("");
  return (
    <FormDisclosure appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <summary data-ui-appearance={appearance}>{project ? "Manage project and members" : "New project"}</summary>
      <form data-ui-appearance={appearance}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send(
            `projects${project ? `/${project.id}` : ""}`,
            {
              title: f.get("title"),
              description: f.get("description"),
              memberUserIds: selected,
              expectedVersion: project?.version ?? 1,
            },
            "projects",
          );
        }}
      >
        <Field appearance={appearance}>
          Project title
          <TextInput appearance={appearance}
            name="title"
            required
            minLength={2}
            maxLength={200}
            defaultValue={project?.title ?? ""}
          />
        </Field>
        <Field appearance={appearance}>
          Description
          <TextArea appearance={appearance}
            name="description"
            maxLength={10000}
            defaultValue={project?.description ?? ""}
          />
        </Field>
        <StaffPicker appearance={appearance}
          label="Add company member"
          required={false}
          value={person}
          onChange={setPerson}
          options={options}
          exclude={selected}
        />
        <Button appearance={appearance}
          variant="secondary"

          type="button"
          disabled={!person}
          onClick={() => {
            setSelected([...new Set([...selected, person])]);
            setPerson("");
          }}
        >
          Add member
        </Button>
        {selected.map((user) => (
          <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-record-item" : "row"} key={user}>
            <span data-ui-appearance={appearance}>
              {options.people.find((p) => p.user_id === user)?.legal_name ?? "Unavailable staff"}
            </span>
            <Button appearance={appearance}
              variant="ghost"

              type="button"
              onClick={() => setSelected(selected.filter((v) => v !== user))}
            >
              Remove member
            </Button>
          </div>
        ))}
        {error && (
          <p data-ui-appearance={appearance} className="error" role="alert">
            {error}
          </p>
        )}
        <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
          Save project
        </Button>
      </form>
    </FormDisclosure>
  );
}
export function RoutineScheduleControls({ appearance = "legacy",
  base,
  template,
  schedules,
  canEdit,
}: ({
  base: string;
  template: TemplateRow;
  schedules: ScheduleRow[];
  canEdit: boolean;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [frequency, setFrequency] = useState("daily"),
    [defaultDueInstant] = useState(() => Date.now() + 86400000);
  function scheduleForm(schedule?: ScheduleRow) {
    return (
      <form data-ui-appearance={appearance}
        key={schedule?.id ?? "new"}
        className={appearance === "standard" ? "ui-form-grid" : "business-form"}
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send(
            `routine-templates/${template.id}/schedules${schedule ? `/${schedule.id}` : ""}`,
            {
              frequency: f.get("frequency"),
              timezone: f.get("timezone"),
              occurrenceTime: f.get("occurrenceTime"),
              startDate: f.get("startDate"),
              endDate: f.get("endDate") || null,
              weekday: f.get("weekday") === "" ? null : Number(f.get("weekday")),
              monthDay: f.get("monthDay") === "" ? null : Number(f.get("monthDay")),
              dueOffsetHours: Number(f.get("dueOffsetHours")),
              expectedVersion: schedule?.version ?? 1,
            },
          );
        }}
      >
        <Field appearance={appearance}>
          Frequency
          <Select appearance={appearance}
            name="frequency"
            defaultValue={schedule?.frequency ?? frequency}
            onChange={(e) => setFrequency(e.target.value)}
          >
            <option data-ui-appearance={appearance} value="daily">Daily</option>
            <option data-ui-appearance={appearance} value="weekly">Weekly</option>
            <option data-ui-appearance={appearance} value="monthly">Monthly</option>
          </Select>
        </Field>
        <Field appearance={appearance}>
          Timezone
          <TextInput appearance={appearance} name="timezone" defaultValue={schedule?.timezone ?? "Asia/Bangkok"} required />
        </Field>
        <Field appearance={appearance}>
          Occurrence time
          <TextInput appearance={appearance}
            name="occurrenceTime"
            type="time"
            defaultValue={schedule?.occurrence_time ?? "09:00"}
            required
          />
        </Field>
        <Field appearance={appearance}>
          Start date
          <TextInput appearance={appearance}
            name="startDate"
            type="date"
            required
            defaultValue={schedule?.start_date ?? new Date().toISOString().slice(0, 10)}
          />
        </Field>
        <Field appearance={appearance}>
          End date
          <TextInput appearance={appearance} name="endDate" type="date" defaultValue={schedule?.end_date ?? ""} />
        </Field>
        <Field appearance={appearance}>
          Weekly weekday
          <Select appearance={appearance} name="weekday" defaultValue={schedule?.weekday ?? ""}>
            <option data-ui-appearance={appearance} value="">Use start weekday</option>
            {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].map(
              (day, i) => (
                <option data-ui-appearance={appearance} key={day} value={i}>
                  {day}
                </option>
              ),
            )}
          </Select>
        </Field>
        <Field appearance={appearance}>
          Monthly day
          <TextInput appearance={appearance}
            name="monthDay"
            type="number"
            min={1}
            max={31}
            defaultValue={schedule?.month_day ?? ""}
            placeholder="Use start day"
          />
        </Field>
        <Field appearance={appearance}>
          Due offset in hours
          <TextInput appearance={appearance}
            name="dueOffsetHours"
            type="number"
            required
            min={0}
            max={8760}
            defaultValue={schedule?.due_offset_hours ?? 24}
          />
        </Field>
        <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
          Save paused schedule
        </Button>
      </form>
    );
  }
  return (
    <>
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Run routine now</h2>
        <form data-ui-appearance={appearance}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            void send(
              `routine-templates/${template.id}/run`,
              {
                dueAt: String(new FormData(e.currentTarget).get("dueAt")),
              },
              "staff-tasks",
            );
          }}
        >
          <PreferredDateTimeField appearance={appearance}
            name="dueAt"
            label="Task due date and time"
            required
            initialInstant={defaultDueInstant}
          />
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy || !template.published}>
            Create occurrence
          </Button>
        </form>
      </section>
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Schedules</h2>
        <p data-ui-appearance={appearance} className="muted">
          Each occurrence creates independent work. Monthly days beyond month-end use the last day.
        </p>
        {schedules.map((s) => (
          <article data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"} key={s.id}>
            <p data-ui-appearance={appearance}>
              <strong data-ui-appearance={appearance}>
                {s.frequency} at {s.occurrence_time} · {s.timezone}
              </strong>{" "}
              · {s.state}
            </p>
            <p data-ui-appearance={appearance}>
              Next occurrence:{" "}
              {s.next_occurrence_at ? formatUiDateTime(s.next_occurrence_at, s.timezone) : "None"}
            </p>
            {s.last_error && <p data-ui-appearance={appearance} role="alert">Generation failed: {s.last_error}</p>}
            {canEdit && s.state !== "ended" && (
              <>
                <Button appearance={appearance}
                  clientManaged type="submit"
                  variant="secondary"

                  disabled={busy}
                  onClick={() =>
                    void send(
                      `routine-schedules/${s.id}/${s.state === "active" ? "pause" : "resume"}`,
                      { expectedVersion: s.version },
                    )
                  }
                >
                  {s.state === "active" ? "Pause schedule" : "Resume schedule"}
                </Button>
                {s.state === "paused" && (
                  <details data-ui-appearance={appearance}>
                    <summary data-ui-appearance={appearance}>Edit paused schedule</summary>
                    {scheduleForm(s)}
                  </details>
                )}
              </>
            )}
          </article>
        ))}
        {canEdit && (
          <details data-ui-appearance={appearance}>
            <summary data-ui-appearance={appearance}>Add schedule</summary>
            <p data-ui-appearance={appearance}>
              Schedules are saved paused. Resume after reviewing assignment and approval
              configuration.
            </p>
            {scheduleForm()}
          </details>
        )}
        {error && (
          <p data-ui-appearance={appearance} className="error" role="alert">
            {error}
          </p>
        )}
      </section>
    </>
  );
}
export function RerouteControl({ appearance = "legacy",
  base,
  approval,
  version,
  options,
}: ({
  base: string;
  approval: ApprovalView;
  version: number;
  options: StaffOptions;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [user, setUser] = useState("");
  if (!approval.instance || !approval.currentApprover) return null;
  return (
    <details data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <summary data-ui-appearance={appearance}>Administratively reroute current step</summary>
      <form data-ui-appearance={appearance}
        onSubmit={(e) => {
          e.preventDefault();
          void send(`approvals/${approval.instance!.id}/reroute`, {
            approverUserId: user,
            expectedVersion: version,
            instanceVersion: approval.instance!.version,
            reason: new FormData(e.currentTarget).get("reason"),
          });
        }}
      >
        <StaffPicker appearance={appearance}
          label="Replacement approver"
          value={user}
          onChange={setUser}
          options={options}
        />
        <Field appearance={appearance}>
          Rerouting reason
          <TextArea appearance={appearance} name="reason" minLength={3} maxLength={2000} required />
        </Field>
        <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
          Record rerouting
        </Button>
        {error && (
          <p data-ui-appearance={appearance} role="alert" className="error">
            {error}
          </p>
        )}
      </form>
    </details>
  );
}
export function StaffTaskLinks({ appearance = "legacy", base }: ({ base: string }) & { appearance?: UiAppearance }) {
  return (
    <nav data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"} aria-label="Staff work">
      <UiActionLink appearance={appearance} variant="secondary" href={`${base}/work?type=staff_task`}>
        Tasks
      </UiActionLink>
      <UiActionLink appearance={appearance} variant="secondary" href={`${base}/projects`}>
        Projects
      </UiActionLink>
    </nav>
  );
}

export function ReadNotification({ appearance = "legacy", base, id }: ({ base: string; id: string }) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base);
  return (
    <span data-ui-appearance={appearance}>
      <Button appearance={appearance}
        clientManaged type="submit"
        variant="ghost"

        disabled={busy}
        onClick={() => void send(`staff/notifications/${id}/read`, {})}
      >
        Mark read
      </Button>
      {error && <span data-ui-appearance={appearance} role="alert">{error}</span>}
    </span>
  );
}
