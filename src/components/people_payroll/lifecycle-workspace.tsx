"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import Link from "@/components/workplace/workspace-link";
import { Plus, ShieldCheck, BriefcaseBusiness, ClipboardList, ArrowRight, GripVertical } from "lucide-react";
import { SetupChecklist } from "@/components/ui/setup-checklist";
import { DraftFormDrawer } from "@/components/ui/draft-form-drawer";
import { useWorkspaceRouteState } from "@/lib/shared/workspace-route-state";
import { WorkspaceCollection, WorkspaceDetail } from "@/components/ui/workspace";
import { Dialog } from "@/components/ui/dialog";
import {PeopleFormDrawer} from "./people-form-drawer";
import {peopleLifecyclePurchased} from "@/lib/people_payroll/purchase-controls";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { retirementLocalTime, retirementInstant, nextRetirementDay } from "@/lib/people_payroll/retirement";
import type { RoleRevision } from "@/lib/identity_access/iam";
import type {
  AccessPackage,
  AccessPackageDefinition,
  LifecycleCase,
  LifecycleInput,
  LifecyclePreview,
  PeopleLifecycleData,
} from "@/lib/people_payroll/people-lifecycle";
import { hasLifecycleCaseDetail } from "@/lib/people_payroll/people-lifecycle";
import { personGpoStatus, positionGpoStatus } from "@/components/people_payroll/gpo-status";
import styles from "@/components/people_payroll/lifecycle.module.css";
import resourceStyles from "@/components/identity_access/technology/resource-console.module.css";

function useCommand(base: string) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const receipt = useRef<{ body: string; key: string } | null>(null);
  async function send<T>(path: string, input: unknown): Promise<T | undefined> {
    const form=document.activeElement?.closest("form")??null,formEditVersion=form?.getAttribute("data-erpx-edit-version")??null,previewOnly=path.endsWith("preview");
    const body = JSON.stringify(input),
      fingerprint = path + body;
    if (receipt.current?.body !== fingerprint)
      receipt.current = { body: fingerprint, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/people/lifecycle/${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": receipt.current.key,
        },
        body,
      },form?{form,formEditVersion}:undefined,previewOnly?"caller":"automatic");
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message ?? "Unable to save. Please try again.");
      receipt.current = null;
      if(form&&formEditVersion!==form.getAttribute("data-erpx-edit-version")){setError("The submitted version completed. Review the changes typed since then before continuing.");return;}
      if (!previewOnly) router.refresh();
      return result.data as T;
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to save.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
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
export function PositionPreparation({
  base,
  data,
  positionId,
  hrDetails,
  surface = "technology",
}: {
  base: string;
  data: PeopleLifecycleData;
  positionId?: string;
  hrDetails?: (id: string) => ReactNode;
  surface?: "people" | "technology";
}) {
  const [selected, setSelected] = useState(positionId ?? null),
    [packageId, setPackageId] = useState(
      data.positions.find((p) => p.id === positionId)?.package_id ?? "",
    ),
    [search, setSearch] = useState(""),
    [branchId, setBranchId] = useState(""),
    [readiness, setReadiness] = useState<"all" | "unbound" | "pending" | "prepared">("all"),
    [visibleCount, setVisibleCount] = useState(12),
    [preview, setPreview] = useState<{
      token: string;
      people: string[];
      definition: AccessPackageDefinition;
    } | null>(null);
  const { send, busy, error } = useCommand(base),
    position = data.positions.find((p) => p.id === selected),
    pkg = data.packages.find((p) => p.id === packageId),
    task = data.tasks.find((t) => t.position_id === selected && t.kind === "position");
  const input =
    position && pkg
      ? {
          positionId: position.id,
          expectedVersion: position.version,
          packageId: pkg.id,
          revision: pkg.published_revision,
        }
      : null;
  const activePositions = data.positions.filter((p) => p.active);
  const publishedPackages = data.packages.filter((p) => p.published_revision > 0);
  const isPrepared = (id: string) =>
    data.tasks.some(
      (task) => task.position_id === id && task.kind === "position" && task.status === "completed",
    );
  const positionState = (p: (typeof data.positions)[number]) =>
    !p.package_id ? "unbound" : isPrepared(p.id) ? "prepared" : "pending";
  const counts = {
    all: activePositions.length,
    unbound: activePositions.filter((p) => positionState(p) === "unbound").length,
    pending: activePositions.filter((p) => positionState(p) === "pending").length,
    prepared: activePositions.filter((p) => positionState(p) === "prepared").length,
  };
  const branches = [
    ...new Set(activePositions.map((p) => p.branch_id).filter((id): id is string => !!id)),
  ]
    .map((id) => ({ id, name: data.nodes.find((node) => node.id === id)?.name ?? "Branch" }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const matchingPositions = activePositions.filter((p) => {
    const branch = data.nodes.find((node) => node.id === p.branch_id)?.name ?? "Company";
    const department =
      data.nodes.find((node) => node.id === p.department_id)?.name ?? "All departments";
    return (
      (!branchId || p.branch_id === branchId) &&
      (readiness === "all" || positionState(p) === readiness) &&
      `${p.title} ${branch} ${department}`
        .toLocaleLowerCase()
        .includes(search.trim().toLocaleLowerCase())
    );
  });
  const visiblePositions =
    surface === "technology" ? matchingPositions.slice(0, visibleCount) : activePositions;
  function chooseReadiness(value: typeof readiness) {
    setReadiness(value);
    setVisibleCount(12);
  }
  function select(id: string) {
    setSelected(id);
    setPackageId(data.positions.find((p) => p.id === id)?.package_id ?? "");
    setPreview(null);
  }
  return (
    <section className="card">
      {data.canPrepare && publishedPackages.length > 0 && (
        <details className={styles.packageShelf}>
          <summary>
            Published access packages <span>{publishedPackages.length}</span>
          </summary>
          <p>Open a position to choose a package, or drag one onto a position.</p>
          <div className={styles.actions}>
            {publishedPackages.map((p) => (
              <span
                key={p.id}
                className="pill"
                draggable
                onDragStart={(e) => e.dataTransfer.setData("application/erpx-access-package", p.id)}
              >
                {p.definition.name} · revision {p.published_revision}
              </span>
            ))}
          </div>
        </details>
      )}
      <div className={styles.heading}>
        <div>
          <h2>
            {surface === "people" ? "Positions for HR placement" : "Position access preparation"}
          </h2>
          <p className="muted">
            {surface === "people"
              ? "Select a position to review its HR placement details."
              : "Choose a position to review its access package and Directory policy. HR placement can proceed while IT prepares access."}
          </p>
        </div>
        {surface === "technology" && (
          <Link href={canonicalUiHref(`${base}/work?type=people_lifecycle`)}>
            Open IT queue <ArrowRight size={15} />
          </Link>
        )}
      </div>
      {surface === "technology" && data.canPrepare && publishedPackages.length === 0 && (
        <div className={styles.packageEmpty}>
          <div>
            <strong>No published access packages yet</strong>
            <p>Create a package in Resources, then return to a position to assign it.</p>
          </div>
          <Link href={canonicalUiHref(`${base}/technology?view=resources`)}>
            Open Resources <ArrowRight size={15} />
          </Link>
        </div>
      )}
      {surface === "technology" && (
        <div className={styles.positionControls}>
          <div
            className={styles.positionStatusFilters}
            role="group"
            aria-label="Filter positions by access status"
          >
            {(
              [
                ["all", "All positions"],
                ["unbound", "No package"],
                ["pending", "In preparation"],
                ["prepared", "Prepared"],
              ] as const
            ).map(([value, label]) => (
              <Button
                variant="plain"
                key={value}
                type="button"
                aria-pressed={readiness === value}
                onClick={() => chooseReadiness(value)}
              >
                <strong>{counts[value]}</strong>
                <span>{label}</span>
              </Button>
            ))}
          </div>
          <div className={styles.positionToolbar}>
            <Field>
              <span>Search positions</span>
              <TextInput
                type="search"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setVisibleCount(12);
                }}
                placeholder="Title, branch, or department"
              />
            </Field>
            <Field>
              <span>Branch</span>
              <Select
                value={branchId}
                onChange={(event) => {
                  setBranchId(event.target.value);
                  setVisibleCount(12);
                }}
              >
                <option value="">All branches</option>
                {branches.map((branch) => (
                  <option key={branch.id} value={branch.id}>
                    {branch.name}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          <p className={styles.positionResultCount} role="status">
            Showing {visiblePositions.length} of {matchingPositions.length} matching positions
          </p>
        </div>
      )}
      <div className={styles.positionGrid}>
        {visiblePositions.map((p) => (
          <Button
            type="submit"
            variant="plain"
            className={styles.positionCard}
            key={p.id}
            onClick={() => select(p.id)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={async (e) => {
              e.preventDefault();
              const id = e.dataTransfer.getData("application/erpx-access-package");
              if (
                data.canPrepare &&
                data.packages.some((x) => x.id === id && x.published_revision)
              ) {
                select(p.id);
                setPackageId(id);
                const pkg = data.packages.find((x) => x.id === id)!;
                const result = await send<typeof preview>("position/preview", {
                  positionId: p.id,
                  expectedVersion: p.version,
                  packageId: id,
                  revision: pkg.published_revision,
                });
                if (result) setPreview(result);
              }
            }}
          >
            <BriefcaseBusiness size={19} />
            <strong>{p.title}</strong>
            <span>
              {data.nodes.find((n) => n.id === p.branch_id)?.name ?? "Company"} ·{" "}
              {data.nodes.find((n) => n.id === p.department_id)?.name ?? "All departments"}
            </span>
            {surface === "people" && (
              <span data-ready={!!p.active}>Available for HR placement</span>
            )}
            <span
              data-ready={surface === "technology" ? positionState(p) === "prepared" : undefined}
            >
              {surface === "people"
                ? positionGpoStatus(p.gpo)
                : positionState(p) === "prepared"
                  ? "Access package prepared"
                  : positionState(p) === "pending"
                    ? "Preparation in progress"
                    : "No access package"}
            </span>
          </Button>
        ))}
      </div>
      {surface === "technology" && matchingPositions.length === 0 && (
        <div className={styles.positionEmpty}>
          <strong>No positions match these filters</strong>
          <p>Try a different title, branch, or access status.</p>
          <Button
            variant="secondary"
            type="button"
            onClick={() => {
              setSearch("");
              setBranchId("");
              chooseReadiness("all");
            }}
          >
            Clear filters
          </Button>
        </div>
      )}
      {surface === "technology" && matchingPositions.length > visibleCount && (
        <Button
          variant="plain"
          type="button"
          className={styles.showMore}
          onClick={() => setVisibleCount((count) => count + 12)}
        >
          Show more positions ({matchingPositions.length - visibleCount} remaining)
        </Button>
      )}
      <Dialog
        open={!!position}
        onClose={() => setSelected(null)}
        title={position?.title ?? "Position"}
        presentation="responsive-panel"
      >
        {position && (
          <div className={styles.stack}>
            <p>
              {data.nodes.find((n) => n.id === position.branch_id)?.name ?? "Company"} ·{" "}
              {data.nodes.find((n) => n.id === position.department_id)?.name ?? "All departments"}
            </p>
            <p>{position.requirements || "No additional work requirements."}</p>
            <p>Available for HR placement · {positionGpoStatus(position.gpo)}</p>
            {surface === "technology" && (
              <p>
                Access package:{" "}
                {positionState(position) === "prepared"
                  ? "prepared"
                  : positionState(position) === "pending"
                    ? "preparation pending"
                    : "none assigned"}{" "}
                · {task?.assignee_id ? "Assigned to configured IT owner" : "Unassigned IT queue"}
              </p>
            )}
            {surface === "technology" && (
              <Link
                href={canonicalUiHref(`${base}/technology?view=organization&position=${encodeURIComponent(position.id)}`)}
              >
                View Directory & GPO for this position
              </Link>
            )}
            {surface === "people" && (
              <p className="muted">
                GPO Directory status does not confirm application on an employee device. IT tracks
                delivery in Technology.
              </p>
            )}
            {data.canManage && data.purchases?.lifecycle.newWork!==false && (
              <UiActionLink
                variant="primary"

                href={canonicalUiHref(`${base}/people?new=person&position=${encodeURIComponent(position.id)}`)}
              >
                Onboard into this position
              </UiActionLink>
            )}
            {hrDetails?.(position.id)}
            {data.canPrepare && (
              <>
                {publishedPackages.length > 0 && (
                  <Field>
                    Published access package
                    <Select
                      value={packageId}
                      onChange={(e) => {
                        setPackageId(e.target.value);
                        setPreview(null);
                      }}
                    >
                      <option value="">Choose a package</option>
                      {publishedPackages.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.definition.name} · revision {p.published_revision}
                        </option>
                      ))}
                    </Select>
                  </Field>
                )}
                <Link href={canonicalUiHref(`${base}/technology?view=resources`)}>
                  Create or prepare an access package
                </Link>
                {publishedPackages.length > 0 && (
                  <Button
                    type="submit"
                    variant="secondary"

                    disabled={busy || !input || data.purchases?.identity.history===false}
                    onClick={async () => {
                      const p = await send<typeof preview>("position/preview", input);
                      if (p) setPreview(p);
                    }}
                  >
                    Preview position access
                  </Button>
                )}
              </>
            )}
            {preview && (
              <>
                <AccessRecipe definition={preview.definition} />
                <p>
                  {preview.people.length
                    ? `Current occupants: ${preview.people.join(", ")}`
                    : "This position has no current occupants."}
                </p>
                <p>
                  Binding this package updates the position recipe. IT selects employees and applies
                  it separately. ERPX roles require independent review.
                </p>
                <Button
                  type="submit"
                  variant="primary"

                  disabled={busy || data.purchases?.identity.newWork===false}
                  onClick={async () => {
                    if (
                      await send("position/bind", {
                        ...input,
                        previewToken: preview.token,
                      })
                    )
                      setSelected(null);
                  }}
                >
                  Assign published package
                </Button>
              </>
            )}
            {error && <p role="alert">{error}</p>}
          </div>
        )}
      </Dialog>
    </section>
  );
}

const blankPackage: AccessPackageDefinition = {
  name: "",
  account: "local",
  connectionId: null,
  grants: [],
  groupIds: [],
  directoryGroupIds: [],
  windowsPolicies: [],
  tasks: [],
};
export function AccessPackages({
  base,
  data,
  roles,
  groups,
  connections = [],
  directoryGroups = [],
}: {
  base: string;
  data: PeopleLifecycleData;
  roles: RoleRevision[];
  groups: { id: string; name: string }[];
  connections?: { id: string; name: string; provider: string }[];
  directoryGroups?: { id: string; name: string; connectionId: string }[];
}) {
  const [editing, setEditingState] = useState<AccessPackage | "new" | null>(null),
    [definition, setDefinition] = useState(blankPackage),
    [reason, setReason] = useState(""),
    [preview, setPreview] = useState<{
      token: string;
      affectedPositions: string[];
      affectedPeople: string[];
      affectedComputers: string[];
    } | null>(null);
  const [packageDirty, setPackageDirty] = useState(false);
  function setEditing(next: AccessPackage | "new" | null) {setEditingState(next);setPackageDirty(false);}
  const {query, update} = useWorkspaceRouteState();
  const selectedPackageId = query.get("package"), packageSearch = query.get("q") ?? "";
  const mobileDetail = !!selectedPackageId;
  const setPackageSearch = (value: string) => update({q:value || null},true);
  const setSelectedPackageId = (value: string) => update({package:value});
  const setMobileDetail = (value: boolean) => {if(!value) update({package:null});};
  const { send, busy, error } = useCommand(base);
  function edit(p: AccessPackage | "new") {
    setEditing(p);
    setDefinition(p === "new" ? blankPackage : p.definition);
    setReason("");
    setPreview(null);
  }
  const input = {
    id: editing && editing !== "new" ? editing.id : undefined,
    expectedVersion: editing && editing !== "new" ? editing.version : 0,
    definition,
    reason,
  };
  function change(next: AccessPackageDefinition) {
    setDefinition(next);
    setPreview(null);
  }
  const visiblePackages = data.packages.filter((item) =>
    item.definition.name.toLocaleLowerCase().includes(packageSearch.trim().toLocaleLowerCase()),
  );
  const selectedPackage =
    visiblePackages.find((item) => item.id === selectedPackageId) ?? visiblePackages[0];
  return (
    <section className={resourceStyles.workspace} data-mobile-detail={mobileDetail}>
      <div className={resourceStyles.toolbar}>
        <Field>
          <span>Search packages</span>
          <TextInput
            type="search"
            value={packageSearch}
            onChange={(event) => setPackageSearch(event.target.value)}
            placeholder="Package name"
          />
        </Field>
        {data.canPrepare && data.purchases?.identity.newWork!==false && (
          <Button type="submit" variant="primary" onClick={() => edit("new")}>
            <Plus size={16} /> New package
          </Button>
        )}
      </div>
      <WorkspaceCollection detailOpen={mobileDetail && !editing}>
        <div className={resourceStyles.list} aria-label="Access packages">
          <div className={resourceStyles.listHeading}>{visiblePackages.length} packages</div>
          {visiblePackages.map((p) => (
            <div key={p.id} className={resourceStyles.actionRow}><Button
              type="submit"
              variant="plain"
              className={resourceStyles.row}
              aria-pressed={selectedPackage?.id === p.id}
              onClick={() => {
                setSelectedPackageId(p.id);
                setMobileDetail(true);
              }}
            >
              <ShieldCheck size={18} aria-hidden="true" />
              <span>
                <strong>{p.definition.name}</strong>
                <small>
                  {p.published_revision ? `Published revision ${p.published_revision}` : "Draft"} ·{" "}
                  {data.positions.filter((x) => x.package_id === p.id).length} positions
                </small>
              </span>
            </Button>{data.canPrepare && p.published_revision > 0 && <Button appearance="standard" type="button" variant="plain" draggable aria-label={`Drag ${p.definition.name}`} title="Drag onto a position, or open a position to choose this package" onDragStart={event => event.dataTransfer.setData("application/erpx-access-package",p.id)} onClick={() => {setSelectedPackageId(p.id);setMobileDetail(true);}}><GripVertical size={16}/></Button>}</div>
          ))}
          {!visiblePackages.length && (
            <p className={resourceStyles.empty}>
              {data.packages.length
                ? "No packages match your search."
                : "No packages yet. Create the first package to prepare access."}
            </p>
          )}
        </div>
        <WorkspaceDetail open={mobileDetail && !editing} title={selectedPackage?.definition.name ?? "Package details"} onClose={() => setMobileDetail(false)}>
          <Button
            variant="plain"
            type="button"
            className={resourceStyles.mobileBack}
            onClick={() => setMobileDetail(false)}
          >
            ← Back to packages
          </Button>
          {selectedPackage ? (
            <>
              <div className={resourceStyles.detailHeading}>
                <div>

                  <p>
                    {selectedPackage.published_revision
                      ? `Published revision ${selectedPackage.published_revision}`
                      : "Draft package"}
                  </p>
                </div>
                {data.canPrepare && data.purchases?.identity.newWork!==false && (
                  <Button variant="secondary" type="button" onClick={() => edit(selectedPackage)}>
                    Edit package
                  </Button>
                )}
              </div>
              <p className={resourceStyles.summary}>
                {data.positions.filter((item) => item.package_id === selectedPackage.id).length}{" "}
                positions use this package. Binding it to a position does not apply access to
                employees automatically.
              </p>
              <AccessRecipe definition={selectedPackage.definition} />
            </>
          ) : (
            <div className={resourceStyles.empty}>
              Select a package to view its resources and preparation steps.
            </div>
          )}
        </WorkspaceDetail>
      </WorkspaceCollection>
      <DraftFormDrawer appearance="standard"
        dirty={packageDirty}
        onDiscard={() => setEditing(null)}
        dismissDisabled={busy}
        open={!!editing}
        onClose={() => setEditing(null)}
        title={editing === "new" ? "New access package" : "Prepare access package"}
      >
        <form
          className={styles.stack}
          onChangeCapture={() => setPackageDirty(true)}
          onSubmit={async (e: FormEvent) => {
            e.preventDefault();
            const result = await send<typeof preview>("packages/preview", input);
            if (result) setPreview(result);
          }}
        >
          <Field>
            Package name
            <TextInput
              required
              minLength={2}
              maxLength={160}
              value={definition.name}
              onChange={(e) => change({ ...definition, name: e.target.value })}
            />
          </Field>
          <Field>
            Account provider
            <Select
              value={definition.account}
              onChange={(e) =>
                change({
                  ...definition,
                  account: e.target.value as AccessPackageDefinition["account"],
                  connectionId: null,
                  directoryGroupIds: [],
                  windowsPolicies: [],
                })
              }
            >
              <option value="local">Local ERPX</option>
              <option value="univention">UCS / Samba AD</option>
              <option value="active_directory">Active Directory</option>
            </Select>
          </Field>
          {definition.account !== "local" && (
            <Field>
              Directory connection
              <Select
                required
                value={definition.connectionId ?? ""}
                onChange={(e) =>
                  change({
                    ...definition,
                    connectionId: e.target.value || null,
                    directoryGroupIds: [],
                    windowsPolicies: [],
                  })
                }
              >
                <option value="">Choose an active connection</option>
                {connections
                  .filter((c) => c.provider === definition.account)
                  .map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
              </Select>
            </Field>
          )}
          <fieldset>
            <legend>ERPX roles</legend>
            {definition.grants.map((g, i) => (
              <div key={i} className={styles.grant}>
                <Field>
                  Role
                  <Select
                    value={g.roleId}
                    onChange={(e) => {
                      const role = roles.find((r) => r.id === e.target.value);
                      change({
                        ...definition,
                        grants: definition.grants.map((x, j) =>
                          i === j
                            ? {
                                ...x,
                                roleId: e.target.value,
                                expectedRoleRevision: role?.revision ?? 1,
                              }
                            : x,
                        ),
                      });
                    }}
                  >
                    {roles.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field>
                  Scope
                  <Select
                    value={g.scopeKind}
                    onChange={(e) =>
                      change({
                        ...definition,
                        grants: definition.grants.map((x, j) =>
                          i === j
                            ? {
                                ...x,
                                scopeKind: e.target.value as typeof g.scopeKind,
                                scopeNodeId: null,
                              }
                            : x,
                        ),
                      })
                    }
                  >
                    <option value="holding">Company</option>
                    <option value="branch">Branch</option>
                    <option value="department">Department</option>
                  </Select>
                </Field>
                {g.scopeKind !== "holding" && (
                  <Field>
                    Access scope
                    <Select
                      required
                      value={g.scopeNodeId ?? ""}
                      onChange={(e) =>
                        change({
                          ...definition,
                          grants: definition.grants.map((x, j) =>
                            i === j ? { ...x, scopeNodeId: e.target.value } : x,
                          ),
                        })
                      }
                    >
                      <option value="">Choose a scope</option>
                      {data.nodes
                        .filter((n) => n.kind === g.scopeKind)
                        .map((n) => (
                          <option key={n.id} value={n.id}>
                            {n.name}
                          </option>
                        ))}
                    </Select>
                  </Field>
                )}
                <Field>
                  Access expires
                  <TextInput
                    type="date"
                    required
                    value={new Date(g.validUntil).toISOString().slice(0, 10)}
                    onChange={(e) => {
                      if (e.target.value)
                        change({
                          ...definition,
                          grants: definition.grants.map((x, j) =>
                            i === j
                              ? {
                                  ...x,
                                  validUntil: Date.parse(e.target.value) + 86400000,
                                }
                              : x,
                          ),
                        });
                    }}
                  />
                </Field>
                <Button
                  variant="secondary"
                  type="button"

                  onClick={() =>
                    change({
                      ...definition,
                      grants: definition.grants.filter((_, j) => i !== j),
                    })
                  }
                >
                  Remove role
                </Button>
              </div>
            ))}
            <Button
              variant="secondary"
              type="button"

              disabled={!roles.length}
              onClick={() => {
                const r = roles.find((r) => r.id === "employee") ?? roles[0];
                change({
                  ...definition,
                  grants: [
                    ...definition.grants,
                    {
                      roleId: r.id,
                      expectedRoleRevision: r.revision,
                      scopeKind: "holding",
                      scopeNodeId: null,
                      validUntil: Date.now() + 90 * 86400000,
                    },
                  ],
                });
              }}
            >
              Add role
            </Button>
          </fieldset>
          <fieldset>
            <legend>ERPX groups</legend>
            {groups.length ? (
              groups.map((g) => (
                <Field orientation="inline" className={styles.check} key={g.id}>
                  <TextInput
                    type="checkbox"
                    checked={definition.groupIds.includes(g.id)}
                    onChange={(e) =>
                      change({
                        ...definition,
                        groupIds: e.target.checked
                          ? [...definition.groupIds, g.id]
                          : definition.groupIds.filter((id) => id !== g.id),
                      })
                    }
                  />
                  {g.name}
                </Field>
              ))
            ) : (
              <p>Create groups in Groups before adding them to a package.</p>
            )}
          </fieldset>
          {definition.connectionId && (
            <fieldset>
              <legend>Directory groups · IT preparation</legend>
              {directoryGroups
                .filter((g) => g.connectionId === definition.connectionId)
                .map((g) => (
                  <Field orientation="inline" className={styles.check} key={g.id}>
                    <TextInput
                      type="checkbox"
                      checked={definition.directoryGroupIds.includes(g.id)}
                      onChange={(e) =>
                        change({
                          ...definition,
                          directoryGroupIds: e.target.checked
                            ? [...definition.directoryGroupIds, g.id]
                            : definition.directoryGroupIds.filter((id) => id !== g.id),
                        })
                      }
                    />
                    {g.name}
                  </Field>
                ))}
              <p className="muted">Membership changes require IT completion in the directory.</p>
            </fieldset>
          )}
          {definition.connectionId && (
            <fieldset>
              <legend>Windows policy references</legend>
              <p className="muted">
                Record an existing GPO and its intended targets. IT must apply it and verify the
                result in Windows.
              </p>
              {definition.windowsPolicies.map((policy, i) => (
                <div className={styles.stack} key={i}>
                  <Field>
                    GPO ID
                    <TextInput
                      required
                      value={policy.id}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j ? { ...p, id: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </Field>
                  <Field>
                    Policy name
                    <TextInput
                      required
                      value={policy.name}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j ? { ...p, name: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </Field>
                  <Field>
                    Policy type
                    <Select
                      value={policy.processor}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j
                              ? {
                                  ...p,
                                  processor: e.target.value as typeof policy.processor,
                                }
                              : p,
                          ),
                        })
                      }
                    >
                      <option value="admx">
                        Administrative policy · USB / lock screen / desktop
                      </option>
                      <option value="preferences">Preference · drive / printer</option>
                    </Select>
                  </Field>
                  <Field>
                    Scope
                    <Select
                      value={policy.scope}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j
                              ? {
                                  ...p,
                                  scope: e.target.value as typeof policy.scope,
                                }
                              : p,
                          ),
                        })
                      }
                    >
                      <option value="user">User</option>
                      <option value="computer">Computer</option>
                    </Select>
                  </Field>
                  <Field>
                    Target OU
                    <TextInput
                      required
                      value={policy.targetOu}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j ? { ...p, targetOu: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </Field>
                  <Field>
                    Security filter group
                    <TextInput
                      value={policy.securityGroup ?? ""}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map((p, j) =>
                            i === j
                              ? {
                                  ...p,
                                  securityGroup: e.target.value || null,
                                }
                              : p,
                          ),
                        })
                      }
                    />
                  </Field>
                  {policy.scope === "computer" && (
                    <Field>
                      Computer group or OU
                      <TextInput
                        required
                        value={policy.computerTarget ?? ""}
                        onChange={(e) =>
                          change({
                            ...definition,
                            windowsPolicies: definition.windowsPolicies.map((p, j) =>
                              i === j
                                ? {
                                    ...p,
                                    computerTarget: e.target.value || null,
                                  }
                                : p,
                            ),
                          })
                        }
                      />
                    </Field>
                  )}
                  <Button
                    variant="secondary"

                    type="button"
                    onClick={() =>
                      change({
                        ...definition,
                        windowsPolicies: definition.windowsPolicies.filter((_, j) => j !== i),
                      })
                    }
                  >
                    Remove reference
                  </Button>
                </div>
              ))}
              <Button
                variant="secondary"

                type="button"
                onClick={() =>
                  change({
                    ...definition,
                    windowsPolicies: [
                      ...definition.windowsPolicies,
                      {
                        id: "",
                        name: "",
                        processor: "admx",
                        scope: "user",
                        targetOu: "",
                        securityGroup: null,
                        computerTarget: null,
                      },
                    ],
                  })
                }
              >
                Add existing GPO reference
              </Button>
            </fieldset>
          )}
          <fieldset>
            <legend>Preparation tasks</legend>
            {definition.tasks.map((task, i) => (
              <div className={styles.grant} key={i}>
                <Field>
                  Type
                  <Select
                    value={task.kind}
                    onChange={(e) =>
                      change({
                        ...definition,
                        tasks: definition.tasks.map((t, j) =>
                          i === j ? { ...t, kind: e.target.value as typeof task.kind } : t,
                        ),
                      })
                    }
                  >
                    {(["equipment", "drive", "printer", "directory", "handover"] as const).map(
                      (kind) => (
                        <option key={kind} value={kind}>
                          {kind}
                        </option>
                      ),
                    )}
                  </Select>
                </Field>
                <Field>
                  Task
                  <TextInput
                    required
                    minLength={2}
                    maxLength={300}
                    value={task.label}
                    onChange={(e) =>
                      change({
                        ...definition,
                        tasks: definition.tasks.map((t, j) =>
                          i === j ? { ...t, label: e.target.value } : t,
                        ),
                      })
                    }
                  />
                </Field>
                <Button
                  variant="secondary"

                  type="button"
                  onClick={() =>
                    change({
                      ...definition,
                      tasks: definition.tasks.filter((_, j) => j !== i),
                    })
                  }
                >
                  Remove task
                </Button>
              </div>
            ))}
            <Button
              variant="secondary"

              type="button"
              onClick={() =>
                change({
                  ...definition,
                  tasks: [...definition.tasks, { kind: "equipment", label: "" }],
                })
              }
            >
              Add preparation task
            </Button>
          </fieldset>
          <Field>
            Reason
            <TextInput
              required
              minLength={3}
              value={reason}
              onChange={(e) => {
                setReason(e.target.value);
                setPreview(null);
              }}
            />
          </Field>
          {data.canPrepare && data.purchases?.identity.newWork!==false && (
            <div className={styles.actions}>
              <Button
                variant="secondary"
                type="button"

                disabled={busy || reason.length < 3}
                onClick={async () => {
                  if (await send("packages/draft", input)) setEditing(null);
                }}
              >
                Save draft
              </Button>
              <Button clientManaged type="submit" variant="primary" disabled={busy}>
                Preview publication
              </Button>
            </div>
          )}
          {preview && (
            <div className={styles.preview}>
              <h3>Publication impact</h3>
              <p>Positions: {preview.affectedPositions.join(", ") || "None yet"}</p>
              <p>People: {preview.affectedPeople.join(", ") || "None yet"}</p>
              <p>Computer targets: {preview.affectedComputers.join(", ") || "None"}</p>
              <p>
                Existing personal grants remain on their reviewed revision until a new lifecycle
                request.
              </p>
              <Button
                variant="primary"
                type="button"

                disabled={busy || data.purchases?.identity.newWork===false}
                onClick={async () => {
                  if (
                    await send("packages/publish", {
                      ...input,
                      previewToken: preview.token,
                    })
                  )
                    setEditing(null);
                }}
              >
                Publish access package
              </Button>
            </div>
          )}
          {error && <p role="alert">{error}</p>}
        </form>
      </DraftFormDrawer>
    </section>
  );
}

export function LifecyclePanel({
  base,
  data,
  person,
  initialCase,
  initialPositionId,
  initialKind,
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
}) {
  const canChange=(kind:string)=>peopleLifecyclePurchased(data.purchases,kind);
  const editableInitial =
    initialCase &&
    initialCase.workflow !== "legacy" &&
    ["draft", "waiting_it"].includes(initialCase.status)
      ? initialCase
      : undefined;
  const { send, busy, error } = useCommand(base),
    [open, setOpen] = useState(!!editableInitial || !!initialPositionId),
    [kind, setKind] = useState<LifecycleInput["kind"]>(
      initialCase?.kind ??
        initialKind ??
        (person.status === "onboarding" ? "onboarding" : "transfer"),
    ),
    [preview, setPreview] = useState<LifecyclePreview | null>(null),
    [draft, setDraft] = useState<LifecycleInput | null>(
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
  const cases = data.cases.filter(
    (c): c is LifecycleCase => c.person_id === person.id && hasLifecycleCaseDetail(c),
  );
  const gpoDelivery =
    data.roster.find((p) => p.engagementId === person.engagementId)?.gpoDelivery ?? "none";
  return (
    <section className="card">
      <div className={styles.heading}>
        <div>
          <h2>Employment & position</h2>
          <p>HR placement is managed here. {personGpoStatus(gpoDelivery)}.</p>
          <p className="muted">
            Directory verification does not confirm application on an employee device.
          </p>
          {data.canPrepare && (
            <Link href={canonicalUiHref(`${base}/technology?view=employees`)}>Open in Technology</Link>
          )}
          <p className="muted">Onboard, transfer, suspend, leave, or retire in this Company.</p>
        </div>
        {data.canManage && data.purchases?.lifecycle.newWork!==false && person.status !== "ended" && (
          <Button
            type="submit"
            clientManaged
            variant="primary"

            onClick={() => {
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
        <Button
          type="submit"
          variant="secondary"
          onClick={() => {
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
        <Button
          type="submit"
          variant="secondary"

          onClick={() => setRehireOpen(true)}
        >
          Create a new engagement for rehire
        </Button>
      )}
      <PeopleFormDrawer
        dismissDisabled={busy}
        open={rehireOpen}
        onClose={() => setRehireOpen(false)}
        title="Rehire · new engagement"
      >
        <form
          className={styles.stack}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            if (
              await send("rehire", {
                personId: person.id,
                engagementId: person.engagementId,
                positionId: f.get("position"),
                startDate: f.get("startDate"),
                reason: f.get("reason"),
              })
            )
              setRehireOpen(false);
          }}
        >
          <p>
            Previous employment and revoked access remain in history. The new engagement starts with
            onboarding and requires new access review.
          </p>
          <Field>
            New position
            <Select name="position" required>
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
          <Field>
            Start date
            <TextInput type="date" name="startDate" required />
          </Field>
          <Field>
            Reason
            <TextInput name="reason" required minLength={3} />
          </Field>
          <Button clientManaged type="submit" variant="primary" disabled={busy || data.purchases?.lifecycle.newWork===false}>
            Create onboarding engagement
          </Button>
          {error && <p role="alert">{error}</p>}
        </form>
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
                <UiActionLink variant="secondary" href={canonicalUiHref(`${base}/people/clearance/${c.id}`)}>
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
              <Button
                type="submit"
                variant="secondary"

                onClick={() => {
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
              <Button
                type="submit"
                variant="secondary"

                onClick={() => {
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
            <Button
              type="submit"
              variant="secondary"

              disabled={busy || !canChange(kind)}
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
            <Button
              type="submit"
              variant="secondary"

              disabled={busy || !canChange(kind)}
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
      <PeopleFormDrawer
        key={step===3?"saved":"draft"}
        saved={step===3}
        dismissDisabled={busy}
        open={open}
        onClose={() => setOpen(false)}
        title={`${person.name} · employment change`}
      >
        <div className={styles.steps} aria-label="Onboarding steps">
          <span data-active={step === 1}>1 · Person & position</span>
          <span data-active={step === 2}>2 · Review placement</span>
          <span data-active={step === 3}>3 · Schedule & track</span>
        </div>
        {step === 1 && (
          <form
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
              const result = await send<LifecyclePreview>("placements/preview", input);
              if (result) {
                setDraft(input);
                setPreview(result);
                setConfirmSupersede(false);
                setStep(2);
              }
            }}
          >
            <p>{person.name} · Existing person and engagement</p>
            <Field>
              Change
              <Select
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
                <Field>
                  Position
                  <Select
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
                <Field orientation="inline">
                  <TextInput
                    type="checkbox"
                    name="acceptRecommendedPay"
                    defaultChecked={draft?.acceptRecommendedPay ?? false}
                  />{" "}
                  Create a pay draft from the Position default if this engagement has no
                  compensation history
                </Field>
                <Field orientation="inline">
                  <TextInput
                    type="checkbox"
                    name="acceptRecommendedLeave"
                    defaultChecked={draft?.acceptRecommendedLeave ?? true}
                  />{" "}
                  Assign the Level leave default
                </Field>
                <Field>
                  Actual reporting manager
                  <Select
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
              <Field>
                Departure reason
                <Select
                  value={departureReason}
                  onChange={(e) => setDepartureReason(e.target.value as "retirement" | "other")}
                >
                  <option value="other">Other departure</option>
                  <option value="retirement">Retirement</option>
                </Select>
              </Field>
            )}
            {kind === "offboarding" && (
              <Field>
                Last working date
                <TextInput
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
            <Field>
              Timezone
              <Select value={timezone} onChange={(e) => setTimezone(e.target.value)}>
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
            <Field>
              {kind === "offboarding"
                ? "Retirement / departure and access cutoff"
                : "Effective date and time"}
              <TextInput
                type="datetime-local"
                name="effective"
                required
                value={effectiveLocal}
                onChange={(e) => setEffectiveLocal(e.target.value)}
              />
            </Field>
            {dateError && <p role="alert">{dateError}</p>}
            {kind === "offboarding" && (
              <p className="muted">
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
                  <Field key={kind}>
                    {kind === "equipment"
                      ? "Equipment and assets"
                      : kind === "payroll"
                        ? "Final payroll"
                        : "Finance clearance"}
                    <Select name={kind} defaultValue={draft?.clearanceOwners?.[kind] ?? ""}>
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
            <Field>
              Reason
              <TextArea name="reason" required minLength={3} defaultValue={draft?.reason} />
            </Field>
            <Button clientManaged type="submit" variant="primary" disabled={busy || !canChange(kind)}>
              Review placement <ArrowRight size={16} />
            </Button>
          </form>
        )}
        {step === 2 && preview && draft && (
          <div className={styles.stack}>
            <p className="muted">
              This HR placement can be scheduled independently of GPO mapping and IT access
              preparation.
            </p>
            {preview.legacyCaseToSupersede && (
              <section className="card" aria-label="Replace older access request">
                <h3>Replace older access request</h3>
                <p>
                  An older {preview.legacyCaseToSupersede.kind} request is{" "}
                  {preview.legacyCaseToSupersede.status.replaceAll("_", " ")} for{" "}
                  <TimeText value={preview.legacyCaseToSupersede.effectiveAt} /> {draft.timezone}.
                  Submitting this HR placement will supersede that request. IT will prepare access
                  separately.
                </p>
                <Field orientation="inline">
                  <TextInput
                    type="checkbox"
                    checked={confirmSupersede}
                    onChange={(event) => setConfirmSupersede(event.target.checked)}
                  />
                  I understand that the older request will be replaced.
                </Field>
              </section>
            )}
            {preview.recommendation && (
              <section className="card">
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
              <Button type="submit" variant="secondary" onClick={() => setStep(1)}>
                Back
              </Button>
              {!preview.legacyCaseToSupersede && (
                <Button
                  type="submit"
                  variant="secondary"

                  disabled={busy || !canChange(kind)}
                  onClick={async () => {
                    if (await send("placements/draft", draft)) {
                      setStep(3);
                    }
                  }}
                >
                  Save draft
                </Button>
              )}
              <Button
                type="submit"
                variant="primary"

                disabled={
                  busy || !canChange(kind) || !preview.ready || (!!preview.legacyCaseToSupersede && !confirmSupersede)
                }
                onClick={async () => {
                  if (
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
                    })
                  )
                    setStep(3);
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
            <UiActionLink
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
    </section>
  );
}

export function PreparationTasks({ base, data }: { base: string; data: PeopleLifecycleData }) {
  const {query, update} = useWorkspaceRouteState();
  const selectedTaskId = query.get("task"), taskSearch = query.get("q") ?? "", taskStatus = query.get("status") ?? "all";
  const mobileDetail = !!selectedTaskId;
  const setTaskSearch = (value: string) => update({q:value || null},true);
  const setTaskStatus = (value: string) => update({status:value === "all" ? null : value});
  const setSelectedTaskId = (value: string) => update({task:value});
  const setMobileDetail = (value: boolean) => {if(!value) update({task:null});};
  const { send, busy, error } = useCommand(base);
  const tasks = data.tasks.filter((task) => task.kind !== "position");
  const visibleTasks = tasks.filter(
    (task) =>
      (taskStatus === "all" || task.status === taskStatus) &&
      task.title.toLocaleLowerCase().includes(taskSearch.trim().toLocaleLowerCase()),
  );
  const selectedTask = visibleTasks.find((task) => task.id === selectedTaskId) ?? visibleTasks[0];
  return (
    <section className={resourceStyles.workspace} data-mobile-detail={mobileDetail}>
      <div className={resourceStyles.toolbar}>
        <Field>
          <span>Search tasks</span>
          <TextInput
            type="search"
            value={taskSearch}
            onChange={(event) => setTaskSearch(event.target.value)}
            placeholder="Task title"
          />
        </Field>
        <Field>
          <span>Status</span>
          <Select value={taskStatus} onChange={(event) => setTaskStatus(event.target.value)}>
            <option value="all">All statuses</option>
            <option value="pending">Pending</option>
            <option value="completed">Completed</option>
          </Select>
        </Field>
      </div>
      <WorkspaceCollection detailOpen={mobileDetail}>
        <div className={resourceStyles.list} aria-label="Preparation tasks">
          <div className={resourceStyles.listHeading}>{visibleTasks.length} tasks</div>
          {visibleTasks.map((task) => (
            <Button
              variant="plain"
              type="button"
              key={task.id}
              className={resourceStyles.row}
              aria-pressed={selectedTask?.id === task.id}
              onClick={() => {
                setSelectedTaskId(task.id);
                setMobileDetail(true);
              }}
            >
              <ClipboardList size={18} aria-hidden="true" />
              <span>
                <strong>{task.title}</strong>
                <small>
                  {task.status.replaceAll("_", " ")} · {task.assignee_id ?? "Unassigned IT"}
                </small>
              </span>
            </Button>
          ))}
          {!visibleTasks.length && (
            <p className={resourceStyles.empty}>No tasks match this view.</p>
          )}
        </div>
        <WorkspaceDetail open={mobileDetail} title={selectedTask?.title ?? "Task details"} onClose={() => setMobileDetail(false)}>
          <Button
            variant="plain"
            type="button"
            className={resourceStyles.mobileBack}
            onClick={() => setMobileDetail(false)}
          >
            ← Back to tasks
          </Button>
          {selectedTask ? (
            <>
              <div className={resourceStyles.detailHeading}>
                <div>

                  <p>{selectedTask.status.replaceAll("_", " ")}</p>
                </div>
              </div>
              <dl className={resourceStyles.facts}>
                <dt>Owner</dt>
                <dd>{selectedTask.assignee_id ?? "Unassigned IT"}</dd>
                <dt>Due date</dt>
                <dd>{selectedTask.due_date ?? "No due date"}</dd>
              </dl>
              {selectedTask.clearance_type && (
                <Link href={canonicalUiHref(`${base}/people/clearance/${selectedTask.case_id}`)}>
                  Open retirement clearance
                </Link>
              )}
              {!selectedTask.clearance_type &&
                data.canPrepare &&
                selectedTask.status === "pending" && (
                  <Button
                    variant="secondary"
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void send(`tasks/${selectedTask.id}/complete`, {
                        expectedVersion: selectedTask.version,
                      })
                    }
                  >
                    Confirm task completed
                  </Button>
                )}
            </>
          ) : (
            <p className={resourceStyles.empty}>Select a task to view its details.</p>
          )}
        </WorkspaceDetail>
      </WorkspaceCollection>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
export function LifecycleSettings({
  base,
  data,
  owners,
}: {
  base: string;
  data: PeopleLifecycleData;
  owners: { id: string; name: string }[];
}) {
  const { send, busy, error } = useCommand(base),
    [saved, setSaved] = useState(false),
    [editing, setEditing] = useState(false);
  return (
    <section className="card">
      <SetupChecklist
        title="Technology handoff status"
        items={[
          {
            id: "it",
            label: "Default IT task owner",
            ready: !!data.settings.itAssigneeId,
            owner: "HR / IT administrator",
            href: canonicalUiHref(`${base}/people/settings?section=templates`),
            detail: data.settings.itAssigneeId
              ? "Default owner selected"
              : "Choose a current IT owner below",
          },
          {
            id: "positions",
            label: "Access package preparation",
            ready: !data.tasks.some((t) => t.kind === "position" && t.status === "pending"),
            owner: "IT",
            href: canonicalUiHref(`${base}/technology?view=organization`),
            detail: "IT can prepare access packages for active positions",
          },
        ]}
      />
      <p className="muted">
        These handoff items route IT work. HR can select positions and schedule placement while IT
        work is pending.
      </p>
      <h2>HR → IT handoff</h2>
      <p>
        IT owner:{" "}
        {owners.find((owner) => owner.id === data.settings.itAssigneeId)?.name ??
          (data.settings.itAssigneeId ? "Assigned" : "Unassigned IT queue")}
      </p>
      <p>
        {data.settings.onboardingTasks.length} onboarding tasks ·{" "}
        {data.settings.offboardingTasks.length} offboarding tasks
      </p>
      <Button variant="secondary" type="button" disabled={data.purchases?.lifecycle.newWork===false} onClick={() => setEditing(true)}>
        Edit handoff settings
      </Button>
      {saved && <p role="status">Settings saved.</p>}
      <PeopleFormDrawer
        dismissDisabled={busy}
        open={editing}
        onClose={() => {
          if (!busy) setEditing(false);
        }}
        title="Edit HR to IT handoff"
      >
        <form
          className={styles.stack}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            if (
              await send("settings", {
                expectedVersion: data.settings.version,
                itAssigneeId: String(f.get("owner")) || null,
                onboardingTasks: String(f.get("onboarding")).split("\n").filter(Boolean),
                offboardingTasks: String(f.get("offboarding")).split("\n").filter(Boolean),
              })
            ) {
              setSaved(true);
              setEditing(false);
            }
          }}
        >
          <Field>
            IT owner
            <Select name="owner" defaultValue={data.settings.itAssigneeId ?? ""}>
              <option value="">Unassigned IT queue</option>
              {owners.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field>
            Standard onboarding tasks
            <TextArea name="onboarding" defaultValue={data.settings.onboardingTasks.join("\n")} />
          </Field>
          <Field>
            Standard offboarding tasks
            <TextArea name="offboarding" defaultValue={data.settings.offboardingTasks.join("\n")} />
          </Field>
          <Button clientManaged type="submit" variant="primary" disabled={busy || data.purchases?.lifecycle.newWork===false}>
            Save templates & handoff
          </Button>
          {error && <p role="alert">{error}</p>}
        </form>
      </PeopleFormDrawer>
    </section>
  );
}
