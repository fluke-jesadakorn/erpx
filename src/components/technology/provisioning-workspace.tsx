"use client";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Dialog } from "@/components/ui/dialog";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { workFetch } from "@/lib/work-fetch";
import {
  provisioningInput,
  type ProvisioningInput,
  type ProvisioningPreview,
  type TechnologyData,
} from "@/lib/technology";
import type { PeopleLifecycleData } from "@/lib/people-lifecycle";
import styles from "./technology.module.css";
import consoleStyles from "./provisioning-console.module.css";
export function ProvisioningWorkspace({
  base,
  lifecycle,
  data,
  canApply,
  section = "people",
}: {
  base: string;
  lifecycle: PeopleLifecycleData;
  data: TechnologyData;
  canApply: boolean;
  section?: "people" | "operations";
}) {
  const router = useRouter(),
    receipt = useRef<{ body: string; key: string } | null>(null),
    activeRowRef = useRef<HTMLButtonElement>(null),
    backRef = useRef<HTMLButtonElement>(null),
    wasMobileDetail = useRef(false);
  const [selected, setSelected] = useState<string[]>([]),
    [open, setOpen] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [preview, setPreview] = useState<ProvisioningPreview | null>(null),
    [input, setInput] = useState<ProvisioningInput | null>(null),
    [detail, setDetail] = useState<string | null>(
      lifecycle.roster[0]?.engagementId ?? null,
    ),
    [operationDetail, setOperationDetail] = useState<string | null>(
      data.operations[0]?.id ?? null,
    ),
    [mobileDetail, setMobileDetail] = useState(false),
    [search, setSearch] = useState(""),
    [statusFilter, setStatusFilter] = useState("all"),
    [source, setSource] = useState<"position" | "supplemental">("position"),
    [withdraw, setWithdraw] = useState<
      import("@/lib/technology").AppliedPackage | null
    >(null);
  const pending = data.operations.some((op) =>
    ["queued", "running", "retry"].includes(op.status),
  );
  useEffect(() => {
    if (!pending) return;
    const timer = setInterval(() => router.refresh(), 5000);
    return () => clearInterval(timer);
  }, [pending, router]);
  useEffect(() => {
    if (!window.matchMedia("(max-width: 720px)").matches) return;
    if (mobileDetail) backRef.current?.focus();
    else if (wasMobileDetail.current) activeRowRef.current?.focus();
    wasMobileDetail.current = mobileDetail;
  }, [mobileDetail]);
  async function send(path: string, body: unknown) {
    setBusy(true);
    setError("");
    const encoded = JSON.stringify(body),
      fingerprint = path + encoded;
    if (receipt.current?.body !== fingerprint)
      receipt.current = { body: fingerprint, key: crypto.randomUUID() };
    try {
      const r = await workFetch(
          "/api/v1" + base + "/technology/provisioning/" + path,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": receipt.current.key,
            },
            body: encoded,
          },
        ),
        result = await r.json();
      if (!r.ok)
        throw Error(result.error?.message ?? "Unable to provision access.");
      receipt.current = null;
      return result.data;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to continue.");
    } finally {
      setBusy(false);
    }
  }
  function status(id: string, positionId: string | null) {
    const position = lifecycle.positions.find((p) => p.id === positionId),
      applied = data.applications.filter(
        (a) =>
          a.engagement_id === id && a.active && a.source_kind === "position",
      );
    if (
      data.operations.some(
        (op) =>
          op.status === "blocked" &&
          data.applications.some(
            (a) => a.engagement_id === id && a.id.startsWith(op.id + ":"),
          ),
      )
    )
      return "Needs attention";
    if (
      data.operations.some(
        (op) =>
          (["queued", "running", "retry"].includes(op.status) ||
            op.reviewsPending > 0) &&
          data.applications.some(
            (a) => a.engagement_id === id && a.id.startsWith(op.id + ":"),
          ),
      )
    )
      return "In progress";
    return applied.some(
      (a) =>
        a.position_id === positionId &&
        a.package_id === position?.package_id &&
        a.package_revision === position?.package_revision,
    )
      ? "Ready"
      : "Waiting for IT";
  }
  const statusById = new Map(
    lifecycle.roster.map((p) => [
      p.engagementId,
      status(p.engagementId, p.positionId),
    ]),
  );
  const normalizedSearch = search.trim().toLocaleLowerCase();
  const filteredPeople = lifecycle.roster.filter((p) => {
    const position =
      lifecycle.positions.find((item) => item.id === p.positionId)?.title ??
      "No position";
    return (
      (statusFilter === "all" ||
        statusById.get(p.engagementId) === statusFilter) &&
      (!normalizedSearch ||
        [p.name, p.engagementId, position].some((value) =>
          value.toLocaleLowerCase().includes(normalizedSearch),
        ))
    );
  });
  const selectableVisible = filteredPeople.filter((p) => p.status === "active");
  const person = filteredPeople.find((p) => p.engagementId === detail) ??
    filteredPeople[0];
  const operation =
    data.operations.find((op) => op.id === operationDetail) ??
    data.operations[0];
  const personApplications = person
    ? data.applications.filter((a) => a.engagement_id === person.engagementId)
    : [];
  return (
    <div className={consoleStyles.workspace}>
      <div className={consoleStyles.toolbar}>
        <div className={consoleStyles.heading}>
          <h2>{section === "operations" ? "Provisioning operations" : "Employee access"}</h2>
          <span className={consoleStyles.count}>
            {section === "operations"
              ? data.operations.length + " operations"
              : filteredPeople.length + " of " + lifecycle.roster.length + " employees"}
          </span>
        </div>
        {section === "people" && canApply && (
          <div className={consoleStyles.actions}>
            {selected.length > 0 && (
              <button
                className={consoleStyles.textAction}
                onClick={() => setSelected([])}
              >
                Clear selection
              </button>
            )}
            <button
              className="button"
              disabled={!selected.length}
              onClick={() => {
                setPreview(null);
                setInput(null);
                setWithdraw(null);
                setOpen(true);
              }}
            >
              Prepare {selected.length || ""} selected
            </button>
          </div>
        )}
      </div>
      {notice && <p className={consoleStyles.notice} role="status">{notice}</p>}
      {error && !open && <p className={consoleStyles.error} role="alert">{error}</p>}
      {section === "people" ? (
        <div className={consoleStyles.console} data-mobile-detail={mobileDetail}>
          <div className={consoleStyles.listPane}>
            <div className={consoleStyles.listControls}>
              <label className={consoleStyles.searchLabel}>
                <span className="sr-only">Search employees</span>
                <input
                  type="search"
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value);
                    setMobileDetail(false);
                  }}
                  placeholder="Search name or position"
                />
              </label>
              <label className={consoleStyles.filterLabel}>
                <span className="sr-only">Filter by IT status</span>
                <select
                  value={statusFilter}
                  onChange={(event) => {
                    setStatusFilter(event.target.value);
                    setMobileDetail(false);
                  }}
                >
                  <option value="all">All IT statuses</option>
                  <option value="Ready">Ready</option>
                  <option value="In progress">In progress</option>
                  <option value="Needs attention">Needs attention</option>
                  <option value="Waiting for IT">Waiting for IT</option>
                </select>
              </label>
            </div>
            <div className={consoleStyles.listHeader}>
              <label>
                <input
                  aria-label="Select visible active employees, up to 100 total"
                  type="checkbox"
                  disabled={!canApply || selectableVisible.length === 0}
                  checked={
                    selectableVisible.length > 0 &&
                    selectableVisible.every((p) => selected.includes(p.engagementId))
                  }
                  onChange={(event) => {
                    const checked = event.currentTarget.checked;
                    setSelected((current) => {
                      const visibleIds = new Set(
                        selectableVisible.map((p) => p.engagementId),
                      );
                      if (!checked)
                        return current.filter((id) => !visibleIds.has(id));
                      const next = [...current];
                      for (const p of selectableVisible) {
                        if (next.length >= 100) break;
                        if (!next.includes(p.engagementId))
                          next.push(p.engagementId);
                      }
                      return next;
                    });
                  }}
                />
                <span>Select visible</span>
              </label>
              <span>{selected.length}/100 selected</span>
            </div>
            <div className={consoleStyles.listScroll} role="list" aria-label="Employees">
              {filteredPeople.map((p) => {
                const position =
                  lifecycle.positions.find((item) => item.id === p.positionId)
                    ?.title ?? "No position";
                const state = statusById.get(p.engagementId) ?? "Waiting for IT";
                return (
                  <div
                    className={consoleStyles.listRow}
                    data-selected={person?.engagementId === p.engagementId}
                    key={p.engagementId}
                    role="listitem"
                  >
                    <input
                      type="checkbox"
                      aria-label={"Select " + p.name}
                      disabled={
                        !canApply ||
                        p.status !== "active" ||
                        (selected.length >= 100 &&
                          !selected.includes(p.engagementId))
                      }
                      checked={selected.includes(p.engagementId)}
                      onChange={(event) => {
                        const checked = event.currentTarget.checked;
                        setSelected((current) =>
                          checked
                            ? [...current, p.engagementId]
                            : current.filter((id) => id !== p.engagementId),
                        );
                      }}
                    />
                    <button
                      ref={person?.engagementId === p.engagementId ? activeRowRef : undefined}
                      className={consoleStyles.rowButton}
                      aria-current={person?.engagementId === p.engagementId ? "true" : undefined}
                      onClick={() => {
                        setDetail(p.engagementId);
                        setMobileDetail(true);
                      }}
                    >
                      <span className={consoleStyles.rowMain}>
                        <strong>{p.name}</strong>
                        <span>{position}</span>
                      </span>
                      <span className={consoleStyles.badge} data-status={state}>{state}</span>
                    </button>
                  </div>
                );
              })}
              {filteredPeople.length === 0 && (
                <div className={consoleStyles.empty}>
                  <strong>No employees found</strong>
                  <p>Try another name, position, or IT status.</p>
                  <button
                    className={consoleStyles.textAction}
                    onClick={() => {
                      setSearch("");
                      setStatusFilter("all");
                    }}
                  >
                    Clear filters
                  </button>
                </div>
              )}
            </div>
          </div>
          <section className={consoleStyles.detailPane} aria-label="Employee access details">
            <button
              ref={backRef}
              className={consoleStyles.back}
              onClick={() => setMobileDetail(false)}
            >
              ← Back to employees
            </button>
            {person ? (
              <>
                <div className={consoleStyles.detailHead}>
                  <div>
                    <span className={consoleStyles.eyebrow}>Employee</span>
                    <h3>{person.name}</h3>
                    <p>
                      {lifecycle.positions.find((p) => p.id === person.positionId)
                        ?.title ?? "No position assigned"}
                    </p>
                  </div>
                  <span
                    className={consoleStyles.badge}
                    data-status={statusById.get(person.engagementId)}
                  >
                    {statusById.get(person.engagementId)}
                  </span>
                </div>
                <div className={consoleStyles.detailScroll}>
                  <div className={consoleStyles.detailSectionHead}>
                    <h4>Applied access</h4>
                    <span>{personApplications.filter((a) => a.active).length} effective</span>
                  </div>
                  {personApplications.length === 0 && (
                    <div className={consoleStyles.emptyDetail}>
                      No access package has been applied to this employee.
                    </div>
                  )}
                  {personApplications.map((a) => (
                    <section className={consoleStyles.accessCard} key={a.id}>
                      <div className={consoleStyles.accessHead}>
                        <div>
                          <strong>{a.definition.name}</strong>
                          <span>Revision {a.package_revision} · {a.source_kind} source</span>
                        </div>
                        <span className={consoleStyles.badge} data-status={a.active ? "Ready" : "Inactive"}>
                          {a.active ? "Effective" : "Inactive / pending"}
                        </span>
                      </div>
                      <dl className={consoleStyles.factGrid}>
                        <div><dt>Expires</dt><dd>{new Date(a.valid_until).toLocaleString()}</dd></div>
                        <div><dt>ERPX roles</dt><dd>{a.definition.grants.map((g) => g.roleId).join(", ") || "None"} <small>(reviewed separately)</small></dd></div>
                        <div><dt>Groups</dt><dd>{[...a.definition.groupIds, ...a.definition.directoryGroupIds].join(", ") || "None"}</dd></div>
                        <div><dt>Exceptions</dt><dd>{a.scope.exclude?.join(", ") || "None"}</dd></div>
                      </dl>
                      {canApply && !a.ended_at && (
                        <button
                          className="button secondary"
                          onClick={() => {
                            setSelected([a.engagement_id]);
                            setWithdraw(a);
                            setPreview(null);
                            setInput(null);
                            setOpen(true);
                          }}
                        >
                          Withdraw this source
                        </button>
                      )}
                    </section>
                  ))}
                </div>
              </>
            ) : (
              <div className={consoleStyles.emptyDetail}>Select an employee to inspect access.</div>
            )}
          </section>
        </div>
      ) : (
        <div className={consoleStyles.console} data-mobile-detail={mobileDetail}>
          <div className={consoleStyles.listPane}>
            <div className={consoleStyles.listHeader}>
              <strong>Recent operations</strong>
              <span>{data.operations.length} total</span>
            </div>
            <div className={consoleStyles.listScroll} role="list" aria-label="Provisioning operations">
              {data.operations.map((op) => (
                <div className={consoleStyles.listRow} data-selected={operation?.id === op.id} key={op.id} role="listitem">
                  <button
                    ref={operation?.id === op.id ? activeRowRef : undefined}
                    className={consoleStyles.rowButton}
                    aria-current={operation?.id === op.id ? "true" : undefined}
                    onClick={() => {
                      setOperationDetail(op.id);
                      setMobileDetail(true);
                    }}
                  >
                    <span className={consoleStyles.rowMain}>
                      <strong>{op.status.replaceAll("_", " ")}</strong>
                      <span>{new Date(op.created_at).toLocaleString()}</span>
                    </span>
                    {op.reviewsPending > 0 && <span className={consoleStyles.reviewCount}>{op.reviewsPending} reviews</span>}
                  </button>
                </div>
              ))}
              {data.operations.length === 0 && (
                <div className={consoleStyles.empty}>No provisioning operations yet.</div>
              )}
            </div>
          </div>
          <section className={consoleStyles.detailPane} aria-label="Operation details">
            <button ref={backRef} className={consoleStyles.back} onClick={() => setMobileDetail(false)}>
              ← Back to operations
            </button>
            {operation ? (
              <>
                <div className={consoleStyles.detailHead}>
                  <div>
                    <span className={consoleStyles.eyebrow}>Provisioning operation</span>
                    <h3>{operation.status.replaceAll("_", " ")}</h3>
                    <p>{new Date(operation.created_at).toLocaleString()}</p>
                  </div>
                </div>
                <div className={consoleStyles.detailScroll}>
                  <div className={consoleStyles.detailSectionHead}>
                    <h4>Progress</h4>
                    <span>{operation.reviewsPending} ERPX role reviews pending</span>
                  </div>
                  {operation.steps.length > 0 ? (
                    <ol className={consoleStyles.steps}>
                      {operation.steps.map((step) => (
                        <li key={step.id}>
                          <strong>{step.label}</strong>
                          <span>{step.status.replaceAll("_", " ")}</span>
                        </li>
                      ))}
                    </ol>
                  ) : (
                    <div className={consoleStyles.emptyDetail}>No directory steps recorded.</div>
                  )}
                  {operation.last_error && (
                    <p className={consoleStyles.error} role="status">{operation.last_error}</p>
                  )}
                  <RecordDocumentPanel
                    key={`${operation.id}:${operation.version}`}
                    base={base}
                    targetType="people_provision_operation"
                    targetId={operation.id}
                    expectedTargetVersion={operation.version}
                    title="Sanitized provisioning evidence"
                    purposes={[{
                      key: "sanitized_provisioning_evidence",
                      label: "Sanitized provisioning evidence",
                      description: "Attach only a redacted review artifact. Never upload passwords, private keys, secrets, configuration exports, or database dumps.",
                      visibility: "Restricted to Company IT provisioning administrators. Sanitization attestation is required before binding.",
                    }]}
                    editable={canApply}
                  />
                  <div className={consoleStyles.detailActions}>
                    {canApply &&
                      ["FRESH_MFA_REQUIRED", "PROVISIONING_RETRY_REQUIRED"].includes(
                        operation.last_error ?? "",
                      ) && (
                        <button
                          className="button secondary"
                          disabled={busy}
                          onClick={async () => {
                            if (
                              await send("retry", {
                                id: operation.id,
                                expectedVersion: operation.version,
                              })
                            )
                              router.refresh();
                          }}
                        >
                          Retry unfinished operation
                        </button>
                      )}
                    {operation.steps.length > 0 && (
                      <Link href={base + "/technology?view=organization"}>
                        Inspect directory steps and retry
                      </Link>
                    )}
                    <Link href={base + "/people/settings?section=reviews"}>
                      Review ERPX role requests
                    </Link>
                  </div>
                </div>
              </>
            ) : (
              <div className={consoleStyles.emptyDetail}>Select an operation to inspect progress.</div>
            )}
          </section>
        </div>
      )}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Prepare employee resources"
        presentation="responsive-panel"
      >
        <form
          className={styles.form}
          onChange={() => {
            setPreview(null);
            setInput(null);
          }}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              pkg = lifecycle.packages.find(
                (p) => p.id === String(f.get("packageId")),
              ),
              parsed = provisioningInput.safeParse({
                engagementIds: selected,
                sourceKind: withdraw?.source_kind ?? source,
                packageId: withdraw?.package_id ?? pkg?.id ?? null,
                revision:
                  withdraw?.package_revision ?? pkg?.published_revision ?? null,
                revokeOnly: !!withdraw,
                revokeApplicationIds: withdraw ? [withdraw.id] : [],
                restoreAccess: f.has("restoreAccess"),
                exclude: String(f.get("exclude") ?? "")
                  .split("\n")
                  .map((s) => s.trim())
                  .filter(Boolean),
                validUntil: withdraw
                  ? Math.max(withdraw.valid_until, Date.now() + 60000)
                  : new Date(String(f.get("expires"))).getTime(),
                reason: String(f.get("reason")),
              });
            if (!parsed.success) {
              setError(parsed.error.issues[0]?.message ?? "Check the request.");
              return;
            }
            const result = await send("preview", parsed.data);
            if (result) {
              setInput(parsed.data);
              setPreview(result);
            }
          }}
        >
          <p>
            {withdraw
              ? `Withdraw applied source: ${withdraw.definition.name} · r${withdraw.package_revision}`
              : `${selected.length} employees selected. HR placement is already effective.`}
          </p>
          {!withdraw && (
            <>
              <label>
                Access source
                <select
                  value={source}
                  onChange={(e) => setSource(e.target.value as typeof source)}
                >
                  <option value="position">
                    Replace applied position package
                  </option>
                  <option value="supplemental">
                    Additional access for these people
                  </option>
                </select>
              </label>
              <label>
                Access package
                <select name="packageId" required={source === "supplemental"}>
                  <option value="">Use each employee’s position package</option>
                  {lifecycle.packages
                    .filter((p) => p.published_revision)
                    .map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.definition.name} · r{p.published_revision}
                      </option>
                    ))}
                </select>
              </label>
              <label>
                Access expires
                <input type="datetime-local" name="expires" required />
              </label>
              <label>
                <input type="checkbox" name="restoreAccess" /> Restore access
                after a completed HR return (old grants stay revoked)
              </label>
              <details>
                <summary>Exceptions</summary>
                <label>
                  Exclude resource keys (one per line)
                  <textarea
                    name="exclude"
                    placeholder={
                      "role:role-id\ngroup:group-id\ndirectory:group-id\ngpo:gpo-id"
                    }
                  />
                </label>
              </details>
            </>
          )}
          <label>
            Reason
            <textarea name="reason" required minLength={3} maxLength={1000} />
          </label>
          <button className="button secondary" disabled={busy}>
            Preview changes
          </button>
          {preview && input && (
            <div className={styles.preview}>
              {preview.employees.map((p) => (
                <section key={p.id}>
                  <h3>{p.name}</h3>
                  <p>
                    {p.packageName} · revision {p.revision}
                  </p>
                  <p>Add: {p.added.join(", ") || "No new grants"}</p>
                  <p>Replace source: {p.removed.join(", ") || "None"}</p>
                  <p>
                    Retain:{" "}
                    {p.retained.join(", ") ||
                      "Other independent access keeps its own conditions"}
                  </p>
                  <p>{p.reviews} ERPX role requests need independent review.</p>
                </section>
              ))}
              {preview.warnings.map((w) => (
                <p key={w}>{w}</p>
              ))}
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={async () => {
                  const result = await send("apply", {
                    ...input,
                    previewToken: preview.token,
                  });
                  if (result) {
                    setOpen(false);
                    setSelected([]);
                    setNotice(
                      "Provisioning queued. ERPX role requests remain subject to independent review.",
                    );
                    router.refresh();
                  }
                }}
              >
                Apply resources & request role reviews
              </button>
            </div>
          )}
          {error && <p role="alert">{error}</p>}
        </form>
      </Dialog>
    </div>
  );
}
