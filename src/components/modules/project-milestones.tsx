"use client";

import { useState, type FormEvent } from "react";
import type { ProjectMilestoneRow } from "@/server/project-milestones";
import type { StaffPerson } from "@/server/staff-policy";
import { useStaffCommand } from "./staff-work-controls";
import { localDateTimeCandidates, localMinuteAt } from "@/lib/calendar-timeline";
import { TimeText, useDisplayTimeZone } from "@/components/time/time-preference-context";
import { utcOffset } from "@/lib/ui-format";


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
  const {timeZone} = useDisplayTimeZone();
  const [editing, setEditing] = useState<ProjectMilestoneRow | null>(null);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [dueChoice, setDueChoice] = useState<number|null>(null);
  const [timeError, setTimeError] = useState("");
  const [ownerUserId, setOwnerUserId] = useState(projectOwnerId);
  const [dependsOnMilestoneIds, setDependsOnMilestoneIds] = useState<string[]>([]);
  const memberIds = new Set([projectOwnerId, ...members]);
  const owners = people.filter((person) => memberIds.has(person.user_id));

  function edit(row: ProjectMilestoneRow) {
    setEditing(row);
    setTitle(row.title);
    setDescription(row.description);
    const local = localMinuteAt(row.due_at,timeZone);
    setDueAt(`${local.date}T${local.time}`);
    setDueChoice(row.due_at);
    setOwnerUserId(memberIds.has(row.owner_user_id) ? row.owner_user_id : projectOwnerId);
    setDependsOnMilestoneIds(row.depends_on_ids);
  }

  function clear() {
    setEditing(null);
    setTitle("");
    setDescription("");
    setDueAt("");
    setDueChoice(null);setTimeError("");
    setOwnerUserId(projectOwnerId);
    setDependsOnMilestoneIds([]);
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const candidates = localDateTimeCandidates(dueAt,timeZone);
    if (!candidates.length) { setTimeError(`This local time does not exist in ${timeZone}. Choose another time.`); return; }
    if (candidates.length > 1 && !candidates.includes(dueChoice ?? -1)) { setTimeError("This time occurs twice. Choose the UTC offset below."); return; }
    setTimeError("");
    const milestonePath = `projects/${encodeURIComponent(projectId)}/milestones${editing ? `/${encodeURIComponent(editing.id)}` : ""}`;
    const result = await send(milestonePath, {
      title,
      description,
      dueAt: new Date(candidates.length === 1 ? candidates[0] : dueChoice!).toISOString(),
      ownerUserId,
      dependsOnMilestoneIds,
      expectedVersion: editing?.version ?? 1,
      reason: editing ? "Milestone details reviewed by the project owner." : "Milestone target approved by the project owner.",
    });
    if (result) clear();
  }

  async function transition(row: ProjectMilestoneRow, action: "start" | "complete" | "reopen" | "cancel") {
    await send(`projects/${encodeURIComponent(projectId)}/milestones/${encodeURIComponent(row.id)}/transition`, {
      action,
      expectedVersion: row.version,
      reason: `Milestone ${action} recorded by the project owner.`,
    });
  }

  return (
    <section className="card" aria-labelledby="project-milestones-heading">
      <h2 id="project-milestones-heading">Project milestones</h2>
      <p>Track dated project outcomes. A milestone is overdue while it is incomplete and its due time has passed.</p>
      {milestones.length ? <ul className="content-stack">
        {milestones.map((milestone) => {
          return <li className="row" key={milestone.id}>
            <div>
              <strong>{milestone.title}</strong>
              <p>{milestone.state.replaceAll("_", " ")} · Due <TimeText value={milestone.due_at} /> · {milestone.owner_name ?? milestone.owner_user_id}{milestone.is_overdue ? " · Overdue" : ""}</p>
              {milestone.depends_on_titles.length > 0 && <p>Prerequisites: {milestone.depends_on_titles.join(", ")}</p>}
              {milestone.blocked_by_count > 0 && <p role="status">Blocked until complete: {milestone.blocked_by_titles.join(", ")}</p>}
              {milestone.description && <p>{milestone.description}</p>}
            </div>
            {canManage && <div className="form-actions">
              {(milestone.state === "planned" || milestone.state === "in_progress") && <button type="button" className="button secondary" disabled={busy || milestone.blocked_by_count > 0} onClick={() => void transition(milestone, "complete")}>Complete</button>}
              {milestone.state === "planned" && <button type="button" className="button secondary" disabled={busy || milestone.blocked_by_count > 0} onClick={() => void transition(milestone, "start")}>Start</button>}
              {milestone.state === "completed" && <button type="button" className="button secondary" disabled={busy} onClick={() => void transition(milestone, "reopen")}>Reopen</button>}
              {(milestone.state === "planned" || milestone.state === "in_progress") && <button type="button" className="button ghost" disabled={busy} onClick={() => void transition(milestone, "cancel")}>Cancel</button>}
              {(milestone.state === "planned" || milestone.state === "in_progress") && <button type="button" className="button ghost" disabled={busy} onClick={() => edit(milestone)}>Edit</button>}
            </div>}
          </li>;
        })}
      </ul> : <p>No milestones have been recorded for this project.</p>}
      {canManage && <form className="content-stack" onSubmit={save}>
        <h3>{editing ? "Edit milestone" : "Add a milestone"}</h3>
        <label>Milestone title<input required minLength={2} maxLength={200} value={title} onChange={(event) => setTitle(event.target.value)} /></label>
        <label>Due date and time · {timeZone}<input required type="datetime-local" value={dueAt} onChange={(event) => {setDueAt(event.target.value);setDueChoice(null);setTimeError("");}} /></label>
        {dueAt && localDateTimeCandidates(dueAt,timeZone).length > 1 && <fieldset><legend>Choose the offset for this repeated local time</legend>{localDateTimeCandidates(dueAt,timeZone).map(instant => <label key={instant}><input type="radio" name="dueOffset" checked={dueChoice===instant} onChange={() => setDueChoice(instant)} />{utcOffset(instant,timeZone)}</label>)}</fieldset>}
        {timeError && <p role="alert">{timeError}</p>}
        <label>Responsible member<select required value={ownerUserId} onChange={(event) => setOwnerUserId(event.target.value)}>
          {owners.map((person) => <option key={person.user_id} value={person.user_id}>{person.legal_name}</option>)}
          {!owners.some((person) => person.user_id === ownerUserId) && <option value={ownerUserId}>{ownerUserId}</option>}
        </select></label>
        <label>Prerequisite milestones<select multiple size={Math.min(5, Math.max(2, milestones.filter((milestone) => milestone.id !== editing?.id && milestone.state !== "cancelled").length))} value={dependsOnMilestoneIds} onChange={(event) => setDependsOnMilestoneIds(Array.from(event.currentTarget.selectedOptions, (option) => option.value))}>
          {milestones.filter((milestone) => milestone.id !== editing?.id && milestone.state !== "cancelled").map((milestone) => <option key={milestone.id} value={milestone.id}>{milestone.title} · {milestone.state.replaceAll("_", " ")}</option>)}
        </select><span>Select any prerequisites that must be completed first. Use Ctrl or Command to select more than one.</span></label>
        <label>Description<textarea maxLength={10000} value={description} onChange={(event) => setDescription(event.target.value)} /></label>
        <div className="form-actions">
          <button className="button" type="submit" disabled={busy || !dueAt}>{editing ? "Save milestone" : "Add milestone"}</button>
          {editing && <button className="button ghost" type="button" disabled={busy} onClick={clear}>Stop editing</button>}
        </div>
      </form>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
