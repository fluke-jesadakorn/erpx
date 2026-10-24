"use client";
import {RoutineSchedules} from "@erpx/feature-operations/components/routine-schedule-controls";
import {useProjectEditorCommand} from "@erpx/feature-projects/components/use-project-editor-command";
import {useProjectPurchases} from "@erpx/feature-projects/components/purchase-context";
import type {ProjectMemberPickerOptions} from "@erpx/feature-projects/lib/worker-display";
import {projectPurchased} from "@erpx/feature-projects/lib/purchase-controls";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useLayoutEffect, useRef, useState, type ComponentProps } from "react";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@erpx/feature-shared/lib/ai/guided-intake";
import { preparedTaskFields } from "@erpx/feature-shared/lib/ai/prepared-work";
import { localMinuteAt } from "@erpx/feature-work_projection/lib/calendar-timeline";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import { TimeText, useDisplayTimeZone } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useStaffCommand } from "@erpx/feature-work_projection/components/use-staff-command";

import type { StaffPerson } from "@erpx/server-operations/public/staff-policy";
import type {TaskDetail,TaskRow,ProjectRow} from '@erpx/server-operations/public/staff-tasks';
import type { TemplateRow } from "@erpx/server-operations/public/routines";
import type {ApprovalView} from '@erpx/server-workflow/public/approvals';
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";
import { type UiAppearance } from "@erpx/ui/components/foundation";

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
type StaffPickerOptions={
 people:readonly Pick<StaffPerson,'user_id'|'legal_name'|'department_id'|'level_id'|'level_name'>[];
 levels:readonly Pick<StaffOptions['levels'][number],'id'|'name'|'rank'>[];
 departments:readonly Pick<StaffOptions['departments'][number],'id'|'display_name'>[];
};
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
  options: StaffPickerOptions;
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
export function Approvers({ appearance = "legacy",
  label,
  values,
  onChange,
  options,
  exclude,
}: ({
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  options: StaffPickerOptions;
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
function ProjectEditorRecovery({appearance,pending,busy,retry}:{appearance:UiAppearance;pending:boolean;busy:boolean;retry:()=>Promise<unknown>}){
 if(!pending)return null;
 return <div data-ui-appearance={appearance} className={appearance==='standard'?'ui-stack':'notice'} role="status"><p data-ui-appearance={appearance}>The original Project request is not confirmed. Retain your newer draft and recover it before submitting another record.</p><Button appearance={appearance} type="button" variant="secondary" disabled={busy} onClick={()=>void retry()}>Recover original Project request</Button></div>;
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
  fixedProjectId,
  projectEditable=true,
}: ({
  base: string;
  options: StaffPickerOptions;
  projects: Pick<ProjectRow,"id"|"title">[];
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
  fixedProjectId?: string;
  projectEditable?:boolean;
}) & { appearance?: UiAppearance }) {
  const { timeZone, ready: timeReady } = useDisplayTimeZone();
  const [defaultDue] = useState(() =>
    initialDueDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDueDate) ? `${initialDueDate}T09:00` : "",
  );
  const creationForm = useRef<HTMLFormElement>(null);
  const adoptedCreationDraft = useRef(false);
  const [savedProjectTask,setSavedProjectTask]=useState<{id:string;version:number;state:string;projectId:string|null;kind:"task"|"routine"}|null>(null);
  const ordinaryCommand=useStaffCommand(base),purchases=useProjectPurchases(),[selectedProjectId,setSelectedProjectId]=useState(fixedProjectId??task?.project_id??initialProjectId??"");
  const source = task ?? template,
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
  const [draftVersion,setDraftVersion]=useState(0),[dirty,setDirty]=useState(false),[editorOpen,setEditorOpen]=useState(false),[visited,setVisited]=useState(false);
  const nativeRecord=task?{kind:'task' as const,id:task.id,version:task.version,projectId:task.project_id,state:task.state}:template?{kind:'routine' as const,id:template.id,version:template.version,projectId:template.project_id}:null;
  const projectCommand=useProjectEditorCommand({kind:'task',base,scope:companyId?{companyId,actorId:userId,available:projectEditable,record:nativeRecord}:null,record:nativeRecord,projects,options,purchases,draftVersion,dirty:Boolean(selectedProjectId||task?.project_id||template?.project_id||savedProjectTask)&&dirty});
  const affiliated=Boolean(selectedProjectId||task?.project_id||template?.project_id||savedProjectTask||projectCommand.pending),{busy,error}=affiliated?projectCommand:ordinaryCommand;
  const available=projectCommand.scope?.available??projectEditable;
  // Adopt text entered into this server-rendered creation form before any
  // controlled rerender can replace it with the initial empty draft.
  useLayoutEffect(() => {
    if (source || clone || adoptedCreationDraft.current) return;
    const form = creationForm.current;
    if (!form) return;
    adoptedCreationDraft.current = true;
    const enteredTitle = form.querySelector<HTMLInputElement>('input[name="title"]');
    const enteredDescription = form.querySelector<HTMLTextAreaElement>('textarea[name="description"]');
    if (enteredTitle) setTitle(enteredTitle.value);
    if (enteredDescription) setDescription(enteredDescription.value);
  }, [source, clone]);
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
  const templateMode = savedProjectTask?savedProjectTask.kind==="routine":!!template || createKind === "routine";
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
    setDraftVersion(value=>value+1);setDirty(true);
    const prepared = preparedTaskFields(guided, options.people);
    if (prepared.title) setTitle(prepared.title);
    if (prepared.description) setDescription(prepared.description);
    if (prepared.dueLocal) setDue(prepared.dueLocal);
    if (prepared.assigneeId) setAssignee(prepared.assigneeId);
    clearPrepared();
  }
  if(!available&&!dirty&&!projectCommand.pending&&!visited)return null;
  return (<>
    <ProjectEditorRecovery appearance={appearance} pending={projectCommand.pending} busy={projectCommand.busy} retry={projectCommand.retry}/>
    {projectCommand.notice&&<p data-ui-appearance={appearance} role="status">{projectCommand.notice}</p>}
    {!!source&&!editorOpen&&(projectCommand.pending||dirty)&&<Button appearance={appearance} variant="secondary" disabled={busy} onClick={()=>setEditorOpen(true)}>Reopen retained task form</Button>}
    <FormDisclosure appearance={appearance} inline={Boolean(!source || clone)} retainFormContents={visited||dirty||projectCommand.pending} dismissDisabled={busy||projectCommand.pending} open={!source||clone||editorOpen} onToggle={event=>{setVisited(true);setEditorOpen(event.currentTarget.open);}} className={appearance === "standard" ? "ui-panel" : "card"}>
      <ProjectEditorRecovery appearance={appearance} pending={projectCommand.pending} busy={projectCommand.busy} retry={projectCommand.retry}/>
      {!!source&&<Button appearance={appearance} variant="secondary" disabled={busy} onClick={()=>setEditorOpen(false)}>Close and retain task form</Button>}
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
      <form data-ui-appearance={appearance} data-work-dirty={dirty?"true":undefined}
        onChangeCapture={()=>{setDraftVersion(value=>value+1);setDirty(true);}}
        ref={creationForm}
        onSubmit={(e) => {
          e.preventDefault();
          if(!available||busy||projectCommand.pending)return;
          const f = new FormData(e.currentTarget),
            routine = savedProjectTask?savedProjectTask.kind==="routine":!fixedProjectId&&(f.get("recordKind") === "routine" || templateMode);
          const payload = {
            title: f.get("title"),
            description: f.get("description"),
            assigneeUserId: assignee,
            projectId: fixedProjectId??(f.get("projectId") || null),
            startApprovers: phase === "start" || phase === "both" ? before : [],
            completionApprovers: phase === "completion" || phase === "both" ? after : [],
            expectedVersion: Math.max(!savedProjectTask||source?.id===savedProjectTask.id?source?.version??1:1,savedProjectTask?.version??1,!savedProjectTask||projectCommand.scope?.record?.id===savedProjectTask.id?projectCommand.scope?.record?.version??1:1),
            ...(routine
              ? { published: f.get("published") === "on" }
              : {
                  dueAt: f.get("dueAt") || null,
                  reason: f.get("reason") ?? "",
                }),
          };
          const path = routine
            ? `routine-templates${savedProjectTask?.kind==="routine"?`/${encodeURIComponent(savedProjectTask.id)}`:template ? `/${encodeURIComponent(template.id)}${clone ? "/clone" : ""}` : ""}`
            : `staff-tasks${task||savedProjectTask ? `/${encodeURIComponent(task?.id??savedProjectTask!.id)}${canRevise||savedProjectTask&&savedProjectTask.state!=="draft" ? "/revise" : ""}` : ""}`;
          if(affiliated&&!projectPurchased(purchases,"projects.tasks","newWork"))return;
          if(affiliated)void projectCommand.send(path,payload,e.currentTarget,{destination:routine?"routine-templates":"staff-tasks",identity:result=>setSavedProjectTask({id:result.id,version:result.version,state:result.state??"draft",projectId:typeof payload.projectId==="string"?payload.projectId:null,kind:routine?"routine":"task"}),saved:()=>setDirty(false)});else void ordinaryCommand.send(path,payload,routine?"routine-templates":"staff-tasks");
        }}
      >
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-grid" : "business-form"}>
          {!source && !fixedProjectId && (
            <Field appearance={appearance}>
              Create as
              <Select appearance={appearance}
                name="recordKind"
                disabled={busy||projectCommand.pending||!!savedProjectTask}
                value={createKind}
                onChange={(e) => {
                  if(busy||projectCommand.pending||savedProjectTask)return;
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
            {fixedProjectId?<><input type="hidden" name="projectId" value={fixedProjectId}/><span>{projects.find(project=>project.id===fixedProjectId)?.title}</span></>:<Select appearance={appearance} name="projectId" value={selectedProjectId} disabled={busy||projectCommand.pending} onChange={event=>{if(!busy&&!projectCommand.pending)setSelectedProjectId(event.target.value);}}>
              <option data-ui-appearance={appearance} value="">Standalone work</option>
              {projects.map((p) => (
                <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                  {p.title}
                </option>
              ))}
            </Select>}
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
        {(canRevise||!!savedProjectTask) && (
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
          <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy || projectCommand.pending || !available || (affiliated&&!projectPurchased(purchases,"projects.tasks","newWork"))}>
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
    </FormDisclosure></>
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
  options: StaffPickerOptions;
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
  commandOwner,
  available=true,
}: ({
  base: string;
  options: ProjectMemberPickerOptions;
  project?: ProjectRow;
  members?: string[];
  commandOwner?:{companyId:string;actorId:string};
  available?:boolean;
}) & { appearance?: UiAppearance }) {
  const [selected, setSelected] = useState(members),
    [person, setPerson] = useState("");
  const purchases=useProjectPurchases(),[formGeneration,setFormGeneration]=useState(0),[savedProject,setSavedProject]=useState<{id:string;version:number;title:string;description:string;memberUserIds:string[]}|null>(null);
  const projectId=project?.id??savedProject?.id;
  const [draftVersion,setDraftVersion]=useState(0),[dirty,setDirty]=useState(false),[editorOpen,setEditorOpen]=useState(false),[visited,setVisited]=useState(false);
  const nativeRecord=project?{kind:'project' as const,id:project.id,version:project.version,lifecycleStatus:project.lifecycle_status}:null;
  const command=useProjectEditorCommand({kind:'project',base,scope:commandOwner?{...commandOwner,available,record:nativeRecord}:null,record:nativeRecord,projects:[],options,members,purchases,draftVersion,dirty}),{busy,error,pending}=command;
  const newWork=projectPurchased(purchases,"projects.wbs","newWork")&&(command.scope?.available??available);
  if(!newWork&&!dirty&&!pending&&!visited)return null;
  return (<>
    <ProjectEditorRecovery appearance={appearance} pending={pending} busy={busy} retry={command.retry}/>
    {command.notice&&<p data-ui-appearance={appearance} role="status">{command.notice}</p>}
    {!editorOpen&&(pending||dirty)&&<Button appearance={appearance} variant="secondary" disabled={busy} onClick={()=>setEditorOpen(true)}>Reopen retained Project form</Button>}
    <FormDisclosure appearance={appearance} retainFormContents={visited||dirty||pending} open={editorOpen} onToggle={event=>{setVisited(true);setEditorOpen(event.currentTarget.open);}} dismissDisabled={busy||pending} onDiscard={()=>{if(busy||pending)return;setDirty(false);setSelected(savedProject?.memberUserIds??members);setPerson("");setFormGeneration(value=>value+1);}} className={appearance === "standard" ? "ui-panel" : "card"}>
      <ProjectEditorRecovery appearance={appearance} pending={pending} busy={busy} retry={command.retry}/>
      {error&&<p data-ui-appearance={appearance} className="error" role="alert">{error}</p>}
      <Button appearance={appearance} variant="secondary" disabled={busy} onClick={()=>setEditorOpen(false)}>Close and retain Project form</Button>
      <summary data-ui-appearance={appearance}>{projectId ? "Manage project and members" : "New project"}</summary>
      <form key={formGeneration} data-ui-appearance={appearance} data-work-dirty={dirty?"true":undefined} onChangeCapture={()=>{setDraftVersion(value=>value+1);setDirty(true);}}
        onSubmit={(e) => {
          e.preventDefault();
          if(!newWork||pending||busy)return;
          const f = new FormData(e.currentTarget);
          const input={title:String(f.get("title")??""),description:String(f.get("description")??""),memberUserIds:[...selected],expectedVersion:Math.max(project?.version??1,savedProject?.version??1,command.scope?.record?.version??1)};
          void command.send(
            `projects${projectId ? `/${encodeURIComponent(projectId)}` : ""}`,
            input,
            e.currentTarget,{destination:"projects",identity:result=>setSavedProject({...input,id:result.id,version:result.version}),saved:()=>setDirty(false)},
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
            defaultValue={savedProject?.title??project?.title ?? ""}
          />
        </Field>
        <Field appearance={appearance}>
          Description
          <TextArea appearance={appearance}
            name="description"
            maxLength={10000}
            defaultValue={savedProject?.description??project?.description ?? ""}
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
          disabled={!person||busy||pending||!newWork}
          onClick={() => {
            if(busy||pending||!newWork)return;setDraftVersion(value=>value+1);setDirty(true);
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
              disabled={busy||pending||!newWork}
              onClick={() => {if(busy||pending||!newWork)return;setDraftVersion(value=>value+1);setDirty(true);setSelected(selected.filter((v) => v !== user));}}
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
        <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy||pending||!newWork}>
          Save project
        </Button>
      </form>
    </FormDisclosure></>
  );
}
export function RoutineScheduleControls(props:ComponentProps<typeof RoutineSchedules>){return <RoutineSchedules {...props}/>;}
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
      <UiActionLink appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/work?type=staff_task`)}>
        Tasks
      </UiActionLink>
      <UiActionLink appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/projects`)}>
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
