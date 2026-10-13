"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";

import { useRef, useState } from "react";
import { RecordLink as Link } from "@/components/records/record-link";
import { useRouter } from "next/navigation";
import { ArrowLeft, ArrowRight, BriefcaseBusiness, Search } from "lucide-react";
import type { AccessPackageDefinition, PeopleLifecycleData } from "@/lib/people_payroll/people-lifecycle";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { positionGpoStatus } from "@/components/people_payroll/gpo-status";
import styles from "@/components/identity_access/technology/position-console.module.css";

type Position = PeopleLifecycleData["positions"][number];
type Readiness = "all" | "unbound" | "pending" | "prepared";
type BindingInput = {
  positionId: string;
  expectedVersion: number;
  packageId: string;
  revision: number;
};
type PositionPreview = BindingInput & {
  token: string;
  people: string[];
  definition: AccessPackageDefinition;
};

function RecipeSummary({ definition }: { definition: AccessPackageDefinition }) {
  return (
    <div className={styles.recipe}>
      <div className={styles.recipeStats}>
        <span>
          {definition.account === "local"
            ? "Local ERPX account"
            : definition.account === "univention"
              ? "UCS account"
              : "Active Directory account"}
        </span>
        <span>{definition.grants.length} ERPX roles</span>
        <span>{definition.groupIds.length} ERPX groups</span>
        <span>{definition.directoryGroupIds.length} Directory groups</span>
        <span>{definition.windowsPolicies.length} Windows policies</span>
      </div>
      {definition.grants.length > 0 && (
        <details>
          <summary>ERPX roles ({definition.grants.length})</summary>
          <ul>
            {definition.grants.map((grant, index) => (
              <li key={`${grant.roleId}-${index}`}>
                {grant.roleId} · {grant.scopeKind === "holding" ? "Company" : grant.scopeKind} ·
                until {<TimeText value={grant.validUntil} />}
              </li>
            ))}
          </ul>
        </details>
      )}
      {definition.tasks.length > 0 && (
        <details>
          <summary>Preparation tasks ({definition.tasks.length})</summary>
          <ul>
            {definition.tasks.map((task, index) => (
              <li key={`${task.kind}-${index}`}>{task.label}</li>
            ))}
          </ul>
        </details>
      )}
      {definition.windowsPolicies.length > 0 && (
        <details>
          <summary>Windows policies ({definition.windowsPolicies.length})</summary>
          <ul>
            {definition.windowsPolicies.map((policy) => (
              <li key={policy.id}>
                {policy.name} ·{" "}
                {policy.processor === "admx"
                  ? "Administrative policy"
                  : "Drive / printer preference"}{" "}
                · OU: {policy.targetOu}
                {policy.computerTarget ? ` · Computers: ${policy.computerTarget}` : ""} · IT
                verification required
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

export function PositionConsole({
  base,
  data,
  positionId,
}: {
  base: string;
  data: PeopleLifecycleData;
  positionId?: string;
}) {
  const router = useRouter();
  const activePositions = data.positions.filter((position) => !!position.active);
  const initial = activePositions.find((position) => position.id === positionId);
  const [selectedId, setSelectedId] = useState(initial?.id ?? activePositions[0]?.id ?? null);
  const [mobileDetailOpen, setMobileDetailOpen] = useState(!!initial);
  const [search, setSearch] = useState("");
  const [branchId, setBranchId] = useState("");
  const [readiness, setReadiness] = useState<Readiness>("all");
  const [draft, setDraft] = useState<{ positionId: string; packageId: string } | null>(null);
  const [preview, setPreview] = useState<PositionPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const receipt = useRef<{ fingerprint: string; key: string } | null>(null);

  const nodes = new Map(data.nodes.map((node) => [node.id, node.name]));
  const publishedPackages = data.packages.filter((pkg) => pkg.published_revision > 0);
  const completedPositionIds = new Set(
    data.tasks
      .filter((task) => task.kind === "position" && task.status === "completed")
      .map((task) => task.position_id),
  );
  const stateFor = (position: Position): Exclude<Readiness, "all"> =>
    !position.package_id
      ? "unbound"
      : completedPositionIds.has(position.id)
        ? "prepared"
        : "pending";
  const counts: Record<Readiness, number> = {
    all: activePositions.length,
    unbound: activePositions.filter((position) => stateFor(position) === "unbound").length,
    pending: activePositions.filter((position) => stateFor(position) === "pending").length,
    prepared: activePositions.filter((position) => stateFor(position) === "prepared").length,
  };
  const branches = [
    ...new Set(
      activePositions.map((position) => position.branch_id).filter((id): id is string => !!id),
    ),
  ]
    .map((id) => ({ id, name: nodes.get(id) ?? "Branch" }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const normalizedSearch = search.trim().toLocaleLowerCase();
  const matchingPositions = activePositions.filter(
    (position) =>
      (!branchId || position.branch_id === branchId) &&
      (readiness === "all" || stateFor(position) === readiness) &&
      `${position.title} ${nodes.get(position.branch_id ?? "") ?? "Company"} ${nodes.get(position.department_id ?? "") ?? "All departments"}`
        .toLocaleLowerCase()
        .includes(normalizedSearch),
  );
  const position =
    matchingPositions.find((candidate) => candidate.id === selectedId) ??
    matchingPositions[0] ??
    null;
  const packageId =
    position && draft?.positionId === position.id ? draft.packageId : (position?.package_id ?? "");
  const chosenPackage = publishedPackages.find((pkg) => pkg.id === packageId);
  const input: BindingInput | null =
    position && chosenPackage
      ? {
          positionId: position.id,
          expectedVersion: position.version,
          packageId: chosenPackage.id,
          revision: chosenPackage.published_revision,
        }
      : null;
  const validPreview =
    preview &&
    input &&
    preview.positionId === input.positionId &&
    preview.expectedVersion === input.expectedVersion &&
    preview.packageId === input.packageId &&
    preview.revision === input.revision
      ? preview
      : null;
  const task = position
    ? data.tasks.find(
        (candidate) => candidate.position_id === position.id && candidate.kind === "position",
      )
    : null;
  const currentPackage = position
    ? data.packages.find((pkg) => pkg.id === position.package_id)
    : null;

  async function send<T>(path: string, payload: unknown): Promise<T | undefined> {
    const body = JSON.stringify(payload);
    const fingerprint = `${path}:${body}`;
    if (receipt.current?.fingerprint !== fingerprint)
      receipt.current = { fingerprint, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/people/lifecycle/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": receipt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok)
        throw new Error(result.error?.message ?? "Unable to save. Please try again.");
      receipt.current = null;
      if (path !== "position/preview") router.refresh();
      return result.data as T;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save. Please try again.");
    } finally {
      setBusy(false);
    }
  }

  function selectPosition(id: string) {
    setSelectedId(id);
    setDraft(null);
    setPreview(null);
    setError("");
    setMobileDetailOpen(true);
  }

  function clearFilters() {
    setSearch("");
    setBranchId("");
    setReadiness("all");
    setPreview(null);
    setMobileDetailOpen(false);
  }

  async function previewBinding(binding: BindingInput) {
    setPreview(null);
    const result = await send<{
      token: string;
      people: string[];
      definition: AccessPackageDefinition;
    }>("position/preview", binding);
    if (result) setPreview({ ...binding, ...result });
  }

  return (
    <section
      className={styles.console}
      aria-label="Position access console"
      data-mobile-detail={mobileDetailOpen}
    >
      <header className={styles.consoleHeader}>
        <div className={styles.consoleSummary}>
          <strong>{activePositions.length} positions</strong>
          <span>
            {counts.prepared} prepared · {counts.pending} in preparation · {counts.unbound} without
            a package
          </span>
        </div>
        <Link href={`${base}/work?type=people_lifecycle`}>
          Open IT queue <ArrowRight size={15} aria-hidden="true" />
        </Link>
      </header>
      <div className={styles.layout}>
        <div className={styles.listPane} aria-label="Positions">
          <div className={styles.listControls}>
            <Field className={styles.searchLabel}>
              <Search size={16} aria-hidden="true" />
              <span className={styles.srOnly}>Search positions</span>
              <TextInput
                type="search"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setPreview(null);
                  setMobileDetailOpen(false);
                }}
                placeholder="Search title, branch, department"
              />
            </Field>
            <div className={styles.filterRow}>
              <Field>
                <span className={styles.srOnly}>Branch</span>
                <Select
                  value={branchId}
                  onChange={(event) => {
                    setBranchId(event.target.value);
                    setPreview(null);
                    setMobileDetailOpen(false);
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
              <Field>
                <span className={styles.srOnly}>Access status</span>
                <Select
                  value={readiness}
                  onChange={(event) => {
                    setReadiness(event.target.value as Readiness);
                    setPreview(null);
                    setMobileDetailOpen(false);
                  }}
                >
                  <option value="all">All statuses ({counts.all})</option>
                  <option value="unbound">No package ({counts.unbound})</option>
                  <option value="pending">In preparation ({counts.pending})</option>
                  <option value="prepared">Prepared ({counts.prepared})</option>
                </Select>
              </Field>
            </div>
            {data.canPrepare && publishedPackages.length > 0 && (
              <details className={styles.packageShelf}>
                <summary>
                  Drag a published package onto a position <span>{publishedPackages.length}</span>
                </summary>
                <div className={styles.packageChips}>
                  {publishedPackages.map((pkg) => (
                    <span
                      key={pkg.id}
                      draggable
                      onDragStart={(event) =>
                        event.dataTransfer.setData("application/erpx-access-package", pkg.id)
                      }
                    >
                      {pkg.definition.name} <small>r{pkg.published_revision}</small>
                    </span>
                  ))}
                </div>
              </details>
            )}
            <div className={styles.resultCount} role="status">
              {matchingPositions.length} matching positions
            </div>
          </div>
          <div className={styles.positionList}>
            {matchingPositions.length > 0 ? (
              <ul>
                {matchingPositions.map((candidate) => {
                  const status = stateFor(candidate);
                  return (
                    <li key={candidate.id}>
                      <Button
                        variant="plain"
                        type="button"
                        className={styles.positionRow}
                        aria-current={candidate.id === position?.id ? "true" : undefined}
                        onClick={() => selectPosition(candidate.id)}
                        onDragOver={data.canPrepare ? (event) => event.preventDefault() : undefined}
                        onDrop={
                          data.canPrepare
                            ? async (event) => {
                                event.preventDefault();
                                const droppedId = event.dataTransfer.getData(
                                  "application/erpx-access-package",
                                );
                                const droppedPackage = publishedPackages.find(
                                  (pkg) => pkg.id === droppedId,
                                );
                                if (!droppedPackage || busy) return;
                                selectPosition(candidate.id);
                                setDraft({ positionId: candidate.id, packageId: droppedId });
                                await previewBinding({
                                  positionId: candidate.id,
                                  expectedVersion: candidate.version,
                                  packageId: droppedId,
                                  revision: droppedPackage.published_revision,
                                });
                              }
                            : undefined
                        }
                      >
                        <BriefcaseBusiness size={17} aria-hidden="true" />
                        <span className={styles.rowText}>
                          <strong>{candidate.title}</strong>
                          <small>
                            {nodes.get(candidate.branch_id ?? "") ?? "Company"} ·{" "}
                            {nodes.get(candidate.department_id ?? "") ?? "All departments"}
                          </small>
                        </span>
                        <span className={styles.rowStatus} data-status={status}>
                          {status === "prepared"
                            ? "Prepared"
                            : status === "pending"
                              ? "In progress"
                              : "No package"}
                        </span>
                      </Button>
                    </li>
                  );
                })}
              </ul>
            ) : (
              <div className={styles.emptyList}>
                <strong>No positions match these filters</strong>
                <p>Try a different title, branch, or access status.</p>
                <Button variant="plain" type="button" onClick={clearFilters}>
                  Clear filters
                </Button>
              </div>
            )}
          </div>
        </div>

        <section className={styles.detailPane} aria-label="Position details">
          <Button
            variant="plain"
            type="button"
            className={styles.mobileBack}
            onClick={() => setMobileDetailOpen(false)}
          >
            <ArrowLeft size={17} aria-hidden="true" /> Back to positions
          </Button>
          {position ? (
            <div className={styles.detailContent} key={position.id}>
              <div className={styles.detailHeading}>
                <div>
                  <span className={styles.eyebrow}>Position access</span>
                  <h2>{position.title}</h2>
                  <p>
                    {nodes.get(position.branch_id ?? "") ?? "Company"} ·{" "}
                    {nodes.get(position.department_id ?? "") ?? "All departments"}
                  </p>
                </div>
                <span className={styles.statusBadge} data-status={stateFor(position)}>
                  {stateFor(position) === "prepared"
                    ? "Prepared"
                    : stateFor(position) === "pending"
                      ? "In preparation"
                      : "No package"}
                </span>
              </div>
              <div className={styles.detailBody}>
                <div className={styles.detailBlock}>
                  <h3>Position and HR placement</h3>
                  <p>{position.requirements || "No additional work requirements recorded."}</p>
                  <p className={styles.muted}>
                    Available for HR placement · {positionGpoStatus(position.gpo)}
                  </p>
                  <div className={styles.linkRow}>
                    {data.canManage && (
                      <Link
                        href={`${base}/people?new=person&position=${encodeURIComponent(position.id)}`}
                      >
                        Onboard into this position <ArrowRight size={14} aria-hidden="true" />
                      </Link>
                    )}
                    {data.canPrepare && (
                      <Link
                        href={`${base}/technology?view=organization&position=${encodeURIComponent(position.id)}`}
                      >
                        View Directory & GPO <ArrowRight size={14} aria-hidden="true" />
                      </Link>
                    )}
                  </div>
                </div>
                <div className={styles.detailBlock}>
                  <div className={styles.blockTitle}>
                    <h3>Access package</h3>
                    {currentPackage && (
                      <span>
                        Revision {position.package_revision ?? currentPackage.published_revision}
                      </span>
                    )}
                  </div>
                  <p>
                    {currentPackage
                      ? currentPackage.definition.name
                      : "No package assigned to this position."}
                  </p>
                  <p className={styles.muted}>
                    {stateFor(position) === "prepared"
                      ? "Preparation complete."
                      : stateFor(position) === "pending"
                        ? "Preparation is still in progress."
                        : "Assign a published package to start preparation."}
                  </p>
                  {data.canPrepare && (
                    <>
                      {publishedPackages.length > 0 && (
                        <Field className={styles.packageSelect}>
                          Published package
                          <Select
                            value={packageId}
                            onChange={(event) => {
                              setDraft({ positionId: position.id, packageId: event.target.value });
                              setPreview(null);
                              setError("");
                            }}
                          >
                            <option value="">Choose a package</option>
                            {publishedPackages.map((pkg) => (
                              <option key={pkg.id} value={pkg.id}>
                                {pkg.definition.name} · revision {pkg.published_revision}
                              </option>
                            ))}
                          </Select>
                        </Field>
                      )}
                      <div className={styles.actions}>
                        {publishedPackages.length > 0 && (
                          <Button
                            variant="primary"
                            type="button"
                            className={styles.primaryButton}
                            disabled={busy || !input}
                            onClick={() => {
                              if (input) void previewBinding(input);
                            }}
                          >
                            Preview access
                          </Button>
                        )}
                        <Link href={`${base}/technology?view=resources&section=packages`}>
                          {publishedPackages.length
                            ? "Manage packages"
                            : "Create an access package"}{" "}
                          <ArrowRight size={14} aria-hidden="true" />
                        </Link>
                      </div>
                    </>
                  )}
                </div>
                <div className={styles.detailBlock}>
                  <h3>IT preparation</h3>
                  <p>
                    {task
                      ? task.title
                      : position.package_id
                        ? "Waiting for the preparation task."
                        : "Starts when a package is assigned."}
                  </p>
                  <p className={styles.muted}>
                    {task
                      ? `${task.status === "completed" ? "Completed" : task.status === "pending" ? "Pending" : task.status} · ${task.assignee_id ? "Assigned to IT owner" : "Unassigned IT queue"}`
                      : "No preparation task yet."}
                  </p>
                  <Link href={`${base}/work?type=people_lifecycle`}>
                    Open IT queue <ArrowRight size={14} aria-hidden="true" />
                  </Link>
                </div>
                {validPreview && (
                  <div className={styles.previewBlock}>
                    <div className={styles.blockTitle}>
                      <h3>Preview before assigning</h3>
                      <span>{validPreview.definition.name}</span>
                    </div>
                    <RecipeSummary definition={validPreview.definition} />
                    <p>
                      {validPreview.people.length
                        ? `Current occupants: ${validPreview.people.join(", ")}`
                        : "This position has no current occupants."}
                    </p>
                    <p className={styles.muted}>
                      Assigning updates the position recipe. IT applies access to employees
                      separately; ERPX roles still need independent review.
                    </p>
                    <Button
                      variant="primary"
                      type="button"
                      className={styles.primaryButton}
                      disabled={busy}
                      onClick={async () => {
                        const result = await send<unknown>("position/bind", {
                          positionId: validPreview.positionId,
                          expectedVersion: validPreview.expectedVersion,
                          packageId: validPreview.packageId,
                          revision: validPreview.revision,
                          previewToken: validPreview.token,
                        });
                        if (result !== undefined) {
                          setPreview(null);
                          setDraft(null);
                        }
                      }}
                    >
                      Assign published package
                    </Button>
                  </div>
                )}
                {error && (
                  <p className={styles.error} role="alert">
                    {error}
                  </p>
                )}
              </div>
            </div>
          ) : (
            <div className={styles.emptyDetail}>
              <BriefcaseBusiness size={28} aria-hidden="true" />
              <strong>Select a position</strong>
              <p>Review its package, preparation, and Directory policy here.</p>
            </div>
          )}
        </section>
      </div>
    </section>
  );
}
