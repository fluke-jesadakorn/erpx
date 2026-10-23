"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordCollectionDrawer, RecordFacts } from "@erpx/feature-records/components/record-collection";
import { WorkspaceCollection, WorkspaceDetail, WorkspaceRecordList, type WorkspaceRecord } from "@erpx/ui/components/workspace";
import type { ReactNode } from "react";
import { Card } from "@erpx/ui/components/foundation";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";
import { FeatureTabBar, FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { RecordDocumentPanel } from "@erpx/feature-files/components/record-document-panel";
import { AttachmentFeatureSurface } from "@erpx/feature-files/components/attachment-feature-context";
import { directoryProviderLabels, iamSections, type AccessDecision, type IamSection, type RoleRevision } from "@erpx/feature-identity_access/lib/iam";
import { evaluateAccess } from "@erpx/feature-identity_access/lib/permissions";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";

import type {iamOverview} from '@erpx/server-identity_access/public/iam-service-reads';
import { Check, CircleHelp, KeyRound, Minus, Network, RefreshCw, Search, ShieldCheck, Users, X } from "lucide-react";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { useEffect, useMemo, useRef, useState } from "react";
import styles from "@erpx/feature-iam/components/iam.module.css";
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
          {(<>
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
                    data.assignments.filter((a) => a.active && Number(a.valid_until) < data.evaluatedAt + 14 * 86400000).length,
                ],
            ].map(([Icon, label, value]) => {
                const I = Icon as typeof Users;
                return (<section className={styles.metric} key={String(label)}>
                      <I size={21}/>
                      <strong>{String(value)}</strong>
                      <span>{String(label)}</span>
                    </section>);
            })}
              </div>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Authentication sources</h3>
                <p>
                  Local database accounts remain available for employees who have not migrated. Each
                  approved directory connects only to this company.
                </p>
                {!data.connections.length ? (<Empty>
                    No directory connections configured. ERPX is operating with local accounts.
                  </Empty>) : (data.connections.map((c) => (<article className={styles.item} key={text(c.id)}>
                      <div>
                        <strong>{text(c.name)}</strong>
                        <p>
                          {directoryProviderLabels[c.provider as keyof typeof directoryProviderLabels]}{" "}
                          · {text(c.status)}
                        </p>
                      </div>
                      <div>
                        <span className={styles.badge}>{text(c.health)}</span>
                        <p>Last sync: {date(c.last_sync_at)}</p>
                        {!!c.last_error && <p className="error">{title(text(c.last_error))}</p>}
                      </div>
                    </article>)))}
              </Card>
              <Card as="section" appearance="standard" className={styles.panel}>
                <h3>Access that needs attention</h3>
                {data.assignments
                .filter((a) => a.active && Number(a.valid_until) < data.evaluatedAt + 14 * 86400000)
                .map((a) => (<p key={text(a.id)}>
                      {data.users.find((u) => u.id === a.user_id)?.name} · {title(text(a.role))} ·
                      expires {date(a.valid_until)}
                    </p>))}
                <Link href={canonicalUiHref(`${base}/iam/reviews`)}>Open reviews & audit →</Link>
              </Card>
            </>)}
          {false}
          {false}
          {false}
          {false}
          {false}
          {false}
          {false}
      </IamSurface>
    </section>);
}
