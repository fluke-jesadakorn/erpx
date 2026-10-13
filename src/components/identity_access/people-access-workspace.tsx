"use client";
import { useEffect, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { Button, Field, TextInput, Select, TextArea } from "@/components/ui/foundation";
import { FormDrawer } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { PeopleAccessChange, PeopleAccessSnapshot, PeopleAccessPreview, AccessSubject } from "@/lib/identity_access/people-access";
import styles from "./people-access-workspace.module.css";

type Editor = { operation: PeopleAccessChange["operation"]; id?: string; subjectId?: string; assignmentId?: string; expectedVersion?: number };
const dateInput = (value: number | null | undefined) => value ? new Date(value - new Date(value).getTimezoneOffset() * 60000).toISOString().slice(0, 16) : "";
export function PeopleAccessWorkspace({ initial, api, initialView = "users" }: { initial: PeopleAccessSnapshot; api: string; initialView?: "users" | "organizations" | "teams" | "roles" }) {
  const router = useRouter();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 30000); return () => clearInterval(timer); }, []);
  const [view, setView] = useState<string>(initialView), [query, setQuery] = useState(""), [kind, setKind] = useState("all");
  const [editor, setEditor] = useState<Editor | null>(null), [preview, setPreview] = useState<PeopleAccessPreview | null>(null);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [notice, setNotice] = useState("");
  const [organization, setOrganization] = useState(""), [scopeKind, setScopeKind] = useState("department");
  const [selected, setSelected] = useState<string | null>(null);
  const subject = initial.subjects.find(s => s.id === editor?.id || s.id === editor?.subjectId);
  const oldOrganization = initial.organizations.find(o => o.id === editor?.id), team = initial.teams.find(t => t.id === editor?.id), role = initial.roles.find(r => r.id === editor?.id);
  const detail = initial.subjects.find(s => s.id === selected);
  const matches = (name: string) => name.toLowerCase().includes(query.toLowerCase());
  const externalName = (s: AccessSubject) => initial.organizations.find(o => o.id === s.organizationId)?.name ?? "No organization";
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
    if (!preview) return; setPending(true); setError("");
    try { const result = await post("/apply", { previewToken: preview.token }, `people-apply-${preview.token}`); setNotice(result.invitationId ? "Access authorized. The invitation is queued for delivery." : "Change applied."); setEditor(null); setPreview(null); router.refresh(); }
    catch (e) { setError((e as Error).message); } finally { setPending(false); }
  }
  const title = editor?.operation === "subject" ? editor.id ? "Edit external user" : "Add external user" : editor?.operation === "organization" ? "External organization" : editor?.operation === "team" ? "External team" : editor?.operation === "role" ? "Custom role" : editor?.operation === "invite" ? "Invite user and enable access" : editor?.operation === "grant" ? "Assign access" : "Withdraw access";
  return <div className={styles.workspace}>
    <nav className={styles.tabs} aria-label="Users and access views">{[["users", "Users"], ["organizations", "External organizations"], ["teams", "External teams"], ["roles", "Roles"]].map(([id, label]) => <Button key={id} appearance="standard" variant="ghost" onClick={() => { setView(id); setSelected(null); }} aria-pressed={view === id}>{label}</Button>)}</nav>
    <div className={styles.toolbar}><TextInput appearance="standard" aria-label="Search users and access" placeholder="Search" value={query} onChange={e => setQuery(e.target.value)} />
      {view === "users" && <Select appearance="standard" aria-label="User type" value={kind} onChange={e => setKind(e.target.value)}><option value="all">All users</option><option value="internal">Internal employees</option><option value="external">External users</option></Select>}
      {initial.canApply && <Button appearance="standard" onClick={() => open({ operation: view === "users" ? "subject" : view === "organizations" ? "organization" : view === "teams" ? "team" : "role" })}>Add {view === "users" ? "external user" : view === "organizations" ? "organization" : view === "teams" ? "team" : "role"}</Button>}
    </div>
    {notice && <p role="status">{notice}</p>}
    <div className={styles.columns}><section className={styles.list} aria-label={view}>
      {view === "users" && initial.subjects.filter(s => (kind === "all" || s.kind === kind) && matches(`${s.name} ${s.email} ${s.kind === "external" ? externalName(s) : ""}`)).map(s => <button className={styles.row} key={s.id} onClick={() => setSelected(s.id)} aria-pressed={selected === s.id}><span><strong>{s.name}</strong><small>{s.email || "No email"}</small></span><span>{s.kind === "internal" ? "Employee" : externalName(s)}<small>{s.expiresAt && s.expiresAt <= now ? "Expired" : s.status}{!s.userId ? " · No account" : ""}</small></span></button>)}
      {view === "organizations" && initial.organizations.filter(o => matches(o.name)).map(o => <article className={styles.card} key={o.id}><strong>{o.name}</strong><p>{o.kind === "independent" ? "Independent contractor" : "External organization"} · {initial.subjects.filter(s => s.organizationId === o.id).length} users</p>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "organization", id: o.id, expectedVersion: o.version })}>Edit</Button>}</article>)}
      {view === "teams" && initial.teams.filter(t => matches(t.name)).map(t => <article className={styles.card} key={t.id}><strong>{t.name}</strong><p>{initial.organizations.find(o => o.id === t.organizationId)?.name} · Works with: {initial.units.find(u => u.id === t.departmentId)?.name ?? "No department"}</p><p>{initial.subjects.filter(s => s.teamId === t.id).map(s => s.name).join(", ") || "No members"}</p><small>Team membership does not grant access.</small>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "team", id: t.id, expectedVersion: t.version })}>Edit team</Button>}</article>)}
      {view === "roles" && initial.roles.filter(r => matches(r.name)).map(r => <article className={styles.card} key={r.id}><strong>{r.name}</strong><p>{r.permissions.length} permissions · {r.scopes.join(", ")}</p>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "role", id: r.builtIn ? undefined : r.id, expectedVersion: r.builtIn ? 0 : r.revision })}>{r.builtIn ? "Create custom role" : "Edit role"}</Button>}</article>)}
      {!initial.subjects.length && view === "users" && <p>No users in this Company.</p>}
      {!initial.organizations.length && view === "organizations" && <p>Add an external organization or an independent contractor affiliation.</p>}
      {!initial.teams.length && view === "teams" && <p>No external teams. Create an organization first, then a team.</p>}
    </section>
    {detail && <aside className={styles.card}><h2>{detail.name}</h2><p>{detail.kind === "external" ? externalName(detail) : "Internal employee"}</p>{detail.kind === "internal" && <p>Employment department: {initial.units.find(u=>u.id===detail.departmentId)?.name ?? "Unassigned"}</p>}{detail.kind === "external" && <><p>Team: {initial.teams.find(t => t.id === detail.teamId)?.name ?? "Unassigned"}</p><p>Internal sponsor: {initial.subjects.find(s => s.personId === detail.sponsorPersonId && s.kind === "internal")?.name ?? "Required before enabling access"}</p><p>Relationship expiry: {detail.expiresAt ? new Date(detail.expiresAt).toLocaleString() : "Required before enabling access"}</p></>}
      {initial.canApply && <div className={styles.actions}>{detail.kind === "external" && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "subject", id: detail.id, expectedVersion: detail.version })}>Edit external user</Button>}<Button appearance="standard" onClick={() => open({ operation: detail.userId ? "grant" : "invite", subjectId: detail.id })}>{detail.userId ? "Assign access" : "Invite user"}</Button></div>}
      <h3>Access assignments</h3>{initial.assignments.filter(a => a.userId === detail.userId).map(a => <div className={styles.assignment} key={a.id}><strong>{initial.roles.find(r => r.id === a.roleId)?.name ?? a.roleId}</strong><p>{a.scopeKind}: {initial.units.find(u => u.id === a.scopeNodeId)?.name ?? "Company"} · {a.active ? "Active" : "Unavailable"}</p><small>Until {new Date(a.validUntil).toLocaleString()}</small>{initial.canApply && <Button appearance="standard" variant="secondary" onClick={() => open({ operation: "revoke", assignmentId: a.id, expectedVersion: a.version })}>Withdraw access</Button>}</div>)}
    </aside>}</div>
    <FormDrawer appearance="standard" open={!!editor} onClose={() => { if (!pending) setEditor(null); }} title={title} dismissDisabled={pending}>
      {editor && <form className={styles.form} key={`${editor.operation}:${editor.id ?? editor.subjectId ?? "new"}`} onSubmit={prepare} onChange={() => setPreview(null)}>
        {["organization", "team", "subject", "role"].includes(editor.operation) && <Field appearance="standard" label="Name"><TextInput appearance="standard" name="name" required defaultValue={subject?.name ?? oldOrganization?.name ?? team?.name ?? role?.name ?? ""} /></Field>}
        {editor.operation === "organization" && <Field appearance="standard" label="Affiliation type"><Select appearance="standard" name="kind" defaultValue={oldOrganization?.kind ?? "organization"}><option value="organization">External organization</option><option value="independent">Independent contractor</option></Select></Field>}
        {["subject", "team"].includes(editor.operation) && <Field appearance="standard" label="External organization"><Select appearance="standard" name="organizationId" required value={organization} onChange={e => setOrganization(e.target.value)}><option value="">Choose organization</option>{initial.organizations.map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</Select></Field>}
        {editor.operation === "team" && <Field appearance="standard" label="Department this team works with"><Select appearance="standard" name="departmentId" defaultValue={team?.departmentId ?? ""}><option value="">No department</option>{initial.units.filter(u => u.kind === "department").map(u => <option key={u.id} value={u.id}>{u.name}</option>)}</Select></Field>}
        {editor.operation === "subject" && <><Field appearance="standard" label="Email"><TextInput appearance="standard" name="email" type="email" required defaultValue={subject?.email ?? ""} /></Field><Field appearance="standard" label="External team"><Select appearance="standard" name="teamId" defaultValue={subject?.teamId ?? ""}><option value="">No team</option>{initial.teams.filter(t => t.organizationId === organization).map(t => <option key={t.id} value={t.id}>{t.name}</option>)}</Select></Field><Field appearance="standard" label="Internal sponsor"><Select appearance="standard" name="sponsorPersonId" defaultValue={subject?.sponsorPersonId ?? ""}><option value="">Choose before enabling access</option>{initial.subjects.filter(s => s.kind === "internal" && s.status === "active").map(s => <option key={s.id} value={s.personId!}>{s.name}</option>)}</Select></Field><Field appearance="standard" label="Relationship expiry"><TextInput appearance="standard" name="expiresAt" type="datetime-local" defaultValue={dateInput(subject?.expiresAt)} /></Field><Field appearance="standard" label="Status"><Select appearance="standard" name="status" defaultValue={subject?.status === "suspended" ? "suspended" : "active"}><option value="active">Active</option><option value="suspended">Suspended</option></Select></Field></>}
        {editor.operation === "role" && <><fieldset><legend>Supported scopes</legend>{["holding", "branch", "department"].map(s => <label className={styles.check} key={s}><input type="checkbox" name="scopes" value={s} defaultChecked={role?.scopes.includes(s) ?? s === "department"} />{s}</label>)}</fieldset><fieldset className={styles.permissions}><legend>Permissions you may delegate</legend>{initial.permissions.map(p => <label className={styles.check} key={p.id}><input type="checkbox" name="permissions" value={p.id} defaultChecked={role?.permissions.includes(p.id) ?? false} /><span>{p.label}<small>{p.id}</small></span></label>)}</fieldset></>}
        {(editor.operation === "grant" || editor.operation === "invite") && <><p>{subject?.name}</p><Field appearance="standard" label="Role"><Select appearance="standard" name="roleId" required><option value="">Choose role</option>{initial.roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}</Select></Field><Field appearance="standard" label="Access scope"><Select appearance="standard" name="scopeKind" value={scopeKind} onChange={e => setScopeKind(e.target.value)}><option value="holding">Company</option><option value="branch">Branch</option><option value="department">Department</option></Select></Field>{scopeKind !== "holding" && <Field appearance="standard" label="Access target"><Select appearance="standard" name="scopeNodeId" required><option value="">Choose target</option>{initial.units.filter(u => u.kind === scopeKind).map(u => <option key={u.id} value={u.id}>{u.name}</option>)}</Select></Field>}<Field appearance="standard" label="Access expiry"><TextInput appearance="standard" name="validUntil" required type="datetime-local" defaultValue={dateInput(Math.min(now + 30 * 86400000, initial.authorizingUntil, subject?.expiresAt ?? Infinity))} /></Field><p>The selected scope grants access; the external team remains a collaboration relationship.</p></>}
        <Field appearance="standard" label="Reason"><TextArea appearance="standard" name="reason" required minLength={3} /></Field>
        {error && <p role="alert">{error} {error.includes("MFA") && <a href="/security">Verify MFA</a>}</p>}
        {preview && <section className={styles.preview} aria-label="Access change preview"><h3>Review change</h3><p>{preview.summary}</p>{preview.affected.length > 0 && <p>Affected users: {preview.affected.join(", ")}</p>}<p>Apply enables this change immediately.</p><Button appearance="standard" type="button" disabled={pending} onClick={apply}>Apply change</Button></section>}
        <Button appearance="standard" type="submit" disabled={pending}>{pending ? "Working…" : "Preview change"}</Button>
      </form>}
    </FormDrawer>
  </div>;
}
