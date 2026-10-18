"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import Link from "@/components/workplace/workspace-link";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { HierarchyTree } from "@/components/ui/hierarchy-tree";
import { AccessEditor } from "@/components/identity_access/access-editor";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type {
  accessDetail,
  accessEntries,
  accessMetadata,
  accessComparison,
} from "@/server/modules/people_payroll/public/people-access";
import styles from "@/components/people_payroll/access-explorer.module.css";
import { type UiAppearance } from "@/components/ui/foundation";
type Meta = ReturnType<typeof accessMetadata>;
type Detail = ReturnType<typeof accessDetail>;
type Entries = ReturnType<typeof accessEntries>;
type Comparison = ReturnType<typeof accessComparison>;
async function read<T>(url: string, signal?: AbortSignal): Promise<T> {
  const response = await workFetch(url, { signal, cache: "no-store" }),
    result = await response.json();
  if (!response.ok) throw Error(result.error?.message ?? "Unable to load access information.");
  return result.data;
}
function Areas({ appearance = "legacy",
  catalog,
  decisions,
  preview,
}: ({
  catalog: Detail["catalog"];
  decisions: Detail["decisions"][number]["decisions"];
  preview: boolean;
}) & { appearance?: UiAppearance }) {
  const visible = catalog.filter((p) => preview || p.availability === "live"),
    areas = [...new Set(visible.map((p) => p.module))];
  return (
    <>
      {areas.map((area) => {
        const permissions = visible.filter((p) => p.module === area),
          rows = decisions.filter((d) => permissions.some((p) => p.id === d.permission));
        return (
          <details data-ui-appearance={appearance} className={styles.area} key={area}>
            <summary data-ui-appearance={appearance}>
              {area.replaceAll("_", " ")}{" "}
              <small data-ui-appearance={appearance}>
                {rows.filter((r) => r.result === "allowed").length} allowed ·{" "}
                {rows.filter((r) => r.result === "conditional").length} conditional ·{" "}
                {permissions.length} actions
              </small>
            </summary>
            {permissions.map((p) => {
              const decision = rows.find((d) => d.permission === p.id);
              return (
                <details data-ui-appearance={appearance} key={p.id}>
                  <summary data-ui-appearance={appearance}>
                    {p.label} · {decision?.result ?? "unavailable"}
                  </summary>
                  <p data-ui-appearance={appearance}>{decision?.reasons.join(" · ")}</p>
                  <p data-ui-appearance={appearance}>{decision?.conditions.join(" · ")}</p>
                  <p data-ui-appearance={appearance}>
                    {decision?.source} · {decision?.scope} · revision {decision?.revision}
                  </p>
                  <p data-ui-appearance={appearance}>
                    {decision?.expiresAt ? (
                      <>
                        Expires <TimeText appearance={appearance} value={decision.expiresAt} />
                      </>
                    ) : (
                      "No expiry evidence"
                    )}
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
export function PeopleAccessExplorer({ appearance = "legacy",
  meta,
  initialEntries,
  base,
  api,
  initialObjectId,
}: ({
  meta: Meta;
  initialEntries: Entries;
  base: string;
  api: string;
  initialObjectId?: string;
}) & { appearance?: UiAppearance }) {
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
    void read<Detail>(`${api}/objects/${encodeURIComponent(initialObjectId)}`, controller.signal)
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
      void read<Entries>(`${api}/entries?${new URLSearchParams({ node, q })}`, c.signal)
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
  const comparison = comparisonResult?.key === comparisonKey ? comparisonResult.data : null;
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
      const result = await read<Detail>(`${api}/objects/${encodeURIComponent(id)}`);
      if (request !== detailRequest.current) return;
      setDetail(result);
      setAssignment("");
      setScreen("detail");
      requestAnimationFrame(() => detailHeading.current?.focus());
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to load this object.");
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
      if (!r.ok) throw Error(result.error?.message ?? "Unable to update ERPX access.");
      attempt.current = null;
      setMessage(result.data?.message ?? "ERPX access updated.");
      setRefresh((v) => v + 1);
      router.refresh();
      if (detail) {
        const updated = await read<Detail>(`${api}/objects/${encodeURIComponent(detail.entry.id)}`);
        setDetail(updated);
      }
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to update access.");
    } finally {
      setPending(false);
    }
  }
  const selectedAssignment = detail?.assignments.find((a) => a.id === assignment),
    decision = detail?.decisions.find((d) => d.assignmentId === assignment);
  const canManage = meta.canManage && detail?.entry.userId !== meta.currentUserId;
  function back(target: "tree" | "results") {
    setScreen(target);
    requestAnimationFrame(() =>
      target === "tree"
        ? treePane.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus()
        : resultsHeading.current?.focus(),
    );
  }
  return (
    <div data-ui-appearance={appearance} className={styles.explorer}>
      <div data-ui-appearance={appearance} className={styles.toolbar}>
        <p data-ui-appearance={appearance}>
          {meta.fullDirectory
            ? "Organization and connected directories · external objects are read-only"
            : "Your ERPX subtree and linked directory identities"}
        </p>
        <Link href={canonicalUiHref(`${base}/iam/matrix`)}>Advanced IAM console ↗</Link>
      </div>
      <nav data-ui-appearance={appearance} className={`${styles.crumbs} ${styles.mobile}`} aria-label="Explorer steps">
        <Button appearance={appearance} type="submit" variant="plain" onClick={() => back("tree")}>
          Tree
        </Button>
        <span data-ui-appearance={appearance}>›</span>
        <Button appearance={appearance} type="submit" variant="plain" onClick={() => back("results")}>
          {selectedNode?.label ?? "Results"}
        </Button>
        {screen === "detail" && (
          <>
            <span data-ui-appearance={appearance}>›</span>
            <span data-ui-appearance={appearance}>Detail</span>
          </>
        )}
      </nav>
      {message && (
        <p data-ui-appearance={appearance} className={styles.message} role="status">
          {message}
        </p>
      )}
      <div data-ui-appearance={appearance} className={styles.panes} data-screen={screen}>
        <section data-ui-appearance={appearance}
          ref={treePane}
          className={`${styles.pane} ${styles.tree}`}
          aria-label="Directory tree"
        >
          <h2 data-ui-appearance={appearance}>Browse your organization</h2>
          <p data-ui-appearance={appearance}>Choose a team, container, or group.</p>
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
            <p data-ui-appearance={appearance}>
              {meta.fullDirectory
                ? "No connected directories yet. Successful synchronization will add their hierarchy here."
                : "Full directory browsing requires a company context and directory-management permission."}
            </p>
          )}
        </section>
        <section data-ui-appearance={appearance} className={`${styles.pane} ${styles.results}`} aria-label="Directory results">
          <h2 data-ui-appearance={appearance} ref={resultsHeading} tabIndex={-1}>
            {selectedNode?.label}
          </h2>
          <Field appearance={appearance}>
            Search people and groups
            <TextInput appearance={appearance}
              className={styles.search}
              type="search"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Name or email"
            />
          </Field>
          <p data-ui-appearance={appearance} aria-live="polite">{entries.total} results</p>
          {entries.items.map((e) => (
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              className={styles.entry}
              key={e.id}
              aria-pressed={detail?.entry.id === e.id}
              onClick={() => void open(e.id)}
            >
              <strong data-ui-appearance={appearance}>{e.label}</strong>
              <small data-ui-appearance={appearance}>{e.email ?? e.kind}</small>
              <div data-ui-appearance={appearance} className={styles.badges}>
                {[e.source, e.account, e.status, e.sync, e.accessStatus]
                  .filter(Boolean)
                  .map((b, i) => (
                    <span data-ui-appearance={appearance} className={styles.badge} key={i}>
                      {b}
                    </span>
                  ))}
              </div>
            </Button>
          ))}
          {!entries.items.length && <p data-ui-appearance={appearance}>No people or groups match this view.</p>}
          {entries.nextCursor && (
            <Button appearance={appearance}
              type="submit"
              variant="secondary"

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
                  setMessage(e instanceof Error ? e.message : "Unable to load more.");
                }
              }}
            >
              Load more
            </Button>
          )}
        </section>
        <section data-ui-appearance={appearance} className={`${styles.pane} ${styles.detail}`} aria-label="Access detail">
          {!detail ? (
            <div data-ui-appearance={appearance} className={styles.empty}>
              <h2 data-ui-appearance={appearance}>Every person. One work context.</h2>
              <p data-ui-appearance={appearance}>
                Select a person or group to understand their ERPX access, directory evidence, and
                assignments.
              </p>
            </div>
          ) : (
            <>
              <h2 data-ui-appearance={appearance} ref={detailHeading} tabIndex={-1}>
                {detail.entry.label}
              </h2>
              <p data-ui-appearance={appearance}>{detail.entry.email}</p>
              <div data-ui-appearance={appearance} className={styles.badges}>
                {[
                  detail.entry.account,
                  detail.entry.source,
                  detail.entry.status,
                  detail.entry.sync,
                  detail.entry.accessStatus,
                ]
                  .filter(Boolean)
                  .map((b, i) => (
                    <span data-ui-appearance={appearance} key={i} className={styles.badge}>
                      {b}
                    </span>
                  ))}
              </div>
              {detail.entry.kind !== "group" && (
                <>
                  <h3 data-ui-appearance={appearance}>Work context</h3>
                  <Field appearance={appearance}>
                    Assignment to inspect
                    <Select appearance={appearance}
                      className={styles.select}
                      value={assignment}
                      onChange={(e) => setAssignment(e.target.value)}
                    >
                      <option data-ui-appearance={appearance} value="">Choose one assignment</option>
                      {detail.assignments.map((a) => (
                        <option data-ui-appearance={appearance} key={String(a.id)} value={String(a.id)}>
                          {meta.roles.find((r) => r.id === a.role)?.name ?? String(a.role)} ·{" "}
                          {String(a.scope_kind)} ·{" "}
                          {meta.tree.find((n) => n.id === a.scope_node_id)?.label ?? "Company"} ·{" "}
                          {a.active ? "active" : "inactive"}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  {!detail.assignments.length && <p data-ui-appearance={appearance}>No ERPX assignments in this work context.</p>}
                  {decision ? (
                    <>
                      <p data-ui-appearance={appearance}>
                        Only this assignment is evaluated. Permissions from other roles are not
                        combined.
                      </p>
                      <Field appearance={appearance} orientation="inline">
                        <TextInput appearance={appearance}
                          type="checkbox"
                          checked={preview}
                          onChange={(e) => setPreview(e.target.checked)}
                        />{" "}
                        Include unavailable capabilities
                      </Field>
                      <Areas appearance={appearance}
                        catalog={detail.catalog}
                        decisions={decision.decisions}
                        preview={preview}
                      />
                    </>
                  ) : (
                    <p data-ui-appearance={appearance}>Select an assignment to see access by business area.</p>
                  )}
                  {detail.entry.kind === "person" && (
                    <Button appearance={appearance}
                      type="submit"
                      variant="secondary"

                      disabled={
                        tray.length >= 4 ||
                        tray.some((t) => t.detail.entry.id === detail.entry.id) ||
                        !detail.assignments.length
                      }
                      onClick={() => setTray((v) => [...v, { detail, assignment }])}
                    >
                      Compare person ({tray.length}/4)
                    </Button>
                  )}
                </>
              )}
              {!!detail.linked.length && (
                <>
                  <h3 data-ui-appearance={appearance}>Linked directory evidence</h3>
                  {detail.linked.map((i) => (
                    <p data-ui-appearance={appearance} key={i.id}>
                      {i.label} · {i.source} · {i.status} · {i.sync}
                    </p>
                  ))}
                </>
              )}
              {!!detail.memberships.length && (
                <details data-ui-appearance={appearance} className={styles.area}>
                  <summary data-ui-appearance={appearance}>Effective group memberships ({detail.memberships.length})</summary>
                  {detail.memberships.map((m, i) => (
                    <p data-ui-appearance={appearance} key={i}>{m.label}</p>
                  ))}
                </details>
              )}
              {!!detail.groups.length && (
                <details data-ui-appearance={appearance} className={styles.area}>
                  <summary data-ui-appearance={appearance}>Direct nested memberships ({detail.groups.length})</summary>
                  {detail.groups.map((g, i) => (
                    <p data-ui-appearance={appearance} key={i}>
                      {g.parentId} → {g.childId}
                    </p>
                  ))}
                </details>
              )}
              {detail.entry.kind === "group" && (
                <>
                  <h3 data-ui-appearance={appearance}>Scoped ERPX role mappings</h3>
                  {!detail.mappings.length && (
                    <p data-ui-appearance={appearance}>No ERPX mappings. Group membership alone grants no ERPX role.</p>
                  )}
                  {detail.mappings.map((m) => (
                    <div data-ui-appearance={appearance} className={styles.area} key={String(m.id)}>
                      <strong data-ui-appearance={appearance}>{String(m.role_id)}</strong>
                      <p data-ui-appearance={appearance}>
                        {String(m.scope_kind)} · {String(m.scope_node_id ?? "Company")} ·{" "}
                        {String(m.status)}
                      </p>
                      {canManage && m.status === "active" && (
                        <CommandReason appearance={appearance}
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
                <details data-ui-appearance={appearance} className={styles.area}>
                  <summary data-ui-appearance={appearance}>Manage ERPX access</summary>
                  <p data-ui-appearance={appearance}>
                    Changes use the existing MFA, delegation, revision, and independent-review
                    checks. External directory objects cannot be edited here.
                  </p>
                  {!detail.entry.userId && detail.entry.personId ? (
                    <AccessEditor appearance={appearance}
                      base={base}
                      personId={detail.entry.personId}
                      hasAccount={false}
                      assignments={[]}
                      canManage
                    />
                  ) : (
                    <GrantForm appearance={appearance}
                      meta={meta}
                      pending={pending}
                      group={detail.entry.kind === "group"}
                      onSubmit={(fields) =>
                        void send(detail.entry.kind === "group" ? "mappings" : "assignments", {
                          ...fields,
                          ...(detail.entry.kind === "group"
                            ? {
                                connectionId: detail.entry.connectionId,
                                groupId: detail.entry.externalId,
                              }
                            : { userId: detail.entry.userId }),
                        })
                      }
                    />
                  )}
                  {selectedAssignment && !selectedAssignment.revoked_at && (
                    <CommandReason appearance={appearance}
                      label="Revoke selected assignment"
                      pending={pending}
                      onSubmit={(reason) =>
                        void send(`assignments/${selectedAssignment.id}/revoke`, {
                          expectedVersion: Number(selectedAssignment.version),
                          reason,
                        })
                      }
                    />
                  )}
                  {detail.entry.userId && meta.scopeKind === "holding" && (
                    <CommandReason appearance={appearance}
                      label="Suspend ERPX account"
                      pending={pending}
                      onSubmit={(reason) =>
                        void send(`users/${detail.entry.userId}/suspend`, {
                          expectedVersion: Number(detail.assignments[0]?.identity_version ?? 1),
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
        <section data-ui-appearance={appearance} className={styles.comparison} aria-label="People comparison">
          <h2 data-ui-appearance={appearance}>Compare work contexts</h2>
          <p data-ui-appearance={appearance}>Up to four people. One role and scope per person; never combined.</p>
          <div data-ui-appearance={appearance}
            className={styles.compareScroll}
            tabIndex={0}
            role="region"
            aria-label="Scrollable comparison"
          >
            <div data-ui-appearance={appearance} className={styles.compareGrid}>
              {tray.map((t) => (
                <article data-ui-appearance={appearance} key={t.detail.entry.id} className={styles.compareCard}>
                  <strong data-ui-appearance={appearance}>{t.detail.entry.label}</strong>
                  <Button appearance={appearance}
                    type="submit"
                    variant="plain"
                    aria-label={`Remove ${t.detail.entry.label} from comparison`}
                    onClick={() =>
                      setTray((v) => v.filter((p) => p.detail.entry.id !== t.detail.entry.id))
                    }
                  >
                    {" "}
                    ×
                  </Button>
                  <Field appearance={appearance}>
                    Work assignment
                    <Select appearance={appearance}
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
                      <option data-ui-appearance={appearance} value="">Choose one assignment</option>
                      {t.detail.assignments.map((a) => (
                        <option data-ui-appearance={appearance} key={String(a.id)} value={String(a.id)}>
                          {String(a.role)} · {String(a.scope_kind)} ·{" "}
                          {meta.tree.find((n) => n.id === a.scope_node_id)?.label ?? "Company"}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  {t.assignment &&
                  comparison?.decisions.find((d) => d.assignmentId === t.assignment) ? (
                    <Areas appearance={appearance}
                      catalog={comparison.catalog}
                      decisions={
                        comparison.decisions.find((d) => d.assignmentId === t.assignment)!.decisions
                      }
                      preview={preview}
                    />
                  ) : (
                    <p data-ui-appearance={appearance}>Choose one assignment to compare.</p>
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
function CommandReason({ appearance = "legacy",
  label,
  pending,
  onSubmit,
}: ({
  label: string;
  pending: boolean;
  onSubmit: (reason: string) => void;
}) & { appearance?: UiAppearance }) {
  return (
    <form data-ui-appearance={appearance}
      className={styles.form}
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(String(new FormData(e.currentTarget).get("reason")));
      }}
    >
      <Field appearance={appearance}>
        {label} — reason
        <TextInput appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
      </Field>
      <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={pending}>
        {label}
      </Button>
    </form>
  );
}
function GrantForm({ appearance = "legacy",
  meta,
  pending,
  group,
  onSubmit,
}: ({
  meta: Meta;
  pending: boolean;
  group: boolean;
  onSubmit: (input: Record<string, unknown>) => void;
}) & { appearance?: UiAppearance }) {
  return (
    <form data-ui-appearance={appearance}
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
      <h3 data-ui-appearance={appearance}>{group ? "Propose role mapping" : "Grant / renew a role"}</h3>
      <Field appearance={appearance}>
        Role
        <Select appearance={appearance} name="role" required>
          <option data-ui-appearance={appearance} value="">Choose a role</option>
          {meta.roles.map((r) => (
            <option data-ui-appearance={appearance} key={r.id} value={r.id}>
              {r.name} · revision {r.revision}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Scope
        <Select appearance={appearance} name="scope" required>
          {meta.scopeKind === "holding" && <option data-ui-appearance={appearance} value="holding">Company</option>}
          {meta.tree
            .filter((n) => n.source === "ERPX" && ["branch", "department"].includes(n.kind))
            .map((n) => (
              <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                {n.label} · {n.kind}
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Expires
        <TextInput appearance={appearance} type="date" name="expires" required />
      </Field>
      <Field appearance={appearance}>
        Reason
        <TextInput appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
      </Field>
      <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={pending || !meta.roles.length}>
        {group ? "Propose mapping" : "Submit grant / renewal"}
      </Button>
    </form>
  );
}
