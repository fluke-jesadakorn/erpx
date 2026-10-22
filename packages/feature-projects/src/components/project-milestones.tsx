"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useState, type FormEvent } from "react";
import type { ProjectMilestoneRow } from "@erpx/server-projects/public/project-milestones";
import type {ProjectWorkerDisplay} from "@erpx/feature-projects/lib/worker-display";
import {useProjectRecordCommand} from "@erpx/feature-projects/components/use-project-record-command";
import {ProjectRecordRecovery} from "@erpx/feature-projects/components/project-record-recovery";
import type {ProjectRecordOwner} from "@erpx/feature-projects/lib/project-record-command";
import {useProjectPurchases} from "@erpx/feature-projects/components/purchase-context";
import {projectPurchased} from "@erpx/feature-projects/lib/purchase-controls";
import {FormDisclosure} from "@erpx/ui/components/form-disclosure";
import { localDateTimeCandidates, localMinuteAt } from "@erpx/feature-work_projection/lib/calendar-timeline";
import { TimeText, useDisplayTimeZone } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { utcOffset } from "@erpx/feature-shared/lib/ui-format";
import { Card } from "@erpx/ui/components/foundation";

export function ProjectMilestoneConsole({
  base,
  projectId,
  projectOwnerId,
  members,
  people,
  milestones,
  canManage,
  ownerContext,
}: {
  base: string;
  projectId: string;
  projectOwnerId: string;
  members: string[];
  people: readonly ProjectWorkerDisplay[];
  milestones: ProjectMilestoneRow[];
  canManage: boolean;
  ownerContext?:ProjectRecordOwner;
}) {
  const purchases=useProjectPurchases(),[editorOpen,setEditorOpen]=useState(false),newWork=projectPurchased(purchases,"projects.wbs","newWork"),settleExisting=projectPurchased(purchases,"projects.wbs","settleExisting");
  const { timeZone } = useDisplayTimeZone();
  const [createdMilestone,setCreatedMilestone]=useState<{id:string;version:number}|null>(null);
  const [editing, setEditing] = useState<ProjectMilestoneRow | null>(null);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [dueChoice, setDueChoice] = useState<number | null>(null);
  const [timeError, setTimeError] = useState("");
  const [ownerUserId, setOwnerUserId] = useState(projectOwnerId);
  const [dependsOnMilestoneIds, setDependsOnMilestoneIds] = useState<string[]>([]);
  const [parentId,setParentId]=useState(''),[position,setPosition]=useState('0');
  const [revision,setRevision]=useState(0),[dirty,setDirty]=useState(false),[editorVisited,setEditorVisited]=useState(false);
  const command=useProjectRecordCommand({kind:'milestones',base,projectId,owner:ownerContext,milestones,members,people,canManage,purchases,draftVersion:revision,dirty}),{send,busy,error,pending}=command;
  const memberIds = new Set([projectOwnerId, ...members]);
  const owners = people.filter((person) => memberIds.has(person.user_id));

  function edit(row: ProjectMilestoneRow) {
    if(pending||busy||dirty&&!window.confirm("Discard this milestone draft before editing another milestone?"))return;
    setRevision(value=>value+1);setDirty(false);setEditorVisited(true);setCreatedMilestone(null);setEditing(row);setEditorOpen(true);
    setTitle(row.title);
    setDescription(row.description);
    const local = localMinuteAt(row.due_at, timeZone);
    setDueAt(`${local.date}T${local.time}`);
    setDueChoice(row.due_at);
    setOwnerUserId(memberIds.has(row.owner_user_id) ? row.owner_user_id : projectOwnerId);
    setDependsOnMilestoneIds(row.depends_on_ids);
    setParentId(row.parent_milestone_id??'');setPosition(String(row.ordinal_position??0));
  }

  function clear() {
    if(pending||busy)return;
    reset();
  }

  function reset() {
    setDirty(false);
    setCreatedMilestone(null);setEditing(null);setEditorOpen(false);
    setTitle("");
    setDescription("");
    setDueAt("");
    setDueChoice(null);
    setTimeError("");
    setOwnerUserId(projectOwnerId);
    setDependsOnMilestoneIds([]);
    setParentId('');setPosition('0');
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if(!newWork||!canManage||busy||pending)return;
    const candidates = localDateTimeCandidates(dueAt, timeZone);
    if (!candidates.length) {
      setTimeError(`This local time does not exist in ${timeZone}. Choose another time.`);
      return;
    }
    if (candidates.length > 1 && !candidates.includes(dueChoice ?? -1)) {
      setTimeError("This time occurs twice. Choose the UTC offset below.");
      return;
    }
    setTimeError("");
    const savedId=editing?.id??createdMilestone?.id;
    const milestonePath = `projects/${encodeURIComponent(projectId)}/milestones${savedId ? `/${encodeURIComponent(savedId)}` : ""}`;
    await send(milestonePath, {
      title,
      description,
      dueAt: new Date(candidates.length === 1 ? candidates[0] : dueChoice!).toISOString(),
      ownerUserId,
      dependsOnMilestoneIds,
      parentMilestoneId:parentId||null,
      ordinalPosition:Number(position),
      expectedVersion: Math.max(editing?.version??1,createdMilestone&&createdMilestone.id===savedId?createdMilestone.version:1),
      reason: editing
        ? "Milestone details reviewed by the project owner."
        : "Milestone target approved by the project owner.",
    },event.currentTarget,{identity:result=>setCreatedMilestone({id:result.id,version:result.version}),saved:reset});
  }

  async function transition(
    row: ProjectMilestoneRow,
    action: "start" | "complete" | "reopen" | "cancel",
  ) {
    await send(
      `projects/${encodeURIComponent(projectId)}/milestones/${encodeURIComponent(row.id)}/transition`,
      {
        action,
        expectedVersion: row.version,
        reason: `Milestone ${action} recorded by the project owner.`,
      },
    );
  }

  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="project-milestones-heading">
      <h2 id="project-milestones-heading">Project milestones</h2>
      <p>
        Track dated project outcomes. A milestone is overdue while it is incomplete and its due time
        has passed.
      </p>
      {milestones.length ? (
        <ul className="ui-stack">
          {milestones.map((milestone) => {
            return (
              <li className="ui-record-item" key={milestone.id}>
                <div>
                  <strong>{milestone.title}</strong>
                  <p>
                    {milestone.state.replaceAll("_", " ")} · Due{" "}
                    <TimeText appearance="standard" value={milestone.due_at} /> ·{" "}
                    {milestone.owner_name ?? milestone.owner_user_id}
                    {milestone.is_overdue ? " · Overdue" : ""}
                  </p>
                  {milestone.depends_on_titles.length > 0 && (
                    <p>Prerequisites: {milestone.depends_on_titles.join(", ")}</p>
                  )}
                  {milestone.blocked_by_count > 0 && (
                    <InlineNotice tone="info" role="status">
                      Blocked until complete: {milestone.blocked_by_titles.join(", ")}
                    </InlineNotice>
                  )}
                  {milestone.description && <p>{milestone.description}</p>}
                </div>
                {canManage && (
                  <div className="ui-form-actions">
                    {settleExisting&&(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy || pending || milestone.blocked_by_count > 0}
                        onClick={() => void transition(milestone, "complete")}
                      >
                        Complete
                      </Button>
                    )}
                    {settleExisting&&milestone.state === "planned" && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy || pending || milestone.blocked_by_count > 0}
                        onClick={() => void transition(milestone, "start")}
                      >
                        Start
                      </Button>
                    )}
                    {newWork&&milestone.state === "completed" && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy || pending}
                        onClick={() => void transition(milestone, "reopen")}
                      >
                        Reopen
                      </Button>
                    )}
                    {settleExisting&&(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        disabled={busy || pending}
                        onClick={() => void transition(milestone, "cancel")}
                      >
                        Cancel
                      </Button>
                    )}
                    {newWork&&(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        disabled={busy || pending}
                        onClick={() => edit(milestone)}
                      >
                        Edit
                      </Button>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      ) : (
        <p>No milestones have been recorded for this project.</p>
      )}
      <ProjectRecordRecovery pending={pending} busy={busy} retry={command.retry}/>
      {command.notice&&<InlineNotice role="status">{command.notice}</InlineNotice>}
      {!editorOpen&&(pending||dirty)&&<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>setEditorOpen(true)}>Reopen retained milestone form</Button>}
      {((canManage&&newWork)||editorOpen||dirty||pending||editorVisited) && (<FormDisclosure appearance="standard" retainFormContents={editorVisited||dirty||pending} open={editorOpen} onToggle={event=>{setEditorVisited(true);setEditorOpen(event.currentTarget.open);}} onDiscard={clear} dismissDisabled={busy||pending} drawerTitle={editing?"Edit milestone":"Add a milestone"}><summary>{editing?"Edit milestone":"Add a milestone"}</summary>
        <form className="ui-stack" data-work-dirty={dirty?"true":undefined} onChangeCapture={()=>{setRevision(value=>value+1);setDirty(true);}} onSubmit={save}>
          <ProjectRecordRecovery pending={pending} busy={busy} retry={command.retry}/>
          {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
          {command.notice&&<InlineNotice role="status">{command.notice}</InlineNotice>}
          <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>setEditorOpen(false)}>Close and retain milestone form</Button>
          <h3>{editing ? "Edit milestone" : "Add a milestone"}</h3>
          <Field appearance="standard">
            Milestone title
            <TextInput appearance="standard"
              required
              minLength={2}
              maxLength={200}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Due date and time · {timeZone}
            <TextInput appearance="standard"
              required
              type="datetime-local"
              value={dueAt}
              onChange={(event) => {
                setDueAt(event.target.value);
                setDueChoice(null);
                setTimeError("");
              }}
            />
          </Field>
          {dueAt && localDateTimeCandidates(dueAt, timeZone).length > 1 && (
            <fieldset>
              <legend>Choose the offset for this repeated local time</legend>
              {localDateTimeCandidates(dueAt, timeZone).map((instant) => (
                <Field appearance="standard" orientation="inline" key={instant}>
                  <TextInput appearance="standard"
                    type="radio"
                    name="dueOffset"
                    checked={dueChoice === instant}
                    onChange={() => setDueChoice(instant)}
                  />
                  {utcOffset(instant, timeZone)}
                </Field>
              ))}
            </fieldset>
          )}
          {timeError && <InlineNotice tone="danger" role="alert">{timeError}</InlineNotice>}
          <Field appearance="standard">
            Responsible member
            <Select appearance="standard"
              required
              value={ownerUserId}
              onChange={(event) => setOwnerUserId(event.target.value)}
            >
              {owners.map((person) => (
                <option key={person.user_id} value={person.user_id}>
                  {person.legal_name}
                </option>
              ))}
              {!owners.some((person) => person.user_id === ownerUserId) && (
                <option value={ownerUserId}>{ownerUserId}</option>
              )}
            </Select>
          </Field>
          <Field appearance="standard">
            Parent work package
            <Select appearance="standard" value={parentId} onChange={e=>setParentId(e.target.value)}>
              <option value="">Top-level package</option>
              {milestones.filter(m=>m.id!==(editing?.id??createdMilestone?.id)&&['planned','in_progress'].includes(m.state)).map(m=><option key={m.id} value={m.id}>{m.title}</option>)}
            </Select>
          </Field>
          <Field appearance="standard">
            Order within the parent
            <TextInput appearance="standard" required type="number" min={0} max={1000000} step={1} value={position} onChange={e=>setPosition(e.target.value)}/>
          </Field>
          <Field appearance="standard">
            Prerequisite milestones
            <Select appearance="standard"
              multiple
              size={Math.min(
                5,
                Math.max(
                  2,
                  milestones.filter(
                    (milestone) => milestone.id !== (editing?.id??createdMilestone?.id) && milestone.state !== "cancelled",
                  ).length,
                ),
              )}
              value={dependsOnMilestoneIds}
              onChange={(event) =>
                setDependsOnMilestoneIds(
                  Array.from(event.currentTarget.selectedOptions, (option) => option.value),
                )
              }
            >
              {milestones
                .filter(
                  (milestone) => milestone.id !== (editing?.id??createdMilestone?.id) && milestone.state !== "cancelled",
                )
                .map((milestone) => (
                  <option key={milestone.id} value={milestone.id}>
                    {milestone.title} · {milestone.state.replaceAll("_", " ")}
                  </option>
                ))}
            </Select>
            <span>
              Select any prerequisites that must be completed first. Use Ctrl or Command to select
              more than one.
            </span>
          </Field>
          <Field appearance="standard">
            Description
            <TextArea appearance="standard"
              maxLength={10000}
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          </Field>
          <div className="ui-form-actions">
            <Button appearance="standard" variant="primary" clientManaged type="submit" disabled={busy || pending || !dueAt || !newWork || !canManage}>
              {editing ? "Save milestone" : "Add milestone"}
            </Button>
            {editing && (
              <Button appearance="standard" variant="ghost" type="button" disabled={busy || pending} onClick={()=>{if(!pending&&!busy&&(!dirty||window.confirm("Discard this milestone draft?")))clear();}}>
                Stop editing
              </Button>
            )}
          </div>
        </form></FormDisclosure>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </Card>
  );
}
