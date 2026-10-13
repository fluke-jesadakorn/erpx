"use client";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { PreferredDateTimeField } from "@/components/people_payroll/time/preferred-date-time-field";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { RecordLink as Link } from "@/components/records/record-link";
import { Dialog } from "@/components/ui/dialog";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { workFetch } from "@/lib/work_projection/work-fetch";
import {
  provisioningInput,
  type ProvisioningInput,
  type ProvisioningPreview,
  type TechnologyData,
} from "@/server/modules/identity_access/public/rules/technology";
import type { PeopleLifecycleData } from "@/lib/people_payroll/people-lifecycle";
import styles from "@/components/identity_access/technology/technology.module.css";
import consoleStyles from "@/components/identity_access/technology/provisioning-console.module.css";
import { type UiAppearance } from "@/components/ui/foundation";
export function ProvisioningWorkspace({ appearance = "legacy",
  base,
  lifecycle,
  data,
  canApply,
  section = "people",
}: ({
  base: string;
  lifecycle: PeopleLifecycleData;
  data: TechnologyData;
  canApply: boolean;
  section?: "people" | "operations";
}) & { appearance?: UiAppearance }) {
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
    [detail, setDetail] = useState<string | null>(lifecycle.roster[0]?.engagementId ?? null),
    [operationDetail, setOperationDetail] = useState<string | null>(data.operations[0]?.id ?? null),
    [mobileDetail, setMobileDetail] = useState(false),
    [search, setSearch] = useState(""),
    [statusFilter, setStatusFilter] = useState("all"),
    [source, setSource] = useState<"position" | "supplemental">("position"),
    [withdraw, setWithdraw] = useState<import("@/server/modules/identity_access/public/rules/technology").AppliedPackage | null>(null);
  const pending = data.operations.some((op) => ["queued", "running", "retry"].includes(op.status));
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
      const r = await workFetch("/api/v1" + base + "/technology/provisioning/" + path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": receipt.current.key,
          },
          body: encoded,
        }),
        result = await r.json();
      if (!r.ok) throw Error(result.error?.message ?? "Unable to provision access.");
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
        (a) => a.engagement_id === id && a.active && a.source_kind === "position",
      );
    if (
      data.operations.some(
        (op) =>
          op.status === "blocked" &&
          data.applications.some((a) => a.engagement_id === id && a.id.startsWith(op.id + ":")),
      )
    )
      return "Needs attention";
    if (
      data.operations.some(
        (op) =>
          (["queued", "running", "retry"].includes(op.status) || op.reviewsPending > 0) &&
          data.applications.some((a) => a.engagement_id === id && a.id.startsWith(op.id + ":")),
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
    lifecycle.roster.map((p) => [p.engagementId, status(p.engagementId, p.positionId)]),
  );
  const normalizedSearch = search.trim().toLocaleLowerCase();
  const filteredPeople = lifecycle.roster.filter((p) => {
    const position =
      lifecycle.positions.find((item) => item.id === p.positionId)?.title ?? "No position";
    return (
      (statusFilter === "all" || statusById.get(p.engagementId) === statusFilter) &&
      (!normalizedSearch ||
        [p.name, p.engagementId, position].some((value) =>
          value.toLocaleLowerCase().includes(normalizedSearch),
        ))
    );
  });
  const selectableVisible = filteredPeople.filter((p) => p.status === "active");
  const person = filteredPeople.find((p) => p.engagementId === detail) ?? filteredPeople[0];
  const operation = data.operations.find((op) => op.id === operationDetail) ?? data.operations[0];
  const personApplications = person
    ? data.applications.filter((a) => a.engagement_id === person.engagementId)
    : [];
  return (
    <div data-ui-appearance={appearance} className={consoleStyles.workspace}>
      <div data-ui-appearance={appearance} className={consoleStyles.toolbar}>
        <div data-ui-appearance={appearance} className={consoleStyles.heading}>
          <h2 data-ui-appearance={appearance}>{section === "operations" ? "Provisioning operations" : "Employee access"}</h2>
          <span data-ui-appearance={appearance} className={consoleStyles.count}>
            {section === "operations"
              ? data.operations.length + " operations"
              : filteredPeople.length + " of " + lifecycle.roster.length + " employees"}
          </span>
        </div>
        {section === "people" && canApply && (
          <div data-ui-appearance={appearance} className={consoleStyles.actions}>
            {selected.length > 0 && (
              <Button appearance={appearance}
                type="submit"
                variant="plain"
                className={consoleStyles.textAction}
                onClick={() => setSelected([])}
              >
                Clear selection
              </Button>
            )}
            <Button appearance={appearance}
              type="submit"
              variant="primary"

              disabled={!selected.length}
              onClick={() => {
                setPreview(null);
                setInput(null);
                setWithdraw(null);
                setOpen(true);
              }}
            >
              Prepare {selected.length || ""} selected
            </Button>
          </div>
        )}
      </div>
      {notice && (
        <p data-ui-appearance={appearance} className={consoleStyles.notice} role="status">
          {notice}
        </p>
      )}
      {error && !open && (
        <p data-ui-appearance={appearance} className={consoleStyles.error} role="alert">
          {error}
        </p>
      )}
      {section === "people" ? (
        <div data-ui-appearance={appearance} className={consoleStyles.console} data-mobile-detail={mobileDetail}>
          <div data-ui-appearance={appearance} className={consoleStyles.listPane}>
            <div data-ui-appearance={appearance} className={consoleStyles.listControls}>
              <Field appearance={appearance} className={consoleStyles.searchLabel}>
                <span data-ui-appearance={appearance} className="sr-only">Search employees</span>
                <TextInput appearance={appearance}
                  type="search"
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value);
                    setMobileDetail(false);
                  }}
                  placeholder="Search name or position"
                />
              </Field>
              <Field appearance={appearance} className={consoleStyles.filterLabel}>
                <span data-ui-appearance={appearance} className="sr-only">Filter by IT status</span>
                <Select appearance={appearance}
                  value={statusFilter}
                  onChange={(event) => {
                    setStatusFilter(event.target.value);
                    setMobileDetail(false);
                  }}
                >
                  <option data-ui-appearance={appearance} value="all">All IT statuses</option>
                  <option data-ui-appearance={appearance} value="Ready">Ready</option>
                  <option data-ui-appearance={appearance} value="In progress">In progress</option>
                  <option data-ui-appearance={appearance} value="Needs attention">Needs attention</option>
                  <option data-ui-appearance={appearance} value="Waiting for IT">Waiting for IT</option>
                </Select>
              </Field>
            </div>
            <div data-ui-appearance={appearance} className={consoleStyles.listHeader}>
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
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
                      const visibleIds = new Set(selectableVisible.map((p) => p.engagementId));
                      if (!checked) return current.filter((id) => !visibleIds.has(id));
                      const next = [...current];
                      for (const p of selectableVisible) {
                        if (next.length >= 100) break;
                        if (!next.includes(p.engagementId)) next.push(p.engagementId);
                      }
                      return next;
                    });
                  }}
                />
                <span data-ui-appearance={appearance}>Select visible</span>
              </Field>
              <span data-ui-appearance={appearance}>{selected.length}/100 selected</span>
            </div>
            <div data-ui-appearance={appearance} className={consoleStyles.listScroll} role="list" aria-label="Employees">
              {filteredPeople.map((p) => {
                const position =
                  lifecycle.positions.find((item) => item.id === p.positionId)?.title ??
                  "No position";
                const state = statusById.get(p.engagementId) ?? "Waiting for IT";
                return (
                  <div data-ui-appearance={appearance}
                    className={consoleStyles.listRow}
                    data-selected={person?.engagementId === p.engagementId}
                    key={p.engagementId}
                    role="listitem"
                  >
                    <TextInput appearance={appearance}
                      type="checkbox"
                      aria-label={"Select " + p.name}
                      disabled={
                        !canApply ||
                        p.status !== "active" ||
                        (selected.length >= 100 && !selected.includes(p.engagementId))
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
                    <Button appearance={appearance}
                      type="submit"
                      variant="plain"
                      ref={person?.engagementId === p.engagementId ? activeRowRef : undefined}
                      className={consoleStyles.rowButton}
                      aria-current={person?.engagementId === p.engagementId ? "true" : undefined}
                      onClick={() => {
                        setDetail(p.engagementId);
                        setMobileDetail(true);
                      }}
                    >
                      <span data-ui-appearance={appearance} className={consoleStyles.rowMain}>
                        <strong data-ui-appearance={appearance}>{p.name}</strong>
                        <span data-ui-appearance={appearance}>{position}</span>
                      </span>
                      <span data-ui-appearance={appearance} className={consoleStyles.badge} data-status={state}>
                        {state}
                      </span>
                    </Button>
                  </div>
                );
              })}
              {filteredPeople.length === 0 && (
                <div data-ui-appearance={appearance} className={consoleStyles.empty}>
                  <strong data-ui-appearance={appearance}>No employees found</strong>
                  <p data-ui-appearance={appearance}>Try another name, position, or IT status.</p>
                  <Button appearance={appearance}
                    type="submit"
                    variant="plain"
                    className={consoleStyles.textAction}
                    onClick={() => {
                      setSearch("");
                      setStatusFilter("all");
                    }}
                  >
                    Clear filters
                  </Button>
                </div>
              )}
            </div>
          </div>
          <section data-ui-appearance={appearance} className={consoleStyles.detailPane} aria-label="Employee access details">
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              ref={backRef}
              className={consoleStyles.back}
              onClick={() => setMobileDetail(false)}
            >
              ← Back to employees
            </Button>
            {person ? (
              <>
                <div data-ui-appearance={appearance} className={consoleStyles.detailHead}>
                  <div data-ui-appearance={appearance}>
                    <span data-ui-appearance={appearance} className={consoleStyles.eyebrow}>Employee</span>
                    <h3 data-ui-appearance={appearance}>{person.name}</h3>
                    <p data-ui-appearance={appearance}>
                      {lifecycle.positions.find((p) => p.id === person.positionId)?.title ??
                        "No position assigned"}
                    </p>
                  </div>
                  <span data-ui-appearance={appearance}
                    className={consoleStyles.badge}
                    data-status={statusById.get(person.engagementId)}
                  >
                    {statusById.get(person.engagementId)}
                  </span>
                </div>
                <div data-ui-appearance={appearance} className={consoleStyles.detailScroll}>
                  <div data-ui-appearance={appearance} className={consoleStyles.detailSectionHead}>
                    <h4 data-ui-appearance={appearance}>Applied access</h4>
                    <span data-ui-appearance={appearance}>{personApplications.filter((a) => a.active).length} effective</span>
                  </div>
                  {personApplications.length === 0 && (
                    <div data-ui-appearance={appearance} className={consoleStyles.emptyDetail}>
                      No access package has been applied to this employee.
                    </div>
                  )}
                  {personApplications.map((a) => (
                    <section data-ui-appearance={appearance} className={consoleStyles.accessCard} key={a.id}>
                      <div data-ui-appearance={appearance} className={consoleStyles.accessHead}>
                        <div data-ui-appearance={appearance}>
                          <strong data-ui-appearance={appearance}>{a.definition.name}</strong>
                          <span data-ui-appearance={appearance}>
                            Revision {a.package_revision} · {a.source_kind} source
                          </span>
                        </div>
                        <span data-ui-appearance={appearance}
                          className={consoleStyles.badge}
                          data-status={a.active ? "Ready" : "Inactive"}
                        >
                          {a.active ? "Effective" : "Inactive / pending"}
                        </span>
                      </div>
                      <dl data-ui-appearance={appearance} className={consoleStyles.factGrid}>
                        <div data-ui-appearance={appearance}>
                          <dt data-ui-appearance={appearance}>Expires</dt>
                          <dd data-ui-appearance={appearance}>
                            <TimeText appearance={appearance} value={a.valid_until} />
                          </dd>
                        </div>
                        <div data-ui-appearance={appearance}>
                          <dt data-ui-appearance={appearance}>ERPX roles</dt>
                          <dd data-ui-appearance={appearance}>
                            {a.definition.grants.map((g) => g.roleId).join(", ") || "None"}{" "}
                            <small data-ui-appearance={appearance}>(reviewed separately)</small>
                          </dd>
                        </div>
                        <div data-ui-appearance={appearance}>
                          <dt data-ui-appearance={appearance}>Groups</dt>
                          <dd data-ui-appearance={appearance}>
                            {[...a.definition.groupIds, ...a.definition.directoryGroupIds].join(
                              ", ",
                            ) || "None"}
                          </dd>
                        </div>
                        <div data-ui-appearance={appearance}>
                          <dt data-ui-appearance={appearance}>Exceptions</dt>
                          <dd data-ui-appearance={appearance}>{a.scope.exclude?.join(", ") || "None"}</dd>
                        </div>
                      </dl>
                      {canApply && !a.ended_at && (
                        <Button appearance={appearance}
                          type="submit"
                          variant="secondary"

                          onClick={() => {
                            setSelected([a.engagement_id]);
                            setWithdraw(a);
                            setPreview(null);
                            setInput(null);
                            setOpen(true);
                          }}
                        >
                          Withdraw this source
                        </Button>
                      )}
                    </section>
                  ))}
                </div>
              </>
            ) : (
              <div data-ui-appearance={appearance} className={consoleStyles.emptyDetail}>Select an employee to inspect access.</div>
            )}
          </section>
        </div>
      ) : (
        <div data-ui-appearance={appearance} className={consoleStyles.console} data-mobile-detail={mobileDetail}>
          <div data-ui-appearance={appearance} className={consoleStyles.listPane}>
            <div data-ui-appearance={appearance} className={consoleStyles.listHeader}>
              <strong data-ui-appearance={appearance}>Recent operations</strong>
              <span data-ui-appearance={appearance}>{data.operations.length} total</span>
            </div>
            <div data-ui-appearance={appearance}
              className={consoleStyles.listScroll}
              role="list"
              aria-label="Provisioning operations"
            >
              {data.operations.map((op) => (
                <div data-ui-appearance={appearance}
                  className={consoleStyles.listRow}
                  data-selected={operation?.id === op.id}
                  key={op.id}
                  role="listitem"
                >
                  <Button appearance={appearance}
                    type="submit"
                    variant="plain"
                    ref={operation?.id === op.id ? activeRowRef : undefined}
                    className={consoleStyles.rowButton}
                    aria-current={operation?.id === op.id ? "true" : undefined}
                    onClick={() => {
                      setOperationDetail(op.id);
                      setMobileDetail(true);
                    }}
                  >
                    <span data-ui-appearance={appearance} className={consoleStyles.rowMain}>
                      <strong data-ui-appearance={appearance}>{op.status.replaceAll("_", " ")}</strong>
                      <span data-ui-appearance={appearance}>
                        <TimeText appearance={appearance} value={op.created_at} />
                      </span>
                    </span>
                    {op.reviewsPending > 0 && (
                      <span data-ui-appearance={appearance} className={consoleStyles.reviewCount}>{op.reviewsPending} reviews</span>
                    )}
                  </Button>
                </div>
              ))}
              {data.operations.length === 0 && (
                <div data-ui-appearance={appearance} className={consoleStyles.empty}>No provisioning operations yet.</div>
              )}
            </div>
          </div>
          <section data-ui-appearance={appearance} className={consoleStyles.detailPane} aria-label="Operation details">
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              ref={backRef}
              className={consoleStyles.back}
              onClick={() => setMobileDetail(false)}
            >
              ← Back to operations
            </Button>
            {operation ? (
              <>
                <div data-ui-appearance={appearance} className={consoleStyles.detailHead}>
                  <div data-ui-appearance={appearance}>
                    <span data-ui-appearance={appearance} className={consoleStyles.eyebrow}>Provisioning operation</span>
                    <h3 data-ui-appearance={appearance}>{operation.status.replaceAll("_", " ")}</h3>
                    <p data-ui-appearance={appearance}>
                      <TimeText appearance={appearance} value={operation.created_at} />
                    </p>
                  </div>
                </div>
                <div data-ui-appearance={appearance} className={consoleStyles.detailScroll}>
                  <div data-ui-appearance={appearance} className={consoleStyles.detailSectionHead}>
                    <h4 data-ui-appearance={appearance}>Progress</h4>
                    <span data-ui-appearance={appearance}>{operation.reviewsPending} ERPX role reviews pending</span>
                  </div>
                  {operation.steps.length > 0 ? (
                    <ol data-ui-appearance={appearance} className={consoleStyles.steps}>
                      {operation.steps.map((step) => (
                        <li data-ui-appearance={appearance} key={step.id}>
                          <strong data-ui-appearance={appearance}>{step.label}</strong>
                          <span data-ui-appearance={appearance}>{step.status.replaceAll("_", " ")}</span>
                        </li>
                      ))}
                    </ol>
                  ) : (
                    <div data-ui-appearance={appearance} className={consoleStyles.emptyDetail}>No directory steps recorded.</div>
                  )}
                  {operation.last_error && (
                    <p data-ui-appearance={appearance} className={consoleStyles.error} role="status">
                      {operation.last_error}
                    </p>
                  )}
                  <RecordDocumentPanel appearance={appearance}
                    key={`${operation.id}:${operation.version}`}
                    base={base}
                    targetType="people_provision_operation"
                    targetId={operation.id}
                    expectedTargetVersion={operation.version}
                    title="Sanitized provisioning evidence"
                    purposes={[
                      {
                        key: "sanitized_provisioning_evidence",
                        label: "Sanitized provisioning evidence",
                        description:
                          "Attach only a redacted review artifact. Never upload passwords, private keys, secrets, configuration exports, or database dumps.",
                        visibility:
                          "Restricted to Company IT provisioning administrators. Sanitization attestation is required before binding.",
                      },
                    ]}
                    editable={canApply}
                  />
                  <div data-ui-appearance={appearance} className={consoleStyles.detailActions}>
                    {canApply &&
                      ["FRESH_MFA_REQUIRED", "PROVISIONING_RETRY_REQUIRED"].includes(
                        operation.last_error ?? "",
                      ) && (
                        <Button appearance={appearance}
                          type="submit"
                          variant="secondary"

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
                        </Button>
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
              <div data-ui-appearance={appearance} className={consoleStyles.emptyDetail}>
                Select an operation to inspect progress.
              </div>
            )}
          </section>
        </div>
      )}
      <Dialog appearance={appearance}
        open={open}
        onClose={() => setOpen(false)}
        title="Prepare employee resources"
        presentation="form-drawer"
      >
        <form data-ui-appearance={appearance}
          className={styles.form}
          onChange={() => {
            setPreview(null);
            setInput(null);
          }}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              pkg = lifecycle.packages.find((p) => p.id === String(f.get("packageId"))),
              parsed = provisioningInput.safeParse({
                engagementIds: selected,
                sourceKind: withdraw?.source_kind ?? source,
                packageId: withdraw?.package_id ?? pkg?.id ?? null,
                revision: withdraw?.package_revision ?? pkg?.published_revision ?? null,
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
          <p data-ui-appearance={appearance}>
            {withdraw
              ? `Withdraw applied source: ${withdraw.definition.name} · r${withdraw.package_revision}`
              : `${selected.length} employees selected. HR placement is already effective.`}
          </p>
          {!withdraw && (
            <>
              <Field appearance={appearance}>
                Access source
                <Select appearance={appearance} value={source} onChange={(e) => setSource(e.target.value as typeof source)}>
                  <option data-ui-appearance={appearance} value="position">Replace applied position package</option>
                  <option data-ui-appearance={appearance} value="supplemental">Additional access for these people</option>
                </Select>
              </Field>
              <Field appearance={appearance}>
                Access package
                <Select appearance={appearance} name="packageId" required={source === "supplemental"}>
                  <option data-ui-appearance={appearance} value="">Use each employee’s position package</option>
                  {lifecycle.packages
                    .filter((p) => p.published_revision)
                    .map((p) => (
                      <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                        {p.definition.name} · r{p.published_revision}
                      </option>
                    ))}
                </Select>
              </Field>
              <PreferredDateTimeField appearance={appearance} name="expires" label="Access expires" required />
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance} type="checkbox" name="restoreAccess" /> Restore access after a completed
                HR return (old grants stay revoked)
              </Field>
              <details data-ui-appearance={appearance}>
                <summary data-ui-appearance={appearance}>Exceptions</summary>
                <Field appearance={appearance}>
                  Exclude resource keys (one per line)
                  <TextArea appearance={appearance}
                    name="exclude"
                    placeholder={"role:role-id\ngroup:group-id\ndirectory:group-id\ngpo:gpo-id"}
                  />
                </Field>
              </details>
            </>
          )}
          <Field appearance={appearance}>
            Reason
            <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
          </Field>
          <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy}>
            Preview changes
          </Button>
          {preview && input && (
            <div data-ui-appearance={appearance} className={styles.preview}>
              {preview.employees.map((p) => (
                <section data-ui-appearance={appearance} key={p.id}>
                  <h3 data-ui-appearance={appearance}>{p.name}</h3>
                  <p data-ui-appearance={appearance}>
                    {p.packageName} · revision {p.revision}
                  </p>
                  <p data-ui-appearance={appearance}>Add: {p.added.join(", ") || "No new grants"}</p>
                  <p data-ui-appearance={appearance}>Replace source: {p.removed.join(", ") || "None"}</p>
                  <p data-ui-appearance={appearance}>
                    Retain:{" "}
                    {p.retained.join(", ") || "Other independent access keeps its own conditions"}
                  </p>
                  <p data-ui-appearance={appearance}>{p.reviews} ERPX role requests need independent review.</p>
                </section>
              ))}
              {preview.warnings.map((w) => (
                <p data-ui-appearance={appearance} key={w}>{w}</p>
              ))}
              <Button appearance={appearance}
                variant="primary"
                type="button"

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
              </Button>
            </div>
          )}
          {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
        </form>
      </Dialog>
    </div>
  );
}
