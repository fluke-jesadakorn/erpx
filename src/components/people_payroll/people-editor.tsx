"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {PeopleFormDrawer} from "./people-form-drawer";
import type {PeoplePurchaseControls} from "@/lib/people_payroll/purchase-controls";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import {useDirectoryOwnerCommand} from "@/lib/people_payroll/use-directory-owner-command";
import type {DirectoryOwnerScope} from "@/lib/people_payroll/directory-owner-command";
import { workFetch } from "@/lib/work_projection/work-fetch";

import { useHydrated } from "@/components/ui/use-hydrated";
import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import type { OrganizationOverview } from "@/lib/organization/organization";
import type { PeopleLifecycleData } from "@/lib/people_payroll/people-lifecycle";
import { positionGpoStatus } from "@/components/people_payroll/gpo-status";
import { type UiAppearance } from "@/components/ui/foundation";
type Row = Record<string, unknown>;
export function PeopleEditor({ appearance = "legacy",
  base,
  person,
  nodes,
  levels = [],
  people = [],
  levelManagedSeparately = false,
  member,
  returnTo,
  organization,
  gpoPositions,
  initialPositionId,
  lifecycleManaged = false,
  initialOpen = false,
  alwaysOpen = false,
  purchases,
  commandScope,
  onCloseRetained,
}: ({
  base: string;
  person?: Row;
  nodes: Row[];
  levels?: Row[];
  people?: Row[];
  levelManagedSeparately?: boolean;
  member?: { id: string; name: string; email: string };
  returnTo?: string;
  organization?: OrganizationOverview;
  gpoPositions?: Pick<PeopleLifecycleData["positions"][number], "id" | "gpo">[];
  initialPositionId?: string;
  lifecycleManaged?: boolean;
  initialOpen?: boolean;
  alwaysOpen?: boolean;
  purchases?:PeoplePurchaseControls;
  commandScope?: DirectoryOwnerScope;
  onCloseRetained?: () => void;
}) & { appearance?: UiAppearance }) {
  const [commandEdits, setCommandEdits] = useState(0);
  const guarded = useDirectoryOwnerCommand("directory", commandScope ? {...commandScope, base, purchases, draftVersion: commandEdits, hasDraft: commandEdits > 0, recordId: person ? String(person.id) : undefined, memberUserId: member?.id ?? (person?.user_id ? String(person.user_id) : undefined)} : null);
  const ready = useHydrated();
  const router = useRouter();
  const [open, setOpen] = useState(initialOpen || !!initialPositionId);
  const [visited, setVisited] = useState(initialOpen || !!initialPositionId || alwaysOpen);
  const [error, setError] = useState("");
  const [legacyPending, setPending] = useState(false);
  const pending = legacyPending || !!commandScope && guarded.busy;
  const [branch, setBranch] = useState(
    String(
      person?.branch_id ??
        organization?.positions.find((p) => p.id === initialPositionId)?.branch_id ??
        nodes.find((n) => n.kind === "branch")?.id ??
        "",
    ),
  );
  const command = useRef<{ payload: string; key: string } | null>(null);
  const [positionId, setPositionId] = useState(
    String(person?.position_id ?? initialPositionId ?? ""),
  );
  const [levelId, setLevelId] = useState(
    String(
      person?.level_id ??
        organization?.positions.find((p) => p.id === initialPositionId)?.level_id ??
        "",
    ),
  );
  const selectedPosition = organization?.positions.find((p) => p.id === positionId);
  const selectedGpo = gpoPositions?.find((p) => p.id === positionId)?.gpo;
  const selectedLevel = organization?.levels.find((l) => l.id === levelId)?.level_number;
  const suggestedPay = organization?.defaults?.pay.find((item) => item.position_id === positionId);
  const suggestedLeave = organization?.defaults?.leave.find((item) => item.level_id === levelId);
  const managerPeople = [
    ...new Map(
      [...people, ...(organization?.people ?? [])].map((p) => [String(p.person_id), p]),
    ).values(),
  ];
  async function send(path: string, input: unknown,form?:HTMLFormElement) {
    if (commandScope) {
      await guarded.send<{id: string}>(path, input, form, result => {setCommandEdits(0); setOpen(false); router.push(returnTo ?? canonicalUiHref(`${base}/people/${result.id}`));});
      return;
    }
    const formEditVersion=form?.getAttribute("data-erpx-edit-version")??null;
    const newWork=path.endsWith("/status")?(input as {status:string}).status==="active":true;
    if(purchases&&!(newWork?purchases.directory.newWork:purchases.directory.settleExisting)){setError("This package does not permit the requested People work.");return;}
    const payload = JSON.stringify(input);
    if (command.current?.payload !== payload)
      command.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/people/directory${path}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": command.current.key,
          },
          body: payload,
        },
        form?{form,formEditVersion}:undefined,
      );
      const result = (await response.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to save.");
      command.current = null;
      if(form&&formEditVersion!==form.getAttribute("data-erpx-edit-version")){setError("The submitted version was saved. Review the changes you have typed since then before saving again.");return;}
      setOpen(false);
      router.push(returnTo ?? canonicalUiHref(`${base}/people/${result.data.id}`));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save.");
    } finally {
      setPending(false);
    }
  }
  const recovery = commandScope && guarded.pending ? <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-notice" : "content-stack"} role="status"><p>The original employee request may have completed. Recover it before submitting a different request; your newer draft is retained.</p><Button appearance={appearance} type="button" variant="secondary" disabled={guarded.busy} onClick={() => void guarded.retry()}>Recover original employee request</Button>{(alwaysOpen ? !!onCloseRetained : open) && <Button appearance={appearance} type="button" variant="secondary" disabled={guarded.busy} onClick={() => {if (!guarded.busy) {if (alwaysOpen) onCloseRetained?.();else setOpen(false);}}}>Close and retain request</Button>}</div> : null;
  return (
    <section data-ui-appearance={appearance} data-directory-command-pending={commandScope && (guarded.pending || guarded.busy) ? "true" : undefined} data-directory-command-busy={pending ? "true" : undefined} className={appearance === "standard" ? "ui-panel" : "card"}>
      <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-section-header" : "section-head"}>
        <h2 data-ui-appearance={appearance}>{person ? "Manage employee" : "Add a person"}</h2>
        {!alwaysOpen && (
          <Button appearance={appearance}
            type="submit"
            variant="primary"

            disabled={!ready || pending || !!commandScope && guarded.pending || purchases?.directory.newWork===false}
            onClick={() => {setVisited(true);setOpen(!open);}}
          >
            {open ? "Close form" : person ? "Edit employee" : "Add person"}
          </Button>
        )}
      </div>
      {recovery}
      {commandScope && visited && !alwaysOpen && !open && <Button appearance={appearance} type="button" variant="secondary" disabled={guarded.busy} onClick={() => setOpen(true)}>Reopen retained employee form</Button>}
      {commandScope && guarded.notice && <p data-ui-appearance={appearance} role="status">{guarded.notice}</p>}
      {commandScope && guarded.error && <p data-ui-appearance={appearance} role="alert">{guarded.error}</p>}
      {error && (
        <p data-ui-appearance={appearance} role="alert" className="error">
          {error}
        </p>
      )}
      {(open || alwaysOpen || !!commandScope && visited) && (
        <PeopleFormDrawer appearance={appearance} inline={alwaysOpen} open={open} retainFormContents={!!commandScope} onClose={() => {if (!commandScope || !guarded.pending && !guarded.busy) setOpen(false);}} title={person ? "Edit employee" : "Add a person"} dismissDisabled={pending || !!commandScope && guarded.pending} onDiscard={()=>{if (commandScope && (guarded.pending || guarded.busy)) return;setBranch(String(person?.branch_id??organization?.positions.find(p=>p.id===initialPositionId)?.branch_id??nodes.find(n=>n.kind==="branch")?.id??""));setPositionId(String(person?.position_id??initialPositionId??""));setLevelId(String(person?.level_id??organization?.positions.find(p=>p.id===initialPositionId)?.level_id??""));setOpen(false);}}>{!alwaysOpen && recovery}<form data-ui-appearance={appearance} onChangeCapture={commandScope ? () => setCommandEdits(value => value + 1) : undefined}
          className={appearance === "standard" ? "ui-form-grid" : "business-form"}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(person ? `/${person.id}` : "", {
              legalName: f.get("legalName"),
              preferredName: f.get("preferredName"),
              email: f.get("email"),
              phone: f.get("phone"),
              workEmail: f.get("workEmail"),
              workPhone: f.get("workPhone"),
              countryCode: f.get("countryCode"),
              branchId: branch,
              departmentId: f.get("departmentId") || null,
              jobTitle: f.get("jobTitle"),
              workerType: f.get("employmentType") === "contractor" ? "contractor" : "employee",
              employmentType: f.get("employmentType"),
              startDate: f.get("startDate"),
              currency: f.get("currency"),
              managerPersonId: f.get("managerPersonId") || null,
              levelId: f.get("levelId") || null,
              positionId: positionId || null,
              acceptRecommendedPay: f.get("acceptRecommendedPay") === "on",
              acceptRecommendedLeave: f.get("acceptRecommendedLeave") === "on",
              reason: f.get("reason") || "Staff profile updated",
              ...(member ? { linkedMemberUserId: member.id } : {}),
              ...(person
                ? {
                    expectedVersion: person.version,
                    ...(person.engagement_id
                      ? {
                          engagementId: person.engagement_id,
                          expectedEngagementVersion: person.engagement_version,
                        }
                      : {}),
                  }
                : {}),
            },e.currentTarget);
          }}
        >
          <Field appearance={appearance}>
            Legal name
            <TextInput appearance={appearance}
              name="legalName"
              required
              minLength={2}
              maxLength={200}
              defaultValue={String(person?.legal_name ?? member?.name ?? "")}
            />
          </Field>
          <Field appearance={appearance}>
            Preferred name
            <TextInput appearance={appearance}
              name="preferredName"
              maxLength={160}
              defaultValue={String(person?.preferred_name ?? "")}
            />
          </Field>
          <Field appearance={appearance}>
            Private email (HR)
            <TextInput appearance={appearance}
              name="email"
              type="email"
              required
              defaultValue={String(person?.email ?? member?.email ?? "")}
              readOnly={!!member}
            />
          </Field>
          <Field appearance={appearance}>
            Private phone (HR)
            <TextInput appearance={appearance} name="phone" type="tel" defaultValue={String(person?.phone ?? "")} />
          </Field>
          <p data-ui-appearance={appearance} className="muted">Published work contacts are visible to everyone in this Holding.</p>
          <Field appearance={appearance}>
            Published work email
            <TextInput appearance={appearance}
              name="workEmail"
              type="email"
              defaultValue={String(person?.work_email ?? "")}
            />
          </Field>
          <Field appearance={appearance}>
            Published work phone
            <TextInput appearance={appearance}
              name="workPhone"
              type="tel"
              defaultValue={String(person?.work_phone ?? "")}
            />
          </Field>
          <Field appearance={appearance}>
            Country code
            <TextInput appearance={appearance}
              name="countryCode"
              required
              pattern="[A-Z]{2}"
              maxLength={2}
              defaultValue={String(person?.country_code ?? "")}
            />
          </Field>
          <Field appearance={appearance}>
            Branch
            <Select appearance={appearance}
              name="branchId"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              required
            >
              {nodes
                .filter((n) => n.kind === "branch")
                .map((n) => (
                  <option data-ui-appearance={appearance} key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Department
            <Select appearance={appearance}
              name="departmentId"
              key={branch}
              defaultValue={String(person?.department_id ?? selectedPosition?.department_id ?? "")}
            >
              <option data-ui-appearance={appearance} value="">No department</option>
              {nodes
                .filter((n) => n.kind === "department" && n.branch_id === branch)
                .map((n) => (
                  <option data-ui-appearance={appearance} key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </Select>
          </Field>
          {organization && (
            <Field appearance={appearance}>
              Position
              <Select appearance={appearance}
                value={positionId}
                onChange={(e) => {
                  setPositionId(e.target.value);
                  const p = organization.positions.find((p) => p.id === e.target.value);
                  if (p) setLevelId(p.level_id);
                }}
              >
                <option data-ui-appearance={appearance} value="">Not assigned — onboarding draft</option>
                {organization.positions
                  .filter((p) => p.active === 1 && (!p.branch_id || p.branch_id === branch))
                  .map((p) => (
                    <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                      {p.title} · Level {p.level_number}
                    </option>
                  ))}
              </Select>
              <small data-ui-appearance={appearance}>
                Positions supply a default level. Confirm the staff level and actual manager below.
              </small>
              {positionId && (
                <small data-ui-appearance={appearance}>
                  {selectedGpo
                    ? positionGpoStatus(selectedGpo)
                    : "GPO preparation is tracked separately by IT."}{" "}
                  HR can save this placement now.
                </small>
              )}
            </Field>
          )}
          {organization && positionId && (
            <section data-ui-appearance={appearance}
              className={appearance === "standard" ? "ui-panel position-recommendations" : "card position-recommendations"}
              aria-label="Position recommendations"
            >
              <strong data-ui-appearance={appearance}>Recommendations for {selectedPosition?.title}</strong>
              <p data-ui-appearance={appearance}>Level {selectedLevel ?? "not set"}</p>
              {suggestedPay && (
                <p data-ui-appearance={appearance}>
                  Default pay: {suggestedPay.currency}{" "}
                  {(suggestedPay.monthly_amount_minor / 100).toLocaleString()} / month · v
                  {suggestedPay.version}
                </p>
              )}
              {suggestedLeave && (
                <p data-ui-appearance={appearance}>
                  Leave: {suggestedLeave.annual_days} annual days · {suggestedLeave.sick_days} sick
                  days / year · policy v{suggestedLeave.version}
                </p>
              )}
              {suggestedPay && (
                <Field appearance={appearance} orientation="inline">
                  <TextInput appearance={appearance} type="checkbox" name="acceptRecommendedPay" defaultChecked={!person} />{" "}
                  Create pay draft from this recommendation (independent approval required)
                </Field>
              )}
              {suggestedLeave && (
                <Field appearance={appearance} orientation="inline">
                  <TextInput appearance={appearance}
                    type="checkbox"
                    name="acceptRecommendedLeave"
                    defaultChecked={!person}
                  />{" "}
                  Assign recommended leave policy
                </Field>
              )}
            </section>
          )}
          {levelManagedSeparately && !organization ? (
            <TextInput appearance={appearance} type="hidden" name="levelId" value={String(person?.level_id ?? "")} />
          ) : (
            <Field appearance={appearance}>
              Staff level
              <Select appearance={appearance}
                name="levelId"
                value={levelId}
                onChange={(e) => setLevelId(e.target.value)}
                required={person?.engagement_status === "active"}
              >
                <option data-ui-appearance={appearance} value="">Not set</option>
                {levels
                  .filter(
                    (l) => (l.active === 1 && l.level_number != null) || l.id === person?.level_id,
                  )
                  .map((l) => (
                    <option data-ui-appearance={appearance} key={String(l.id)} value={String(l.id)}>
                      {String(l.name)} ·{" "}
                      {l.level_number
                        ? `Level ${l.level_number}`
                        : "Legacy level — map to numbered catalog"}
                    </option>
                  ))}
              </Select>
            </Field>
          )}
          <Field appearance={appearance}>
            Reporting manager
            <Select appearance={appearance}
              name="managerPersonId"
              key={`${positionId}-${levelId}`}
              defaultValue={selectedPosition?.is_ceo ? "" : String(person?.manager_person_id ?? "")}
              disabled={Boolean(selectedPosition?.is_ceo)}
            >
              <option data-ui-appearance={appearance} value="">
                {selectedPosition?.is_ceo
                  ? "CEO — no reporting manager"
                  : "Choose a manager before activation"}
              </option>
              {managerPeople
                .filter(
                  (p) =>
                    p.person_id !== person?.id &&
                    (!selectedLevel ||
                      (p.level_number != null && Number(p.level_number) < selectedLevel)) &&
                    (!p.status || p.status === "active"),
                )
                .map((p) => (
                  <option data-ui-appearance={appearance} key={String(p.engagement_id)} value={String(p.person_id)}>
                    {String(p.legal_name)}
                    {p.level_number ? ` · Level ${p.level_number}` : ""}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Job title
            <TextInput appearance={appearance}
              name="jobTitle"
              key={positionId}
              required
              minLength={2}
              defaultValue={
                selectedPosition?.title ??
                String(person?.job_title ?? selectedPosition?.title ?? "")
              }
            />
          </Field>
          <Field appearance={appearance}>
            Employment type
            <Select appearance={appearance}
              name="employmentType"
              defaultValue={String(person?.employment_type ?? "salaried")}
            >
              <option data-ui-appearance={appearance} value="salaried">Salaried employee</option>
              <option data-ui-appearance={appearance} value="daily">Daily employee</option>
              <option data-ui-appearance={appearance} value="hourly">Hourly employee</option>
              <option data-ui-appearance={appearance} value="shift">Shift employee</option>
            </Select>
          </Field>
          <Field appearance={appearance}>
            Start date
            <TextInput appearance={appearance}
              name="startDate"
              type="date"
              required
              defaultValue={String(person?.start_date ?? new Date().toISOString().slice(0, 10))}
            />
          </Field>
          <Field appearance={appearance}>
            Pay currency
            <Select appearance={appearance} name="currency" defaultValue={String(person?.currency ?? "USD")}>
              {["THB", "USD", "EUR", "GBP", "SGD", "CAD"].map((c) => (
                <option data-ui-appearance={appearance} key={c}>{c}</option>
              ))}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Change reason
            <TextInput appearance={appearance}
              name="reason"
              minLength={3}
              maxLength={1000}
              defaultValue={person ? "" : "Initial onboarding"}
              placeholder="Describe a transfer, promotion or reporting change"
              required={!!person}
            />
          </Field>
          <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
            <Button appearance={appearance}
              clientManaged variant="primary"

              type="submit"
              disabled={pending || !ready || purchases?.directory.newWork===false}
            >
              {pending ? "Saving…" : "Save employee"}
            </Button>
          </div>
        </form></PeopleFormDrawer>
      )}
      {person && (
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-actions" : "form-actions"}>
          {(!lifecycleManaged ||
            person.status === "archived" ||
            person.engagement_status === "onboarding") && (
            <Button appearance={appearance}
              type="submit"
              variant="secondary"

              disabled={pending || !ready || (person.status === "active" && person.engagement_status === "active" ? purchases?.directory.settleExisting===false : purchases?.directory.newWork===false)}
              onClick={() =>
                void send(`/${person.id}/status`, {
                  status:
                    person.status === "active" && person.engagement_status === "active"
                      ? "inactive"
                      : "active",
                  expectedVersion: person.version,
                })
              }
            >
              {person.status === "active" && person.engagement_status === "active"
                ? "Deactivate employee"
                : "Activate employee"}
            </Button>
          )}
          <Button appearance={appearance}
            type="submit"
            variant="secondary"

            disabled={
              pending ||
              !ready ||
              purchases?.directory.settleExisting===false ||
              person.status === "archived" ||
              (lifecycleManaged && person.engagement_status !== "ended")
            }
            onClick={() =>
              void send(`/${person.id}/status`, {
                status: "archived",
                expectedVersion: person.version,
              })
            }
          >
            Archive employee
          </Button>
          <p data-ui-appearance={appearance} className="muted">
            {lifecycleManaged
              ? "Use the lifecycle panel to suspend access or end employment. Archive retains history after employment ends."
              : "Deactivation revokes this company’s role assignments. Archived profiles retain their history and can be restored."}
          </p>
        </div>
      )}
    </section>
  );
}
