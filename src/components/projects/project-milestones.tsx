"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState, type FormEvent } from "react";
import type { ProjectMilestoneRow } from "@/server/modules/projects/public/project-milestones";
import type { StaffPerson } from "@/server/modules/operations/public/staff-policy";
import { useStaffCommand } from "@/components/work_projection/staff-work-controls";
import { localDateTimeCandidates, localMinuteAt } from "@/lib/work_projection/calendar-timeline";
import { TimeText, useDisplayTimeZone } from "@/components/people_payroll/time/time-preference-context";
import { utcOffset } from "@/lib/shared/ui-format";
import { Card } from "@/components/ui/foundation";

export function ProjectMilestoneConsole({
  base,
  projectId,
  projectOwnerId,
  members,
  people,
  milestones,
  canManage,
}: {
  base: string;
  projectId: string;
  projectOwnerId: string;
  members: string[];
  people: StaffPerson[];
  milestones: ProjectMilestoneRow[];
  canManage: boolean;
}) {
  const { send, busy, error } = useStaffCommand(base);
  const { timeZone } = useDisplayTimeZone();
  const [editing, setEditing] = useState<ProjectMilestoneRow | null>(null);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [dueChoice, setDueChoice] = useState<number | null>(null);
  const [timeError, setTimeError] = useState("");
  const [ownerUserId, setOwnerUserId] = useState(projectOwnerId);
  const [dependsOnMilestoneIds, setDependsOnMilestoneIds] = useState<string[]>([]);
  const [parentId,setParentId]=useState(''),[position,setPosition]=useState('0');
  const memberIds = new Set([projectOwnerId, ...members]);
  const owners = people.filter((person) => memberIds.has(person.user_id));

  function edit(row: ProjectMilestoneRow) {
    setEditing(row);
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
    setEditing(null);
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
    const milestonePath = `projects/${encodeURIComponent(projectId)}/milestones${editing ? `/${encodeURIComponent(editing.id)}` : ""}`;
    const result = await send(milestonePath, {
      title,
      description,
      dueAt: new Date(candidates.length === 1 ? candidates[0] : dueChoice!).toISOString(),
      ownerUserId,
      dependsOnMilestoneIds,
      parentMilestoneId:parentId||null,
      ordinalPosition:Number(position),
      expectedVersion: editing?.version ?? 1,
      reason: editing
        ? "Milestone details reviewed by the project owner."
        : "Milestone target approved by the project owner.",
    });
    if (result) clear();
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
                    {(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy || milestone.blocked_by_count > 0}
                        onClick={() => void transition(milestone, "complete")}
                      >
                        Complete
                      </Button>
                    )}
                    {milestone.state === "planned" && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy || milestone.blocked_by_count > 0}
                        onClick={() => void transition(milestone, "start")}
                      >
                        Start
                      </Button>
                    )}
                    {milestone.state === "completed" && (
                      <Button appearance="standard"
                        variant="secondary"
                        type="button"
                        disabled={busy}
                        onClick={() => void transition(milestone, "reopen")}
                      >
                        Reopen
                      </Button>
                    )}
                    {(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        disabled={busy}
                        onClick={() => void transition(milestone, "cancel")}
                      >
                        Cancel
                      </Button>
                    )}
                    {(milestone.state === "planned" || milestone.state === "in_progress") && (
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        disabled={busy}
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
      {canManage && (
        <form className="ui-stack" onSubmit={save}>
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
              {milestones.filter(m=>m.id!==editing?.id&&['planned','in_progress'].includes(m.state)).map(m=><option key={m.id} value={m.id}>{m.title}</option>)}
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
                    (milestone) => milestone.id !== editing?.id && milestone.state !== "cancelled",
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
                  (milestone) => milestone.id !== editing?.id && milestone.state !== "cancelled",
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
            <Button appearance="standard" variant="primary" clientManaged type="submit" disabled={busy || !dueAt}>
              {editing ? "Save milestone" : "Add milestone"}
            </Button>
            {editing && (
              <Button appearance="standard" variant="ghost" type="button" disabled={busy} onClick={clear}>
                Stop editing
              </Button>
            )}
          </div>
        </form>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </Card>
  );
}
