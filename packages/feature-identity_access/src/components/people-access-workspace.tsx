"use client";
import { useEffect, useRef, useState, type FormEvent } from "react";
import {useSearchParams} from "@erpx/feature-shared/lib/destination-search";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { Button, Field, TextInput, Select, TextArea } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FormDrawer } from "@erpx/ui/components/dialog";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { PeopleAccessChange, PeopleAccessSnapshot, PeopleAccessPreview, PeopleRelationshipsSnapshot, AccessSubject } from "@erpx/feature-identity_access/lib/people-access";
import styles from "@erpx/feature-identity_access/components/people-access-workspace.module.css";

type Editor = { operation: PeopleAccessChange["operation"]; id?: string; subjectId?: string; assignmentId?: string; expectedVersion?: number };
const dateInput = (value: number | null | undefined) => value ? new Date(value - new Date(value).getTimezoneOffset() * 60000).toISOString().slice(0, 16) : "";
export function PeopleAccessWorkspace({ initial: supplied, api, base, classification, initialView = "users" }: { initial: PeopleAccessSnapshot | PeopleRelationshipsSnapshot; api: string; base: string; classification?: "external" | "unclassified"; initialView?: "users" | "organizations" | "teams" | "roles" }) {
  const relationships = "canManage" in supplied;
  const initial: PeopleAccessSnapshot = relationships ? { ...supplied, roles: [], permissions: [], assignments: [], canApply: false, authorizingUntil: 0 } : supplied;
  const canManage = relationships && supplied.canManage;
  const canEdit = relationships ? canManage : initial.canApply;
  const router = useRouter(), formRef = useRef<HTMLFormElement>(null);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 30000); return () => clearInterval(timer); }, []);
  const route = useSearchParams(), view = initialView;
  function updateRoute(values: Record<string, string | null>, replace = false) {
    const url = new URL(window.location.href), params = url.searchParams;
    for (const [key, value] of Object.entries(values)) { if (value) params.set(key, value); else params.delete(key); }
    window.history[replace ? "replaceState" : "pushState"](null, "", `${url.pathname}${params.size ? `?${params}` : ""}${url.hash}`);
  }
  const query = route.get("q") ?? "", kind = classification ?? (route.get("kind") === "internal" ? "employee" : route.get("kind")) ?? "all";
  const setQuery = (value: string) => updateRoute({ q: value }, true);
  const setKind = (value: string) => updateRoute({ kind: value, subject: null });
  const selected = route.get("subject") ?? route.get("user") ?? route.get("member") ?? route.get("person"), setSelected = (value: string | null) => updateRoute({ subject: value, user: null, member: null, person: null });
  const [editor, setEditor] = useState<Editor | null>(null), [preview, setPreview] = useState<PeopleAccessPreview | null>(null);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [notice, setNotice] = useState("");
  const [organization, setOrganization] = useState(""), [scopeKind, setScopeKind] = useState("department");
  const subject = initial.subjects.find(s => s.id === editor?.id || s.id === editor?.subjectId);
  const oldOrganization = initial.organizations.find(o => o.id === editor?.id), team = initial.teams.find(t => t.id === editor?.id), role = initial.roles.find(r => r.id === editor?.id);
  const selectedAccount = selected?.startsWith("account:") ? selected.slice("account:".length) : selected;
  const detail = initial.subjects.find(s => !!selected && (s.id === selected || s.userId === selectedAccount || !!selectedAccount && !!s.linkedUserIds?.includes(selectedAccount) || s.personId === selected) && (kind === "all" || (s.classification ?? (s.kind === "internal" ? "employee" : s.kind)) === kind));
  const matches = (name: string) => name.toLowerCase().includes(query.toLowerCase());
  const externalName = (s: AccessSubject) => initial.organizations.find(o => o.id === s.organizationId)?.name ?? "No organization";
  const matchesSubject = (s: AccessSubject) => (kind === "all" || (s.classification ?? (s.kind === "internal" ? "employee" : s.kind)) === kind)
    && matches(`${s.name} ${s.email} ${s.kind === "external" ? externalName(s) : ""}`)
    && (!route.get("organization") || s.organizationId === route.get("organization"))
    && (!route.get("team") || s.teamId === route.get("team"))
    && (!route.get("status") || (s.expiresAt && s.expiresAt <= now ? "expired" : s.status) === route.get("status"));
  function open(next: Editor) { setEditor(next); setPreview(null); setError(""); setOrganization(initial.subjects.find(s => s.id === next.id)?.organizationId ?? initial.teams.find(t => t.id === next.id)?.organizationId ?? initial.organizations[0]?.id ?? ""); setScopeKind("department"); }
  async function post(path: string, payload: unknown, commandKey = crypto.randomUUID()) {
    const response = await workFetch(api + path, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": commandKey }, body: JSON.stringify(payload) });
    const body = await response.json(); if (!response.ok) throw new Error(body.error?.message ?? "The change could not be completed."); return body.data;
  }
  async function prepare(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); if (!editor) return;
    const f = new FormData(event.currentTarget), text = (key: string) => String(f.get(key) ?? ""), nullable = (key: string) => text(key) || null;
    const common = { id: editor.id, expectedVersion: editor.expectedVersion ?? 0, name: text("name"), reason: text("reason") };
    let change: unknown;
    switch (editor.operation) {
      case "account": change = { operation: "account", name: text("name"), email: text("email"), reason: text("reason") }; break;
      case "organization": change = { ...common, operation: "organization", kind: text("kind") }; break;
      case "team": change = { ...common, operation: "team", organizationId: text("organizationId"), departmentId: nullable("departmentId") }; break;
      case "subject": change = { ...common, operation: "subject", email: text("email"), organizationId: text("organizationId"), teamId: nullable("teamId"), sponsorPersonId: nullable("sponsorPersonId"), expiresAt: text("expiresAt") ? new Date(text("expiresAt")).getTime() : null, status: text("status") }; break;
      case "role": change = { ...common, operation: "role", permissions: f.getAll("permissions"), scopes: f.getAll("scopes") }; break;
      case "grant": case "invite": change = { operation: editor.operation, subjectId: editor.subjectId, roleId: text("roleId"), scopeKind: text("scopeKind"), scopeNodeId: text("scopeKind") === "holding" ? null : nullable("scopeNodeId"), validUntil: new Date(text("validUntil")).getTime(), reason: text("reason") }; break;
      case "revoke": change = { operation: "revoke", assignmentId: editor.assignmentId, expectedVersion: editor.expectedVersion, reason: text("reason") };
    }
    setPending(true); setError(""); try { setPreview(await post("/preview", { change })); } catch (e) { setError((e as Error).message); } finally { setPending(false); }
  }
  async function apply() {
    if (!preview) return; const form=formRef.current, formEditVersion=form?.getAttribute("data-erpx-edit-version"); setPending(true); setError("");
    try { const result = await post("/apply", { previewToken: preview.token }, `people-apply-${preview.token}`); if(form&&formEditVersion)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:{form,formEditVersion}})); setNotice(result.invitationId ? "Access authorized. The invitation is queued for delivery." : "Change applied."); setEditor(null); setPreview(null); router.refresh(); }
    catch (e) { setError((e as Error).message); } finally { setPending(false); }
  }
  const title = editor?.operation === "account" ? "Prepare account" : editor?.operation === "subject" ? editor.id ? "Edit external relationship" : "Add external collaborator" : editor?.operation === "organization" ? "External organization" : editor?.operation === "team" ? "External team" : editor?.operation === "role" ? "Custom role" : editor?.operation === "invite" ? "Invite user and enable access" : editor?.operation === "grant" ? "Assign access" : "Withdraw access";
  return <div className={styles.workspace}>
    {relationships && classification !== "unclassified" && <nav className={styles.tabs} aria-label="External collaboration views">{[["users", "Collaborators"], ["organizations", "External organizations"], ["teams", "External teams"]].map(([id, label]) => <Link key={id} href={canonicalUiHref(`${base}/people?view=external&section=${id}`)} aria-current={view === id ? "page" : undefined}>{label}</Link>)}</nav>}
    {!relationships && <p><Link href={canonicalUiHref(`${base}/technology?view=settings&section=roles`)}>Roles and permissions</Link> · <Link href={canonicalUiHref(`${base}/technology?view=resources&section=packages`)}>Access packages and position mappings</Link></p>}
    <div className={styles.toolbar}><TextInput appearance="standard" aria-label="Search people and accounts" placeholder="Search" value={query} onChange={e => setQuery(e.target.value)} />
      {view === "users" && !classification && <Select appearance="standard" aria-label="User type" value={kind} onChange={e => setKind(e.target.value)}><option value="all">All users</option><option value="employee">Employees</option><option value="external">External collaborators</option><option value="unclassified">Unclassified</option></Select>}
      {view === "users" && <Select appearance="standard" aria-label="Relationship status" value={route.get("status") ?? ""} onChange={e=>updateRoute({status:e.target.value})}><option value="">All statuses</option><option value="active">Active</option><option value="suspended">Suspended</option><option value="invited">Invited</option><option value="expired">Expired</option></Select>}
      {view === "users" && classification === "external" && <><Select appearance="standard" aria-label="Filter external organization" value={route.get("organization") ?? ""} onChange={e=>updateRoute({organization:e.target.value,team:null})}><option value="">All organizations</option>{initial.organizations.map(o=><option key={o.id} value={o.id}>{o.name}</option>)}</Select><Select appearance="standard" aria-label="Filter external team" value={route.get("team") ?? ""} onChange={e=>updateRoute({team:e.target.value})}><option value="">All teams</option>{initial.teams.filter(t=>!route.get("organization")||t.organizationId===route.get("organization")).map(t=><option key={t.id} value={t.id}>{t.name}</option>)}</Select></>}
      {canEdit && classification !== "unclassified" && <Button appearance="standard" onClick={() => open({ operation: !relationships ? "account" : view === "users" ? "subject" : view === "organizations" ? "organization" : "team" })}>{!relationships ? "Prepare account" : view === "users" ? "Add external collaborator" : view === "organizations" ? "Add organization" : "Add team"}</Button>}
    </div>
    {notice && <p role="status">{notice}</p>}
    <div className={styles.columns}><section className={styles.list} aria-label={view}>
      {view === "users" && initial.subjects.filter(matchesSubject).map(s => <button className={styles.row} key={s.id} onClick={() => setSelected(s.id)} aria-pressed={selected === s.id}><span><strong>{s.name}</strong><small>{s.email || "No email"}</small></span><span>{s.kind === "internal" ? "Employee" : s.kind === "unclassified" ? "Unclassified" : externalName(s)}<small>{s.expiresAt && s.expiresAt <= now ? "Expired" : s.status}{!s.userId ? " · No account" : ` · ${s.accountSource === "directory" ? "Directory" : "Local"} account`}</small></span></button>)}
      {view === "organizations" && initial.organizations.filter(o => matches(o.name)).map(o => <article className={styles.card} key={o.id}><strong>{o.name}</strong><p>{o.kind === "independent" ? "Independent contractor" : "External organization"} · {initial.subjects.filter(s => s.organizationId === o.id).length} users</p>{canManage && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "organization", id: o.id, expectedVersion: o.version })}>Edit</Button>}</article>)}
      {view === "teams" && initial.teams.filter(t => matches(t.name)).map(t => <article className={styles.card} key={t.id}><strong>{t.name}</strong><p>{initial.organizations.find(o => o.id === t.organizationId)?.name} · Works with: {initial.units.find(u => u.id === t.departmentId)?.name ?? "No department"}</p><p>{initial.subjects.filter(s => s.teamId === t.id).map(s => s.name).join(", ") || "No members"}</p><small>Team membership does not grant access.</small>{canManage && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "team", id: t.id, expectedVersion: t.version })}>Edit team</Button>}</article>)}
      {view === "roles" && initial.roles.filter(r => matches(r.name)).map(r => <article className={styles.card} key={r.id}><strong>{r.name}</strong><p>{r.permissions.length} permissions · {r.scopes.join(", ")}</p>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "role", id: r.builtIn ? undefined : r.id, expectedVersion: r.builtIn ? 0 : r.revision })}>{r.builtIn ? "Create custom role" : "Edit role"}</Button>}</article>)}
      {view === "users" && !initial.subjects.some(matchesSubject) && <p>No matching people or accounts.</p>}
      {!initial.organizations.length && view === "organizations" && <p>Add an external organization or an independent contractor affiliation.</p>}
      {!initial.teams.length && view === "teams" && <p>No external teams. Create an organization first, then a team.</p>}
    </section>
    </div>
    <FormDrawer appearance="standard" open={!!detail && !editor} onClose={() => setSelected(null)} title={detail?.name ?? "Person details"}>
      {detail && <div className={styles.card}><p>{detail.email}</p><p>{detail.kind === "external" ? externalName(detail) : detail.kind === "internal" ? "Employee" : "Unclassified account"}</p>
        {detail.kind === "unclassified" && <p>This account has no employee or external relationship in this Company.</p>}
        {detail.kind === "external" && <><p>Team: {initial.teams.find(t => t.id === detail.teamId)?.name ?? "Unassigned"}</p><p>Internal sponsor: {initial.subjects.find(s => s.personId === detail.sponsorPersonId && s.kind === "internal")?.name ?? "Not assigned"}</p><p>Relationship expiry: {detail.expiresAt ? new Date(detail.expiresAt).toLocaleString() : "Not recorded"}</p><p>Status: {detail.expiresAt && detail.expiresAt <= now ? "Expired" : detail.status}</p></>}
        {relationships && canManage && detail.kind !== "internal" && <Button appearance="standard" onClick={() => open({ operation: "subject", id: detail.id, expectedVersion: detail.version })}>{detail.kind === "unclassified" ? "Record external relationship" : "Edit relationship"}</Button>}
        {detail.personId && detail.kind === "internal" && <p><Link href={canonicalUiHref(`${base}/people/${encodeURIComponent(detail.personId)}`)}>Open employee profile</Link></p>}
        {relationships && !!detail.linkedUserIds?.length && <p><Link href={canonicalUiHref(`${base}/technology?view=resources&section=accounts&subject=${encodeURIComponent(detail.id)}`)}>Related accounts ({detail.linkedUserIds.length})</Link></p>}
        {!relationships && <><p>Account source: {detail.userId ? detail.accountSource === "directory" ? "Directory" : "Local" : "Awaiting invitation"}</p><p>Linked accounts: {detail.linkedUserIds?.length ?? (detail.userId ? 1 : 0)}</p>
          {detail.kind !== "internal" && <p><Link href={canonicalUiHref(`${base}/people?view=${detail.kind === "external" ? "external&section=users" : "unclassified"}&subject=${encodeURIComponent(detail.id)}`)}>Open person relationship</Link></p>}
          {initial.canApply && <Button appearance="standard" onClick={() => open({ operation: detail.userId ? "grant" : "invite", subjectId: detail.id })}>{detail.userId ? "Assign access" : "Invite user"}</Button>}
          <h3>Access assignments</h3>{initial.assignments.filter(a => detail.linkedUserIds?.includes(a.userId) || a.userId === detail.userId).map(a => <div className={styles.assignment} key={a.id}><strong>{initial.roles.find(r => r.id === a.roleId)?.name ?? a.roleId}</strong><p>{a.scopeKind}: {initial.units.find(u => u.id === a.scopeNodeId)?.name ?? "Company"} · {a.active ? "Active" : "Unavailable"}</p><small>Until {new Date(a.validUntil).toLocaleString()}</small>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "revoke", assignmentId: a.id, expectedVersion: a.version })}>Withdraw access</Button>}</div>)}
        </>}
      </div>}
    </FormDrawer>
    <FormDrawer appearance="standard" open={!!editor} onClose={() => { if (!pending) setEditor(null); }} title={title} dismissDisabled={pending}>
      {editor && <form ref={formRef} className={styles.form} key={`${editor.operation}:${editor.id ?? editor.subjectId ?? "new"}`} onSubmit={prepare} onChange={() => setPreview(null)}>
        {["account", "organization", "team", "subject", "role"].includes(editor.operation) && <Field appearance="standard" label="Name"><TextInput appearance="standard" name="name" required defaultValue={subject?.name ?? oldOrganization?.name ?? team?.name ?? role?.name ?? ""} /></Field>}
        {editor.operation === "account" && <Field appearance="standard" label="Email"><TextInput appearance="standard" name="email" type="email" required /></Field>}
        {editor.operation === "organization" && <Field appearance="standard" label="Affiliation type"><Select appearance="standard" name="kind" defaultValue={oldOrganization?.kind ?? "organization"}><option value="organization">External organization</option><option value="independent">Independent contractor</option></Select></Field>}
        {["subject", "team"].includes(editor.operation) && <Field appearance="standard" label="External organization"><Select appearance="standard" name="organizationId" required value={organization} onChange={e => setOrganization(e.target.value)}><option value="">Choose organization</option>{initial.organizations.map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</Select></Field>}
        {editor.operation === "team" && <Field appearance="standard" label="Department this team works with"><Select appearance="standard" name="departmentId" defaultValue={team?.departmentId ?? ""}><option value="">No department</option>{initial.units.filter(u => u.kind === "department").map(u => <option key={u.id} value={u.id}>{u.name}</option>)}</Select></Field>}
        {editor.operation === "subject" && <><Field appearance="standard" label="Email"><TextInput appearance="standard" name="email" type="email" required defaultValue={subject?.email ?? ""} /></Field><Field appearance="standard" label="External team"><Select appearance="standard" name="teamId" defaultValue={subject?.teamId ?? ""}><option value="">No team</option>{initial.teams.filter(t => t.organizationId === organization).map(t => <option key={t.id} value={t.id}>{t.name}</option>)}</Select></Field><Field appearance="standard" label="Internal sponsor"><Select appearance="standard" name="sponsorPersonId" defaultValue={subject?.sponsorPersonId ?? ""}><option value="">Choose before enabling access</option>{initial.subjects.filter(s => s.kind === "internal" && s.status === "active").map(s => <option key={s.id} value={s.personId!}>{s.name}</option>)}</Select></Field><Field appearance="standard" label="Relationship expiry"><TextInput appearance="standard" name="expiresAt" type="datetime-local" defaultValue={dateInput(subject?.expiresAt)} /></Field><Field appearance="standard" label="Status"><Select appearance="standard" name="status" defaultValue={subject?.status === "suspended" ? "suspended" : "active"}><option value="active">Active</option><option value="suspended">Suspended</option></Select></Field></>}
        {editor.operation === "role" && <><fieldset><legend>Supported scopes</legend>{["holding", "branch", "department"].map(s => <label className={styles.check} key={s}><input type="checkbox" name="scopes" value={s} defaultChecked={role?.scopes.includes(s) ?? s === "department"} />{s}</label>)}</fieldset><fieldset className={styles.permissions}><legend>Permissions you may delegate</legend>{initial.permissions.map(p => <label className={styles.check} key={p.id}><input type="checkbox" name="permissions" value={p.id} defaultChecked={role?.permissions.includes(p.id) ?? false} /><span>{p.label}<small>{p.id}</small></span></label>)}</fieldset></>}
        {(editor.operation === "grant" || editor.operation === "invite") && <><p>{subject?.name}</p><Field appearance="standard" label="Role"><Select appearance="standard" name="roleId" required><option value="">Choose role</option>{initial.roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}</Select></Field><Field appearance="standard" label="Access scope"><Select appearance="standard" name="scopeKind" value={scopeKind} onChange={e => setScopeKind(e.target.value)}><option value="holding">Company</option><option value="branch">Branch</option><option value="department">Department</option></Select></Field>{scopeKind !== "holding" && <Field appearance="standard" label="Access target"><Select appearance="standard" name="scopeNodeId" required><option value="">Choose target</option>{initial.units.filter(u => u.kind === scopeKind).map(u => <option key={u.id} value={u.id}>{u.name}</option>)}</Select></Field>}<Field appearance="standard" label="Access expiry"><TextInput appearance="standard" name="validUntil" required type="datetime-local" defaultValue={dateInput(Math.min(now + 30 * 86400000, initial.authorizingUntil, subject?.expiresAt ?? Infinity))} /></Field><p>The selected scope grants access; the external team remains a collaboration relationship.</p></>}
        <Field appearance="standard" label="Reason"><TextArea appearance="standard" name="reason" required minLength={3} /></Field>
        {error && <p role="alert">{error} {error.includes("MFA") && <a href="/security">Verify MFA</a>}</p>}
        {preview && <section className={styles.preview} aria-label="Access change preview"><h3>Review change</h3><p>{preview.summary}</p>{preview.affected.length > 0 && <p>Affected users: {preview.affected.join(", ")}</p>}<p>{relationships ? "Relationship status and expiry affect sign-in eligibility immediately. Grants and their expiry remain unchanged." : "Apply saves this change immediately. Invitations and access are separate commands."}</p><Button appearance="standard" type="button" disabled={pending} onClick={apply}>Apply change</Button></section>}
        <Button appearance="standard" type="submit" disabled={pending}>{pending ? "Working…" : "Preview change"}</Button>
      </form>}
    </FormDrawer>
  </div>;
}
