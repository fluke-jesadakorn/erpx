"use client";
import Link from "next/link";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { HierarchyTree } from "@/components/ui/hierarchy-tree";
import { AccessEditor } from "@/components/modules/access-editor";
import { workFetch } from "@/lib/work-fetch";
import type {
  accessDetail,
  accessEntries,
  accessMetadata,
  accessComparison,
} from "@/server/people-access";
import styles from "./access-explorer.module.css";
type Meta = ReturnType<typeof accessMetadata>;
type Detail = ReturnType<typeof accessDetail>;
type Entries = ReturnType<typeof accessEntries>;
type Comparison = ReturnType<typeof accessComparison>;
async function read<T>(url: string, signal?: AbortSignal): Promise<T> {
  const response = await workFetch(url, { signal, cache: "no-store" }),
    result = await response.json();
  if (!response.ok)
    throw Error(result.error?.message ?? "Unable to load access information.");
  return result.data;
}
function Areas({
  catalog,
  decisions,
  preview,
}: {
  catalog: Detail["catalog"];
  decisions: Detail["decisions"][number]["decisions"];
  preview: boolean;
}) {
  const visible = catalog.filter((p) => preview || p.availability === "live"),
    areas = [...new Set(visible.map((p) => p.module))];
  return (
    <>
      {areas.map((area) => {
        const permissions = visible.filter((p) => p.module === area),
          rows = decisions.filter((d) =>
            permissions.some((p) => p.id === d.permission),
          );
        return (
          <details className={styles.area} key={area}>
            <summary>
              {area.replaceAll("_", " ")}{" "}
              <small>
                {rows.filter((r) => r.result === "allowed").length} allowed ·{" "}
                {rows.filter((r) => r.result === "conditional").length}{" "}
                conditional · {permissions.length} actions
              </small>
            </summary>
            {permissions.map((p) => {
              const decision = rows.find((d) => d.permission === p.id);
              return (
                <details key={p.id}>
                  <summary>
                    {p.label} · {decision?.result ?? "unavailable"}
                  </summary>
                  <p>{decision?.reasons.join(" · ")}</p>
                  <p>{decision?.conditions.join(" · ")}</p>
                  <p>
                    {decision?.source} · {decision?.scope} · revision{" "}
                    {decision?.revision}
                  </p>
                  <p>
                    {decision?.expiresAt
                      ? `Expires ${new Date(decision.expiresAt).toLocaleDateString()}`
                      : "No expiry evidence"}
                  </p>
                </details>
              );
            })}
          </details>
        );
      })}
    </>
  );
}
export function PeopleAccessExplorer({
  meta,
  initialEntries,
  base,
  api,
  initialObjectId,
}: {
  meta: Meta;
  initialEntries: Entries;
  base: string;
  api: string;
  initialObjectId?: string;
}) {
  const router = useRouter(),
    [node, setNode] = useState(meta.root),
    [expanded, setExpanded] = useState([meta.root]),
    [entries, setEntries] = useState(initialEntries),
    [q, setQ] = useState(""),
    [detail, setDetail] = useState<Detail | null>(null),
    [assignment, setAssignment] = useState(""),
    [screen, setScreen] = useState("tree"),
    [message, setMessage] = useState(""),
    [pending, setPending] = useState(false),
    [preview, setPreview] = useState(false),
    [tray, setTray] = useState<{ detail: Detail; assignment: string }[]>([]),
    [comparisonResult, setComparisonResult] = useState<{
      key: string;
      data: Comparison;
    } | null>(null),
    [refresh, setRefresh] = useState(0);
  const resultsHeading = useRef<HTMLHeadingElement>(null),
    detailHeading = useRef<HTMLHeadingElement>(null),
    treePane = useRef<HTMLElement>(null),
    detailRequest = useRef(0),
    attempt = useRef<{ payload: string; key: string } | null>(null);
  const loadedQuery = useRef(JSON.stringify([api, meta.root, "", 0]));
  useEffect(() => {
    if (!initialObjectId) return;
    const controller = new AbortController();
    void read<Detail>(
      `${api}/objects/${encodeURIComponent(initialObjectId)}`,
      controller.signal,
    )
      .then((result) => {
        setDetail(result);
        setScreen("detail");
        requestAnimationFrame(() => detailHeading.current?.focus());
      })
      .catch((error) => {
        if (!controller.signal.aborted) setMessage(error.message);
      });
    return () => controller.abort();
  }, [api, initialObjectId]);
  useEffect(() => {
    const query = JSON.stringify([api, node, q, refresh]);
    if (loadedQuery.current === query) return;
    const c = new AbortController();
    const timer = setTimeout(() => {
      void read<Entries>(
        `${api}/entries?${new URLSearchParams({ node, q })}`,
        c.signal,
      )
        .then((result) => {
          loadedQuery.current = query;
          setEntries(result);
        })
        .catch((e) => {
          if (!c.signal.aborted) setMessage(e.message);
        });
    }, 180);
    return () => {
      c.abort();
      clearTimeout(timer);
    };
  }, [api, node, q, refresh]);
  const compareIds = tray
    .map((t) => t.assignment)
    .filter(Boolean)
    .join("|");
  const comparisonKey = JSON.stringify([api, compareIds, refresh]);
  const comparison =
    comparisonResult?.key === comparisonKey ? comparisonResult.data : null;
  useEffect(() => {
    const c = new AbortController();
    if (compareIds) {
      const params = new URLSearchParams();
      compareIds.split("|").forEach((id) => params.append("assignment", id));
      void read<Comparison>(`${api}/compare?${params}`, c.signal)
        .then((data) => setComparisonResult({ key: comparisonKey, data }))
        .catch((e) => {
          if (!c.signal.aborted) setMessage(e.message);
        });
    }
    return () => c.abort();
  }, [api, compareIds, comparisonKey]);
  const selectedNode = meta.tree.find((n) => n.id === node);
  async function open(id: string) {
    const request = ++detailRequest.current;
    setMessage("");
    try {
      const result = await read<Detail>(
        `${api}/objects/${encodeURIComponent(id)}`,
      );
      if (request !== detailRequest.current) return;
      setDetail(result);
      setAssignment("");
      setScreen("detail");
      requestAnimationFrame(() => detailHeading.current?.focus());
    } catch (e) {
      setMessage(
        e instanceof Error ? e.message : "Unable to load this object.",
      );
    }
  }
  async function send(path: string, input: unknown) {
    const payload = JSON.stringify({ path, input });
    if (attempt.current?.payload !== payload)
      attempt.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setMessage("");
    try {
      const r = await workFetch(`/api/v1${base}/iam/${path}`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": attempt.current.key,
          },
          body: JSON.stringify(input),
        }),
        result = await r.json();
      if (!r.ok)
        throw Error(result.error?.message ?? "Unable to update ERPX access.");
      attempt.current = null;
      setMessage(result.data?.message ?? "ERPX access updated.");
      setRefresh((v) => v + 1);
      router.refresh();
      if (detail) {
        const updated = await read<Detail>(
          `${api}/objects/${encodeURIComponent(detail.entry.id)}`,
        );
        setDetail(updated);
      }
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to update access.");
    } finally {
      setPending(false);
    }
  }
  const selectedAssignment = detail?.assignments.find(
      (a) => a.id === assignment,
    ),
    decision = detail?.decisions.find((d) => d.assignmentId === assignment);
  const canManage =
    meta.canManage && detail?.entry.userId !== meta.currentUserId;
  function back(target: "tree" | "results") {
    setScreen(target);
    requestAnimationFrame(() =>
      target === "tree"
        ? treePane.current
            ?.querySelector<HTMLElement>('[aria-selected="true"]')
            ?.focus()
        : resultsHeading.current?.focus(),
    );
  }
  return (
    <div className={styles.explorer}>
      <div className={styles.toolbar}>
        <p>
          {meta.fullDirectory
            ? "Organization and connected directories · external objects are read-only"
            : "Your ERPX subtree and linked directory identities"}
        </p>
        <Link href={`${base}/iam/matrix`}>Advanced IAM console ↗</Link>
      </div>
      <nav
        className={`${styles.crumbs} ${styles.mobile}`}
        aria-label="Explorer steps"
      >
        <button onClick={() => back("tree")}>Tree</button>
        <span>›</span>
        <button onClick={() => back("results")}>
          {selectedNode?.label ?? "Results"}
        </button>
        {screen === "detail" && (
          <>
            <span>›</span>
            <span>Detail</span>
          </>
        )}
      </nav>
      {message && (
        <p className={styles.message} role="status">
          {message}
        </p>
      )}
      <div className={styles.panes} data-screen={screen}>
        <section
          ref={treePane}
          className={`${styles.pane} ${styles.tree}`}
          aria-label="Directory tree"
        >
          <h2>Browse your organization</h2>
          <p>Choose a team, container, or group.</p>
          <HierarchyTree
            nodes={meta.tree}
            selectedId={node}
            expandedIds={expanded}
            onExpand={setExpanded}
            onSelect={(id) => {
              setNode(id);
              setQ("");
              setScreen("results");
              requestAnimationFrame(() => resultsHeading.current?.focus());
            }}
          />
          {!meta.tree.some((n) => n.kind === "connection") && (
            <p>
              {meta.fullDirectory
                ? "No connected directories yet. Successful synchronization will add their hierarchy here."
                : "Full directory browsing requires a company context and directory-management permission."}
            </p>
          )}
        </section>
        <section
          className={`${styles.pane} ${styles.results}`}
          aria-label="Directory results"
        >
          <h2 ref={resultsHeading} tabIndex={-1}>
            {selectedNode?.label}
          </h2>
          <label>
            Search people and groups
            <input
              className={styles.search}
              type="search"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Name or email"
            />
          </label>
          <p aria-live="polite">{entries.total} results</p>
          {entries.items.map((e) => (
            <button
              className={styles.entry}
              key={e.id}
              aria-pressed={detail?.entry.id === e.id}
              onClick={() => void open(e.id)}
            >
              <strong>{e.label}</strong>
              <small>{e.email ?? e.kind}</small>
              <div className={styles.badges}>
                {[e.source, e.account, e.status, e.sync, e.accessStatus]
                  .filter(Boolean)
                  .map((b, i) => (
                    <span className={styles.badge} key={i}>
                      {b}
                    </span>
                  ))}
              </div>
            </button>
          ))}
          {!entries.items.length && <p>No people or groups match this view.</p>}
          {entries.nextCursor && (
            <button
              className="button secondary"
              onClick={async () => {
                try {
                  const next = await read<Entries>(
                    `${api}/entries?${new URLSearchParams({ node, q, cursor: entries.nextCursor! })}`,
                  );
                  setEntries((v) => ({
                    ...next,
                    items: [...v.items, ...next.items],
                  }));
                } catch (e) {
                  setMessage(
                    e instanceof Error ? e.message : "Unable to load more.",
                  );
                }
              }}
            >
              Load more
            </button>
          )}
        </section>
        <section
          className={`${styles.pane} ${styles.detail}`}
          aria-label="Access detail"
        >
          {!detail ? (
            <div className={styles.empty}>
              <h2>Every person. One work context.</h2>
              <p>
                Select a person or group to understand their ERPX access,
                directory evidence, and assignments.
              </p>
            </div>
          ) : (
            <>
              <h2 ref={detailHeading} tabIndex={-1}>
                {detail.entry.label}
              </h2>
              <p>{detail.entry.email}</p>
              <div className={styles.badges}>
                {[
                  detail.entry.account,
                  detail.entry.source,
                  detail.entry.status,
                  detail.entry.sync,
                  detail.entry.accessStatus,
                ]
                  .filter(Boolean)
                  .map((b, i) => (
                    <span key={i} className={styles.badge}>
                      {b}
                    </span>
                  ))}
              </div>
              {detail.entry.kind !== "group" && (
                <>
                  <h3>Work context</h3>
                  <label>
                    Assignment to inspect
                    <select
                      className={styles.select}
                      value={assignment}
                      onChange={(e) => setAssignment(e.target.value)}
                    >
                      <option value="">Choose one assignment</option>
                      {detail.assignments.map((a) => (
                        <option key={String(a.id)} value={String(a.id)}>
                          {meta.roles.find((r) => r.id === a.role)?.name ??
                            String(a.role)}{" "}
                          · {String(a.scope_kind)} ·{" "}
                          {meta.tree.find((n) => n.id === a.scope_node_id)
                            ?.label ?? "Company"}{" "}
                          · {a.active ? "active" : "inactive"}
                        </option>
                      ))}
                    </select>
                  </label>
                  {!detail.assignments.length && (
                    <p>No ERPX assignments in this work context.</p>
                  )}
                  {decision ? (
                    <>
                      <p>
                        Only this assignment is evaluated. Permissions from
                        other roles are not combined.
                      </p>
                      <label>
                        <input
                          type="checkbox"
                          checked={preview}
                          onChange={(e) => setPreview(e.target.checked)}
                        />{" "}
                        Include unavailable capabilities
                      </label>
                      <Areas
                        catalog={detail.catalog}
                        decisions={decision.decisions}
                        preview={preview}
                      />
                    </>
                  ) : (
                    <p>Select an assignment to see access by business area.</p>
                  )}
                  {detail.entry.kind === "person" && (
                    <button
                      className="button secondary"
                      disabled={
                        tray.length >= 4 ||
                        tray.some(
                          (t) => t.detail.entry.id === detail.entry.id,
                        ) ||
                        !detail.assignments.length
                      }
                      onClick={() =>
                        setTray((v) => [...v, { detail, assignment }])
                      }
                    >
                      Compare person ({tray.length}/4)
                    </button>
                  )}
                </>
              )}
              {!!detail.linked.length && (
                <>
                  <h3>Linked directory evidence</h3>
                  {detail.linked.map((i) => (
                    <p key={i.id}>
                      {i.label} · {i.source} · {i.status} · {i.sync}
                    </p>
                  ))}
                </>
              )}
              {!!detail.memberships.length && (
                <details className={styles.area}>
                  <summary>
                    Effective group memberships ({detail.memberships.length})
                  </summary>
                  {detail.memberships.map((m, i) => (
                    <p key={i}>{m.label}</p>
                  ))}
                </details>
              )}
              {!!detail.groups.length && (
                <details className={styles.area}>
                  <summary>
                    Direct nested memberships ({detail.groups.length})
                  </summary>
                  {detail.groups.map((g, i) => (
                    <p key={i}>
                      {g.parentId} → {g.childId}
                    </p>
                  ))}
                </details>
              )}
              {detail.entry.kind === "group" && (
                <>
                  <h3>Scoped ERPX role mappings</h3>
                  {!detail.mappings.length && (
                    <p>
                      No ERPX mappings. Group membership alone grants no ERPX
                      role.
                    </p>
                  )}
                  {detail.mappings.map((m) => (
                    <div className={styles.area} key={String(m.id)}>
                      <strong>{String(m.role_id)}</strong>
                      <p>
                        {String(m.scope_kind)} ·{" "}
                        {String(m.scope_node_id ?? "Company")} ·{" "}
                        {String(m.status)}
                      </p>
                      {canManage && m.status === "active" && (
                        <CommandReason
                          label="Revoke mapping"
                          pending={pending}
                          onSubmit={(reason) =>
                            void send(`mappings/${m.id}/revoke`, {
                              expectedVersion: Number(m.version),
                              reason,
                            })
                          }
                        />
                      )}
                    </div>
                  ))}
                </>
              )}
              {canManage && detail.entry.kind !== "identity" && (
                <details className={styles.area}>
                  <summary>Manage ERPX access</summary>
                  <p>
                    Changes use the existing MFA, delegation, revision, and
                    independent-review checks. External directory objects cannot
                    be edited here.
                  </p>
                  {!detail.entry.userId && detail.entry.personId ? (
                    <AccessEditor
                      base={base}
                      personId={detail.entry.personId}
                      hasAccount={false}
                      assignments={[]}
                      canManage
                    />
                  ) : (
                    <GrantForm
                      meta={meta}
                      pending={pending}
                      group={detail.entry.kind === "group"}
                      onSubmit={(fields) =>
                        void send(
                          detail.entry.kind === "group"
                            ? "mappings"
                            : "assignments",
                          {
                            ...fields,
                            ...(detail.entry.kind === "group"
                              ? {
                                  connectionId: detail.entry.connectionId,
                                  groupId: detail.entry.externalId,
                                }
                              : { userId: detail.entry.userId }),
                          },
                        )
                      }
                    />
                  )}
                  {selectedAssignment && !selectedAssignment.revoked_at && (
                    <CommandReason
                      label="Revoke selected assignment"
                      pending={pending}
                      onSubmit={(reason) =>
                        void send(
                          `assignments/${selectedAssignment.id}/revoke`,
                          {
                            expectedVersion: Number(selectedAssignment.version),
                            reason,
                          },
                        )
                      }
                    />
                  )}
                  {detail.entry.userId && meta.scopeKind === "holding" && (
                    <CommandReason
                      label="Suspend ERPX account"
                      pending={pending}
                      onSubmit={(reason) =>
                        void send(`users/${detail.entry.userId}/suspend`, {
                          expectedVersion: Number(
                            detail.assignments[0]?.identity_version ?? 1,
                          ),
                          reason,
                        })
                      }
                    />
                  )}
                </details>
              )}
            </>
          )}
        </section>
      </div>
      {!!tray.length && (
        <section className={styles.comparison} aria-label="People comparison">
          <h2>Compare work contexts</h2>
          <p>
            Up to four people. One role and scope per person; never combined.
          </p>
          <div
            className={styles.compareScroll}
            tabIndex={0}
            role="region"
            aria-label="Scrollable comparison"
          >
            <div className={styles.compareGrid}>
              {tray.map((t) => (
                <article key={t.detail.entry.id} className={styles.compareCard}>
                  <strong>{t.detail.entry.label}</strong>
                  <button
                    aria-label={`Remove ${t.detail.entry.label} from comparison`}
                    onClick={() =>
                      setTray((v) =>
                        v.filter(
                          (p) => p.detail.entry.id !== t.detail.entry.id,
                        ),
                      )
                    }
                  >
                    {" "}
                    ×
                  </button>
                  <label>
                    Work assignment
                    <select
                      className={styles.select}
                      value={t.assignment}
                      onChange={(e) =>
                        setTray((v) =>
                          v.map((p) =>
                            p.detail.entry.id === t.detail.entry.id
                              ? { ...p, assignment: e.target.value }
                              : p,
                          ),
                        )
                      }
                    >
                      <option value="">Choose one assignment</option>
                      {t.detail.assignments.map((a) => (
                        <option key={String(a.id)} value={String(a.id)}>
                          {String(a.role)} · {String(a.scope_kind)} ·{" "}
                          {meta.tree.find((n) => n.id === a.scope_node_id)
                            ?.label ?? "Company"}
                        </option>
                      ))}
                    </select>
                  </label>
                  {t.assignment &&
                  comparison?.decisions.find(
                    (d) => d.assignmentId === t.assignment,
                  ) ? (
                    <Areas
                      catalog={comparison.catalog}
                      decisions={
                        comparison.decisions.find(
                          (d) => d.assignmentId === t.assignment,
                        )!.decisions
                      }
                      preview={preview}
                    />
                  ) : (
                    <p>Choose one assignment to compare.</p>
                  )}
                </article>
              ))}
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
function CommandReason({
  label,
  pending,
  onSubmit,
}: {
  label: string;
  pending: boolean;
  onSubmit: (reason: string) => void;
}) {
  return (
    <form
      className={styles.form}
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(String(new FormData(e.currentTarget).get("reason")));
      }}
    >
      <label>
        {label} — reason
        <input name="reason" required minLength={3} maxLength={1000} />
      </label>
      <button className="button secondary" disabled={pending}>
        {label}
      </button>
    </form>
  );
}
function GrantForm({
  meta,
  pending,
  group,
  onSubmit,
}: {
  meta: Meta;
  pending: boolean;
  group: boolean;
  onSubmit: (input: Record<string, unknown>) => void;
}) {
  return (
    <form
      className={styles.form}
      onSubmit={(e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          role = meta.roles.find((r) => r.id === f.get("role")),
          scope = String(f.get("scope")),
          n = meta.tree.find((n) => n.id === scope);
        onSubmit({
          roleId: role?.id,
          expectedRoleRevision: role?.revision,
          scopeKind: scope === "holding" ? "holding" : n?.kind,
          scopeNodeId: scope === "holding" ? null : scope,
          validUntil: new Date(`${f.get("expires")}T23:59:59`).getTime(),
          reason: f.get("reason"),
        });
      }}
    >
      <h3>{group ? "Propose role mapping" : "Grant / renew a role"}</h3>
      <label>
        Role
        <select name="role" required>
          <option value="">Choose a role</option>
          {meta.roles.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} · revision {r.revision}
            </option>
          ))}
        </select>
      </label>
      <label>
        Scope
        <select name="scope" required>
          {meta.scopeKind === "holding" && (
            <option value="holding">Company</option>
          )}
          {meta.tree
            .filter(
              (n) =>
                n.source === "ERPX" &&
                ["branch", "department"].includes(n.kind),
            )
            .map((n) => (
              <option key={n.id} value={n.id}>
                {n.label} · {n.kind}
              </option>
            ))}
        </select>
      </label>
      <label>
        Expires
        <input type="date" name="expires" required />
      </label>
      <label>
        Reason
        <input name="reason" required minLength={3} maxLength={1000} />
      </label>
      <button className="button" disabled={pending || !meta.roles.length}>
        {group ? "Propose mapping" : "Submit grant / renewal"}
      </button>
    </form>
  );
}
