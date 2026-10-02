"use client";

import { useRef, useState, type FormEvent } from "react";
import Link from "next/link";
import { ArrowLeft, ArrowRight, CheckCircle2 } from "lucide-react";
import { retirementInstant, retirementLocalTime } from "@/lib/retirement";
import { formatUiDateTime } from "@/lib/ui-format";
import { workFetch } from "@/lib/work-fetch";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { LifecycleInput, LifecyclePreview } from "@/lib/people-lifecycle";
import type { PositionMoveInput, PositionMovePreview } from "@/lib/position-moves";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import styles from "./people-move-review.module.css";

export type PeopleMoveKind = "person" | "position" | "department";
export type PeopleMoveTargetKind = "position" | "department" | "level" | "branch";
type RelocationInput = {
  action: "relocate";
  targetNodeId: string;
  kind: "department";
  slug: null;
  displayName: string;
  branchId: string;
  expectedNodeVersion: number;
  expectedVersion: 0;
  reason: string;
};
type RelocationPreview = {
  impact: { staff: number; positions: number; assignments: number; child_nodes: number; warehouses: number };
  previewSnapshot: string;
  sourceBranchId: string | null;
  destinationBranchId: string;
};
type Preview =
  | { kind: "person"; input: LifecycleInput; data: LifecyclePreview }
  | { kind: "position"; input: PositionMoveInput; data: PositionMovePreview }
  | { kind: "department"; input: RelocationInput; data: RelocationPreview };

type Props = {
  base: string;
  kind: PeopleMoveKind;
  id: string;
  targetKind: PeopleMoveTargetKind;
  targetId: string;
  directory: HoldingPeopleDirectory;
  organization: OrganizationAccessData;
};

function formattedDate(value: number, timeZone: string) {
  return `${formatUiDateTime(value, timeZone)} · ${timeZone}`;
}

function moveUrl(base: string, kind: PeopleMoveKind, id: string, targetKind: PeopleMoveTargetKind, targetId: string) {
  const search = new URLSearchParams({ kind, id, targetKind, targetId });
  return `${base}/people/move?${search}`;
}

async function command<T>(url: string, input: unknown, idempotencyKey: string): Promise<T> {
  const response = await workFetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": idempotencyKey },
    body: JSON.stringify(input),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error?.message ?? result.message ?? "This change could not be reviewed. Refresh and try again.");
  return result.data as T;
}

export function PeopleMoveReview({ base, kind, id, targetKind, targetId, directory, organization }: Props) {
  const [timezone, setTimezone] = useState("Asia/Bangkok");
  const [effectiveLocal, setEffectiveLocal] = useState(() => retirementLocalTime(Date.now() + 3600000, "Asia/Bangkok"));
  const [reason, setReason] = useState("");
  const [managerId, setManagerId] = useState(() => organization.people.find(person => person.engagement_id === id)?.manager_person_id ?? "");
  const [reportsToId, setReportsToId] = useState(() => organization.positions.find(position => position.id === id)?.reports_to_position_id ?? "");
  const [managerAssignments, setManagerAssignments] = useState<Record<string, string>>({});
  const [acceptRecommendedLeave, setAcceptRecommendedLeave] = useState(false);
  const [acknowledgeLegacy, setAcknowledgeLegacy] = useState(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [complete, setComplete] = useState("");
  const submitKey = useRef<string | null>(null);

  const person = directory.people.find(item => item.id === id);
  const position = directory.positions.find(item => item.id === id);
  const department = directory.nodes.find(item => item.id === id && item.kind === "department");
  const hrPerson = organization.people.find(item => item.engagement_id === id);
  const hrPosition = organization.positions.find(item => item.id === id);
  const hrDepartment = organization.nodes.find(item => item.id === id && item.kind === "department");
  const targetPosition = targetKind === "position" ? directory.positions.find(item => item.id === targetId) : undefined;
  const targetDepartment = targetKind === "department" ? directory.nodes.find(item => item.id === targetId && item.kind === "department") : undefined;
  const targetLevel = targetKind === "level" ? directory.levels.find(item => item.id === targetId) : undefined;
  const targetBranch = targetKind === "branch" ? directory.nodes.find(item => item.id === targetId && (item.kind === "branch" || item.kind === "headquarters")) : undefined;
  const nodeName = (nodeId: string | null | undefined) => directory.nodes.find(item => item.id === nodeId)?.name ?? "Company-wide";
  const levelName = (levelId: string | null | undefined) => directory.levels.find(item => item.id === levelId)?.name ?? "Unassigned";
  const positionName = (positionId: string | null | undefined) => directory.positions.find(item => item.id === positionId)?.title ?? "Unassigned";
  const managerName = (personId: string | null | undefined) => organization.people.find(item => item.person_id === personId)?.legal_name ?? "No manager";
  const sourceName = kind === "person" ? person?.legalName : kind === "position" ? position?.title : department?.name;
  const targetName = targetPosition?.title ?? targetDepartment?.name ?? (targetLevel ? `${targetLevel.code} · ${targetLevel.name}` : undefined) ?? targetBranch?.name ?? "Unknown destination";
  const companyId = person?.companyId ?? position?.companyId ?? department?.companyId;
  const companyName = directory.companies.find(item => item.id === companyId)?.name ?? "Company";
  const sourcePath = kind === "person"
    ? [companyName, nodeName(person?.branchId), nodeName(person?.departmentId), positionName(person?.positionId)].join(" › ")
    : kind === "position" ? [companyName, nodeName(position?.branchId), nodeName(position?.departmentId), levelName(position?.levelId)].join(" › ")
      : [companyName, nodeName(department?.branchId)].join(" › ");
  const targetPath = targetKind === "position"
    ? [companyName, nodeName(targetPosition?.branchId), nodeName(targetPosition?.departmentId), targetName].join(" › ")
    : targetKind === "department" ? [companyName, nodeName(targetDepartment?.branchId), targetName].join(" › ")
      : targetKind === "level" ? `${companyName} › Company levels › ${targetName}` : `${companyName} › ${targetName}`;
  const holders = kind === "position" ? organization.people.filter(item => item.position_id === id && item.status !== "ended") : [];
  const existingTargetHolders = targetKind === "position" ? directory.people.filter(item => item.positionId === targetId).length : 0;
  const backHref = moveUrl(base, kind, id, targetKind, targetId);
  const peopleHref = `${base}/people?${kind === "person" ? "person" : kind}=${encodeURIComponent(kind === "person" ? person?.personId ?? id : id)}`;

  async function review(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (reason.trim().length < 3) return;
    setBusy(true); setError(""); setAcknowledgeLegacy(false); submitKey.current = null;
    try {
      if (kind === "person" && person && hrPerson && targetKind === "position") {
        const input: LifecycleInput = {
          expectedVersion: 0,
          kind: hrPerson.status === "onboarding" ? "onboarding" : "transfer",
          personId: person.personId,
          engagementId: person.engagementId,
          positionId: targetId,
          managerPersonId: managerId || null,
          acceptRecommendedPay: false,
          acceptRecommendedLeave,
          effectiveAt: retirementInstant(effectiveLocal, timezone),
          timezone,
          reason: reason.trim(),
          departureReason: "other",
          clearanceOwners: { equipment: null, payroll: null, finance: null },
          lastWorkingDate: null,
        };
        const data = await command<LifecyclePreview>(`/api/v1${base}/people/lifecycle/placements/preview`, input, crypto.randomUUID());
        setPreview({ kind: "person", input, data });
      } else if (kind === "position" && hrPosition && (targetKind === "department" || targetKind === "level")) {
        const input: PositionMoveInput = {
          positionId: id,
          expectedVersion: hrPosition.version,
          targetKind,
          targetId,
          effectiveAt: retirementInstant(effectiveLocal, timezone),
          timezone,
          reportsToPositionId: reportsToId || null,
          managerAssignments: holders.map(holder => ({ engagementId: holder.engagement_id, managerPersonId: (managerAssignments[holder.engagement_id] ?? holder.manager_person_id) || null })),
          reason: reason.trim(),
        };
        const data = await command<PositionMovePreview>(`/api/v1${base}/people/organization/position-moves/preview`, input, crypto.randomUUID());
        setPreview({ kind: "position", input, data });
      } else if (kind === "department" && hrDepartment && targetKind === "branch") {
        const input: RelocationInput = {
          action: "relocate", targetNodeId: id, kind: "department", slug: null,
          displayName: hrDepartment.display_name, branchId: targetId,
          expectedNodeVersion: hrDepartment.hierarchy_revision, expectedVersion: 0, reason: reason.trim(),
        };
        const data = await command<RelocationPreview>(`/api/v1${base}/people/organization/changes/preview`, input, crypto.randomUUID());
        setPreview({ kind: "department", input, data });
      } else {
        setError("This source or destination changed. Return to destination selection and choose it again.");
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Unable to preview this change.");
    } finally { setBusy(false); }
  }

  async function confirm() {
    if (!preview) return;
    setBusy(true); setError("");
    try {
      submitKey.current ??= crypto.randomUUID();
      if (preview.kind === "person") {
        await command(`/api/v1${base}/people/lifecycle/placements/submit`, {
          ...preview.input, previewToken: preview.data.token,
          ...(preview.data.legacyCaseToSupersede ? { supersedeLegacyCase: {
            id: preview.data.legacyCaseToSupersede.id,
            expectedVersion: preview.data.legacyCaseToSupersede.version,
          } } : {}),
        }, submitKey.current);
        setComplete("Person placement scheduled. Directory and GPO preparation follows the HR change.");
      } else if (preview.kind === "position") {
        await command(`/api/v1${base}/people/organization/position-moves/submit`, {
          ...preview.input, previewToken: preview.data.previewToken, expiresAt: preview.data.expiresAt,
        }, submitKey.current);
        setComplete(`Position change scheduled for ${preview.data.affectedCount} people. Approved salaries remain unchanged.`);
      } else {
        await command(`/api/v1${base}/people/organization/changes/submit`, {
          ...preview.input, previewSnapshot: preview.data.previewSnapshot,
        }, submitKey.current);
        setComplete("Department move submitted for independent review. It takes effect after approval.");
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Unable to confirm this change. Review it again.");
    } finally { setBusy(false); }
  }

  if (!sourceName || targetName === "Unknown destination") return <div className={styles.workspace}><p role="alert">This move is no longer available. Choose the source and destination again.</p><Link className="button secondary" href={backHref}>Choose destination</Link></div>;

  return <div className={styles.workspace} data-testid="people-move-review">
    <div className={styles.stepHeading}><span className={styles.step}>Step 2 of 2</span><Link href={backHref} className={styles.back} aria-disabled={busy} tabIndex={busy ? -1 : undefined} onClick={event => { if (busy) event.preventDefault(); }}><ArrowLeft size={17} /> Choose another destination</Link></div>
    <div className={styles.route}>
      <div className={styles.routeCard}><span>From · {kind}</span><strong>{sourceName}</strong><small>{sourcePath}</small></div>
      <ArrowRight className={styles.routeArrow} size={20} aria-hidden="true" />
      <div className={styles.routeCard}><span>To · {targetKind}</span><strong>{targetName}</strong><small>{targetPath}</small></div>
    </div>
    {complete ? <div className={styles.complete} role="status"><CheckCircle2 size={28} /><div><h2>Change submitted</h2><p>{complete}</p><Link className="button" href={peopleHref}>Back to People</Link></div></div> : <>
      {!preview ? <form onSubmit={event => void review(event)} className={styles.form}>
        <h2>Review change details</h2>
        {kind === "person" && <><p>{existingTargetHolders} people currently hold this destination position. This move adds a holder; it does not replace anyone.</p><label>Proposed reporting manager<select value={managerId} onChange={event => setManagerId(event.target.value)}><option value="">No manager</option>{organization.people.filter(item => item.status === "active" && item.person_id !== person?.personId).map(item => <option key={item.engagement_id} value={item.person_id}>{item.legal_name}</option>)}</select></label><label className={styles.check}><input type="checkbox" checked={acceptRecommendedLeave} onChange={event => setAcceptRecommendedLeave(event.target.checked)} /> Apply destination Level’s leave default, if configured</label></>}
        {kind === "position" && <><p>All {holders.length} non-ended placements, including scheduled starts, move with this position.</p><label>Reports to position<select value={reportsToId} onChange={event => setReportsToId(event.target.value)}><option value="">No parent position</option>{organization.positions.filter(item => item.active && item.id !== id).map(item => <option key={item.id} value={item.id}>{item.title} · L{item.level_number}</option>)}</select></label>{holders.length > 0 && <details className={styles.managerDetails}><summary>Actual reporting managers · {holders.length} people</summary><div className={styles.managerGrid}>{holders.map(holder => <label key={holder.engagement_id}>{holder.legal_name}<select value={managerAssignments[holder.engagement_id] ?? holder.manager_person_id ?? ""} onChange={event => setManagerAssignments(values => ({ ...values, [holder.engagement_id]: event.target.value }))}><option value="">No manager</option>{organization.people.filter(item => item.status === "active" && item.person_id !== holder.person_id).map(item => <option key={item.engagement_id} value={item.person_id}>{item.legal_name}</option>)}</select></label>)}</div></details>}</>}
        {kind === "department" ? <p>The department and its people, positions, and scoped access assignments move together after another authorized HR administrator approves this request.</p> : <div className={styles.dateGrid}><label>Timezone<select value={timezone} onChange={event => { setTimezone(event.target.value); setEffectiveLocal(retirementLocalTime(Date.now() + 3600000, event.target.value)); }}><option>Asia/Bangkok</option><option>UTC</option><option>Asia/Singapore</option></select></label><label>Effective date and time<input type="datetime-local" required value={effectiveLocal} onChange={event => setEffectiveLocal(event.target.value)} /></label></div>}
        <label>Reason<textarea value={reason} onChange={event => setReason(event.target.value)} minLength={3} maxLength={1000} required placeholder="Explain this change for the audit history" /></label>
        <p className={styles.assurance}>Approved salary stays unchanged. IT policy reconciliation and Directory delivery are tracked separately from the HR change.</p>
        {error && <p className={styles.error} role="alert">{error}</p>}
        <div className={styles.actions}><Link className="button secondary" href={backHref} aria-disabled={busy} tabIndex={busy ? -1 : undefined} onClick={event => { if (busy) event.preventDefault(); }}>Back</Link><Link className="button secondary" href={peopleHref} aria-disabled={busy} tabIndex={busy ? -1 : undefined} onClick={event => { if (busy) event.preventDefault(); }}>Cancel</Link><button className="button" type="submit" disabled={busy || reason.trim().length < 3}>{busy ? "Checking impact…" : "Preview impact"}</button></div>
      </form> : <div className={styles.preview}>
        <h2>Impact preview</h2>
        <p><strong>Reason:</strong> {preview.input.reason}</p>
        {preview.kind === "person" && <><div className={styles.facts}><div><span>Effective</span><strong>{formattedDate(preview.input.effectiveAt, timezone)}</strong></div><div><span>Position</span><strong>{positionName(person?.positionId)} → {targetName}</strong></div><div><span>Manager</span><strong>{managerName(hrPerson?.manager_person_id)} → {managerName(preview.input.managerPersonId)}</strong></div><div><span>Existing holders</span><strong>{existingTargetHolders}; this person is added</strong></div></div><p>{preview.data.affectedPeople.length} person affected. HR placement proceeds independently of IT delivery.</p>{preview.data.recommendation && <p>Destination leave default: {preview.data.recommendation.annualDays ?? "—"} annual days · {preview.data.recommendation.sickDays ?? "—"} sick days. {preview.input.acceptRecommendedLeave ? "Apply at the effective date." : "Current entitlement continues."}</p>}{preview.data.legacyCaseToSupersede && <label className={styles.check}><input type="checkbox" checked={acknowledgeLegacy} onChange={event => setAcknowledgeLegacy(event.target.checked)} /> Replace the older {preview.data.legacyCaseToSupersede.kind} request ({preview.data.legacyCaseToSupersede.status.replaceAll("_", " ")})</label>}</>}
        {preview.kind === "position" && <><div className={styles.facts}><div><span>Effective</span><strong>{formattedDate(preview.input.effectiveAt, timezone)}</strong></div><div><span>People</span><strong>{preview.data.affectedCount}</strong></div><div><span>Department</span><strong>{nodeName(preview.data.before.departmentId)} → {nodeName(preview.data.after.departmentId)}</strong></div><div><span>Level</span><strong>{levelName(preview.data.before.levelId)} → {levelName(preview.data.after.levelId)}</strong></div><div><span>Reports to</span><strong>{positionName(preview.data.before.reportsToPositionId)} → {positionName(preview.data.after.reportsToPositionId)}</strong></div></div><details className={styles.managerDetails}><summary>People and reporting changes · {preview.data.people.length}</summary><ul>{preview.data.people.map(item => <li key={item.engagementId}><strong>{item.name}</strong> · {levelName(item.before.levelId)} → {levelName(item.after.levelId)} · Manager: {managerName(item.before.managerPersonId)} → {managerName(item.after.managerPersonId)} · {item.leaveNote}</li>)}</ul></details><p>GPO impact: {preview.data.policyChanges.added.length} added · {preview.data.policyChanges.removed.length} removed · {preview.data.policyChanges.retained.length} retained. Directory delivery follows the HR effective date.</p></>}
        {preview.kind === "department" && <><div className={styles.facts}><div><span>Effective</span><strong>After independent approval</strong></div><div><span>People</span><strong>{preview.data.impact.staff}</strong></div><div><span>Positions</span><strong>{preview.data.impact.positions}</strong></div><div><span>Access assignments</span><strong>{preview.data.impact.assignments}</strong></div><div><span>Child units</span><strong>{preview.data.impact.child_nodes}</strong></div></div><p>A different authorized HR administrator must approve the department move. If affected records change, a new review is required.</p></>}
        <p className={styles.assurance}>Approved salary stays unchanged. IT policy reconciliation and Directory delivery are tracked separately from the HR change.</p>
        {preview.kind !== "department" && preview.data.blockers.map((blocker, index) => <p key={index} className={styles.error} role="alert">{blocker}</p>)}
        {error && <p className={styles.error} role="alert">{error}</p>}
        <div className={styles.actions}><button className="button secondary" type="button" disabled={busy} onClick={() => { setPreview(null); setError(""); }}>Edit details</button><Link className="button secondary" href={peopleHref} aria-disabled={busy} tabIndex={busy ? -1 : undefined} onClick={event => { if (busy) event.preventDefault(); }}>Cancel</Link><button className="button" type="button" disabled={busy || (preview.kind !== "department" && !preview.data.ready) || (preview.kind === "person" && !!preview.data.legacyCaseToSupersede && !acknowledgeLegacy)} onClick={() => void confirm()}>{busy ? "Submitting…" : preview.kind === "department" ? "Submit for approval" : "Confirm and schedule"}</button></div>
      </div>}
    </>}
  </div>;
}
