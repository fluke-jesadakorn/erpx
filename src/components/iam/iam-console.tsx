"use client";
import { Card, FilterBar } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { FeatureTabBar, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { AttachmentFeatureSurface } from "@/components/attachments/attachment-feature-context";

import { flowKinds } from "@/lib/flow";
import {
  directoryProviderLabels,
  iamSections,
  type AccessDecision,
  type IamSection,
  type RoleRevision,
} from "@/lib/iam";
import { evaluateAccess } from "@/lib/permissions";
import { workFetch } from "@/lib/work-fetch";
import { TimeText } from "@/components/time/time-preference-context";
import type { iamOverview } from "@/server/iam-service";
import {
  Check,
  CircleHelp,
  Download,
  KeyRound,
  Minus,
  Network,
  RefreshCw,
  Search,
  ShieldCheck,
  Users,
  X,
} from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import styles from "./iam.module.css";
type Data = Awaited<ReturnType<typeof iamOverview>>;
type Row = Record<string, unknown>;
const text = (v: unknown) => String(v ?? "");
const date = (v: unknown) => (v ? <TimeText appearance="standard" value={Number(v)} /> : "—");
const day = (v: number) => new Date(v).toISOString().slice(0, 10);
const title = (s: string) => s.replaceAll("_", " ").replaceAll(".", " · ");
const field = (f: FormData, k: string) => String(f.get(k) ?? "").trim();
const num = (f: FormData, k: string) => Number(f.get(k));
const symbols = {
  allowed: Check,
  denied: X,
  conditional: CircleHelp,
  unavailable: Minus,
};
function SanitizedIamDocuments({
  base,
  targetType,
  targetId,
  version,
  editable,
}: {
  base: string;
  targetType: "iam_review" | "iam_migration";
  targetId: string;
  version: number;
  editable: boolean;
}) {
  const [open, setOpen] = useState(false),
    review = targetType === "iam_review";
  return (
    <AttachmentFeatureSurface>
      <details className={styles.review} onToggle={(event) => setOpen(event.currentTarget.open)}>
        <summary>{review ? "Sanitized review evidence" : "Sanitized migration evidence"}</summary>
        {open && (
          <RecordDocumentPanel appearance="standard"
            base={base}
            targetType={targetType}
            targetId={targetId}
            expectedTargetVersion={version}
            title={review ? "Sanitized review evidence" : "Sanitized migration evidence"}
            purposes={[
              {
                key: review ? "sanitized_review_evidence" : "migration_evidence",
                label: review ? "Sanitized review evidence" : "Sanitized migration evidence",
                description:
                  "Attach only a redacted review artifact. Never upload passwords, private keys, secrets, configuration exports, or database dumps.",
                visibility:
                  "Restricted to Company IAM administrators. Sanitization attestation is required before binding.",
              },
            ]}
            editable={editable}
          />
        )}
      </details>
    </AttachmentFeatureSurface>
  );
}
const labels = {
  allowed: "Allowed",
  denied: "Denied",
  conditional: "Conditional",
  unavailable: "Unavailable",
};
function Decision({ decision, onClick }: { decision: AccessDecision; onClick: () => void }) {
  const Icon = symbols[decision.result];
  return (
    <Button appearance="standard"
      type="submit"
      variant="ghost"
      className={styles.decision}
      data-result={decision.result}
      onClick={onClick}
    >
      <Icon size={15} aria-hidden />
      <span>{labels[decision.result]}</span>
    </Button>
  );
}
function Empty({ children }: { children: React.ReactNode }) {
  return <div className={styles.empty}>{children}</div>;
}
export function IamConsole({
  data,
  base,
  api,
  section,
  embedded = false,
}: {
  embedded?: boolean;
  data: Data;
  base: string;
  api: string;
  section: IamSection;
}) {
  const detailRef = useRef<HTMLElement>(null);
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [error, setError] = useState(""),
    [query, setQuery] = useState(""),
    [module, setModule] = useState(""),
    [provider, setProvider] = useState(""),
    [status, setStatus] = useState(""),
    [scope, setScope] = useState(""),
    [matrixMode, setMatrixMode] = useState<"users" | "roles">("users"),
    [selected, setSelected] = useState<string[]>([data.currentAssignmentId]),
    [detail, setDetail] = useState<AccessDecision | null>(null),
    [editing, setEditing] = useState<RoleRevision | null>(null),
    [chosenPermissions, setChosenPermissions] = useState<string[]>([]),
    [chosenGrantable, setChosenGrantable] = useState<string[]>([]),
    [directoryProvider, setDirectoryProvider] =
      useState<keyof typeof directoryProviderLabels>("active_directory"),
    [method, setMethod] = useState("ldaps"),
    [chosenConnection, setChosenConnection] = useState(""),
    [identitySelection, setIdentitySelection] = useState<string[]>([]),
    [secret, setSecret] = useState(""),
    [auditResult, setAuditResult] = useState<{
      items: Row[];
      nextCursor: string | null;
    } | null>(null),
    [inspector, setInspector] = useState<unknown>(null),
    [editingConnection, setEditingConnection] = useState<Row | null>(null);
  useEffect(() => {
    if (detail) detailRef.current?.focus();
  }, [detail]);
  const can = (p: string) => data.permissions.includes(data.platform ? `platform.${p}` : p),
    companyScope = ["holding", "platform"].includes(data.scopeKind),
    end = Math.min(data.evaluatedAt + 90 * 86400000, data.validUntil);
  async function send(path: string, payload: unknown, account = false) {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const response = await workFetch(`${account ? "/api/v1/account/iam" : api}/${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": crypto.randomUUID(),
        },
        body: JSON.stringify(payload),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "The operation failed.");
      if (body.data.secret) setSecret(body.data.secret);
      if (path === "mfa/verify") setSecret("");
      if (path === "connections") setEditingConnection(null);
      setMessage(
        body.data.message ??
          (body.data.status === "pending" ? "Submitted for independent review." : "Saved."),
      );
      setAuditResult(null);
      router.refresh();
      return body.data;
    } catch (e) {
      setError(e instanceof Error ? e.message : "The operation failed.");
      return null;
    } finally {
      setBusy(false);
    }
  }
  const auditRows = auditResult?.items ?? data.audit;
  const lastAudit = data.audit.at(-1);
  const auditCursor = auditResult
    ? auditResult.nextCursor
    : data.audit.length === 100 && lastAudit
      ? JSON.stringify({
          createdAt: Number(lastAudit.created_at),
          id: String(lastAudit.id),
        })
      : null;
  async function loadAudit(append = false) {
    setBusy(true);
    setError("");
    try {
      const q = new URLSearchParams({ search: query });
      if (append && auditCursor) q.set("cursor", auditCursor);
      const response = await workFetch(`${api}/audit?${q}`),
        body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Audit history is unavailable.");
      setAuditResult({
        items: append ? [...auditRows, ...body.data.items] : body.data.items,
        nextCursor: body.data.nextCursor,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Audit history is unavailable.");
    } finally {
      setBusy(false);
    }
  }
  const filteredAssignments = data.assignments.filter(
    (a) =>
      (section === "matrix" ||
        !query ||
        `${data.users.find((u) => u.id === a.user_id)?.name} ${a.role}`
          .toLowerCase()
          .includes(query.toLowerCase())) &&
      (!provider || a.connection_id === provider || (provider === "local" && !a.connection_id)) &&
      (!status ||
        (status === "active"
          ? a.active
          : status === "expired"
            ? Number(a.valid_until) <= data.evaluatedAt
            : !!a.revoked_at)) &&
      (!scope || a.scope_node_id === scope || (scope === "holding" && a.scope_kind === "holding")),
  );
  const matrixColumns =
    matrixMode === "users"
      ? filteredAssignments
          .filter((a) => selected.includes(text(a.id)))
          .map((a) => ({
            id: text(a.id),
            label: `${data.users.find((u) => u.id === a.user_id)?.name ?? a.user_id} · ${data.roles.find((r) => r.id === a.role)?.name ?? a.role}`,
            decisions: data.decisions.find((d) => d.assignmentId === a.id)?.decisions ?? [],
          }))
      : data.roles
          .filter((r) => selected.includes(r.id))
          .map((r) => ({
            id: r.id,
            label: r.name,
            decisions: data.catalog.map((p) =>
              evaluateAccess(
                {
                  role: r.id,
                  permissions: r.permissions,
                  policyRevision: r.revision,
                },
                p.id,
                { inspect: true },
              ),
            ),
          }));
  const catalog = data.catalog.filter(
    (p) =>
      (!module || p.module === module) &&
      (!query || `${p.label} ${p.id}`.toLowerCase().includes(query.toLowerCase())),
  );
  const affected = editing ? data.assignments.filter((a) => a.role === editing.id && a.active) : [];
  const added = chosenPermissions.filter((p) => !editing?.permissions.includes(p)),
    removed = editing?.permissions.filter((p) => !chosenPermissions.includes(p)) ?? [];
  const connection = data.connections.find((c) => c.id === chosenConnection);
  const filteredIdentities = data.identities.filter(
    (i) => i.connection_id === chosenConnection && i.enabled && !i.user_id,
  );
  const selectedIdentities = filteredIdentities.filter((i) =>
    identitySelection.includes(text(i.id)),
  );
  const activeCount = data.assignments.filter((a) => a.active).length;
  const pageTitle = iamSections.find(([id]) => id === section)?.[1] ?? "Identity & access";
  const userOptions = useMemo(
    () => data.users.filter((u) => u.id !== data.currentUserId),
    [data.users, data.currentUserId],
  );
  function renderRoleSelect() {
    return (
      <Field appearance="standard">
        Role
        <Select appearance="standard" name="roleId" required>
          <option value="">Choose a role</option>
          {data.roles
            .filter((r) =>
              [...r.permissions, ...r.grantablePermissions].every((p) =>
                data.grantablePermissions.includes(p),
              ),
            )
            .map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
        </Select>
      </Field>
    );
  }
  function renderScopeFields() {
    return (
      <>
        <Field appearance="standard">
          Scope
          <Select appearance="standard" name="scopeKind" defaultValue={data.scopeKind}>
            {(data.platform
              ? ["platform"]
              : companyScope
                ? ["holding", "branch", "department"]
                : data.scopeKind === "branch"
                  ? ["branch", "department"]
                  : ["department"]
            ).map((k) => (
              <option key={k}>{k}</option>
            ))}
          </Select>
        </Field>
        {!data.platform && (
          <Field appearance="standard">
            Branch or department
            <Select appearance="standard" name="scopeNodeId">
              <option value="">Company scope</option>
              {data.nodes
                .filter((n) => n.kind !== "company")
                .map((n) => (
                  <option key={text(n.id)} value={text(n.id)}>
                    {text(n.display_name)} · {text(n.kind)}
                  </option>
                ))}
            </Select>
          </Field>
        )}
        <Field appearance="standard">
          Valid until
          <TextInput appearance="standard"
            type="date"
            name="validUntil"
            required
            defaultValue={day(end)}
            max={day(data.validUntil)}
            min={day(data.evaluatedAt + 86400000)}
          />
        </Field>
      </>
    );
  }
  function grantFields(f: FormData) {
    return {
      roleId: field(f, "roleId"),
      expectedRoleRevision: data.roles.find((r) => r.id === field(f, "roleId"))?.revision,
      scopeKind: field(f, "scopeKind"),
      scopeNodeId: field(f, "scopeNodeId") || null,
      validUntil: new Date(field(f, "validUntil")).getTime(),
      reason: field(f, "reason"),
    };
  }
  function renderReason() {
    return (
      <Field appearance="standard" className={styles.wide}>
        Reason
        <TextArea appearance="standard"
          name="reason"
          required
          minLength={3}
          maxLength={1000}
          placeholder="Explain the business need and requested scope."
        />
      </Field>
    );
  }
  function submit(label = "Submit for review") {
    return (
      <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
        {busy ? "Working…" : label}
      </Button>
    );
  }
  const filters = (
    <div className={styles.filters}>
      <Field appearance="standard">
        <Search size={15} aria-hidden />
        Search
        <TextInput appearance="standard"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={section === "matrix" ? "Page or action" : "Name or role"}
        />
      </Field>
      <Field appearance="standard">
        Provider
        <Select appearance="standard" value={provider} onChange={(e) => setProvider(e.target.value)}>
          <option value="">All providers</option>
          <option value="local">Local accounts</option>
          {data.connections.map((c) => (
            <option key={text(c.id)} value={text(c.id)}>
              {text(c.name)}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Scope
        <Select appearance="standard" value={scope} onChange={(e) => setScope(e.target.value)}>
          <option value="">All authorized scopes</option>
          <option value="holding">Company</option>
          {data.nodes
            .filter((n) => n.kind !== "company")
            .map((n) => (
              <option key={text(n.id)} value={text(n.id)}>
                {text(n.display_name)}
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Assignment status
        <Select appearance="standard" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">All statuses</option>
          {["active", "expired", "revoked"].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </Select>
      </Field>
    </div>
  );
  return (
    <section className={styles.console}>
      {!embedded && (
        <header className={styles.header}>
          <div>
            <p className={styles.eyebrow}>
              {data.platform ? "Platform" : "Company"} administration
            </p>
            <h1>Identity & access</h1>
            <p>Understand who can do what, and why.</p>
          </div>
          <span className={styles.scope}>
            <ShieldCheck size={17} />
            {title(data.scopeKind)} scope
          </span>
        </header>
      )}
      {!embedded && (
        <FeatureTabBar appearance="standard" aria-label="Identity and access">
          {iamSections
            .filter(
              ([id]) =>
                id === "my-access" ||
                (can("iam.read") &&
                  ((id !== "connections" && id !== "groups") || can("iam.directory.manage"))),
            )
            .map(([id, label]) => (
              <Link
                href={`${base}/iam/${id}`}
                key={id}
                aria-current={id === section ? "page" : undefined}
              >
                {label}
              </Link>
            ))}
        </FeatureTabBar>
      )}
      <div aria-live="polite">{message && <p className={styles.success}>{message}</p>}</div>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <WidgetGrid pageKey={base === "/platform" ? `platform-iam-${section}` : undefined}>
        <FeatureWidget appearance="standard" id={`iam-${section}`} title={pageTitle}>
          <div className={styles.sectionHeading}>
            <h2>{pageTitle}</h2>
            <Button appearance="standard"
              type="submit"
              variant="secondary"

              onClick={() => router.refresh()}
              disabled={busy}
            >
              <RefreshCw size={14} />
              Refresh
            </Button>
          </div>
          {section === "overview" && (
            <>
              <div className={styles.metrics}>
                {[
                  [Users, "Active assignments", activeCount],
                  [
                    Network,
                    "Active directories",
                    data.connections.filter((c) => c.status === "active").length,
                  ],
                  [
                    CircleHelp,
                    "Pending reviews",
                    data.reviews.filter((r) => r.status === "pending").length,
                  ],
                  [
                    KeyRound,
                    "Expiring in 14 days",
                    data.assignments.filter(
                      (a) => a.active && Number(a.valid_until) < data.evaluatedAt + 14 * 86400000,
                    ).length,
                  ],
                ].map(([Icon, label, value]) => {
                  const I = Icon as typeof Users;
                  return (
                    <section className={styles.metric} key={String(label)}>
                      <I size={21} />
                      <strong>{String(value)}</strong>
                      <span>{String(label)}</span>
                    </section>
                  );
                })}
              </div>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Authentication sources</h3>
                <p>
                  Local database accounts remain available for employees who have not migrated. Each
                  approved directory connects only to this company.
                </p>
                {!data.connections.length ? (
                  <Empty>
                    No directory connections configured. ERPX is operating with local accounts.
                  </Empty>
                ) : (
                  data.connections.map((c) => (
                    <article className={styles.item} key={text(c.id)}>
                      <div>
                        <strong>{text(c.name)}</strong>
                        <p>
                          {
                            directoryProviderLabels[
                              c.provider as keyof typeof directoryProviderLabels
                            ]
                          }{" "}
                          · {text(c.status)}
                        </p>
                      </div>
                      <div>
                        <span className={styles.badge}>{text(c.health)}</span>
                        <p>Last sync: {date(c.last_sync_at)}</p>
                        {!!c.last_error && <p className="error">{title(text(c.last_error))}</p>}
                      </div>
                    </article>
                  ))
                )}
              </Card>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Access that needs attention</h3>
                {data.assignments
                  .filter(
                    (a) => a.active && Number(a.valid_until) < data.evaluatedAt + 14 * 86400000,
                  )
                  .map((a) => (
                    <p key={text(a.id)}>
                      {data.users.find((u) => u.id === a.user_id)?.name} · {title(text(a.role))} ·
                      expires {date(a.valid_until)}
                    </p>
                  ))}
                <Link href={`${base}/iam/reviews`}>Open reviews & audit →</Link>
              </Card>
            </>
          )}
          {section === "users" && (
            <>
              {filters}
              <Card as="section" appearance="standard" className={styles.panel}>
                <TableFrame appearance="standard" label="Records" className={styles.tableWrap}>
                  <Table className="business-table">
                    <thead>
                      <tr>
                        <th>User / source</th>
                        <th>Role and scope</th>
                        <th>Status</th>
                        <th>Validity</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredAssignments.map((a) => (
                        <tr key={text(a.id)}>
                          <td>
                            <strong>
                              {data.users.find((u) => u.id === a.user_id)?.name ?? text(a.user_id)}
                            </strong>
                            <small>
                              {a.login_policy === "directory"
                                ? "Directory identity"
                                : a.login_policy === "emergency"
                                  ? "Protected local recovery"
                                  : "Local account"}
                            </small>
                            {!!a.person_id && <small>Linked employee</small>}
                          </td>
                          <td>
                            {data.roles.find((r) => r.id === a.role)?.name ?? title(text(a.role))}
                            <small>
                              {text(a.scope_kind)} ·{" "}
                              {text(
                                data.nodes.find((n) => n.id === a.scope_node_id)?.display_name,
                              ) || "Company"}
                            </small>
                            <small>
                              {a.id === data.currentAssignmentId
                                ? "Selected work role"
                                : "Available after switching role"}
                            </small>
                          </td>
                          <td>
                            <span className={styles.badge}>
                              {a.active
                                ? "Active"
                                : a.revoked_at
                                  ? "Revoked"
                                  : a.identity_status === "superseded"
                                    ? "Directory required"
                                    : "Unavailable"}
                            </span>
                          </td>
                          <td>
                            {date(a.valid_until)}
                            <small>
                              {text(a.source)}
                              {a.parent_id ? " · Parent bounded" : ""}
                            </small>
                          </td>
                          <td>
                            {can("iam.manage") && a.active && a.user_id !== data.currentUserId && (
                              <Button appearance="standard"
                                type="submit"
                                variant="secondary"

                                disabled={busy}
                                onClick={() =>
                                  send(`assignments/${a.id}/revoke`, {
                                    expectedVersion: a.version,
                                    reason: "Revoked from IAM user administration",
                                  })
                                }
                              >
                                Revoke
                              </Button>
                            )}
                            {can("iam.manage") &&
                              companyScope &&
                              a.user_id !== data.currentUserId && (
                                <Button appearance="standard"
                                  type="submit"
                                  variant="secondary"

                                  disabled={busy}
                                  onClick={() => send(`users/${a.user_id}/sessions`, {})}
                                >
                                  Revoke sessions
                                </Button>
                              )}
                            {can("iam.manage") &&
                              companyScope &&
                              a.identity_status === "suspended" && (
                                <Button appearance="standard"
                                  type="submit"
                                  variant="secondary"

                                  disabled={busy}
                                  onClick={() =>
                                    send(`users/${a.user_id}/restore`, {
                                      expectedVersion: a.identity_version,
                                      reason: "Request restoration of suspended company access",
                                    })
                                  }
                                >
                                  Request restoration
                                </Button>
                              )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
                {!filteredAssignments.length && <Empty>No assignments match the filters.</Empty>}
              </Card>
              {can("iam.manage") && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Grant or renew access</h3>
                  <p>
                    Renewal creates a new assignment. An independent reviewer approves every grant.
                    Choose the permissions deliberately; grants expire within your authority.
                  </p>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget);
                      void send("assignments", {
                        ...grantFields(f),
                        userId: field(f, "userId"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      User
                      <Select appearance="standard" name="userId" required>
                        <option value="">Choose an account</option>
                        {userOptions.map((u) => (
                          <option key={u.id} value={u.id}>
                            {u.name} ·{" "}
                            {data.people.find((p) => p.user_id === u.id)?.legal_name ?? u.email}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    {renderRoleSelect()}
                    {renderScopeFields()}
                    {renderReason()}
                    {submit()}
                  </form>
                </Card>
              )}
              {can("iam.manage") && companyScope && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Suspend company access</h3>
                  <p>
                    Suspension immediately invalidates this company’s sessions and grants. It
                    preserves employee history.
                  </p>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget),
                        id = field(f, "userId");
                      void send(`users/${id}/suspend`, {
                        expectedVersion: Number(
                          data.assignments.find((a) => a.user_id === id)?.identity_version ?? 1,
                        ),
                        reason: field(f, "reason"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      User
                      <Select appearance="standard" name="userId" required>
                        {userOptions.map((u) => (
                          <option key={u.id} value={u.id}>
                            {u.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    {renderReason()}
                    {submit("Suspend access")}
                  </form>
                </Card>
              )}
            </>
          )}
          {section === "roles" && (
            <>
              <div className={styles.roleGrid}>
                {data.roles.map((r) => (
                  <Card as="section" appearance="standard" className={styles.panel} key={r.id}>
                    <h3>{r.name}</h3>
                    <p>
                      {r.builtIn ? "Protected built-in role" : "Company custom role"} · revision{" "}
                      {r.revision}
                    </p>
                    <p>
                      {r.permissions.length} permissions · {r.scopes.join(", ")}
                    </p>
                    <details>
                      <summary>Permissions</summary>
                      <ul>
                        {r.permissions.map((p) => (
                          <li key={p}>{data.catalog.find((c) => c.id === p)?.label ?? p}</li>
                        ))}
                      </ul>
                    </details>
                    {can("iam.roles.manage") && companyScope && (
                      <div className={styles.actions}>
                        <Button appearance="standard"
                          type="submit"
                          variant="secondary"

                          onClick={() => {
                            setEditing(null);
                            setChosenPermissions(
                              r.permissions.filter((p) => data.grantablePermissions.includes(p)),
                            );
                            setChosenGrantable([]);
                          }}
                        >
                          Clone
                        </Button>
                        {!r.builtIn && (
                          <Button appearance="standard"
                            type="submit"
                            variant="secondary"

                            onClick={() => {
                              setEditing(r);
                              setChosenPermissions(r.permissions);
                              setChosenGrantable(r.grantablePermissions);
                            }}
                          >
                            Edit revision
                          </Button>
                        )}
                      </div>
                    )}
                  </Card>
                ))}
              </div>
              {can("iam.roles.manage") && companyScope && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>{editing ? `Revise ${editing.name}` : "Create custom role"}</h3>
                  <form
                    className={styles.form}
                    key={editing?.id ?? "new"}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget);
                      void send("roles", {
                        id: editing?.id,
                        name: field(f, "name"),
                        expectedVersion: editing?.revision ?? 0,
                        permissions: chosenPermissions,
                        grantablePermissions: chosenGrantable,
                        scopes: f.getAll("scopes"),
                        reason: field(f, "reason"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Role name
                      <TextInput appearance="standard" name="name" defaultValue={editing?.name} required minLength={2} />
                    </Field>
                    <fieldset>
                      <legend>Allowed scopes</legend>
                      {(data.platform ? ["platform"] : ["holding", "branch", "department"]).map(
                        (s) => (
                          <Field appearance="standard" orientation="inline" className={styles.check} key={s}>
                            <TextInput appearance="standard"
                              type="checkbox"
                              name="scopes"
                              value={s}
                              defaultChecked={
                                editing
                                  ? editing.scopes.includes(s)
                                  : s === (data.platform ? "platform" : "holding")
                              }
                            />
                            {title(s)}
                          </Field>
                        ),
                      )}
                    </fieldset>
                    <fieldset className={styles.wide}>
                      <legend>Permissions and delegation</legend>
                      <p>
                        “May grant” lets this role delegate a permission. It does not grant access
                        to the underlying business records.
                      </p>
                      <div className={styles.permissionList}>
                        {data.catalog
                          .filter((p) => p.availability === "live")
                          .map((p) => (
                            <div className={styles.permissionRow} key={p.id}>
                              <Field appearance="standard" orientation="inline" className={styles.check}>
                                <TextInput appearance="standard"
                                  type="checkbox"
                                  disabled={!data.grantablePermissions.includes(p.id)}
                                  checked={chosenPermissions.includes(p.id)}
                                  onChange={(e) =>
                                    setChosenPermissions((v) =>
                                      e.target.checked ? [...v, p.id] : v.filter((x) => x !== p.id),
                                    )
                                  }
                                />
                                <span>
                                  {p.label}
                                  <small>
                                    {p.module} {p.risk === "privileged" ? "· Privileged" : ""}
                                  </small>
                                </span>
                              </Field>
                              <Field appearance="standard" orientation="inline" className={styles.check}>
                                <TextInput appearance="standard"
                                  aria-label={`May grant: ${p.label}`}
                                  type="checkbox"
                                  disabled={!data.grantablePermissions.includes(p.id)}
                                  checked={chosenGrantable.includes(p.id)}
                                  onChange={(e) =>
                                    setChosenGrantable((v) =>
                                      e.target.checked ? [...v, p.id] : v.filter((x) => x !== p.id),
                                    )
                                  }
                                />
                                May grant
                              </Field>
                            </div>
                          ))}
                      </div>
                    </fieldset>
                    <section className={styles.impact}>
                      <h4>Impact preview</h4>
                      <p>
                        {affected.length} active assignments ·{" "}
                        {new Set(affected.map((a) => a.user_id)).size} affected users ·{" "}
                        {added.length} permissions added · {removed.length} removed
                      </p>
                      <p>
                        {affected
                          .map((a) => data.users.find((u) => u.id === a.user_id)?.name ?? a.user_id)
                          .join(", ") || "No existing assignments."}
                      </p>
                      <details>
                        <summary>Permission changes</summary>
                        {added.map((p) => (
                          <p key={p}>+ {data.catalog.find((c) => c.id === p)?.label}</p>
                        ))}
                        {removed.map((p) => (
                          <p key={p}>− {data.catalog.find((c) => c.id === p)?.label}</p>
                        ))}
                      </details>
                      <p>
                        Publication invalidates affected work contexts. Users must select their role
                        again.
                      </p>
                    </section>
                    {renderReason()}
                    {submit("Submit revision for review")}
                  </form>
                </Card>
              )}
            </>
          )}
          {(section === "matrix" || section === "my-access") && (
            <>
              {section === "matrix" && (
                <>
                  {filters}
                  <div className={styles.filters}>
                    <Field appearance="standard">
                      Compare
                      <Select appearance="standard"
                        value={matrixMode}
                        onChange={(e) => {
                          setMatrixMode(e.target.value as "users" | "roles");
                          setSelected([]);
                        }}
                      >
                        <option value="users">User assignments</option>
                        <option value="roles">Role definitions</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Module
                      <Select appearance="standard" value={module} onChange={(e) => setModule(e.target.value)}>
                        <option value="">All modules</option>
                        {[...new Set(data.catalog.map((p) => p.module))].map((m) => (
                          <option key={m} value={m}>
                            {title(m)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                  </div>
                  <fieldset className={styles.selection}>
                    <legend>Select up to 12 columns</legend>
                    {(matrixMode === "roles"
                      ? data.roles.map((r) => ({ id: r.id, label: r.name }))
                      : filteredAssignments.map((a) => ({
                          id: text(a.id),
                          label: `${data.users.find((u) => u.id === a.user_id)?.name ?? a.user_id} · ${data.roles.find((r) => r.id === a.role)?.name ?? a.role}`,
                        }))
                    ).map((a) => (
                      <Field appearance="standard" orientation="inline" className={styles.check} key={a.id}>
                        <TextInput appearance="standard"
                          type="checkbox"
                          checked={selected.includes(a.id)}
                          disabled={!selected.includes(a.id) && selected.length >= 12}
                          onChange={(e) =>
                            setSelected((s) =>
                              e.target.checked ? [...s, a.id] : s.filter((v) => v !== a.id),
                            )
                          }
                        />
                        {a.label}
                      </Field>
                    ))}
                  </fieldset>
                  {can("iam.export") && matrixMode === "users" && selected.length > 0 && (
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"

                      disabled={busy}
                      onClick={async () => {
                        setBusy(true);
                        setError("");
                        try {
                          const response = await workFetch(
                            `${api}/matrix/export?${selected.map((s) => "assignment=" + encodeURIComponent(s)).join("&")}`,
                          );
                          if (!response.ok) {
                            const body = await response.json();
                            throw new Error(body.error?.message ?? "Export failed.");
                          }
                          const href = URL.createObjectURL(await response.blob());
                          const download = document.createElement("a");
                          download.href = href;
                          download.download = "erpx-access.csv";
                          download.click();
                          URL.revokeObjectURL(href);
                        } catch (e) {
                          setError(e instanceof Error ? e.message : "Export failed.");
                        } finally {
                          setBusy(false);
                        }
                      }}
                    >
                      <Download size={15} />
                      Export selected access
                    </Button>
                  )}
                </>
              )}
              {section === "my-access" && (
                <>
                  <p>
                    Your selected work role is shown below. Choose another assignment to inspect
                    access available after switching; permissions are evaluated separately.
                  </p>
                  <fieldset className={styles.selection}>
                    <legend>Your assignments</legend>
                    {data.assignments.map((a) => (
                      <Field appearance="standard" orientation="inline" key={text(a.id)} className={styles.check}>
                        <TextInput appearance="standard"
                          type="checkbox"
                          checked={selected.includes(text(a.id))}
                          onChange={(e) =>
                            setSelected((s) =>
                              e.target.checked ? [...s, text(a.id)] : s.filter((v) => v !== a.id),
                            )
                          }
                        />
                        {data.roles.find((r) => r.id === a.role)?.name ?? text(a.role)}
                        {a.id === data.currentAssignmentId ? " · Selected" : ""}
                      </Field>
                    ))}
                  </fieldset>
                </>
              )}
              <Card as="section" appearance="standard" className={styles.panel}>
                <p className="muted">
                  Conditional access still requires the listed business checks. Role comparisons
                  describe potential access; user comparisons include assignment validity.
                </p>
                <div className={styles.matrixWrap}>
                  <TableFrame appearance="standard" label="Records"><Table
                    className={styles.matrix}
                    onKeyDown={(e) => {
                      const keys = ["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp"];
                      if (!keys.includes(e.key)) return;
                      const target = e.target as HTMLElement,
                        cell = target.closest("td");
                      if (!cell) return;
                      const row = cell.parentElement as HTMLTableRowElement,
                        table = row.closest("table")!,
                        r = row.rowIndex,
                        c = (cell as HTMLTableCellElement).cellIndex;
                      const next =
                        table.rows[
                          r + (e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0)
                        ]?.cells[
                          c + (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0)
                        ]?.querySelector("button");
                      if (next) {
                        e.preventDefault();
                        next.focus();
                      }
                    }}
                  >
                    <caption>
                      Access by permission and selected assignment or role. Use arrow keys between
                      cells.
                    </caption>
                    <thead>
                      <tr>
                        <th scope="col">Page or business action</th>
                        {matrixColumns.map((c) => (
                          <th scope="col" key={c.id}>
                            {c.label}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {catalog.map((p) => (
                        <tr key={p.id}>
                          <th scope="row">
                            {p.label}
                            <small>{title(p.module)}</small>
                          </th>
                          {matrixColumns.map((c) => (
                            <td key={c.id}>
                              <Decision
                                decision={
                                  c.decisions.find((d) => d.permission === p.id) ??
                                  evaluateAccess({ role: "" }, p.id)
                                }
                                onClick={() =>
                                  setDetail(c.decisions.find((d) => d.permission === p.id) ?? null)
                                }
                              />
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </Table></TableFrame>
                </div>
                {!matrixColumns.length && (
                  <Empty>Select a role or user assignment to compare permissions.</Empty>
                )}
              </Card>
              {detail && (
                <section
                  className={styles.detail}
                  role="region"
                  aria-label="Access explanation"
                  ref={detailRef}
                  tabIndex={-1}
                >
                  <Button appearance="standard"
                    type="submit"
                    variant="secondary"

                    onClick={() => setDetail(null)}
                  >
                    Close explanation
                  </Button>
                  <h3>{data.catalog.find((p) => p.id === detail.permission)?.label}</h3>
                  <strong>{labels[detail.result]}</strong>
                  <p>{detail.reasons.join(" ")}</p>
                  <dl>
                    <dt>Granting role</dt>
                    <dd>{data.roles.find((r) => r.id === detail.role)?.name ?? detail.role}</dd>
                    <dt>Scope</dt>
                    <dd>{detail.scope ?? "Role definition"}</dd>
                    <dt>Policy revision</dt>
                    <dd>{detail.revision}</dd>
                    <dt>Grant source</dt>
                    <dd>{detail.source ?? "Role definition"}</dd>
                    <dt>Expiry</dt>
                    <dd>{date(detail.expiresAt)}</dd>
                  </dl>
                  {detail.conditions.length > 0 && (
                    <>
                      <h4>Remaining conditions</h4>
                      <ul>
                        {detail.conditions.map((c) => (
                          <li key={c}>{c}</li>
                        ))}
                      </ul>
                    </>
                  )}
                </section>
              )}
              {section === "matrix" && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Read-only access inspector</h3>
                  <p>
                    Inspect an assignment without impersonating its user. This explanation cannot
                    authorize execution.
                  </p>
                  <form
                    className={styles.form}
                    onSubmit={async (e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget),
                        a = data.assignments.find((a) => a.id === field(f, "assignmentId"));
                      setError("");
                      try {
                        const q = new URLSearchParams({
                          assignmentId: field(f, "assignmentId"),
                          userId: text(a?.user_id),
                          permission: field(f, "permission"),
                          recordId: field(f, "recordId"),
                          recordType: field(f, "recordType"),
                          workspaceId: field(f, "workspaceId"),
                        });
                        const response = await workFetch(`${api}/inspector?${q}`),
                          r = await response.json();
                        if (!response.ok) throw new Error(r.error?.message);
                        setInspector(r.data);
                      } catch (e) {
                        setError(e instanceof Error ? e.message : "Inspection failed.");
                      }
                    }}
                  >
                    <Field appearance="standard">
                      Person and assignment
                      <Select appearance="standard" name="assignmentId" required>
                        {filteredAssignments.map((a) => (
                          <option key={text(a.id)} value={text(a.id)}>
                            {data.users.find((u) => u.id === a.user_id)?.name} ·{" "}
                            {title(text(a.role))}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Action
                      <Select appearance="standard" name="permission">
                        {data.catalog.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.label}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Record type
                      <Select appearance="standard" name="recordType">
                        <option value="">Choose a record type</option>
                        {flowKinds.map((k) => (
                          <option key={k} value={k}>
                            {title(k)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Workspace
                      <Select appearance="standard" name="workspaceId">
                        <option value="">Assignment scope</option>
                        <option value={data.companyId}>Company</option>
                        {data.nodes
                          .filter((n) => n.kind === "branch" || n.kind === "department")
                          .map((n) => (
                            <option key={text(n.id)} value={text(n.id)}>
                              {text(n.display_name)}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Record ID (optional)
                      <TextInput appearance="standard" name="recordId" />
                    </Field>
                    {submit("Inspect")}
                  </form>
                  {!!inspector && (
                    <pre className={styles.evidence}>{JSON.stringify(inspector, null, 2)}</pre>
                  )}
                </Card>
              )}
            </>
          )}
          {section === "connections" && can("iam.directory.manage") && companyScope && (
            <>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Connection workflow</h3>
                <ol className={styles.steps}>
                  {[
                    "Choose provider",
                    "Configure",
                    "Test",
                    "Preview users & groups",
                    "Define mappings",
                    "Activate",
                  ].map((s) => (
                    <li key={s}>{s}</li>
                  ))}
                </ol>
                <p>
                  Saving a configuration keeps it inactive. Credentials and trusted certificates
                  come from the external secret profile configured on the ERPX server.
                </p>
              </Card>
              {data.connections.map((c) => (
                <Card as="section" appearance="standard" className={styles.panel} key={text(c.id)}>
                  <div className={styles.sectionHeading}>
                    <div>
                      <h3>{text(c.name)}</h3>
                      <p>
                        {
                          directoryProviderLabels[
                            c.provider as keyof typeof directoryProviderLabels
                          ]
                        }{" "}
                        · {text((c.config as Row)?.mode).toUpperCase()} · revision {text(c.version)}
                      </p>
                    </div>
                    <span className={styles.badge}>{text(c.status)}</span>
                  </div>
                  <p>
                    {text(c.health)} · Last successful sync {date(c.last_sync_at)}
                  </p>
                  {!!c.last_error && <p className="error">{title(text(c.last_error))}</p>}
                  <div className={styles.actions}>
                    {c.status !== "active" && (
                      <Button appearance="standard"
                        type="submit"
                        variant="secondary"

                        onClick={() => {
                          setEditingConnection(c);
                          setDirectoryProvider(c.provider as keyof typeof directoryProviderLabels);
                          setMethod(text((c.config as Row).mode));
                        }}
                      >
                        Edit configuration
                      </Button>
                    )}
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"

                      disabled={busy}
                      onClick={() =>
                        send(`connections/${c.id}/test`, {
                          expectedVersion: c.version,
                        })
                      }
                    >
                      Test connection
                    </Button>
                    {c.status === "tested" && (
                      <Button appearance="standard"
                        type="submit"
                        variant="primary"

                        disabled={busy}
                        onClick={() =>
                          send(`connections/${c.id}/activate`, {
                            expectedVersion: c.version,
                          })
                        }
                      >
                        Request activation
                      </Button>
                    )}
                    {c.status === "active" && (
                      <>
                        <Button appearance="standard"
                          type="submit"
                          variant="secondary"

                          disabled={busy}
                          onClick={() =>
                            send(`connections/${c.id}/sync`, {
                              expectedVersion: c.version,
                            })
                          }
                        >
                          Synchronize now
                        </Button>
                      </>
                    )}
                    {c.status !== "disabled" && can("iam.directory.manage") && companyScope && (
                      <Button appearance="standard"
                        type="button"
                        variant="secondary"
                        disabled={busy}
                        onClick={() => send(`connections/${c.id}/disable`, { expectedVersion: c.version })}
                      >
                        Disable connection
                      </Button>
                    )}
                  </div>
                  <details>
                    <summary>Preview users and groups</summary>
                    {data.previews
                      .filter((p) => p.connection_id === c.id)
                      .map((p) => (
                        <div key={text(p.connection_id)}>
                          <p>
                            Read at {date(p.created_at)}. These identities have no access until
                            approved migration.
                          </p>
                          <ul>
                            {(
                              p.snapshot_json as {
                                users: {
                                  externalId: string;
                                  name: string;
                                  username: string;
                                }[];
                              }
                            ).users.map((u) => (
                              <li key={u.externalId}>
                                {u.name} · {u.username}
                                <small>{u.externalId}</small>
                              </li>
                            ))}
                          </ul>
                        </div>
                      ))}
                  </details>
                  {c.status === "active" && (
                    <p>
                      Company sign-in:{" "}
                      <Link href={`/login?connection=${c.id}`}>Open directory sign-in</Link>
                    </p>
                  )}
                </Card>
              ))}
              {!data.connections.length && (
                <Empty>
                  No connections yet. Add AD, Entra ID, or Univention UCS when your company is
                  ready.
                </Empty>
              )}
              {can("iam.directory.manage") && companyScope && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>
                    {editingConnection ? "Edit inactive connection" : "Add directory connection"}
                  </h3>
                  {editingConnection && (
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"

                      onClick={() => setEditingConnection(null)}
                    >
                      Cancel editing
                    </Button>
                  )}
                  <form
                    key={text(editingConnection?.id) || "new-connection"}
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget);
                      const config: Record<string, string> = {
                        mode: directoryProvider === "entra_id" ? "oidc" : method,
                        secretProfile: field(f, "secretProfile"),
                        authority: field(f, "authority"),
                      };
                      for (const k of [
                        "ldapUrl",
                        "baseDn",
                        "groupBaseDn",
                        "userFilter",
                        "issuer",
                        "tenantId",
                        "clientId",
                      ])
                        if (field(f, k)) config[k] = field(f, k);
                      void send("connections", {
                        id: editingConnection?.id,
                        expectedVersion: Number(editingConnection?.version ?? 0),
                        name: field(f, "name"),
                        provider: directoryProvider,
                        config,
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Name
                      <TextInput appearance="standard"
                        name="name"
                        required
                        placeholder="Corporate directory"
                        defaultValue={text(editingConnection?.name)}
                      />
                    </Field>
                    <Field appearance="standard">
                      Provider
                      <Select appearance="standard"
                        disabled={!!editingConnection}
                        value={directoryProvider}
                        onChange={(e) => {
                          const p = e.target.value as typeof directoryProvider;
                          setDirectoryProvider(p);
                          setMethod(p === "entra_id" ? "oidc" : "ldaps");
                        }}
                      >
                        {Object.entries(directoryProviderLabels).map(([id, label]) => (
                          <option key={id} value={id}>
                            {label}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    {directoryProvider === "univention" && (
                      <Field appearance="standard">
                        UCS sign-in method
                        <Select appearance="standard" value={method} onChange={(e) => setMethod(e.target.value)}>
                          <option value="ldaps">Certificate-validated LDAPS</option>
                          <option value="oidc">Keycloak OIDC</option>
                        </Select>
                      </Field>
                    )}
                    <Field appearance="standard">
                      Directory authority
                      <TextInput appearance="standard"
                        name="authority"
                        defaultValue={text(
                          (editingConnection?.config as Row | undefined)?.authority,
                        )}
                        required
                        placeholder="corp.example.com"
                      />
                    </Field>
                    <Field appearance="standard">
                      External secret profile
                      <TextInput appearance="standard"
                        name="secretProfile"
                        defaultValue={text(
                          (editingConnection?.config as Row | undefined)?.secretProfile,
                        )}
                        required
                        pattern="[A-Za-z0-9_-]+"
                        placeholder="corp-identity"
                      />
                    </Field>
                    {directoryProvider !== "entra_id" && (
                      <>
                        <Field appearance="standard">
                          LDAPS URL
                          <TextInput appearance="standard"
                            name="ldapUrl"
                            defaultValue={text(
                              (editingConnection?.config as Row | undefined)?.ldapUrl,
                            )}
                            type="url"
                            required
                            placeholder="ldaps://directory.example.com:636"
                          />
                        </Field>
                        <Field appearance="standard">
                          Base DN
                          <TextInput appearance="standard"
                            name="baseDn"
                            defaultValue={text(
                              (editingConnection?.config as Row | undefined)?.baseDn,
                            )}
                            required
                            placeholder="dc=example,dc=com"
                          />
                        </Field>
                        <Field appearance="standard">
                          Group base DN (optional)
                          <TextInput appearance="standard"
                            name="groupBaseDn"
                            defaultValue={text(
                              (editingConnection?.config as Row | undefined)?.groupBaseDn,
                            )}
                          />
                        </Field>
                        <Field appearance="standard">
                          User filter (optional)
                          <TextInput appearance="standard"
                            name="userFilter"
                            defaultValue={text(
                              (editingConnection?.config as Row | undefined)?.userFilter,
                            )}
                            placeholder="Provider default"
                          />
                        </Field>
                      </>
                    )}
                    {(directoryProvider === "entra_id" || method === "oidc") && (
                      <>
                        <Field appearance="standard">
                          Client ID
                          <TextInput appearance="standard"
                            name="clientId"
                            defaultValue={text(
                              (editingConnection?.config as Row | undefined)?.clientId,
                            )}
                            required
                          />
                        </Field>
                        {directoryProvider === "entra_id" ? (
                          <Field appearance="standard">
                            Entra tenant ID
                            <TextInput appearance="standard"
                              name="tenantId"
                              defaultValue={text(
                                (editingConnection?.config as Row | undefined)?.tenantId,
                              )}
                              required
                              placeholder="Tenant UUID"
                            />
                          </Field>
                        ) : (
                          <Field appearance="standard">
                            Keycloak issuer
                            <TextInput appearance="standard"
                              name="issuer"
                              defaultValue={text(
                                (editingConnection?.config as Row | undefined)?.issuer,
                              )}
                              type="url"
                              required
                              placeholder="https://sso.example.com/realms/ucs"
                            />
                          </Field>
                        )}
                      </>
                    )}
                    {submit("Save inactive connection")}
                  </form>
                </Card>
              )}
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Selective employee migration</h3>
                <p>
                  Select exact identities, link each to its employee record, and propose replacement
                  grants. Employees outside the saved batch keep their existing login. Group
                  membership alone never migrates an employee.
                </p>
                <Field appearance="standard">
                  Active directory
                  <Select appearance="standard"
                    value={chosenConnection}
                    onChange={(e) => {
                      setChosenConnection(e.target.value);
                      setIdentitySelection([]);
                    }}
                  >
                    <option value="">Choose a connection</option>
                    {data.connections
                      .filter((c) => c.status === "active")
                      .map((c) => (
                        <option value={text(c.id)} key={text(c.id)}>
                          {text(c.name)}
                        </option>
                      ))}
                  </Select>
                </Field>
                {!!connection && (
                  <>
                    <Field appearance="standard">
                      Select current group members
                      <Select appearance="standard"
                        defaultValue=""
                        onChange={(e) =>
                          setIdentitySelection(
                            filteredIdentities
                              .filter((i) => (i.groups_json as string[]).includes(e.target.value))
                              .map((i) => text(i.id)),
                          )
                        }
                      >
                        <option value="">Choose a group to preview</option>
                        {data.groups
                          .filter((g) => g.connection_id === chosenConnection)
                          .map((g) => (
                            <option key={text(g.external_id)} value={text(g.external_id)}>
                              {text(g.name)}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    <fieldset className={styles.selection}>
                      <legend>Explicit identities in this batch</legend>
                      {filteredIdentities.map((i) => (
                        <Field appearance="standard" orientation="inline" className={styles.check} key={text(i.id)}>
                          <TextInput appearance="standard"
                            type="checkbox"
                            checked={identitySelection.includes(text(i.id))}
                            onChange={(e) =>
                              setIdentitySelection((s) =>
                                e.target.checked ? [...s, text(i.id)] : s.filter((v) => v !== i.id),
                              )
                            }
                          />
                          {text(i.name)} · {text(i.username)}
                          <small>{text(i.external_id)}</small>
                        </Field>
                      ))}
                    </fieldset>
                    {!filteredIdentities.length && (
                      <Empty>Synchronize this connection to preview unlinked identities.</Empty>
                    )}
                  </>
                )}
                {selectedIdentities.length > 0 && can("iam.manage") && (
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget),
                        grants = [grantFields(f)];
                      void send("migrations", {
                        connectionId: chosenConnection,
                        reason: field(f, "reason"),
                        users: selectedIdentities.map((i) => {
                          const person = data.people.find(
                            (p) => p.id === field(f, `person-${i.id}`),
                          );
                          return {
                            identityId: i.id,
                            personId: person?.id,
                            localUserId: person?.user_id,
                            grants,
                          };
                        }),
                      });
                    }}
                  >
                    {selectedIdentities.map((i) => (
                      <Field appearance="standard" key={text(i.id)}>
                        {text(i.name)} → Employee
                        <Select appearance="standard" name={`person-${i.id}`} required>
                          <option value="">Verify and select employee</option>
                          {data.people
                            .filter((p) => p.user_id && p.user_id !== data.currentUserId)
                            .map((p) => (
                              <option key={p.id} value={p.id}>
                                {p.legal_name}
                              </option>
                            ))}
                        </Select>
                      </Field>
                    ))}
                    <p className={styles.wide}>
                      Explicit replacement grant for the {selectedIdentities.length} selected
                      employees:
                    </p>
                    {renderRoleSelect()}
                    {renderScopeFields()}
                    {renderReason()}
                    {submit("Prepare migration for review")}
                  </form>
                )}
                {data.migrations.map((b) => (
                  <article className={styles.review} key={text(b.id)}>
                    <p>
                      Batch {text(b.id).slice(0, 8)} · {text(b.status)} ·{" "}
                      {(b.users_json as unknown[]).length} employees · {date(b.created_at)}
                    </p>
                    <SanitizedIamDocuments
                      base={base}
                      targetType="iam_migration"
                      targetId={text(b.id)}
                      version={Number(b.version)}
                      editable={can("iam.manage")}
                    />
                  </article>
                ))}
              </Card>
            </>
          )}
          {section === "groups" && (
            <>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Directory membership evidence</h3>
                {data.groups.length ? (
                  <TableFrame appearance="standard" label="Records" className={styles.tableWrap}>
                    <Table className="business-table">
                      <thead>
                        <tr>
                          <th>Group</th>
                          <th>Connection</th>
                          <th>Observed</th>
                        </tr>
                      </thead>
                      <tbody>
                        {data.groups.map((g) => (
                          <tr key={text(g.connection_id) + text(g.external_id)}>
                            <td>
                              {text(g.name)}
                              <small>{text(g.external_id)}</small>
                            </td>
                            <td>
                              {text(data.connections.find((c) => c.id === g.connection_id)?.name)}
                            </td>
                            <td>
                              {date(g.observed_at)}
                              <small>
                                {Number(g.observed_at) < data.evaluatedAt - 300000
                                  ? "Stale; refresh required"
                                  : "Fresh"}
                              </small>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                ) : (
                  <Empty>
                    No synchronized groups. Test and activate a directory connection first.
                  </Empty>
                )}
              </Card>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Scoped role mappings</h3>
                {data.mappings.map((m) => (
                  <article className={styles.item} key={text(m.id)}>
                    <div>
                      <strong>
                        {text(
                          data.groups.find(
                            (g) =>
                              g.external_id === m.group_id && g.connection_id === m.connection_id,
                          )?.name,
                        ) || text(m.group_id)}
                      </strong>
                      <p>
                        {data.roles.find((r) => r.id === m.role_id)?.name} · {text(m.scope_kind)} ·
                        until {date(m.valid_until)}
                      </p>
                      <span>{text(m.status)}</span>
                    </div>
                    {m.status === "active" && can("iam.manage") && (
                      <Button appearance="standard"
                        type="submit"
                        variant="secondary"

                        disabled={busy}
                        onClick={() =>
                          send(`mappings/${m.id}/revoke`, {
                            expectedVersion: m.version,
                            reason: "Revoke group-derived access",
                          })
                        }
                      >
                        Revoke mapping
                      </Button>
                    )}
                  </article>
                ))}
                {can("iam.manage") && (
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget),
                        g = data.groups[num(f, "groupIndex")];
                      if (g)
                        void send("mappings", {
                          ...grantFields(f),
                          connectionId: g.connection_id,
                          groupId: g.external_id,
                        });
                    }}
                  >
                    <Field appearance="standard">
                      Directory group
                      <Select appearance="standard" name="groupIndex" required>
                        <option value="">Choose a group</option>
                        {data.groups.map((g, i) => (
                          <option key={i} value={i}>
                            {text(g.name)} ·{" "}
                            {text(data.connections.find((c) => c.id === g.connection_id)?.name)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    {renderRoleSelect()}
                    {renderScopeFields()}
                    {renderReason()}
                    {submit("Propose group mapping")}
                  </form>
                )}
              </Card>
            </>
          )}
          {section === "reviews" && (
            <>
              {!data.platform && can("iam.review") && companyScope && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Periodic access certification</h3>
                  <p>
                    Capture every active assignment, then have a different administrator keep or
                    revoke each one against the frozen role, scope, source and expiry.
                  </p>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      const f = new FormData(e.currentTarget),
                        due = new Date(field(f, "dueAt") + "T23:59:59").getTime();
                      void send("access-reviews", {
                        name: field(f, "name"),
                        dueAt: due,
                        reason: field(f, "reason"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Campaign name
                      <TextInput appearance="standard"
                        name="name"
                        required
                        minLength={3}
                        maxLength={120}
                        placeholder="Quarterly access review"
                      />
                    </Field>
                    <Field appearance="standard">
                      Due date
                      <TextInput appearance="standard"
                        name="dueAt"
                        type="date"
                        required
                        min={day(data.evaluatedAt + 86400000)}
                        max={day(data.evaluatedAt + 366 * 86400000)}
                      />
                    </Field>
                    {renderReason()}
                    {submit("Start access review")}
                  </form>
                  {data.accessReviewCampaigns.length ? (
                    data.accessReviewCampaigns.map((c) => {
                      const items = data.accessReviewItems.filter((i) => i.campaign_id === c.id),
                        pending = items.filter((i) => i.decision === "pending").length,
                        decided = items.length - pending,
                        overdue = c.status === "open" && Number(c.due_at) < data.evaluatedAt;
                      return (
                        <article id={text(c.id)} className={styles.review} key={text(c.id)}>
                          <div className={styles.sectionHeading}>
                            <h4>{text(c.name)}</h4>
                            <span className={styles.badge}>
                              {overdue ? "overdue" : text(c.status)}
                            </span>
                          </div>
                          <p>{text(c.reason)}</p>
                          <small>
                            {decided}/{items.length} certified · due {date(c.due_at)} · revision{" "}
                            {text(c.version)}
                          </small>
                          <details open={c.status === "open"}>
                            <summary>Review assignments ({pending} pending)</summary>
                            {items.map((item) => (
                              <div className={styles.item} key={text(item.id)} data-iam-review-item={text(item.id)}>
                                <div>
                                  <strong>
                                    {data.users.find((u) => u.id === item.user_id)?.name ??
                                      text(item.user_id)}
                                  </strong>
                                  <p>
                                    {text(item.role)} · {text(item.scope_kind)}
                                    {item.scope_node_id
                                      ? ` · ${data.nodes.find((n) => n.id === item.scope_node_id)?.display_name ?? text(item.scope_node_id)}`
                                      : ""}{" "}
                                    · {text(item.source)} · until {date(item.valid_until)}
                                  </p>
                                  <span className={styles.badge}>{text(item.decision)}</span>
                                  {Boolean(item.decision_reason) && (
                                    <small>{text(item.decision_reason)}</small>
                                  )}
                                </div>
                                {c.status === "open" &&
                                  item.decision === "pending" &&
                                  c.created_by !== data.currentUserId &&
                                  item.user_id !== data.currentUserId && (
                                    <form
                                      className={styles.form}
                                      onSubmit={(e) => {
                                        e.preventDefault();
                                        const f = new FormData(e.currentTarget);
                                        void send(`access-review-items/${item.id}/decide`, {
                                          expectedCampaignVersion: Number(c.version),
                                          decision: field(f, "decision"),
                                          reason: field(f, "reason"),
                                        });
                                      }}
                                    >
                                      <Field appearance="standard">
                                        Decision
                                        <Select appearance="standard" name="decision">
                                          <option value="keep">Keep access</option>
                                          <option value="revoke">Revoke immediately</option>
                                        </Select>
                                      </Field>
                                      {renderReason()}
                                      {submit("Record certification")}
                                    </form>
                                  )}
                              </div>
                            ))}
                          </details>
                          {c.status === "open" &&
                            c.created_by === data.currentUserId &&
                            decided === 0 && (
                              <form
                                className={styles.form}
                                onSubmit={(e) => {
                                  e.preventDefault();
                                  void send(`access-reviews/${c.id}/cancel`, {
                                    expectedVersion: Number(c.version),
                                    reason: field(new FormData(e.currentTarget), "reason"),
                                  });
                                }}
                              >
                                {renderReason()}
                                {submit("Cancel untouched campaign")}
                              </form>
                            )}
                        </article>
                      );
                    })
                  ) : (
                    <Empty>No access certification campaigns yet.</Empty>
                  )}
                </Card>
              )}
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Independent reviews</h3>
                <p>
                  Approval requires a different Person and MFA verified within the last five
                  minutes. Changes are revalidated before publication.
                </p>
                {data.reviews.length ? (
                  data.reviews.map((r) => (
                    <article className={styles.review} key={text(r.id)}>
                      <div className={styles.sectionHeading}>
                        <h4>{title(text(r.kind))}</h4>
                        <span className={styles.badge}>{text(r.status)}</span>
                      </div>
                      <p>{text(r.reason)}</p>
                      <small>
                        Proposed by{" "}
                        {data.users.find((u) => u.id === r.created_by)?.name ?? text(r.created_by)}{" "}
                        · {date(r.created_at)}
                      </small>
                      <details>
                        <summary>Review exact change and impact</summary>
                        <pre className={styles.evidence}>{JSON.stringify(r.payload, null, 2)}</pre>
                      </details>
                      <SanitizedIamDocuments
                        base={base}
                        targetType="iam_review"
                        targetId={text(r.id)}
                        version={Number(r.version)}
                        editable={can("iam.manage")}
                      />
                      {r.status === "pending" &&
                        can("iam.review") &&
                        r.created_by !== data.currentUserId && (
                          <form
                            className={styles.form}
                            onSubmit={(e) => {
                              e.preventDefault();
                              const f = new FormData(e.currentTarget);
                              void send(`reviews/${r.id}`, {
                                expectedVersion: r.version,
                                decision: field(f, "decision"),
                                reason: field(f, "reason"),
                              });
                            }}
                          >
                            <Field appearance="standard">
                              Decision
                              <Select appearance="standard" name="decision">
                                <option value="approve">Approve</option>
                                <option value="reject">Reject</option>
                              </Select>
                            </Field>
                            {renderReason()}
                            {submit("Record decision")}
                          </form>
                        )}
                    </article>
                  ))
                ) : (
                  <Empty>No reviews in this scope.</Empty>
                )}
              </Card>
              {can("iam.recovery") && companyScope && (
                <Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Protected emergency administrator</h3>
                  <p>
                    Designate an existing local owner with an enrolled authenticator before
                    migrating employees. Another authorized reviewer must approve.
                  </p>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      e.preventDefault();
                      void send("emergency", {
                        userId: field(new FormData(e.currentTarget), "userId"),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Local administrator
                      <Select appearance="standard" name="userId" required>
                        <option value="">Choose an administrator</option>
                        {data.users
                          .filter((u) =>
                            data.assignments.some(
                              (a) =>
                                a.user_id === u.id &&
                                ["holding_owner", "platform_admin"].includes(text(a.role)) &&
                                a.active,
                            ),
                          )
                          .map((u) => (
                            <option value={u.id} key={u.id}>
                              {u.name}
                            </option>
                          ))}
                      </Select>
                    </Field>
                    {submit("Request recovery designation")}
                  </form>
                </Card>
              )}
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Audit history</h3>
                {companyScope && (
                  <FilterBar
                    className={styles.filters}
                    onSubmit={(e) => {
                      e.preventDefault();
                      void loadAudit();
                    }}
                  >
                    <Field appearance="standard">
                      Search audit
                      <TextInput appearance="standard"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder="Action, actor ID or target ID"
                      />
                    </Field>
                    <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                      Search history
                    </Button>
                  </FilterBar>
                )}
                {auditRows.map((a) => (
                  <details className={styles.review} key={text(a.id)}>
                    <summary>
                      {title(text(a.action))} · {date(a.created_at)} ·{" "}
                      {data.users.find((u) => u.id === a.actor_id)?.name ?? text(a.actor_id)}
                    </summary>
                    <pre className={styles.evidence}>{JSON.stringify(a.evidence, null, 2)}</pre>
                  </details>
                ))}
                {!auditRows.length && <Empty>No matching IAM events.</Empty>}
                {companyScope && auditCursor && (
                  <Button appearance="standard"
                    type="submit"
                    variant="secondary"

                    disabled={busy}
                    onClick={() => loadAudit(true)}
                  >
                    Load older events
                  </Button>
                )}
              </Card>
            </>
          )}
          {section === "my-access" && (
            <Card as="section" appearance="standard" className={styles.panel}>
              <h3>Authenticator and sessions</h3>
              <p>
                Verify a six-digit authenticator code before approving or changing sensitive access.
              </p>
              {!data.mfaEnabled && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

                  disabled={busy}
                  onClick={() => send("mfa/enroll", {}, true)}
                >
                  Set up authenticator
                </Button>
              )}
              {secret && (
                <div className={styles.impact}>
                  <p>
                    Add an account in your authenticator using this setup key, then verify its code:
                  </p>
                  <code>{secret}</code>
                </div>
              )}
              <form
                className={styles.form}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send(
                    "mfa/verify",
                    { code: field(new FormData(e.currentTarget), "code") },
                    true,
                  );
                }}
              >
                <Field appearance="standard">
                  Authenticator code
                  <TextInput appearance="standard"
                    name="code"
                    autoComplete="one-time-code"
                    inputMode="numeric"
                    pattern="[0-9]{6}"
                    required
                    maxLength={6}
                  />
                </Field>
                {submit("Verify MFA")}
              </form>
              <h4>Your sessions</h4>
              {data.ownSessions.map((s) => (
                <article className={styles.item} key={text(s.id)}>
                  <div>
                    <strong>
                      {s.id === data.currentSessionId ? "Current session" : "Signed-in session"}
                    </strong>
                    <p>{text(s.userAgent) || "Unknown device"}</p>
                    <small>Expires {text(s.expiresAt)}</small>
                  </div>
                  <Button appearance="standard"
                    type="submit"
                    variant="secondary"

                    disabled={busy}
                    onClick={() => send("sessions/revoke", { id: s.id }, true)}
                  >
                    Revoke session
                  </Button>
                </article>
              ))}
            </Card>
          )}
        </FeatureWidget>
      </WidgetGrid>
    </section>
  );
}
