"use client";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {ActionLink as UiActionLink, Button, Field, FormLayout, InlineNotice, Panel, Select, TextArea, TextInput} from "@erpx/ui/components/foundation";
import {TimeText} from "@erpx/feature-people_payroll/components/time/time-preference-context";
import {useEffect, useRef, useState, type FormEvent} from "react";
import {contextIsSwitching, registerUnsavedWork} from "@erpx/feature-work_projection/lib/context-switch";
import Link from "@erpx/workplace/components/workspace-link";
import {ShieldCheck, BriefcaseBusiness, ClipboardList, ArrowRight} from "lucide-react";
import {PeopleFormDrawer} from "@erpx/feature-people_payroll/components/people-form-drawer";
import {peopleLifecyclePurchased} from "@erpx/feature-people_payroll/lib/purchase-controls";
import {retirementLocalTime, retirementInstant, nextRetirementDay} from "@erpx/feature-people_payroll/lib/retirement";
import {hasLifecycleCaseDetail, type AccessPackageDefinition, type LifecycleCase, type LifecycleInput, type LifecyclePreview, type PeopleLifecycleData} from "@erpx/feature-people_payroll/lib/people-lifecycle";
import {personGpoStatus, positionGpoStatus} from "@erpx/feature-people_payroll/components/gpo-status";
import {useDirectoryOwnerCommand} from "@erpx/feature-people_payroll/lib/use-directory-owner-command";
import styles from "@erpx/feature-people_payroll/components/directory-lifecycle-panel.module.css";
export function AccessRecipe({ definition }: { definition: AccessPackageDefinition }) {
  return (
    <div className={styles.recipe}>
      <p>
        <strong>{definition.name}</strong> ·{" "}
        {definition.account === "local"
          ? "Local ERPX account"
          : definition.account === "univention"
            ? "UCS account"
            : "Active Directory account"}
      </p>
      <div>
        <ShieldCheck size={17} />
        <span>
          {definition.grants.length
            ? definition.grants.map((g, index) => (
                <span key={`${g.roleId}:${index}`}>
                  {index > 0 ? "; " : ""}
                  {g.roleId} · {g.scopeKind === "holding" ? "Company" : g.scopeKind} · until{" "}
                  <TimeText value={g.validUntil} />
                </span>
              ))
            : "No ERPX roles requested"}
        </span>
      </div>
      <div>
        <BriefcaseBusiness size={17} />
        <span>
          {definition.groupIds.length} ERPX groups · {definition.directoryGroupIds.length} directory
          groups · {definition.windowsPolicies.length} Windows policies
        </span>
      </div>
      {definition.tasks.map((t, i) => (
        <div key={i}>
          <ClipboardList size={17} />
          <span>{t.label}</span>
        </div>
      ))}
      {definition.windowsPolicies.map((p) => (
        <p key={p.id}>
          {p.name} ·{" "}
          {p.processor === "admx" ? "Windows administrative policy" : "Drive / printer preference"}{" "}
          · OU: {p.targetOu}
          {p.computerTarget ? ` · Computers: ${p.computerTarget}` : ""} · IT verification required
        </p>
      ))}
    </div>
  );
}
export function DirectoryLifecyclePanel({
  base,
  data,
  person,
  initialCase,
  initialPositionId,
  initialKind,
  parentOpen = true,
  onCloseRetained,
}: {
  base: string;
  data: PeopleLifecycleData;
  person: {
    id: string;
    engagementId: string;
    positionId: string | null;
    managerPersonId: string | null;
    name: string;
    status?: string;
  };
  initialCase?: LifecycleCase;
  initialPositionId?: string;
  initialKind?: LifecycleInput["kind"];
  parentOpen?: boolean;
  onCloseRetained?: () => void;
}) {
  const canChange=(kind:string)=>peopleLifecyclePurchased(data.purchases,kind);
  const editableInitial =
    initialCase &&
    initialCase.workflow !== "legacy" &&
    ["draft", "waiting_it"].includes(initialCase.status)
      ? initialCase
      : undefined;
  const [editVersion, setEditVersion] = useState(0), [edited, setEdited] = useState(false);
  const dirtyOwners = useRef(new Set<"change" | "rehire">()), editRevision = useRef(0), draftForms = useRef<Partial<Record<"change" | "rehire", HTMLFormElement>>>({});
  const {send: sendOwned, busy, error, notice, pending, retry} = useDirectoryOwnerCommand("lifecycle", {base, ownerKey: person.id, source: JSON.stringify([data, person]), purchases: data.purchases, canManage: data.canManage, canReview: data.canReview, personId: person.id, engagementId: person.engagementId, cases: data.cases.filter(item => item.person_id === person.id), draftVersion: editVersion, hasDraft: edited}),
    [open, setOpen] = useState(!!editableInitial || !!initialPositionId),
    [kind, setKind] = useState<LifecycleInput["kind"]>(
      initialCase?.kind ??
        initialKind ??
        (person.status === "onboarding" ? "onboarding" : "transfer"),
    ),
    [preview, setPreview] = useState<LifecyclePreview | null>(null),
    [draft, setDraftState] = useState<LifecycleInput | null>(
      editableInitial
        ? {
            ...editableInitial.definition,
            id: editableInitial.id,
            expectedVersion: editableInitial.version,
          }
        : null,
    ),
    [step, setStep] = useState(1),
    [confirmSupersede, setConfirmSupersede] = useState(false),
    [initialTime] = useState(() => Date.now() + 3600000),
    [rehireOpen, setRehireOpen] = useState(false);
  const [departureReason, setDepartureReason] = useState<"retirement" | "other">(
    initialCase?.definition.departureReason ?? "other",
  );
  const [timezone, setTimezone] = useState(initialCase?.definition.timezone ?? "Asia/Bangkok");
  const [effectiveLocal, setEffectiveLocal] = useState(() =>
    retirementLocalTime(
      initialCase?.effective_at ?? initialTime,
      initialCase?.definition.timezone ?? "Asia/Bangkok",
    ),
  );
  const [dateError, setDateError] = useState("");
  const [retainedDrawer, setRetainedDrawer] = useState<"change" | "rehire" | null>(null);
  function advanceRevision() {editRevision.current++;setEditVersion(editRevision.current);}
  function markDraft(event:FormEvent<HTMLElement>) {
    const form=(event.target as HTMLElement).closest<HTMLFormElement>("form[data-directory-lifecycle-draft]"),owner=form?.dataset.directoryLifecycleOwner;
    if (!form || owner!=="change" && owner!=="rehire") return;
    draftForms.current[owner]=form;dirtyOwners.current.add(owner);setEdited(true);advanceRevision();
  }
  function clearDraft(owner:"change"|"rehire") {dirtyOwners.current.delete(owner);setEdited(dirtyOwners.current.size>0);}
  function setDraft(value: LifecycleInput | null) {advanceRevision();setDraftState(value);}
  function send<T>(path:string,input:unknown,form?:HTMLFormElement,onCompleted?:(data:T)=>void) {
    const submittedRevision=editRevision.current,owner=path==="rehire"?"rehire":path==="placements/draft"||path==="placements/submit"?"change":null;
    return sendOwned<T>(path,input,form,data=>{if(owner&&editRevision.current===submittedRevision)clearDraft(owner);onCompleted?.(data);});
  }
  function discardDraft(owner:"change"|"rehire") {
    if (pending || busy) return;
    draftForms.current[owner]?.reset();clearDraft(owner);advanceRevision();setRetainedDrawer(null);
    if (owner==="rehire") {setRehireOpen(false);return;}
    setDraftState(editableInitial?{...editableInitial.definition,id:editableInitial.id,expectedVersion:editableInitial.version}:null);
    setPreview(null);setStep(1);setConfirmSupersede(false);setKind(initialCase?.kind??initialKind??(person.status==="onboarding"?"onboarding":"transfer"));setDepartureReason(initialCase?.definition.departureReason??"other");setTimezone(initialCase?.definition.timezone??"Asia/Bangkok");setEffectiveLocal(retirementLocalTime(initialCase?.effective_at??initialTime,initialCase?.definition.timezone??"Asia/Bangkok"));setOpen(false);
  }
  useEffect(()=>{
    const dirty=()=>dirtyOwners.current.size>0,unregister=registerUnsavedWork(dirty),unload=(event:BeforeUnloadEvent)=>{if(dirty()&&!contextIsSwitching())event.preventDefault();};
    window.addEventListener("beforeunload",unload);return()=>{unregister();window.removeEventListener("beforeunload",unload);};
  },[]);
  const cases = data.cases.filter(
    (c): c is LifecycleCase => c.person_id === person.id && hasLifecycleCaseDetail(c),
  );
  const gpoDelivery =
    data.roster.find((p) => p.engagementId === person.engagementId)?.gpoDelivery ?? "none";
  return (
    <Panel appearance="standard" data-directory-command-pending={pending || busy ? "true" : undefined} data-directory-command-busy={busy ? "true" : undefined} data-directory-owner-dirty={edited ? "true" : undefined} onInputCapture={markDraft} onChangeCapture={markDraft}>
      {pending && <InlineNotice><p>The original employment request may have completed. Recover it before submitting a different request.</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => void retry()}>Recover original employment request</Button>{onCloseRetained && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => {if (!busy) onCloseRetained();}}>Close and retain person details</Button>}</InlineNotice>}
      {retainedDrawer && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => {if (!busy) {if (retainedDrawer === "rehire") setRehireOpen(true);else setOpen(true);setRetainedDrawer(null);}}}>Reopen retained employment form</Button>}
      {notice && <InlineNotice>{notice}</InlineNotice>}
      <div className={styles.heading}>
        <div>
          <h2>Employment & position</h2>
          <p>HR placement is managed here. {personGpoStatus(gpoDelivery)}.</p>
          <p className={styles.muted}>
            Directory verification does not confirm application on an employee device.
          </p>
          {data.canPrepare && (
            <Link href={canonicalUiHref(`${base}/technology?view=employees`)}>Open in Technology</Link>
          )}
          <p className={styles.muted}>Onboard, transfer, suspend, leave, or retire in this Company.</p>
        </div>
        {data.canManage && data.purchases?.lifecycle.newWork!==false && person.status !== "ended" && (
          <Button appearance="standard"
            type="submit"
            clientManaged
            variant="primary"

            disabled={busy || pending || edited}
            onClick={() => {
              if (busy || pending || dirtyOwners.current.size) return;
              setRetainedDrawer(null);
              setDraft(null);
              setDepartureReason("other");
              setPreview(null);
              setConfirmSupersede(false);
              setKind(person.status === "onboarding" ? "onboarding" : "transfer");
              setOpen(true);
              setStep(1);
            }}
          >
            Start a change
          </Button>
        )}
      </div>
      {data.canManage && data.purchases?.lifecycle.settleExisting!==false && person.status !== "ended" && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          disabled={busy || pending || edited}
          onClick={() => {
            if (busy || pending || dirtyOwners.current.size) return;
            setRetainedDrawer(null);
            setDraft(null);
            setDepartureReason("retirement");
            setKind("offboarding");
            setPreview(null);
            setStep(1);
            setOpen(true);
          }}
        >
          Retire employee
        </Button>
      )}
      {data.canManage && data.purchases?.lifecycle.newWork!==false && person.status === "ended" && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"

          disabled={busy || pending || edited}
          onClick={() => {if (!busy && !pending && !dirtyOwners.current.size) {setRetainedDrawer(null);setRehireOpen(true);}}}
        >
          Create a new engagement for rehire
        </Button>
      )}
      <PeopleFormDrawer appearance="standard"
        dismissDisabled={busy || pending}
        open={rehireOpen && parentOpen}
        retainFormContents
        onClose={() => {if (!busy && !pending) setRehireOpen(false);}}
        onDiscard={() => discardDraft("rehire")}
        title="Rehire · new engagement"
      >
        {pending && <InlineNotice><p>The original employment request may have completed. Recover it before submitting a different request; your newer draft is retained.</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => void retry()}>Recover original employment request</Button><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => {if (!busy) {setRetainedDrawer("rehire");setRehireOpen(false);}}}>Close and retain request</Button></InlineNotice>}
        <FormLayout layout="stack" data-directory-lifecycle-draft data-directory-lifecycle-owner="rehire"
          className={styles.stack}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            await send("rehire", {
                personId: person.id,
                engagementId: person.engagementId,
                positionId: f.get("position"),
                startDate: f.get("startDate"),
                reason: f.get("reason"),
              }, e.currentTarget, () => setRehireOpen(false));
          }}
        >
          <p>
            Previous employment and revoked access remain in history. The new engagement starts with
            onboarding and requires new access review.
          </p>
          <Field appearance="standard">
            New position
            <Select appearance="standard" name="position" required>
              <option value="">Choose a position</option>
              {data.positions
                .filter((p) => p.active)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.title} · {data.nodes.find((n) => n.id === p.branch_id)?.name ?? "Company"} ·{" "}
                    {data.nodes.find((n) => n.id === p.department_id)?.name ?? "All departments"}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Start date
            <TextInput appearance="standard" type="date" name="startDate" required />
          </Field>
          <Field appearance="standard">
            Reason
            <TextInput appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || data.purchases?.lifecycle.newWork===false}>
            Create onboarding engagement
          </Button>
          {error && <p role="alert">{error}</p>}
        </FormLayout>
      </PeopleFormDrawer>
      {cases.map((c) => (
        <div className={styles.case} key={c.id}>
          <strong>
            {c.definition.departureReason === "retirement" ? "Retirement" : c.kind} ·{" "}
            {c.status.replaceAll("_", " ")}
          </strong>
          <p>
            Effective <TimeText value={c.effective_at} /> {c.definition.timezone ?? "Asia/Bangkok"}{" "}
            · {c.definition.reason}
          </p>
          {c.definition.departureReason === "retirement" && (
            <div className={styles.stack}>
              <p>
                Retirement:{" "}
                {c.cutoffEffective
                  ? "Effective"
                  : c.status === "cancelled"
                    ? "Cancelled"
                    : "Not yet effective"}{" "}
                · Last working day: {c.definition.lastWorkingDate}
              </p>
              <p>
                Clearance:{" "}
                {data.tasks.some(
                  (t) => t.case_id === c.id && t.clearance_type && t.status === "pending",
                )
                  ? "In progress"
                  : ["draft", "cancelled"].includes(c.status)
                    ? "Not started"
                    : "Complete"}
              </p>
              {!["draft", "cancelled"].includes(c.status) && (
                <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/people/clearance/${c.id}`)}>
                  Track all six departments
                </UiActionLink>
              )}
            </div>
          )}
          {data.tasks.some((t) => t.case_id === c.id && t.status === "pending") && (
            <p>
              <Link
                href={
                  c.definition.departureReason === "retirement"
                    ? canonicalUiHref(`${base}/people/clearance/${c.id}`)
                    : canonicalUiHref(`${base}/people/access?view=devices`)
                }
              >
                {data.tasks.filter((t) => t.case_id === c.id && t.status === "pending").length}{" "}
                preparation tasks pending
              </Link>
              {c.status === "completed" && ["offboarding", "suspend"].includes(c.kind)
                ? " · ERPX access revoked; directory completion tracked separately"
                : ""}
            </p>
          )}
          {["offboarding", "suspend"].includes(c.kind) &&
            ["scheduled", "completed"].includes(c.status) && (
              <p>
                {c.cutoffEffective ? "ERPX access revoked" : "ERPX revocation scheduled"} ·{" "}
                {c.directoryStatus === "verified"
                  ? "Directory disable verified"
                  : c.directoryStatus === "not_required"
                    ? "No linked directory account"
                    : "Directory work pending"}
              </p>
            )}
          {c.last_error && (
            <p role="status">
              {c.last_error === "ACCESS_REVIEW_REJECTED"
                ? "Access review was rejected. Cancel this case and submit a revised request."
                : "Processing needs attention. ERPX departure cutoffs remain effective. Retry is scheduled."}
            </p>
          )}
          {c.status === "waiting_it" && (
            <p>
              This older access request is waiting for IT preparation or account linking. HR
              placement can continue separately.
            </p>
          )}
          {data.canManage && canChange(c.kind) &&
            c.workflow !== "legacy" &&
            ["draft", "waiting_it"].includes(c.status) && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

                disabled={busy || pending || edited}
                onClick={() => {
                  if (busy || pending || dirtyOwners.current.size) return;
                  setRetainedDrawer(null);
                  setDraft({
                    ...c.definition,
                    id: c.id,
                    expectedVersion: c.version,
                  });
                  setKind(c.kind);
                  setDepartureReason(c.definition.departureReason ?? "other");
                  setTimezone(c.definition.timezone ?? "Asia/Bangkok");
                  setEffectiveLocal(
                    retirementLocalTime(c.effective_at, c.definition.timezone ?? "Asia/Bangkok"),
                  );
                  setOpen(true);
                  setStep(1);
                }}
              >
                Continue draft
              </Button>
            )}
          {data.canManage && canChange(c.kind) &&
            c.workflow === "legacy" &&
            ["waiting_it", "pending_review", "scheduled"].includes(c.status) &&
            !c.cutoffEffective &&
            ["onboarding", "transfer"].includes(c.kind) && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

                disabled={busy || pending || edited}
                onClick={() => {
                  if (busy || pending || dirtyOwners.current.size) return;
                  setRetainedDrawer(null);
                  setDraft({ ...c.definition, id: undefined, expectedVersion: 0 });
                  setKind(c.kind);
                  setTimezone(c.definition.timezone ?? "Asia/Bangkok");
                  setEffectiveLocal(
                    retirementLocalTime(c.effective_at, c.definition.timezone ?? "Asia/Bangkok"),
                  );
                  setPreview(null);
                  setConfirmSupersede(false);
                  setStep(1);
                  setOpen(true);
                }}
              >
                Continue as HR placement
              </Button>
            )}
          {data.canReview && canChange(c.kind) && c.status === "pending_review" && (
            <Button appearance="standard"
              type="submit"
              variant="secondary"

              disabled={busy || !canChange(c.kind)}
              onClick={() =>
                void send(`cases/${c.id}/request-access`, {
                  expectedVersion: c.version,
                })
              }
            >
              Request individual access review
            </Button>
          )}
          {data.canManage && data.purchases?.lifecycle.settleExisting!==false && !["completed", "cancelled"].includes(c.status) && (
            <Button appearance="standard"
              type="submit"
              variant="secondary"

              disabled={busy}
              onClick={() =>
                void send(`cases/${c.id}/cancel`, {
                  expectedVersion: c.version,
                })
              }
            >
              {c.status === "draft" ? "Cancel draft" : "Cancel scheduled change"}
            </Button>
          )}
          <Link href={canonicalUiHref(`${base}/work?type=people_lifecycle`)}>Track preparation and reviews</Link>
        </div>
      ))}
      <PeopleFormDrawer appearance="standard"
        key={step===3?"saved":"draft"}
        saved={step===3}
        dismissDisabled={busy || pending}
        open={open && parentOpen}
        retainFormContents
        onClose={() => {if (!busy && !pending) setOpen(false);}}
        onDiscard={() => discardDraft("change")}
        title={`${person.name} · employment change`}
      >
        {pending && <InlineNotice><p>The original employment request may have completed. Recover it before submitting a different request; your newer draft is retained.</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => void retry()}>Recover original employment request</Button><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => {if (!busy) {setRetainedDrawer("change");setOpen(false);}}}>Close and retain request</Button></InlineNotice>}
        <div className={styles.steps} aria-label="Onboarding steps">
          <span data-active={step === 1}>1 · Person & position</span>
          <span data-active={step === 2}>2 · Review placement</span>
          <span data-active={step === 3}>3 · Schedule & track</span>
        </div>
        {step === 1 && (
          <FormLayout layout="stack" data-directory-lifecycle-draft data-directory-lifecycle-owner="change"
            className={styles.stack}
            onSubmit={async (e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                existing = cases.find((c) => c.id === draft?.id);
              let cutoff: number;
              try {
                cutoff = retirementInstant(effectiveLocal, timezone);
                setDateError("");
              } catch (error) {
                setDateError(error instanceof Error ? error.message : "Choose a valid time.");
                return;
              }
              const input: LifecycleInput = {
                id: existing?.id,
                expectedVersion: existing?.version ?? 0,
                kind,
                personId: person.id,
                engagementId: person.engagementId,
                positionId: ["onboarding", "transfer"].includes(kind)
                  ? String(f.get("position"))
                  : null,
                managerPersonId: String(f.get("manager") ?? "") || null,
                acceptRecommendedPay: f.get("acceptRecommendedPay") === "on",
                acceptRecommendedLeave: f.get("acceptRecommendedLeave") === "on",
                effectiveAt: cutoff,
                departureReason: kind === "offboarding" ? departureReason : "other",
                timezone,
                clearanceOwners: {
                  equipment: String(f.get("equipment") ?? "") || null,
                  payroll: String(f.get("payroll") ?? "") || null,
                  finance: String(f.get("finance") ?? "") || null,
                },
                lastWorkingDate: String(f.get("lastWorkingDate") ?? "") || null,
                reason: String(f.get("reason")),
              };
              await send<LifecyclePreview>("placements/preview", input, e.currentTarget, result => {
                setDraft(input);
                setPreview(result);
                setConfirmSupersede(false);
                setStep(2);
              });
            }}
          >
            <p>{person.name} · Existing person and engagement</p>
            <Field appearance="standard">
              Change
              <Select appearance="standard"
                value={kind}
                disabled={!!draft?.id}
                onChange={(e) => setKind(e.target.value as typeof kind)}
              >
                <option value="onboarding">Onboarding / new engagement</option>
                <option value="transfer">Transfer position or branch</option>
                <option value="suspend">Temporary suspension</option>
                <option value="offboarding">End employment & revoke Company access</option>
              </Select>
            </Field>
            {["onboarding", "transfer"].includes(kind) && (
              <>
                <Field appearance="standard">
                  Position
                  <Select appearance="standard"
                    name="position"
                    required
                    defaultValue={draft?.positionId ?? initialPositionId ?? person.positionId ?? ""}
                  >
                    <option value="">Choose a position</option>
                    {data.positions
                      .filter((p) => p.active)
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title} ·{" "}
                          {data.nodes.find((n) => n.id === p.branch_id)?.name ?? "Company"} ·{" "}
                          {data.nodes.find((n) => n.id === p.department_id)?.name ??
                            "All departments"}{" "}
                          · {positionGpoStatus(p.gpo)} · HR placement available
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    name="acceptRecommendedPay"
                    defaultChecked={draft?.acceptRecommendedPay ?? false}
                  />{" "}
                  Create a pay draft from the Position default if this engagement has no
                  compensation history
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    name="acceptRecommendedLeave"
                    defaultChecked={draft?.acceptRecommendedLeave ?? true}
                  />{" "}
                  Assign the Level leave default
                </Field>
                <Field appearance="standard">
                  Actual reporting manager
                  <Select appearance="standard"
                    name="manager"
                    defaultValue={draft?.managerPersonId ?? person.managerPersonId ?? ""}
                  >
                    <option value="">Choose a manager</option>
                    {data.managers
                      .filter((m) => m.id !== person.id)
                      .map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.name}
                        </option>
                      ))}
                  </Select>
                </Field>
              </>
            )}
            {kind === "offboarding" && (
              <Field appearance="standard">
                Departure reason
                <Select appearance="standard"
                  value={departureReason}
                  onChange={(e) => setDepartureReason(e.target.value as "retirement" | "other")}
                >
                  <option value="other">Other departure</option>
                  <option value="retirement">Retirement</option>
                </Select>
              </Field>
            )}
            {kind === "offboarding" && (
              <Field appearance="standard">
                Last working date
                <TextInput appearance="standard"
                  type="date"
                  name="lastWorkingDate"
                  required
                  defaultValue={draft?.lastWorkingDate ?? ""}
                  onChange={(e) => {
                    if (e.target.value) setEffectiveLocal(nextRetirementDay(e.target.value));
                  }}
                />
              </Field>
            )}
            <Field appearance="standard">
              Timezone
              <Select appearance="standard" value={timezone} onChange={(e) => setTimezone(e.target.value)}>
                {[
                  ...new Set([
                    timezone,
                    "Asia/Bangkok",
                    "UTC",
                    "Asia/Singapore",
                    "Europe/London",
                    "America/New_York",
                  ]),
                ].map((zone) => (
                  <option key={zone}>{zone}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              {kind === "offboarding"
                ? "Retirement / departure and access cutoff"
                : "Effective date and time"}
              <TextInput appearance="standard"
                type="datetime-local"
                name="effective"
                required
                value={effectiveLocal}
                onChange={(e) => setEffectiveLocal(e.target.value)}
              />
            </Field>
            {dateError && <p role="alert">{dateError}</p>}
            {kind === "offboarding" && (
              <p className={styles.muted}>
                Default: midnight after the last working day in {timezone}. Review or change it
                before confirming. Pending clearance does not delay cutoff.
              </p>
            )}
            {kind === "offboarding" && departureReason === "retirement" && (
              <fieldset>
                <legend>Clearance owners</legend>
                <p>
                  HR: you · Handover: eligible reporting manager · Directory: configured IT owner
                </p>
                {(["equipment", "payroll", "finance"] as const).map((kind) => (
                  <Field appearance="standard" key={kind}>
                    {kind === "equipment"
                      ? "Equipment and assets"
                      : kind === "payroll"
                        ? "Final payroll"
                        : "Finance clearance"}
                    <Select appearance="standard" name={kind} defaultValue={draft?.clearanceOwners?.[kind] ?? ""}>
                      <option value="">Unassigned — keep in department queue</option>
                      {data.clearanceOwners
                        ?.filter((owner) => owner[kind])
                        .map((owner) => (
                          <option key={owner.id} value={owner.id}>
                            {owner.name}
                          </option>
                        ))}
                    </Select>
                  </Field>
                ))}
              </fieldset>
            )}
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} defaultValue={draft?.reason} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !canChange(kind)}>
              Review placement <ArrowRight size={16} />
            </Button>
          </FormLayout>
        )}
        {step === 2 && preview && draft && (
          <div className={styles.stack}>
            <p className={styles.muted}>
              This HR placement can be scheduled independently of GPO mapping and IT access
              preparation.
            </p>
            {preview.legacyCaseToSupersede && (
              <section className={styles.callout} aria-label="Replace older access request">
                <h3>Replace older access request</h3>
                <p>
                  An older {preview.legacyCaseToSupersede.kind} request is{" "}
                  {preview.legacyCaseToSupersede.status.replaceAll("_", " ")} for{" "}
                  <TimeText value={preview.legacyCaseToSupersede.effectiveAt} /> {draft.timezone}.
                  Submitting this HR placement will supersede that request. IT will prepare access
                  separately.
                </p>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    checked={confirmSupersede}
                    onChange={(event) => setConfirmSupersede(event.target.checked)}
                  />
                  I understand that the older request will be replaced.
                </Field>
              </section>
            )}
            {preview.recommendation && (
              <section className={styles.callout}>
                <strong>Recommended defaults to confirm</strong>
                <p>Level: {preview.recommendation.levelName ?? "No level default"}</p>
                <p>
                  Pay:{" "}
                  {preview.recommendation.monthlyAmountMinor != null
                    ? `${preview.recommendation.currency} ${(preview.recommendation.monthlyAmountMinor / 100).toLocaleString()} / month`
                    : "Unavailable"}{" "}
                  {draft.acceptRecommendedPay
                    ? "· Create draft for independent approval"
                    : "· Not selected"}
                </p>
                <p>
                  Leave: {preview.recommendation.annualDays ?? "—"} annual days ·{" "}
                  {preview.recommendation.sickDays ?? "—"} sick days{" "}
                  {draft.acceptRecommendedLeave ? "· Assign at effective date" : "· Not selected"}
                </p>
              </section>
            )}
            {preview.definition && <AccessRecipe definition={preview.definition} />}
            <p>
              {departureReason === "retirement" ? "Retirement" : "Change"} effective:{" "}
              <TimeText value={draft.effectiveAt} /> {draft.timezone}
            </p>
            {draft.lastWorkingDate && (
              <p>Engagement ends: {draft.lastWorkingDate} · Access cutoff is tracked separately.</p>
            )}
            <p>Verified linked accounts: {preview.accounts.length}</p>
            <p>Access ending: {preview.removed.join(", ") || "None"}</p>
            {kind === "transfer" && (
              <p>
                Applied access stays in place until IT changes it, subject to its expiry and
                revocation conditions. IT manages resources in Technology.
              </p>
            )}
            {preview.blockers.map((b) => (
              <p className={styles.pending} key={b}>
                {b}
              </p>
            ))}
            {["offboarding", "suspend"].includes(kind) && (
              <p>
                ERPX access ends across every verified account in this Company at the selected time.
                Directory changes and equipment handover are tracked separately.
              </p>
            )}
            <div className={styles.actions}>
              <Button appearance="standard" type="submit" variant="secondary" onClick={() => setStep(1)}>
                Back
              </Button>
              {!preview.legacyCaseToSupersede && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={busy || !canChange(kind)}
                  onClick={async () => {
                    await send("placements/draft", draft, undefined, () => setStep(3));
                  }}
                >
                  Save draft
                </Button>
              )}
              <Button appearance="standard"
                type="submit"
                variant="primary"

                disabled={
                  busy || !canChange(kind) || !preview.ready || (!!preview.legacyCaseToSupersede && !confirmSupersede)
                }
                onClick={async () => {
                  await send("placements/submit", {
                      ...draft,
                      previewToken: preview.token,
                      ...(preview.legacyCaseToSupersede
                        ? {
                            supersedeLegacyCase: {
                              id: preview.legacyCaseToSupersede.id,
                              expectedVersion: preview.legacyCaseToSupersede.version,
                            },
                          }
                        : {}),
                    }, undefined, () => setStep(3));
                }}
              >
                {preview.ready ? "Submit for scheduled processing" : "Save and track pending work"}
              </Button>
            </div>
          </div>
        )}
        {step === 3 && (
          <div className={styles.stack}>
            <h3>Request saved</h3>
            <p>
              Track responsible colleagues in Work. Departure cutoff and clearance progress are
              independent; HR placement proceeds on the effective date. IT provisions resources
              separately.
            </p>
            <UiActionLink appearance="standard"
              variant="primary"

              href={canonicalUiHref(`${base}/work?type=people_lifecycle`)}
            >
              Open Work
            </UiActionLink>
          </div>
        )}
        {error && <p role="alert">{error}</p>}
      </PeopleFormDrawer>
      {error && !open && <p role="alert">{error}</p>}
    </Panel>
  );
}

