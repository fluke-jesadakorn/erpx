"use client";

import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { useRef, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import type { StaffPerson } from "@/server/staff-policy";
import type { TaskDetail, TaskRow, ProjectRow } from "@/server/staff-tasks";
import type { ScheduleRow, TemplateRow } from "@/server/routines";
import type { ApprovalView } from "@/server/approvals";

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
        throw new Error(
          data.error?.message ?? "Unable to save. Your details are still here.",
        );
      attempt.current = null;
      if (destination) router.push(`${base}/${destination}/${data.data.id}`);
      startRefresh(() => router.refresh());
      return data.data;
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Connection interrupted. Retry to finish the same action.",
      );
      return null;
    } finally {
      setBusy(false);
    }
  }
  return { send, busy: busy || refreshing, error };
}
export function StaffPicker({
  label,
  value,
  onChange,
  options,
  exclude = [],
  required = true,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: StaffOptions;
  exclude?: string[];
  required?: boolean;
}) {
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
    <fieldset className="staff-picker">
      <legend>{label}</legend>
      <div className="business-form">
        <label>
          Department filter
          <select
            value={department}
            onChange={(e) => setDepartment(e.target.value)}
          >
            <option value="">All departments</option>
            {options.departments.map((d) => (
              <option key={d.id} value={d.id}>
                {d.display_name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Level filter
          <select value={level} onChange={(e) => setLevel(e.target.value)}>
            <option value="">All levels</option>
            <option value="unset">Not set</option>
            {options.levels.map((l) => (
              <option key={l.id} value={l.id}>
                {l.name} · rank {l.rank}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label>
        {label}
        <select
          value={value}
          onChange={(e) => onChange(e.target.value)}
          required={required}
        >
          <option value="">Choose staff</option>
          {value && !visible.some((p) => p.user_id === value) && (
            <option value={value}>
              {selected?.legal_name ?? "Unavailable staff"} · selected
            </option>
          )}
          {visible.map((p) => (
            <option key={p.user_id} value={p.user_id}>
              {p.legal_name} · {p.level_name ?? "Not set"}
            </option>
          ))}
        </select>
      </label>
    </fieldset>
  );
}
function Approvers({
  label,
  values,
  onChange,
  options,
  exclude,
}: {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  options: StaffOptions;
  exclude: string[];
}) {
  return (
    <fieldset>
      <legend>{label}</legend>
      {values.map((value, index) => (
        <div className="staff-step" key={index}>
          <StaffPicker
            label={`Step ${index + 1} approver`}
            value={value}
            onChange={(user) =>
              onChange(values.map((v, i) => (i === index ? user : v)))
            }
            options={options}
            exclude={[...exclude, ...values.filter((_, i) => i !== index)]}
          />
          <div className="form-actions">
            <button
              type="button"
              className="button secondary"
              disabled={index === 0}
              onClick={() => {
                const next = [...values];
                [next[index - 1], next[index]] = [next[index], next[index - 1]];
                onChange(next);
              }}
            >
              Move step {index + 1} up
            </button>
            <button
              type="button"
              className="button ghost"
              onClick={() => onChange(values.filter((_, i) => i !== index))}
            >
              Remove step {index + 1}
            </button>
          </div>
        </div>
      ))}
      <button
        type="button"
        className="button secondary"
        disabled={values.length >= 20}
        onClick={() => onChange([...values, ""])}
      >
        Add {label.toLowerCase()} step
      </button>
    </fieldset>
  );
}
function localInput(timestamp: number) {
  const date = new Date(timestamp);
  return new Date(timestamp - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
}
export function TaskEditor({
  base,
  options,
  projects,
  userId,
  task,
  template,
  clone = false,
  canRevise = false,
  createAs = "task",
}: {
  base: string;
  options: StaffOptions;
  projects: ProjectRow[];
  userId: string;
  task?: TaskRow;
  template?: TemplateRow;
  clone?: boolean;
  canRevise?: boolean;
  createAs?: "task" | "routine";
}) {
  const [defaultDue] = useState(() => localInput(Date.now() + 86400000));
  const source = task ?? template,
    { send, busy, error } = useStaffCommand(base),
    [assignee, setAssignee] = useState(source?.assignee_user_id ?? userId),
    [before, setBefore] = useState<string[]>(
      source ? JSON.parse(source.start_approvers_json) : [],
    ),
    [after, setAfter] = useState<string[]>(
      source ? JSON.parse(source.completion_approvers_json) : [""],
    ),
    [phase, setPhase] = useState(
      source
        ? JSON.parse(source.start_approvers_json).length
          ? JSON.parse(source.completion_approvers_json).length
            ? "both"
            : "start"
          : "completion"
        : "completion",
    );
  const creator = clone
    ? userId
    : source
      ? "creator_user_id" in source
        ? source.creator_user_id
        : userId
      : userId;
  const [createKind, setCreateKind] = useState(createAs);
  const templateMode = !!template || createKind === "routine";
  return (
    <details className="card" open={!source || clone}>
      <summary>
        {templateMode
          ? clone
            ? "Clone routine template"
            : source
              ? "Edit routine template"
              : "New routine template"
          : canRevise
            ? "Revise task and restart approvals"
            : task
              ? "Edit draft"
              : "New task"}
      </summary>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),
            routine = f.get("recordKind") === "routine" || templateMode;
          const payload = {
            title: f.get("title"),
            description: f.get("description"),
            assigneeUserId: assignee,
            projectId: f.get("projectId") || null,
            startApprovers: phase === "completion" ? [] : before,
            completionApprovers: phase === "start" ? [] : after,
            expectedVersion: source?.version ?? 1,
            ...(routine
              ? { published: f.get("published") === "on" }
              : {
                  dueAt: new Date(String(f.get("dueAt"))).toISOString(),
                  reason: f.get("reason") ?? "",
                }),
          };
          const path = routine
            ? `routine-templates${template ? `/${template.id}${clone ? "/clone" : ""}` : ""}`
            : `staff-tasks${task ? `/${task.id}${canRevise ? "/revise" : ""}` : ""}`;
          void send(
            path,
            payload,
            routine ? "routine-templates" : "staff-tasks",
          );
        }}
      >
        <div className="business-form">
          {!source && (
            <label>
              Create as
              <select
                name="recordKind"
                value={createKind}
                onChange={(e) =>
                  setCreateKind(e.target.value as "task" | "routine")
                }
              >
                <option value="task">Operational or project task</option>
                <option value="routine">Reusable routine template</option>
              </select>
            </label>
          )}
          <label>
            Title
            <input
              name="title"
              required
              minLength={2}
              maxLength={200}
              defaultValue={`${clone ? "Copy of " : ""}${source?.title ?? ""}`}
            />
          </label>
          <label>
            Project
            <select name="projectId" defaultValue={source?.project_id ?? ""}>
              <option value="">Standalone work</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.title}
                </option>
              ))}
            </select>
          </label>
          {!templateMode && (
            <label>
              Due date and time
              <input
                name="dueAt"
                type="datetime-local"
                required
                defaultValue={task ? localInput(task.due_at) : defaultDue}
              />
            </label>
          )}
        </div>
        <label>
          Description
          <textarea
            name="description"
            maxLength={10000}
            defaultValue={source?.description ?? ""}
          />
        </label>
        <StaffPicker
          label="Accountable assignee"
          value={assignee}
          onChange={setAssignee}
          options={options}
        />
        <label>
          Approval timing
          <select
            value={phase}
            onChange={(e) => {
              const next = e.target.value;
              setPhase(next);
              if (next !== "completion" && !before.length) setBefore([""]);
              if (next !== "start" && !after.length) setAfter([""]);
            }}
          >
            <option value="completion">After completion</option>
            <option value="start">Before work starts</option>
            <option value="both">Before work and after completion</option>
          </select>
        </label>
        {phase !== "completion" && (
          <Approvers
            label="Start approval"
            values={before}
            onChange={setBefore}
            options={options}
            exclude={[assignee, creator]}
          />
        )}
        {phase !== "start" && (
          <Approvers
            label="Completion approval"
            values={after}
            onChange={setAfter}
            options={options}
            exclude={[assignee]}
          />
        )}
        {templateMode && (
          <label className="staff-check">
            <input
              name="published"
              type="checkbox"
              defaultChecked={!template || !!template.published}
            />
            Publish for company staff
          </label>
        )}
        {canRevise && (
          <label>
            Revision reason
            <textarea name="reason" required minLength={3} maxLength={2000} />
            <small>
              This returns the task to draft. Submit it again to restart
              approvals. Previous decisions remain in history.
            </small>
          </label>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="form-actions">
          <button className="button" disabled={busy}>
            {busy
              ? "Saving…"
              : canRevise
                ? "Save revision"
                : clone
                  ? "Create my copy"
                  : templateMode
                    ? "Save routine template"
                    : "Save task draft"}
          </button>
        </div>
      </form>
    </details>
  );
}
export function TaskActions({
  base,
  detail,
}: {
  base: string;
  detail: TaskDetail;
}) {
  const { send, busy, error } = useStaffCommand(base),
    [note, setNote] = useState(detail.task.completion_note),
    [uploading, setUploading] = useState(false),
    [uploadError, setUploadError] = useState("");
  const router = useRouter();
  const [refreshing, startRefresh] = useTransition();
  const labels: Record<string, string> = {
    submit: "Submit task",
    start: "Start work",
    "save-progress": "Save progress",
    complete: "Submit completion",
    approve: "Approve current step",
    reject: "Reject with reason",
    cancel: "Cancel task",
  };
  const actions = detail.permittedActions.filter((a) => labels[a]);
  const canUpload =
    detail.permittedActions.includes("edit") ||
    detail.permittedActions.includes("save-progress");
  async function upload(file: File) {
    setUploading(true);
    setUploadError("");
    try {
      const intent = await workFetch(`/api/v1${base}/uploads`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ownerType: "staff_task",
          ownerId: detail.task.id,
          purpose: "task_evidence",
          filename: file.name,
          mediaType: file.type || "text/csv",
          bytes: file.size,
        }),
      });
      const prepared = (await intent.json()) as {
        data: { uploadUrl: string };
        error?: { message: string };
      };
      if (!intent.ok)
        throw new Error(prepared.error?.message ?? "Upload could not start.");
      const response = await workFetch(prepared.data.uploadUrl, {
        method: "PUT",
        headers: { "Content-Type": file.type || "text/csv" },
        body: file,
      });
      const result = (await response.json()) as { error?: { message: string } };
      if (!response.ok)
        throw new Error(result.error?.message ?? "Upload failed.");
      startRefresh(() => router.refresh());
    } catch (e) {
      setUploadError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      setUploading(false);
    }
  }
  return (
    <>
      {(actions.length > 0 || detail.blockedReason) && (
        <section className="card">
          <h2>Next action</h2>
          {detail.blockedReason && (
            <p className="notice" role="status">
              Blocked: {detail.blockedReason}
            </p>
          )}
          {actions.length > 0 && (
            <>
              <label>
                Completion or decision note
                <textarea
                  aria-label="Completion or decision note"
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  maxLength={10000}
                />
              </label>
              <div className="form-actions">
                {actions.map((action) => (
                  <button
                    key={action}
                    className={`button ${["reject", "cancel", "save-progress"].includes(action) ? "secondary" : ""}`}
                    disabled={busy || uploading || refreshing}
                    onClick={() =>
                      void send(`staff-tasks/${detail.task.id}/transition`, {
                        action,
                        expectedVersion: detail.task.version,
                        note,
                      })
                    }
                  >
                    {labels[action]}
                  </button>
                ))}
              </div>
            </>
          )}
          {error && (
            <p role="alert" className="error">
              {error}
            </p>
          )}
        </section>
      )}
      <section className="card">
        <h2>Supporting attachments</h2>
        {detail.attachments.map((file) => (
          <a
            className="row"
            key={file.id}
            href={`/api/v1${base}/attachments/${file.id}/content`}
          >
            {file.original_name} · {Math.ceil(file.bytes / 1024)} KB
          </a>
        ))}
        {!detail.attachments.length && (
          <p className="muted">No supporting files attached.</p>
        )}
        {canUpload && (
          <label>
            Attach supporting file
            <input
              type="file"
              disabled={uploading || busy || refreshing}
              accept="image/jpeg,image/png,image/webp,application/pdf,text/csv"
              onChange={(e) => {
                const file = e.currentTarget.files?.[0];
                e.currentTarget.value = "";
                if (file) void upload(file);
              }}
            />
          </label>
        )}
        {uploadError && (
          <p role="alert" className="error">
            {uploadError}
          </p>
        )}
      </section>
    </>
  );
}
export function ProjectEditor({
  base,
  options,
  project,
  members = [],
}: {
  base: string;
  options: StaffOptions;
  project?: ProjectRow;
  members?: string[];
}) {
  const { send, busy, error } = useStaffCommand(base),
    [selected, setSelected] = useState(members),
    [person, setPerson] = useState("");
  return (
    <details className="card">
      <summary>
        {project ? "Manage project and members" : "New project"}
      </summary>
      <form
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
        <label>
          Project title
          <input
            name="title"
            required
            minLength={2}
            maxLength={200}
            defaultValue={project?.title ?? ""}
          />
        </label>
        <label>
          Description
          <textarea
            name="description"
            maxLength={10000}
            defaultValue={project?.description ?? ""}
          />
        </label>
        <StaffPicker
          label="Add company member"
          required={false}
          value={person}
          onChange={setPerson}
          options={options}
          exclude={selected}
        />
        <button
          className="button secondary"
          type="button"
          disabled={!person}
          onClick={() => {
            setSelected([...new Set([...selected, person])]);
            setPerson("");
          }}
        >
          Add member
        </button>
        {selected.map((user) => (
          <div className="row" key={user}>
            <span>
              {options.people.find((p) => p.user_id === user)?.legal_name ??
                "Unavailable staff"}
            </span>
            <button
              className="button ghost"
              type="button"
              onClick={() => setSelected(selected.filter((v) => v !== user))}
            >
              Remove member
            </button>
          </div>
        ))}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <button className="button" disabled={busy}>
          Save project
        </button>
      </form>
    </details>
  );
}
export function RoutineScheduleControls({
  base,
  template,
  schedules,
  canEdit,
}: {
  base: string;
  template: TemplateRow;
  schedules: ScheduleRow[];
  canEdit: boolean;
}) {
  const { send, busy, error } = useStaffCommand(base),
    [frequency, setFrequency] = useState("daily"),
    [defaultDue] = useState(() => localInput(Date.now() + 86400000));
  function scheduleForm(schedule?: ScheduleRow) {
    return (
      <form
        key={schedule?.id ?? "new"}
        className="business-form"
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
              weekday:
                f.get("weekday") === "" ? null : Number(f.get("weekday")),
              monthDay:
                f.get("monthDay") === "" ? null : Number(f.get("monthDay")),
              dueOffsetHours: Number(f.get("dueOffsetHours")),
              expectedVersion: schedule?.version ?? 1,
            },
          );
        }}
      >
        <label>
          Frequency
          <select
            name="frequency"
            defaultValue={schedule?.frequency ?? frequency}
            onChange={(e) => setFrequency(e.target.value)}
          >
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
            <option value="monthly">Monthly</option>
          </select>
        </label>
        <label>
          Timezone
          <input
            name="timezone"
            defaultValue={schedule?.timezone ?? "Asia/Bangkok"}
            required
          />
        </label>
        <label>
          Occurrence time
          <input
            name="occurrenceTime"
            type="time"
            defaultValue={schedule?.occurrence_time ?? "09:00"}
            required
          />
        </label>
        <label>
          Start date
          <input
            name="startDate"
            type="date"
            required
            defaultValue={
              schedule?.start_date ?? new Date().toISOString().slice(0, 10)
            }
          />
        </label>
        <label>
          End date
          <input
            name="endDate"
            type="date"
            defaultValue={schedule?.end_date ?? ""}
          />
        </label>
        <label>
          Weekly weekday
          <select name="weekday" defaultValue={schedule?.weekday ?? ""}>
            <option value="">Use start weekday</option>
            {[
              "Sunday",
              "Monday",
              "Tuesday",
              "Wednesday",
              "Thursday",
              "Friday",
              "Saturday",
            ].map((day, i) => (
              <option key={day} value={i}>
                {day}
              </option>
            ))}
          </select>
        </label>
        <label>
          Monthly day
          <input
            name="monthDay"
            type="number"
            min={1}
            max={31}
            defaultValue={schedule?.month_day ?? ""}
            placeholder="Use start day"
          />
        </label>
        <label>
          Due offset in hours
          <input
            name="dueOffsetHours"
            type="number"
            required
            min={0}
            max={8760}
            defaultValue={schedule?.due_offset_hours ?? 24}
          />
        </label>
        <button className="button secondary" disabled={busy}>
          Save paused schedule
        </button>
      </form>
    );
  }
  return (
    <>
      <section className="card">
        <h2>Run routine now</h2>
        <form
          className="business-form"
          onSubmit={(e) => {
            e.preventDefault();
            void send(
              `routine-templates/${template.id}/run`,
              {
                dueAt: new Date(
                  String(new FormData(e.currentTarget).get("dueAt")),
                ).toISOString(),
              },
              "staff-tasks",
            );
          }}
        >
          <label>
            Task due date and time
            <input
              name="dueAt"
              type="datetime-local"
              required
              defaultValue={defaultDue}
            />
          </label>
          <button className="button" disabled={busy || !template.published}>
            Create occurrence
          </button>
        </form>
      </section>
      <section className="card">
        <h2>Schedules</h2>
        <p className="muted">
          Each occurrence creates independent work. Monthly days beyond
          month-end use the last day.
        </p>
        {schedules.map((s) => (
          <article className="card" key={s.id}>
            <p>
              <strong>
                {s.frequency} at {s.occurrence_time} · {s.timezone}
              </strong>{" "}
              · {s.state}
            </p>
            <p>
              Next occurrence:{" "}
              {s.next_occurrence_at
                ? new Date(s.next_occurrence_at).toLocaleString("en-GB", {
                    timeZone: s.timezone,
                  })
                : "None"}
            </p>
            {s.last_error && (
              <p role="alert">Generation failed: {s.last_error}</p>
            )}
            {canEdit && s.state !== "ended" && (
              <>
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={() =>
                    void send(
                      `routine-schedules/${s.id}/${s.state === "active" ? "pause" : "resume"}`,
                      { expectedVersion: s.version },
                    )
                  }
                >
                  {s.state === "active" ? "Pause schedule" : "Resume schedule"}
                </button>
                {s.state === "paused" && (
                  <details>
                    <summary>Edit paused schedule</summary>
                    {scheduleForm(s)}
                  </details>
                )}
              </>
            )}
          </article>
        ))}
        {canEdit && (
          <details>
            <summary>Add schedule</summary>
            <p>
              Schedules are saved paused. Resume after reviewing assignment and
              approval configuration.
            </p>
            {scheduleForm()}
          </details>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </section>
    </>
  );
}
export function RerouteControl({
  base,
  approval,
  version,
  options,
}: {
  base: string;
  approval: ApprovalView;
  version: number;
  options: StaffOptions;
}) {
  const { send, busy, error } = useStaffCommand(base),
    [user, setUser] = useState("");
  if (!approval.instance || !approval.currentApprover) return null;
  return (
    <details className="card">
      <summary>Administratively reroute current step</summary>
      <form
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
        <StaffPicker
          label="Replacement approver"
          value={user}
          onChange={setUser}
          options={options}
        />
        <label>
          Rerouting reason
          <textarea name="reason" minLength={3} maxLength={2000} required />
        </label>
        <button className="button secondary" disabled={busy}>
          Record rerouting
        </button>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
      </form>
    </details>
  );
}
export function StaffTaskLinks({ base }: { base: string }) {
  return (
    <nav className="form-actions" aria-label="Staff work">
      <Link className="button secondary" href={`${base}/work?type=staff_task`}>
        Tasks
      </Link>
      <Link className="button secondary" href={`${base}/projects`}>
        Projects
      </Link>
    </nav>
  );
}

export function ReadNotification({ base, id }: { base: string; id: string }) {
  const { send, busy, error } = useStaffCommand(base);
  return (
    <span>
      <button
        className="button ghost"
        disabled={busy}
        onClick={() => void send(`staff/notifications/${id}/read`, {})}
      >
        Mark read
      </button>
      {error && <span role="alert">{error}</span>}
    </span>
  );
}
