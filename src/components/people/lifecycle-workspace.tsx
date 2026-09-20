"use client";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import {
  Plus,
  ShieldCheck,
  BriefcaseBusiness,
  ClipboardList,
  ArrowRight,
} from "lucide-react";
import { SetupChecklist } from "@/components/ui/setup-checklist";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import { retirementLocalTime, retirementInstant, nextRetirementDay } from "@/lib/retirement";
import type { RoleRevision } from "@/lib/iam";
import type {
  AccessPackage,
  AccessPackageDefinition,
  LifecycleCase,
  LifecycleInput,
  LifecyclePreview,
  PeopleLifecycleData,
} from "@/lib/people-lifecycle";
import styles from "./lifecycle.module.css";

function useCommand(base: string) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const receipt = useRef<{ body: string; key: string } | null>(null);
  async function send<T>(path: string, input: unknown): Promise<T | undefined> {
    const body = JSON.stringify(input),
      fingerprint = path + body;
    if (receipt.current?.body !== fingerprint)
      receipt.current = { body: fingerprint, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
        `/api/v1${base}/people/lifecycle/${path}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": receipt.current.key,
          },
          body,
        },
      );
      const result = await response.json();
      if (!response.ok)
        throw Error(
          result.error?.message ?? "Unable to save. Please try again.",
        );
      receipt.current = null;
      if (!path.endsWith("preview")) router.refresh();
      return result.data as T;
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to save.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function AccessRecipe({
  definition,
}: {
  definition: AccessPackageDefinition;
}) {
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
            ? definition.grants
                .map(
                  (g) =>
                    `${g.roleId} · ${g.scopeKind === "holding" ? "Company" : g.scopeKind} · until ${new Date(g.validUntil).toLocaleDateString()}`,
                )
                .join("; ")
            : "No ERPX roles requested"}
        </span>
      </div>
      <div>
        <BriefcaseBusiness size={17} />
        <span>
          {definition.groupIds.length} ERPX groups ·{" "}
          {definition.directoryGroupIds.length} directory groups ·{" "}
          {definition.windowsPolicies.length} Windows policies
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
          {p.processor === "admx"
            ? "Windows administrative policy"
            : "Drive / printer preference"}{" "}
          · OU: {p.targetOu}
          {p.computerTarget ? ` · Computers: ${p.computerTarget}` : ""} · IT
          verification required
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
}: {
  base: string;
  data: PeopleLifecycleData;
  positionId?: string;
  hrDetails?: (id: string) => ReactNode;
}) {
  const [selected, setSelected] = useState(positionId ?? null),
    [packageId, setPackageId] = useState(""),
    [preview, setPreview] = useState<{
      token: string;
      people: string[];
      definition: AccessPackageDefinition;
    } | null>(null);
  const { send, busy, error } = useCommand(base),
    position = data.positions.find((p) => p.id === selected),
    pkg = data.packages.find((p) => p.id === packageId),
    task = data.tasks.find(
      (t) => t.position_id === selected && t.kind === "position",
    );
  const input =
    position && pkg
      ? {
          positionId: position.id,
          expectedVersion: position.version,
          packageId: pkg.id,
          revision: pkg.published_revision,
        }
      : null;
  function select(id: string) {
    setSelected(id);
    setPackageId(data.positions.find((p) => p.id === id)?.package_id ?? "");
    setPreview(null);
  }
  return (
    <section className="card">
      {data.canPrepare && (
        <div className={styles.stack}>
          <h3>Published access packages</h3>
          <p>
            Drag a package onto a position, or open a position and choose its
            package.
          </p>
          <div className={styles.actions}>
            {data.packages
              .filter((p) => p.published_revision > 0)
              .map((p) => (
                <span
                  key={p.id}
                  className="pill"
                  draggable
                  onDragStart={(e) =>
                    e.dataTransfer.setData(
                      "application/erpx-access-package",
                      p.id,
                    )
                  }
                >
                  {p.definition.name} · revision {p.published_revision}
                </span>
              ))}
          </div>
        </div>
      )}
      <div className={styles.heading}>
        <div>
          <h2>Position readiness</h2>
          <p className="muted">
            HR defines the work. IT prepares a reusable access package.
          </p>
        </div>
        <Link href={`${base}/work?type=people_lifecycle`}>
          Open IT queue <ArrowRight size={15} />
        </Link>
      </div>
      <div className={styles.positionGrid}>
        {data.positions
          .filter((p) => p.active)
          .map((p) => (
            <button
              className={styles.positionCard}
              key={p.id}
              onClick={() => select(p.id)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={async (e) => {
                e.preventDefault();
                const id = e.dataTransfer.getData(
                  "application/erpx-access-package",
                );
                if (
                  data.canPrepare &&
                  data.packages.some((x) => x.id === id && x.published_revision)
                ) {
                  select(p.id);
                  setPackageId(id);
                  const pkg = data.packages.find((x) => x.id === id)!;
                  const result = await send<typeof preview>(
                    "position/preview",
                    {
                      positionId: p.id,
                      expectedVersion: p.version,
                      packageId: id,
                      revision: pkg.published_revision,
                    },
                  );
                  if (result) setPreview(result);
                }
              }}
            >
              <BriefcaseBusiness size={19} />
              <strong>{p.title}</strong>
              <span>
                {data.nodes.find((n) => n.id === p.branch_id)?.name ??
                  "Company"}{" "}
                ·{" "}
                {data.nodes.find((n) => n.id === p.department_id)?.name ??
                  "All departments"}
              </span>
              <span>HR ready</span>
              <span
                data-ready={
                  !!p.package_id &&
                  data.tasks.some(
                    (t) => t.position_id === p.id && t.status === "completed",
                  )
                }
              >
                {p.package_id &&
                data.tasks.some(
                  (t) => t.position_id === p.id && t.status === "completed",
                )
                  ? "IT prepared"
                  : "Waiting for IT"}
              </span>
            </button>
          ))}
      </div>
      <Dialog
        open={!!position}
        onClose={() => setSelected(null)}
        title={position?.title ?? "Position"}
        presentation="responsive-panel"
      >
        {position && (
          <div className={styles.stack}>
            <p>
              {data.nodes.find((n) => n.id === position.branch_id)?.name ??
                "Company"}{" "}
              ·{" "}
              {data.nodes.find((n) => n.id === position.department_id)?.name ??
                "All departments"}
            </p>
            <p>{position.requirements || "No additional work requirements."}</p>
            <p>
              HR ready ·{" "}
              {position.package_id && task?.status === "completed"
                ? "IT prepared"
                : "IT preparation pending"}
            </p>
            <p>
              {task?.assignee_id
                ? "Assigned to configured IT owner"
                : "Unassigned IT queue"}
            </p>
            {data.canManage && (
              <Link
                className="button"
                href={`${base}/people?position=${encodeURIComponent(position.id)}`}
              >
                Onboard into this position
              </Link>
            )}
            {hrDetails?.(position.id)}
            {data.canPrepare && (
              <>
                <label>
                  Published access package
                  <select
                    value={packageId}
                    onChange={(e) => {
                      setPackageId(e.target.value);
                      setPreview(null);
                    }}
                  >
                    <option value="">Choose a package</option>
                    {data.packages
                      .filter((p) => p.published_revision > 0)
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.definition.name} · revision {p.published_revision}
                        </option>
                      ))}
                  </select>
                </label>
                <Link href={`${base}/technology?view=resources`}>
                  Create or prepare an access package
                </Link>
                <button
                  className="button secondary"
                  disabled={busy || !input}
                  onClick={async () => {
                    const p = await send<typeof preview>(
                      "position/preview",
                      input,
                    );
                    if (p) setPreview(p);
                  }}
                >
                  Preview position access
                </button>
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
                  Binding this package updates the position recipe. IT selects employees and applies it separately. ERPX roles require independent review.
                </p>
                <button
                  className="button"
                  disabled={busy}
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
                </button>
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
  const [editing, setEditing] = useState<AccessPackage | "new" | null>(null),
    [definition, setDefinition] = useState(blankPackage),
    [reason, setReason] = useState(""),
    [preview, setPreview] = useState<{
      token: string;
      affectedPositions: string[];
      affectedPeople: string[];
      affectedComputers: string[];
    } | null>(null);
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
  return (
    <section className="card">
      <div className={styles.heading}>
        <div>
          <h2>Access packages</h2>
          <p className="muted">
            Share a published package across positions. Individual access is
            reviewed separately.
          </p>
        </div>
        {data.canPrepare && (
          <button className="button" onClick={() => edit("new")}>
            <Plus size={16} /> New package
          </button>
        )}
      </div>
      {!data.packages.length && (
        <p className={styles.empty}>
          Create the first access package, then assign it to a position.
        </p>
      )}
      <div className={styles.positionGrid}>
        {data.packages.map((p) => (
          <button
            className={styles.positionCard}
            key={p.id}
            draggable={data.canPrepare && p.published_revision > 0}
            onDragStart={(e) =>
              e.dataTransfer.setData("application/erpx-access-package", p.id)
            }
            onClick={() => edit(p)}
          >
            <ShieldCheck size={20} />
            <strong>{p.definition.name}</strong>
            <span>
              {p.published_revision
                ? `Published revision ${p.published_revision}`
                : "Draft"}
            </span>
            <span>
              {data.positions.filter((x) => x.package_id === p.id).length}{" "}
              positions
            </span>
          </button>
        ))}
      </div>
      <Dialog
        open={!!editing}
        onClose={() => setEditing(null)}
        title={
          editing === "new" ? "New access package" : "Prepare access package"
        }
        presentation="responsive-panel"
      >
        <form
          className={styles.stack}
          onSubmit={async (e: FormEvent) => {
            e.preventDefault();
            const result = await send<typeof preview>(
              "packages/preview",
              input,
            );
            if (result) setPreview(result);
          }}
        >
          <label>
            Package name
            <input
              required
              minLength={2}
              maxLength={160}
              value={definition.name}
              onChange={(e) => change({ ...definition, name: e.target.value })}
            />
          </label>
          <label>
            Account provider
            <select
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
            </select>
          </label>
          {definition.account !== "local" && (
            <label>
              Directory connection
              <select
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
              </select>
            </label>
          )}
          <fieldset>
            <legend>ERPX roles</legend>
            {definition.grants.map((g, i) => (
              <div key={i} className={styles.grant}>
                <label>
                  Role
                  <select
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
                  </select>
                </label>
                <label>
                  Scope
                  <select
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
                  </select>
                </label>
                {g.scopeKind !== "holding" && (
                  <label>
                    Access scope
                    <select
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
                    </select>
                  </label>
                )}
                <label>
                  Access expires
                  <input
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
                                  validUntil:
                                    Date.parse(e.target.value) + 86400000,
                                }
                              : x,
                          ),
                        });
                    }}
                  />
                </label>
                <button
                  type="button"
                  className="button secondary"
                  onClick={() =>
                    change({
                      ...definition,
                      grants: definition.grants.filter((_, j) => i !== j),
                    })
                  }
                >
                  Remove role
                </button>
              </div>
            ))}
            <button
              type="button"
              className="button secondary"
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
            </button>
          </fieldset>
          <fieldset>
            <legend>ERPX groups</legend>
            {groups.length ? (
              groups.map((g) => (
                <label className={styles.check} key={g.id}>
                  <input
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
                </label>
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
                  <label className={styles.check} key={g.id}>
                    <input
                      type="checkbox"
                      checked={definition.directoryGroupIds.includes(g.id)}
                      onChange={(e) =>
                        change({
                          ...definition,
                          directoryGroupIds: e.target.checked
                            ? [...definition.directoryGroupIds, g.id]
                            : definition.directoryGroupIds.filter(
                                (id) => id !== g.id,
                              ),
                        })
                      }
                    />
                    {g.name}
                  </label>
                ))}
              <p className="muted">
                Membership changes require IT completion in the directory.
              </p>
            </fieldset>
          )}
          {definition.connectionId && (
            <fieldset>
              <legend>Windows policy references</legend>
              <p className="muted">
                Record an existing GPO and its intended targets. IT must apply
                it and verify the result in Windows.
              </p>
              {definition.windowsPolicies.map((policy, i) => (
                <div className={styles.stack} key={i}>
                  <label>
                    GPO ID
                    <input
                      required
                      value={policy.id}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
                              i === j ? { ...p, id: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    Policy name
                    <input
                      required
                      value={policy.name}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
                              i === j ? { ...p, name: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    Policy type
                    <select
                      value={policy.processor}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
                              i === j
                                ? {
                                    ...p,
                                    processor: e.target
                                      .value as typeof policy.processor,
                                  }
                                : p,
                          ),
                        })
                      }
                    >
                      <option value="admx">
                        Administrative policy · USB / lock screen / desktop
                      </option>
                      <option value="preferences">
                        Preference · drive / printer
                      </option>
                    </select>
                  </label>
                  <label>
                    Scope
                    <select
                      value={policy.scope}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
                              i === j
                                ? {
                                    ...p,
                                    scope: e.target
                                      .value as typeof policy.scope,
                                  }
                                : p,
                          ),
                        })
                      }
                    >
                      <option value="user">User</option>
                      <option value="computer">Computer</option>
                    </select>
                  </label>
                  <label>
                    Target OU
                    <input
                      required
                      value={policy.targetOu}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
                              i === j ? { ...p, targetOu: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    Security filter group
                    <input
                      value={policy.securityGroup ?? ""}
                      onChange={(e) =>
                        change({
                          ...definition,
                          windowsPolicies: definition.windowsPolicies.map(
                            (p, j) =>
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
                  </label>
                  {policy.scope === "computer" && (
                    <label>
                      Computer group or OU
                      <input
                        required
                        value={policy.computerTarget ?? ""}
                        onChange={(e) =>
                          change({
                            ...definition,
                            windowsPolicies: definition.windowsPolicies.map(
                              (p, j) =>
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
                    </label>
                  )}
                  <button
                    className="button secondary"
                    type="button"
                    onClick={() =>
                      change({
                        ...definition,
                        windowsPolicies: definition.windowsPolicies.filter(
                          (_, j) => j !== i,
                        ),
                      })
                    }
                  >
                    Remove reference
                  </button>
                </div>
              ))}
              <button
                className="button secondary"
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
              </button>
            </fieldset>
          )}
          <fieldset>
            <legend>Preparation tasks</legend>
            {definition.tasks.map((task, i) => (
              <div className={styles.grant} key={i}>
                <label>
                  Type
                  <select
                    value={task.kind}
                    onChange={(e) =>
                      change({
                        ...definition,
                        tasks: definition.tasks.map((t, j) =>
                          i === j
                            ? { ...t, kind: e.target.value as typeof task.kind }
                            : t,
                        ),
                      })
                    }
                  >
                    {(
                      [
                        "equipment",
                        "drive",
                        "printer",
                        "directory",
                        "handover",
                      ] as const
                    ).map((kind) => (
                      <option key={kind} value={kind}>
                        {kind}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Task
                  <input
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
                </label>
                <button
                  className="button secondary"
                  type="button"
                  onClick={() =>
                    change({
                      ...definition,
                      tasks: definition.tasks.filter((_, j) => j !== i),
                    })
                  }
                >
                  Remove task
                </button>
              </div>
            ))}
            <button
              className="button secondary"
              type="button"
              onClick={() =>
                change({
                  ...definition,
                  tasks: [
                    ...definition.tasks,
                    { kind: "equipment", label: "" },
                  ],
                })
              }
            >
              Add preparation task
            </button>
          </fieldset>
          <label>
            Reason
            <input
              required
              minLength={3}
              value={reason}
              onChange={(e) => {
                setReason(e.target.value);
                setPreview(null);
              }}
            />
          </label>
          {data.canPrepare && (
            <div className={styles.actions}>
              <button
                type="button"
                className="button secondary"
                disabled={busy || reason.length < 3}
                onClick={async () => {
                  if (await send("packages/draft", input)) setEditing(null);
                }}
              >
                Save draft
              </button>
              <button className="button" disabled={busy}>
                Preview publication
              </button>
            </div>
          )}
          {preview && (
            <div className={styles.preview}>
              <h3>Publication impact</h3>
              <p>
                Positions: {preview.affectedPositions.join(", ") || "None yet"}
              </p>
              <p>People: {preview.affectedPeople.join(", ") || "None yet"}</p>
              <p>
                Computer targets:{" "}
                {preview.affectedComputers.join(", ") || "None"}
              </p>
              <p>
                Existing personal grants remain on their reviewed revision until
                a new lifecycle request.
              </p>
              <button
                type="button"
                className="button"
                disabled={busy}
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
              </button>
            </div>
          )}
          {error && <p role="alert">{error}</p>}
        </form>
      </Dialog>
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
  const editableInitial =
    initialCase && ["draft", "waiting_it"].includes(initialCase.status)
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
    [initialTime] = useState(() => Date.now() + 3600000),
    [rehireOpen, setRehireOpen] = useState(false);
  const [departureReason,setDepartureReason] = useState<"retirement"|"other">(initialCase?.definition.departureReason ?? "other");
  const [timezone,setTimezone] = useState(initialCase?.definition.timezone ?? "Asia/Bangkok");
  const [effectiveLocal,setEffectiveLocal] = useState(() => retirementLocalTime(initialCase?.effective_at ?? initialTime,initialCase?.definition.timezone ?? "Asia/Bangkok"));
  const [dateError,setDateError] = useState("");
  const cases = data.cases.filter((c) => c.person_id === person.id);
  const itStatus=data.roster.find(p=>p.engagementId===person.engagementId)?.itStatus??"waiting";
  return (
    <section className="card">
      <div className={styles.heading}>
        <div>
          <h2>Employment & position</h2>
          <p>IT preparation: {{waiting:"Waiting for IT",working:"In progress",ready:"Ready",problem:"Needs attention"}[itStatus]}</p>
          {data.canPrepare&&<Link href={`${base}/technology?view=employees`}>Open in Technology</Link>}
          <p className="muted">
            Onboard, transfer, suspend, leave, or retire in this Company.
          </p>
        </div>
        {data.canManage && person.status !== "ended" && (
          <button
            className="button"
            onClick={() => {
              setDraft(null);
              setDepartureReason("other");
              setPreview(null);
              setKind(
                person.status === "onboarding" ? "onboarding" : "transfer",
              );
              setOpen(true);
              setStep(1);
            }}
          >
            Start a change
          </button>
        )}
      </div>
      {data.canManage && person.status !== "ended" && <button className="button secondary" onClick={() => {setDraft(null);setDepartureReason("retirement");setKind("offboarding");setPreview(null);setStep(1);setOpen(true);}}>Retire employee</button>}
      {data.canManage && person.status === "ended" && (
        <button
          className="button secondary"
          onClick={() => setRehireOpen(true)}
        >
          Create a new engagement for rehire
        </button>
      )}
      <Dialog
        open={rehireOpen}
        onClose={() => setRehireOpen(false)}
        title="Rehire · new engagement"
        presentation="responsive-panel"
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
            Previous employment and revoked access remain in history. The new
            engagement starts with onboarding and requires new access review.
          </p>
          <label>
            New position
            <select name="position" required>
              <option value="">Choose a position</option>
              {data.positions
                .filter((p) => p.active)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.title} ·{" "}
                    {data.nodes.find((n) => n.id === p.branch_id)?.name ??
                      "Company"}{" "}
                    ·{" "}
                    {data.nodes.find((n) => n.id === p.department_id)?.name ??
                      "All departments"}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Start date
            <input type="date" name="startDate" required />
          </label>
          <label>
            Reason
            <input name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Create onboarding engagement
          </button>
          {error && <p role="alert">{error}</p>}
        </form>
      </Dialog>
      {cases.map((c) => (
        <div className={styles.case} key={c.id}>
          <strong>
            {c.definition.departureReason === "retirement" ? "Retirement" : c.kind} · {c.status.replaceAll("_", " ")}
          </strong>
          <p>
            Effective {new Date(c.effective_at).toLocaleString("en-GB",{timeZone:c.definition.timezone ?? "Asia/Bangkok"})} {c.definition.timezone ?? "Asia/Bangkok"} ·{" "}
            {c.definition.reason}
          </p>
          {c.definition.departureReason === "retirement" && <div className={styles.stack}><p>Retirement: {c.cutoffEffective ? "Effective" : c.status === "cancelled" ? "Cancelled" : "Not yet effective"} · Last working day: {c.definition.lastWorkingDate}</p><p>Clearance: {data.tasks.some(t=>t.case_id===c.id && t.clearance_type && t.status==="pending") ? "In progress" : ["draft","cancelled"].includes(c.status) ? "Not started" : "Complete"}</p>{!["draft","cancelled"].includes(c.status) && <Link className="button secondary" href={`${base}/people/clearance/${c.id}`}>Track all six departments</Link>}</div>}
          {data.tasks.some(
            (t) => t.case_id === c.id && t.status === "pending",
          ) && (
            <p>
              <Link href={c.definition.departureReason === "retirement" ? `${base}/people/clearance/${c.id}` : `${base}/people/access?view=devices`}>
                {
                  data.tasks.filter(
                    (t) => t.case_id === c.id && t.status === "pending",
                  ).length
                }{" "}
                preparation tasks pending
              </Link>
              {c.status === "completed" &&
              ["offboarding", "suspend"].includes(c.kind)
                ? " · ERPX access revoked; directory completion tracked separately"
                : ""}
            </p>
          )}
          {["offboarding", "suspend"].includes(c.kind) &&
            ["scheduled", "completed"].includes(c.status) && (
              <p>
                {c.cutoffEffective
                  ? "ERPX access revoked"
                  : "ERPX revocation scheduled"}{" "}
                ·{" "}
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
              Waiting for IT preparation, complete HR details, or verified
              account linking.
            </p>
          )}
          {data.canManage && ["draft", "waiting_it"].includes(c.status) && (
            <button
              className="button secondary"
              onClick={() => {
                setDraft({
                  ...c.definition,
                  id: c.id,
                  expectedVersion: c.version,
                });
                setKind(c.kind);
                setDepartureReason(c.definition.departureReason ?? "other");
                setTimezone(c.definition.timezone ?? "Asia/Bangkok");
                setEffectiveLocal(retirementLocalTime(c.effective_at,c.definition.timezone ?? "Asia/Bangkok"));
                setOpen(true);
                setStep(1);
              }}
            >
              Continue draft
            </button>
          )}
          {data.canReview && c.status === "pending_review" && (
            <button
              className="button secondary"
              disabled={busy}
              onClick={() =>
                void send(`cases/${c.id}/request-access`, {
                  expectedVersion: c.version,
                })
              }
            >
              Request individual access review
            </button>
          )}
          {data.canManage && !["completed", "cancelled"].includes(c.status) && (
            <button
              className="button secondary"
              disabled={busy}
              onClick={() =>
                void send(`cases/${c.id}/cancel`, {
                  expectedVersion: c.version,
                })
              }
            >
              {c.status === "draft" ? "Cancel draft" : "Cancel scheduled change"}
            </button>
          )}
          <Link href={`${base}/work?type=people_lifecycle`}>
            Track preparation and reviews
          </Link>
        </div>
      ))}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={`${person.name} · employment change`}
        presentation="responsive-panel"
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
              try {cutoff = retirementInstant(effectiveLocal,timezone); setDateError("");}
              catch (error) {setDateError(error instanceof Error ? error.message : "Choose a valid time.");return;}
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
                effectiveAt: cutoff,
                departureReason: kind === "offboarding" ? departureReason : "other",
                timezone,
                clearanceOwners: {equipment:String(f.get("equipment")??"")||null,payroll:String(f.get("payroll")??"")||null,finance:String(f.get("finance")??"")||null},
                lastWorkingDate: String(f.get("lastWorkingDate") ?? "") || null,
                reason: String(f.get("reason")),
              };
              const result = await send<LifecyclePreview>(
                "placements/preview",
                input,
              );
              if (result) {
                setDraft(input);
                setPreview(result);
                setStep(2);
              }
            }}
          >
            <p>{person.name} · Existing person and engagement</p>
            <label>
              Change
              <select
                value={kind}
                disabled={!!draft?.id}
                onChange={(e) => setKind(e.target.value as typeof kind)}
              >
                <option value="onboarding">Onboarding / new engagement</option>
                <option value="transfer">Transfer position or branch</option>
                <option value="suspend">Temporary suspension</option>
                <option value="offboarding">
                  End employment & revoke Company access
                </option>
              </select>
            </label>
            {["onboarding", "transfer"].includes(kind) && (
              <>
                <label>
                  Position
                  <select
                    name="position"
                    required
                    defaultValue={
                      draft?.positionId ??
                      initialPositionId ??
                      person.positionId ??
                      ""
                    }
                  >
                    <option value="">Choose a position</option>
                    {data.positions
                      .filter((p) => p.active)
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title} ·{" "}
                          {data.nodes.find((n) => n.id === p.branch_id)?.name ??
                            "Company"}{" "}
                          ·{" "}
                          {data.nodes.find((n) => n.id === p.department_id)
                            ?.name ?? "All departments"}{" "}
                          ·{" "}
                          {p.package_id &&
                          data.tasks.some(
                            (t) =>
                              t.position_id === p.id &&
                              t.status === "completed",
                          )
                            ? "IT prepared"
                            : "Waiting for IT"}
                        </option>
                      ))}
                  </select>
                </label>
                <label>
                  Actual reporting manager
                  <select
                    name="manager"
                    defaultValue={
                      draft?.managerPersonId ?? person.managerPersonId ?? ""
                    }
                  >
                    <option value="">Choose a manager</option>
                    {data.managers
                      .filter((m) => m.id !== person.id)
                      .map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.name}
                        </option>
                      ))}
                  </select>
                </label>
              </>
            )}
            {kind === "offboarding" && <label>Departure reason<select value={departureReason} onChange={e=>setDepartureReason(e.target.value as "retirement"|"other")}><option value="other">Other departure</option><option value="retirement">Retirement</option></select></label>}
            {kind === "offboarding" && (
              <label>
                Last working date
                <input
                  type="date"
                  name="lastWorkingDate"
                  required
                  defaultValue={draft?.lastWorkingDate ?? ""}
                  onChange={e=>{if(e.target.value)setEffectiveLocal(nextRetirementDay(e.target.value));}}
                />
              </label>
            )}
            <label>Timezone<select value={timezone} onChange={e=>setTimezone(e.target.value)}>{[...new Set([timezone,"Asia/Bangkok","UTC","Asia/Singapore","Europe/London","America/New_York"])].map(zone=><option key={zone}>{zone}</option>)}</select></label>
            <label>{kind === "offboarding" ? "Retirement / departure and access cutoff" : "Effective date and time"}<input type="datetime-local" name="effective" required value={effectiveLocal} onChange={e=>setEffectiveLocal(e.target.value)}/></label>
            {dateError && <p role="alert">{dateError}</p>}
            {kind === "offboarding" && <p className="muted">Default: midnight after the last working day in {timezone}. Review or change it before confirming. Pending clearance does not delay cutoff.</p>}
            {kind === "offboarding" && departureReason === "retirement" && <fieldset><legend>Clearance owners</legend><p>HR: you · Handover: eligible reporting manager · Directory: configured IT owner</p>{(["equipment","payroll","finance"] as const).map(kind=><label key={kind}>{kind === "equipment" ? "Equipment and assets" : kind === "payroll" ? "Final payroll" : "Finance clearance"}<select name={kind} defaultValue={draft?.clearanceOwners?.[kind]??""}><option value="">Unassigned — keep in department queue</option>{data.clearanceOwners?.filter(owner=>owner[kind]).map(owner=><option key={owner.id} value={owner.id}>{owner.name}</option>)}</select></label>)}</fieldset>}
            <label>
              Reason
              <textarea
                name="reason"
                required
                minLength={3}
                defaultValue={draft?.reason}
              />
            </label>
            <button className="button" disabled={busy}>
              Review placement <ArrowRight size={16} />
            </button>
          </form>
        )}
        {step === 2 && preview && draft && (
          <div className={styles.stack}>
            {preview.definition && (
              <AccessRecipe definition={preview.definition} />
            )}
            <p>{departureReason === "retirement" ? "Retirement" : "Change"} effective: {new Date(draft.effectiveAt).toLocaleString("en-GB",{timeZone:draft.timezone})} {draft.timezone}</p>
            {draft.lastWorkingDate && <p>Engagement ends: {draft.lastWorkingDate} · Access cutoff is tracked separately.</p>}
            <p>Verified linked accounts: {preview.accounts.length}</p>
            <p>Access ending: {preview.removed.join(", ") || "None"}</p>
            {kind === "transfer" && (
              <p>
                Applied access stays in place until IT changes it, subject to its expiry and revocation conditions. IT manages resources in Technology.
              </p>
            )}
            {preview.blockers.map((b) => (
              <p className={styles.pending} key={b}>
                {b}
              </p>
            ))}
            {["offboarding", "suspend"].includes(kind) && (
              <p>
                ERPX access ends across every verified account in this Company
                at the selected time. Directory changes and equipment handover
                are tracked separately.
              </p>
            )}
            <div className={styles.actions}>
              <button className="button secondary" onClick={() => setStep(1)}>
                Back
              </button>
              <button
                className="button secondary"
                disabled={busy}
                onClick={async () => {
                  if (await send("placements/draft", draft)) {
                    setStep(3);
                  }
                }}
              >
                Save draft
              </button>
              <button
                className="button"
                disabled={busy || !preview.ready}
                onClick={async () => {
                  if (
                    await send("placements/submit", {
                      ...draft,
                      previewToken: preview.token,
                    })
                  )
                    setStep(3);
                }}
              >
                {preview.ready
                  ? "Submit for scheduled processing"
                  : "Save and track pending work"}
              </button>
            </div>
          </div>
        )}
        {step === 3 && (
          <div className={styles.stack}>
            <h3>Request saved</h3>
            <p>Track responsible colleagues in Work. Departure cutoff and clearance progress are independent; HR placement proceeds on the effective date. IT provisions resources separately.</p>
            <Link
              className="button"
              href={`${base}/work?type=people_lifecycle`}
            >
              Open Work
            </Link>
          </div>
        )}
        {error && <p role="alert">{error}</p>}
      </Dialog>
      {error && !open && <p role="alert">{error}</p>}
    </section>
  );
}

export function PreparationTasks({
  base,
  data,
}: {
  base: string;
  data: PeopleLifecycleData;
}) {
  const { send, busy, error } = useCommand(base);
  return (
    <section className="card">
      <h2>Preparation tasks</h2>
      {data.tasks
        .filter((t) => t.kind !== "position")
        .map((t) => (
          <div className={styles.case} key={t.id}>
            <strong>{t.title}</strong>
            {t.clearance_type && <Link href={`${base}/people/clearance/${t.case_id}`}>Open retirement clearance</Link>}
            <span>
              {t.status} · {t.assignee_id ?? "Unassigned IT"}
            </span>
            {!t.clearance_type && data.canPrepare && t.status === "pending" && (
              <button
                className="button secondary"
                disabled={busy}
                onClick={() =>
                  void send(`tasks/${t.id}/complete`, {
                    expectedVersion: t.version,
                  })
                }
              >
                Confirm task completed
              </button>
            )}
          </div>
        ))}
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
    [saved, setSaved] = useState(false);
  return (
    <section className="card">
      <SetupChecklist title="People readiness" items={[{id:"it",label:"IT handoff owner",ready:!!data.settings.itAssigneeId,owner:"HR / IT administrator",href:`${base}/people/settings?section=templates`,detail:data.settings.itAssigneeId ? "Default owner selected" : "Choose a current IT owner below"},{id:"positions",label:"Position preparation",ready:!data.tasks.some(t=>t.kind==="position"&&t.status==="pending"),owner:"IT",href:`${base}/people/organization`,detail:"Prepare access packages for active positions"}]} />
      <h2>HR → IT handoff</h2>
      <form
        className={styles.stack}
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          if (
            await send("settings", {
              expectedVersion: data.settings.version,
              itAssigneeId: String(f.get("owner")) || null,
              onboardingTasks: String(f.get("onboarding"))
                .split("\n")
                .filter(Boolean),
              offboardingTasks: String(f.get("offboarding"))
                .split("\n")
                .filter(Boolean),
            })
          )
            setSaved(true);
        }}
      >
        <label>
          IT owner
          <select name="owner" defaultValue={data.settings.itAssigneeId ?? ""}>
            <option value="">Unassigned IT queue</option>
            {owners.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Standard onboarding tasks
          <textarea
            name="onboarding"
            defaultValue={data.settings.onboardingTasks.join("\n")}
          />
        </label>
        <label>
          Standard offboarding tasks
          <textarea
            name="offboarding"
            defaultValue={data.settings.offboardingTasks.join("\n")}
          />
        </label>
        <button className="button" disabled={busy}>
          Save templates & handoff
        </button>
        {saved && <p role="status">Settings saved.</p>}
        {error && <p role="alert">{error}</p>}
      </form>
    </section>
  );
}
