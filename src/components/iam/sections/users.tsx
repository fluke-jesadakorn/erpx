"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { CollectionRecordLink, RecordCollectionDrawer, RecordFacts } from "@/components/records/record-collection";
import { WorkspaceCollection, WorkspaceDetail, WorkspaceRecordList, type WorkspaceRecord } from "@/components/ui/workspace";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import type { ReactNode } from "react";
import { Card } from "@/components/ui/foundation";
import { InlineNotice } from "@/components/ui/foundation";
import { Button, Field, Select, Table, TableFrame, TextArea, TextInput } from "@/components/ui/foundation";
import { FeatureTabBar, FeatureWidget, WidgetGrid } from "@/components/shared/features/feature-page";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { directoryProviderLabels, iamSections, type AccessDecision, type IamSection, type RoleRevision } from "@/lib/identity_access/iam";
import { evaluateAccess } from "@/lib/identity_access/permissions";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import type { iamOverview } from "@/server/modules/identity_access/public/iam-service";
import { Check, CircleHelp, Minus, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import { RecordLink as Link } from "@/components/records/record-link";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useRouter } from "@/lib/shared/destination-router";
import { useEffect, useMemo, useRef, useState } from "react";
import styles from "@/components/iam/iam.module.css";
type Data = Awaited<ReturnType<typeof iamOverview>>;
type Row = Record<string, unknown>;
const text = (v: unknown) => String(v ?? "");
const date = (v: unknown) => (v ? <TimeText appearance="standard" value={Number(v)}/> : "—");
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
function SanitizedIamDocuments({ base, targetType, targetId, version, editable, }: {
    base: string;
    targetType: "iam_review" | "iam_migration";
    targetId: string;
    version: number;
    editable: boolean;
}) {
    const [open, setOpen] = useState(false), review = targetType === "iam_review";
    return (<AttachmentFeatureSurface>
      <details className={styles.review} onToggle={(event) => setOpen(event.currentTarget.open)}>
        <summary>{review ? "Sanitized review evidence" : "Sanitized migration evidence"}</summary>
        {open && (<RecordDocumentPanel appearance="standard" base={base} targetType={targetType} targetId={targetId} expectedTargetVersion={version} title={review ? "Sanitized review evidence" : "Sanitized migration evidence"} purposes={[
                {
                    key: review ? "sanitized_review_evidence" : "migration_evidence",
                    label: review ? "Sanitized review evidence" : "Sanitized migration evidence",
                    description: "Attach only a redacted review artifact. Never upload passwords, private keys, secrets, configuration exports, or database dumps.",
                    visibility: "Restricted to Company IAM administrators. Sanitization attestation is required before binding.",
                },
            ]} editable={editable}/>)}
      </details>
    </AttachmentFeatureSurface>);
}
const labels = {
    allowed: "Allowed",
    denied: "Denied",
    conditional: "Conditional",
    unavailable: "Unavailable",
};
function Decision({ decision, onClick }: {
    decision: AccessDecision;
    onClick: () => void;
}) {
    const Icon = symbols[decision.result];
    return (<Button appearance="standard" type="submit" variant="ghost" className={styles.decision} data-result={decision.result} onClick={onClick}>
      <Icon size={15} aria-hidden/>
      <span>{labels[decision.result]}</span>
    </Button>);
}
function Empty({ children }: {
    children: React.ReactNode;
}) {
    return <div className={styles.empty}>{children}</div>;
}
function IamSurface({ flat, pageKey, title, id, children }: {
    flat: boolean;
    pageKey?: string;
    title: string;
    id: string;
    children: ReactNode;
}) {
    return flat ? <section>{children}</section> : <WidgetGrid pageKey={pageKey}><FeatureWidget appearance="standard" id={id} title={title}>{children}</FeatureWidget></WidgetGrid>;
}
function IamRecords({ workspace, records, selected, onSelect, title, editorOpen, children }: {
    workspace: boolean;
    records: WorkspaceRecord[];
    selected: string | null;
    onSelect: (id: string | null) => void;
    title: string;
    editorOpen: boolean;
    children: ReactNode;
}) {
    if (!workspace)
        return <>{children}</>;
    const open = !!selected && !editorOpen;
    return <WorkspaceCollection detailOpen={open}><WorkspaceRecordList items={records} selected={selected} onSelect={onSelect} label={title}/><WorkspaceDetail open={open} title={records.find(record => record.id === selected)?.title ?? title} onClose={() => onSelect(null)}><div className={styles.workspaceDetails}>{children}</div></WorkspaceDetail></WorkspaceCollection>;
}
export function IamConsole({ data, base, api, section, embedded = false, presentation = "standalone", }: {
    presentation?: "standalone" | "workspace";
    embedded?: boolean;
    data: Data;
    base: string;
    api: string;
    section: IamSection;
}) {
    const queryParams = useSearchParams();
    const workspace = presentation === "workspace";
    const selectedRole = queryParams.get("role");
    const selectedDirectoryConnection = queryParams.get("connection");
    const [roleEditorOpen, setRoleEditorOpen] = useState(false);
    const [connectionEditorOpen, setConnectionEditorOpen] = useState(false);
    function selectRecord(key: string, id: string | null) {
        const params = new URLSearchParams(queryParams);
        if (id)
            params.set(key, id);
        else
            params.delete(key);
        window.history.pushState(null, "", `${window.location.pathname}?${params}`);
    }
    const detailRef = useRef<HTMLElement>(null);
    const router = useRouter(), [busy, setBusy] = useState(false), [message, setMessage] = useState(""), [error, setError] = useState(""), [query, setQuery] = useState(""), [module, setModule] = useState(""), [provider, setProvider] = useState(""), [status, setStatus] = useState(""), [scope, setScope] = useState(""), [matrixMode, setMatrixMode] = useState<"users" | "roles">("users"), [selected, setSelected] = useState<string[]>([data.currentAssignmentId]), [detail, setDetail] = useState<AccessDecision | null>(null), [editing, setEditing] = useState<RoleRevision | null>(null), [chosenPermissions, setChosenPermissions] = useState<string[]>([]), [chosenGrantable, setChosenGrantable] = useState<string[]>([]), [directoryProvider, setDirectoryProvider] = useState<keyof typeof directoryProviderLabels>("active_directory"), [method, setMethod] = useState("ldaps"), [chosenConnection, setChosenConnection] = useState(""), [identitySelection, setIdentitySelection] = useState<string[]>([]), [secret, setSecret] = useState(""), [auditResult, setAuditResult] = useState<{
        items: Row[];
        nextCursor: string | null;
    } | null>(null), [inspector, setInspector] = useState<unknown>(null), [editingConnection, setEditingConnection] = useState<Row | null>(null);
    const [roleEditorSession, setRoleEditorSession] = useState(0);
    const [connectionEditorSession, setConnectionEditorSession] = useState(0);
    useEffect(() => {
        if (detail)
            detailRef.current?.focus();
    }, [detail]);
    const can = (p: string) => data.permissions.includes(data.platform ? `platform.${p}` : p), companyScope = ["holding", "platform"].includes(data.scopeKind), end = Math.min(data.evaluatedAt + 90 * 86400000, data.validUntil);
    async function send(path: string, payload: unknown, account = false) {
        const submittingForm = document.activeElement instanceof HTMLElement ? document.activeElement.closest("form") : null;
        const formEditVersion = submittingForm?.getAttribute("data-erpx-edit-version");
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
            if (!response.ok)
                throw new Error(body.error?.message ?? "The operation failed.");
            if (body.data.secret)
                setSecret(body.data.secret);
            if (path === "mfa/verify")
                setSecret("");
            if (path === "connections")
                setEditingConnection(null);
            setMessage(body.data.message ??
                (body.data.status === "pending" ? "Submitted for independent review." : "Saved."));
            setAuditResult(null);
            if (submittingForm && formEditVersion)
                window.dispatchEvent(new CustomEvent("erpx:record-command-saved", { detail: { form: submittingForm, formEditVersion } }));
            router.refresh();
            return body.data;
        }
        catch (e) {
            setError(e instanceof Error ? e.message : "The operation failed.");
            return null;
        }
        finally {
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
            if (append && auditCursor)
                q.set("cursor", auditCursor);
            const response = await workFetch(`${api}/audit?${q}`), body = await response.json();
            if (!response.ok)
                throw new Error(body.error?.message ?? "Audit history is unavailable.");
            setAuditResult({
                items: append ? [...auditRows, ...body.data.items] : body.data.items,
                nextCursor: body.data.nextCursor,
            });
        }
        catch (e) {
            setError(e instanceof Error ? e.message : "Audit history is unavailable.");
        }
        finally {
            setBusy(false);
        }
    }
    const filteredAssignments = data.assignments.filter((a) => (!query ||
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
        (!scope || a.scope_node_id === scope || (scope === "holding" && a.scope_kind === "holding")));
    const matrixColumns = matrixMode === "users"
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
            decisions: data.catalog.map((p) => evaluateAccess({
                role: r.id,
                permissions: r.permissions,
                policyRevision: r.revision,
            }, p.id, { inspect: true })),
        }));
    const catalog = data.catalog.filter((p) => (!module || p.module === module) &&
        (!query || `${p.label} ${p.id}`.toLowerCase().includes(query.toLowerCase())));
    const affected = editing ? data.assignments.filter((a) => a.role === editing.id && a.active) : [];
    const added = chosenPermissions.filter((p) => !editing?.permissions.includes(p)), removed = editing?.permissions.filter((p) => !chosenPermissions.includes(p)) ?? [];
    const connection = data.connections.find((c) => c.id === chosenConnection);
    const filteredIdentities = data.identities.filter((i) => i.connection_id === chosenConnection && i.enabled && !i.user_id);
    const selectedIdentities = filteredIdentities.filter((i) => identitySelection.includes(text(i.id)));
    const activeCount = data.assignments.filter((a) => a.active).length;
    const pageTitle = iamSections.find(([id]) => id === section)?.[1] ?? "Identity & access";
    const userOptions = useMemo(() => data.users.filter((u) => u.id !== data.currentUserId), [data.users, data.currentUserId]);
    function renderRoleSelect() {
        return (<Field appearance="standard">

        Role
        <Select appearance="standard" name="roleId" required>
          <option value="">Choose a role</option>
          {data.roles
                .filter((r) => [...r.permissions, ...r.grantablePermissions].every((p) => data.grantablePermissions.includes(p)))
                .map((r) => (<option key={r.id} value={r.id}>
                {r.name}
              </option>))}
        </Select>
      </Field>);
    }
    function renderScopeFields() {
        return (<>
        <Field appearance="standard">
          Scope
          <Select appearance="standard" name="scopeKind" defaultValue={data.scopeKind}>
            {(data.platform
                ? ["platform"]
                : companyScope
                    ? ["holding", "branch", "department"]
                    : data.scopeKind === "branch"
                        ? ["branch", "department"]
                        : ["department"]).map((k) => (<option key={k}>{k}</option>))}
          </Select>
        </Field>
        {!data.platform && (<Field appearance="standard">
            Branch or department
            <Select appearance="standard" name="scopeNodeId">
              <option value="">Company scope</option>
              {data.nodes
                    .filter((n) => n.kind !== "company")
                    .map((n) => (<option key={text(n.id)} value={text(n.id)}>
                    {text(n.display_name)} · {text(n.kind)}
                  </option>))}
            </Select>
          </Field>)}
        <Field appearance="standard">
          Valid until
          <TextInput appearance="standard" type="date" name="validUntil" required defaultValue={day(end)} max={day(data.validUntil)} min={day(data.evaluatedAt + 86400000)}/>
        </Field>
      </>);
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
        return (<Field appearance="standard" className={styles.wide}>
        Reason
        <TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000} placeholder="Explain the business need and requested scope."/>
      </Field>);
    }
    function submit(label = "Submit for review") {
        return (<Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
        {busy ? "Working…" : label}
      </Button>);
    }
    const filters = (<div className={styles.filters}>
      <Field appearance="standard">
        <Search size={15} aria-hidden/>
        Search
        <TextInput appearance="standard" value={query} onChange={(e) => setQuery(e.target.value)} placeholder={"Name or role"}/>
      </Field>
      <Field appearance="standard">
        Provider
        <Select appearance="standard" value={provider} onChange={(e) => setProvider(e.target.value)}>
          <option value="">All providers</option>
          <option value="local">Local accounts</option>
          {data.connections.map((c) => (<option key={text(c.id)} value={text(c.id)}>
              {text(c.name)}
            </option>))}
        </Select>
      </Field>
      <Field appearance="standard">
        Scope
        <Select appearance="standard" value={scope} onChange={(e) => setScope(e.target.value)}>
          <option value="">All authorized scopes</option>
          <option value="holding">Company</option>
          {data.nodes
            .filter((n) => n.kind !== "company")
            .map((n) => (<option key={text(n.id)} value={text(n.id)}>
                {text(n.display_name)}
              </option>))}
        </Select>
      </Field>
      <Field appearance="standard">
        Assignment status
        <Select appearance="standard" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">All statuses</option>
          {["active", "expired", "revoked"].map((s) => (<option key={s}>{s}</option>))}
        </Select>
      </Field>
    </div>);
    return (<section className={styles.console}>
      <RecordCollectionDrawer<Data> kind="iam_assignment" title="Work assignment" endpoint={api} select={(snapshot, id) => snapshot.assignments.find(row => text(row.id) === id) ?? null} render={(record, snapshot) => <><p>{snapshot.users.find(user => user.id === record.user_id)?.name}</p><RecordFacts record={record}/>{can("iam.manage") && !!record.active && record.user_id !== snapshot.currentUserId && <Button appearance="standard" variant="secondary" disabled={busy} onClick={() => send(`assignments/${record.id}/revoke`, { expectedVersion: record.version, reason: "Revoked from IAM user administration" })}>Revoke</Button>}</>}/>
      <RecordCollectionDrawer<Data> kind="iam_group" title="Directory group" endpoint={api} select={(snapshot, id) => snapshot.groups.find(row => `${text(row.connection_id)}:${text(row.external_id)}` === id) ?? null}/>
      {!embedded && (<header className={styles.header}>
          <div>
            <p className={styles.eyebrow}>
              {data.platform ? "Platform" : "Company"} administration
            </p>
            <h1>Identity & access</h1>
            <p>Understand who can do what, and why.</p>
          </div>
          <span className={styles.scope}>
            <ShieldCheck size={17}/>
            {title(data.scopeKind)} scope
          </span>
        </header>)}
      {!embedded && (<FeatureTabBar appearance="standard" aria-label="Identity and access">
          {iamSections
                .filter(([id]) => id === "my-access" ||
                (can("iam.read") &&
                    ((id !== "connections" && id !== "groups") || can("iam.directory.manage"))))
                .map(([id, label]) => (<Link href={canonicalUiHref(`${base}/iam/${id}`)} key={id} aria-current={id === section ? "page" : undefined}>
                {label}
              </Link>))}
        </FeatureTabBar>)}
      <div aria-live="polite">{message && <p className={styles.success}>{message}</p>}</div>
      {error && (<InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>)}
      <IamSurface flat={presentation === "workspace"} pageKey={base === "/platform" ? `platform-iam-${section}` : undefined} id={`iam-${section}`} title={pageTitle}>
          <div className={styles.sectionHeading}>
            <h2>{pageTitle}</h2>
            <Button appearance="standard" type="submit" variant="secondary" onClick={() => router.refresh()} disabled={busy}>
              <RefreshCw size={14}/>
              Refresh
            </Button>
          </div>
          {false}
          {(<>
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
                      {filteredAssignments.map((a) => (<tr key={text(a.id)}>
                          <td>
                            <strong>
                              <CollectionRecordLink kind="iam_assignment" id={text(a.id)}>{data.users.find((u) => u.id === a.user_id)?.name ?? text(a.user_id)}</CollectionRecordLink>
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
                              {text(data.nodes.find((n) => n.id === a.scope_node_id)?.display_name) || "Company"}
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
                            {can("iam.manage") && a.active && a.user_id !== data.currentUserId && (<Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => send(`assignments/${a.id}/revoke`, {
                        expectedVersion: a.version,
                        reason: "Revoked from IAM user administration",
                    })}>
                                Revoke
                              </Button>)}
                            {can("iam.manage") &&
                    companyScope &&
                    a.user_id !== data.currentUserId && (<Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => send(`users/${a.user_id}/sessions`, {})}>
                                  Revoke sessions
                                </Button>)}
                            {can("iam.manage") &&
                    companyScope &&
                    a.identity_status === "suspended" && (<Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => send(`users/${a.user_id}/restore`, {
                        expectedVersion: a.identity_version,
                        reason: "Request restoration of suspended company access",
                    })}>
                                  Request restoration
                                </Button>)}
                          </td>
                        </tr>))}
                    </tbody>
                  </Table>
                </TableFrame>
                {!filteredAssignments.length && <Empty>No assignments match the filters.</Empty>}
              </Card>
              {can("iam.manage") && (<Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Grant or renew access</h3>
                  <p>
                    Renewal creates a new assignment. An independent reviewer approves every grant.
                    Choose the permissions deliberately; grants expire within your authority.
                  </p>
                  <FormDisclosure appearance="standard" drawerTitle="Grant or renew access" error={error}><summary>Grant or renew access</summary><form className={styles.form} onSubmit={(e) => {
                    e.preventDefault();
                    const f = new FormData(e.currentTarget);
                    void send("assignments", {
                        ...grantFields(f),
                        userId: field(f, "userId"),
                    });
                }}>
                    <Field appearance="standard">
                      User
                      <Select appearance="standard" name="userId" required>
                        <option value="">Choose an account</option>
                        {userOptions.map((u) => (<option key={u.id} value={u.id}>
                            {u.name} ·{" "}
                            {data.people.find((p) => p.user_id === u.id)?.legal_name ?? u.email}
                          </option>))}
                      </Select>
                    </Field>
                    {renderRoleSelect()}
                    {renderScopeFields()}
                    {renderReason()}
                    {submit()}
                  </form></FormDisclosure>
                </Card>)}
              {can("iam.manage") && companyScope && (<Card as="section" appearance="standard" className={styles.panel}>
                  <h3>Suspend company access</h3>
                  <p>
                    Suspension immediately invalidates this company’s sessions and grants. It
                    preserves employee history.
                  </p>
                  <FormDisclosure appearance="standard" drawerTitle="Suspend company access" error={error}><summary>Suspend company access</summary><form className={styles.form} onSubmit={(e) => {
                    e.preventDefault();
                    const f = new FormData(e.currentTarget), id = field(f, "userId");
                    void send(`users/${id}/suspend`, {
                        expectedVersion: Number(data.assignments.find((a) => a.user_id === id)?.identity_version ?? 1),
                        reason: field(f, "reason"),
                    });
                }}>
                    <Field appearance="standard">
                      User
                      <Select appearance="standard" name="userId" required>
                        {userOptions.map((u) => (<option key={u.id} value={u.id}>
                            {u.name}
                          </option>))}
                      </Select>
                    </Field>
                    {renderReason()}
                    {submit("Suspend access")}
                  </form></FormDisclosure>
                </Card>)}
            </>)}
          {false}
          {false}
          {false}
          {false}
          {false}
          {false}
      </IamSurface>
    </section>);
}
