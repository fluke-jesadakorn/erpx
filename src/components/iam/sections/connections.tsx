"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordCollectionDrawer, RecordFacts } from "@/components/records/record-collection";
import { WorkspaceCollection, WorkspaceDetail, WorkspaceRecordList, type WorkspaceRecord } from "@/components/ui/workspace";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import type { ReactNode } from "react";
import { Card } from "@/components/ui/foundation";
import { InlineNotice } from "@/components/ui/foundation";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";
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
          {false}
          {false}
          {false}
          {can("iam.directory.manage") && companyScope && (<>
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
            ].map((s) => (<li key={s}>{s}</li>))}
                </ol>
                <p>
                  Saving a configuration keeps it inactive. Credentials and trusted certificates
                  come from the external secret profile configured on the ERPX server.
                </p>
              </Card>
              <IamRecords workspace={workspace} records={data.connections.map(connection => ({ id: text(connection.id), title: text(connection.name), description: `${directoryProviderLabels[connection.provider as keyof typeof directoryProviderLabels]} · ${text(connection.status)} · ${text(connection.health)}` }))} selected={selectedDirectoryConnection} onSelect={id => selectRecord("connection", id)} title="Directory connections" editorOpen={connectionEditorOpen}>
              {data.connections.filter(connection => !workspace || text(connection.id) === selectedDirectoryConnection).map((c) => (<Card as="section" appearance="standard" className={styles.panel} key={text(c.id)}>
                  <div className={styles.sectionHeading}>
                    <div>
                      {!workspace && <h3>{text(c.name)}</h3>}
                      <p>
                        {directoryProviderLabels[c.provider as keyof typeof directoryProviderLabels]}{" "}
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
                    {c.status !== "active" && (<Button appearance="standard" type="submit" variant="secondary" onClick={() => {
                        setEditingConnection(c);
                        setConnectionEditorSession(value => value + 1);
                        setConnectionEditorOpen(true);
                        setDirectoryProvider(c.provider as keyof typeof directoryProviderLabels);
                        setMethod(text((c.config as Row).mode));
                    }}>
                        Edit configuration
                      </Button>)}
                    <Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => send(`connections/${c.id}/test`, {
                    expectedVersion: c.version,
                })}>
                      Test connection
                    </Button>
                    {c.status === "tested" && (<Button appearance="standard" type="submit" variant="primary" disabled={busy} onClick={() => send(`connections/${c.id}/activate`, {
                        expectedVersion: c.version,
                    })}>
                        Request activation
                      </Button>)}
                    {c.status === "active" && (<>
                        <Button appearance="standard" type="submit" variant="secondary" disabled={busy} onClick={() => send(`connections/${c.id}/sync`, {
                        expectedVersion: c.version,
                    })}>
                          Synchronize now
                        </Button>
                      </>)}
                    {c.status !== "disabled" && can("iam.directory.manage") && companyScope && (<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => send(`connections/${c.id}/disable`, { expectedVersion: c.version })}>
                        Disable connection
                      </Button>)}
                  </div>
                  <details>
                    <summary>Preview users and groups</summary>
                    {data.previews
                    .filter((p) => p.connection_id === c.id)
                    .map((p) => (<div key={text(p.connection_id)}>
                          <p>
                            Read at {date(p.created_at)}. These identities have no access until
                            approved migration.
                          </p>
                          <ul>
                            {(p.snapshot_json as {
                        users: {
                            externalId: string;
                            name: string;
                            username: string;
                        }[];
                    }).users.map((u) => (<li key={u.externalId}>
                                {u.name} · {u.username}
                                <small>{u.externalId}</small>
                              </li>))}
                          </ul>
                        </div>))}
                  </details>
                  {c.status === "active" && (<p>
                      Company sign-in:{" "}
                      <Link href={`/login?connection=${c.id}`}>Open directory sign-in</Link>
                    </p>)}
                </Card>))}
              </IamRecords>
              {!data.connections.length && (<Empty>
                  No connections yet. Add AD, Entra ID, or Univention UCS when your company is
                  ready.
                </Empty>)}
              {can("iam.directory.manage") && companyScope && (<Card as="section" appearance="standard" className={styles.panel}>
                  <h3>
                    {editingConnection ? "Edit inactive connection" : "Add directory connection"}
                  </h3>
                  {editingConnection && (<Button appearance="standard" type="submit" variant="secondary" onClick={() => setEditingConnection(null)}>
                      Cancel editing
                    </Button>)}
                  <FormDisclosure key={connectionEditorSession} open={connectionEditorSession > 0} onToggle={event => setConnectionEditorOpen(event.currentTarget.open)} appearance="standard" drawerTitle="Configure directory connection" error={error}><summary>Configure directory connection</summary><form key={text(editingConnection?.id) || "new-connection"} className={styles.form} onSubmit={(e) => {
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
                        if (field(f, k))
                            config[k] = field(f, k);
                    void send("connections", {
                        id: editingConnection?.id,
                        expectedVersion: Number(editingConnection?.version ?? 0),
                        name: field(f, "name"),
                        provider: directoryProvider,
                        config,
                    });
                }}>
                    <Field appearance="standard">
                      Name
                      <TextInput appearance="standard" name="name" required placeholder="Corporate directory" defaultValue={text(editingConnection?.name)}/>
                    </Field>
                    <Field appearance="standard">
                      Provider
                      <Select appearance="standard" disabled={!!editingConnection} value={directoryProvider} onChange={(e) => {
                    const p = e.target.value as typeof directoryProvider;
                    setDirectoryProvider(p);
                    setMethod(p === "entra_id" ? "oidc" : "ldaps");
                }}>
                        {Object.entries(directoryProviderLabels).map(([id, label]) => (<option key={id} value={id}>
                            {label}
                          </option>))}
                      </Select>
                    </Field>
                    {directoryProvider === "univention" && (<Field appearance="standard">
                        UCS sign-in method
                        <Select appearance="standard" value={method} onChange={(e) => setMethod(e.target.value)}>
                          <option value="ldaps">Certificate-validated LDAPS</option>
                          <option value="oidc">Keycloak OIDC</option>
                        </Select>
                      </Field>)}
                    <Field appearance="standard">
                      Directory authority
                      <TextInput appearance="standard" name="authority" defaultValue={text((editingConnection?.config as Row | undefined)?.authority)} required placeholder="corp.example.com"/>
                    </Field>
                    <Field appearance="standard">
                      External secret profile
                      <TextInput appearance="standard" name="secretProfile" defaultValue={text((editingConnection?.config as Row | undefined)?.secretProfile)} required pattern="[A-Za-z0-9_-]+" placeholder="corp-identity"/>
                    </Field>
                    {directoryProvider !== "entra_id" && (<>
                        <Field appearance="standard">
                          LDAPS URL
                          <TextInput appearance="standard" name="ldapUrl" defaultValue={text((editingConnection?.config as Row | undefined)?.ldapUrl)} type="url" required placeholder="ldaps://directory.example.com:636"/>
                        </Field>
                        <Field appearance="standard">
                          Base DN
                          <TextInput appearance="standard" name="baseDn" defaultValue={text((editingConnection?.config as Row | undefined)?.baseDn)} required placeholder="dc=example,dc=com"/>
                        </Field>
                        <Field appearance="standard">
                          Group base DN (optional)
                          <TextInput appearance="standard" name="groupBaseDn" defaultValue={text((editingConnection?.config as Row | undefined)?.groupBaseDn)}/>
                        </Field>
                        <Field appearance="standard">
                          User filter (optional)
                          <TextInput appearance="standard" name="userFilter" defaultValue={text((editingConnection?.config as Row | undefined)?.userFilter)} placeholder="Provider default"/>
                        </Field>
                      </>)}
                    {(directoryProvider === "entra_id" || method === "oidc") && (<>
                        <Field appearance="standard">
                          Client ID
                          <TextInput appearance="standard" name="clientId" defaultValue={text((editingConnection?.config as Row | undefined)?.clientId)} required/>
                        </Field>
                        {directoryProvider === "entra_id" ? (<Field appearance="standard">
                            Entra tenant ID
                            <TextInput appearance="standard" name="tenantId" defaultValue={text((editingConnection?.config as Row | undefined)?.tenantId)} required placeholder="Tenant UUID"/>
                          </Field>) : (<Field appearance="standard">
                            Keycloak issuer
                            <TextInput appearance="standard" name="issuer" defaultValue={text((editingConnection?.config as Row | undefined)?.issuer)} type="url" required placeholder="https://sso.example.com/realms/ucs"/>
                          </Field>)}
                      </>)}
                    {submit("Save inactive connection")}
                  </form></FormDisclosure>
                </Card>)}
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Selective employee migration</h3>
                <p>
                  Select exact identities, link each to its employee record, and propose replacement
                  grants. Employees outside the saved batch keep their existing login. Group
                  membership alone never migrates an employee.
                </p>
                <Field appearance="standard">
                  Active directory
                  <Select appearance="standard" value={chosenConnection} onChange={(e) => {
                setChosenConnection(e.target.value);
                setIdentitySelection([]);
            }}>
                    <option value="">Choose a connection</option>
                    {data.connections
                .filter((c) => c.status === "active")
                .map((c) => (<option value={text(c.id)} key={text(c.id)}>
                          {text(c.name)}
                        </option>))}
                  </Select>
                </Field>
                {!!connection && (<>
                    <Field appearance="standard">
                      Select current group members
                      <Select appearance="standard" defaultValue="" onChange={(e) => setIdentitySelection(filteredIdentities
                    .filter((i) => (i.groups_json as string[]).includes(e.target.value))
                    .map((i) => text(i.id)))}>
                        <option value="">Choose a group to preview</option>
                        {data.groups
                    .filter((g) => g.connection_id === chosenConnection)
                    .map((g) => (<option key={text(g.external_id)} value={text(g.external_id)}>
                              {text(g.name)}
                            </option>))}
                      </Select>
                    </Field>
                    <fieldset className={styles.selection}>
                      <legend>Explicit identities in this batch</legend>
                      {filteredIdentities.map((i) => (<Field appearance="standard" orientation="inline" className={styles.check} key={text(i.id)}>
                          <TextInput appearance="standard" type="checkbox" checked={identitySelection.includes(text(i.id))} onChange={(e) => setIdentitySelection((s) => e.target.checked ? [...s, text(i.id)] : s.filter((v) => v !== i.id))}/>
                          {text(i.name)} · {text(i.username)}
                          <small>{text(i.external_id)}</small>
                        </Field>))}
                    </fieldset>
                    {!filteredIdentities.length && (<Empty>Synchronize this connection to preview unlinked identities.</Empty>)}
                  </>)}
                {selectedIdentities.length > 0 && can("iam.manage") && (<FormDisclosure appearance="standard" drawerTitle="Selective employee migration" error={error}><summary>Selective employee migration</summary><form className={styles.form} onSubmit={(e) => {
                    e.preventDefault();
                    const f = new FormData(e.currentTarget), grants = [grantFields(f)];
                    void send("migrations", {
                        connectionId: chosenConnection,
                        reason: field(f, "reason"),
                        users: selectedIdentities.map((i) => {
                            const person = data.people.find((p) => p.id === field(f, `person-${i.id}`));
                            return {
                                identityId: i.id,
                                personId: person?.id,
                                localUserId: person?.user_id,
                                grants,
                            };
                        }),
                    });
                }}>
                    {selectedIdentities.map((i) => (<Field appearance="standard" key={text(i.id)}>
                        {text(i.name)} → Employee
                        <Select appearance="standard" name={`person-${i.id}`} required>
                          <option value="">Verify and select employee</option>
                          {data.people
                        .filter((p) => p.user_id && p.user_id !== data.currentUserId)
                        .map((p) => (<option key={p.id} value={p.id}>
                                {p.legal_name}
                              </option>))}
                        </Select>
                      </Field>))}
                    <p className={styles.wide}>
                      Explicit replacement grant for the {selectedIdentities.length} selected
                      employees:
                    </p>
                    {renderRoleSelect()}
                    {renderScopeFields()}
                    {renderReason()}
                    {submit("Prepare migration for review")}
                  </form></FormDisclosure>)}
                {data.migrations.map((b) => (<article className={styles.review} key={text(b.id)}>
                    <p>
                      Batch {text(b.id).slice(0, 8)} · {text(b.status)} ·{" "}
                      {(b.users_json as unknown[]).length} employees · {date(b.created_at)}
                    </p>
                    <SanitizedIamDocuments base={base} targetType="iam_migration" targetId={text(b.id)} version={Number(b.version)} editable={can("iam.manage")}/>
                  </article>))}
              </Card>
            </>)}
          {false}
          {false}
          {false}
      </IamSurface>
    </section>);
}
