"use client";
import {
  useRef,
  useState,
  type ReactNode,
  type DragEvent,
  type FormEvent,
  type ComponentProps,
} from "react";
import Link from "next/link";
import {
  ArrowLeft,
  Building2,
  ChevronRight,
  CircleHelp,
  GitBranch,
  Plus,
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Users,
  UserRound,
} from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import {
  attributeTypes,
  personRuleFields,
  ruleMatches,
  type AccessPolicy,
  type AttributeDefinition,
  type LocalGroup,
  type ManagementChange,
  type ManagementImpact,
  type ManagementSnapshot,
  type MembershipRule,
  type PolicyAttachment,
} from "@/lib/user-management";
import type { RoleRevision } from "@/lib/iam";
import styles from "./user-management.module.css";
import { LifecyclePanel } from "./lifecycle-workspace";
import type { PeopleLifecycleData } from "@/lib/people-lifecycle";
import { PeopleAccessExplorer } from "./access-explorer";

type View = "people" | "groups" | "policies" | "attributes";
type Selection = {
  kind: "person" | "group" | "unit" | "policy" | "attribute";
  id: string;
};
type Editor =
  | { kind: "group"; item?: LocalGroup }
  | { kind: "attribute"; item?: AttributeDefinition }
  | { kind: "policy"; item?: AccessPolicy }
  | {
      kind: "attachment";
      item?: PolicyAttachment;
      policyId?: string;
      target?: "group" | "unit";
      targetId?: string;
    }
  | { kind: "membership"; groupId?: string; personIds: string[] };
type Drag =
  { kind: "people"; ids: string[] } | { kind: "group" | "policy"; id: string };
async function request<T>(
  url: string,
  body?: unknown,
  key?: string,
): Promise<T> {
  const r = await workFetch(
      url,
      body
        ? {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": key ?? crypto.randomUUID(),
            },
            body: JSON.stringify(body),
          }
        : { cache: "no-store" },
    ),
    v = await r.json();
  if (!r.ok) throw Error(v.error?.message ?? "Unable to load user management.");
  return v.data;
}
const str = (f: FormData, k: string) => String(f.get(k) ?? "");
function Reason() {
  return (
    <label>
      Reason
      <input
        name="reason"
        required
        minLength={3}
        maxLength={1000}
        placeholder="Why are you making this change?"
      />
    </label>
  );
}

export function UserManagement({
  initial,
  roles,
  base,
  api,
  accounts,
  initialView = "people",
  views,
  lifecycle,
  compactConsole = false,
}: {
  lifecycle?: PeopleLifecycleData | null;
  initialView?: View;
  views?: View[];
  compactConsole?: boolean;
  initial: ManagementSnapshot;
  roles: RoleRevision[];
  base: string;
  api: string;
  accounts: Pick<
    ComponentProps<typeof PeopleAccessExplorer>,
    "meta" | "initialEntries"
  >;
}) {
  const [movePerson,setMovePerson] = useState<string|null>(null);
  const [data, setData] = useState(initial),
    [view, setView] = useState<View>(initialView),
    [selection, setSelection] = useState<Selection | null>(null),
    [filter, setFilter] = useState(""),
    [showArchived, setShowArchived] = useState(false),
    [unit, setUnit] = useState<string | null>(null),
    [selected, setSelected] = useState<string[]>([]),
    [editor, setEditor] = useState<Editor | null>(null),
    [screen, setScreen] = useState(compactConsole ? "results" : "tree"),
    [targetFilter, setTargetFilter] = useState<"all" | "person" | "group" | "unit">("all"),
    [advanced, setAdvanced] = useState(false),
    [accountObjectId, setAccountObjectId] = useState<string>(),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [preview, setPreview] = useState<{
      change: ManagementChange;
      previewToken: string;
      impact: ManagementImpact;
      key: string;
    } | null>(null),
    [reviewIds, setReviewIds] = useState<string[]>([]);
  const drag = useRef<Drag | null>(null),
    heading = useRef<HTMLHeadingElement>(null);
  const canManageGroups=!!data.canManageGroups || data.canManage;
  const iamApi = api.replace(/\/people\/access$/, "/iam");
  const select = (s: Selection) => {
    setSelection(s);
    setScreen("detail");
    requestAnimationFrame(() => heading.current?.focus());
  };
  const changeView = (v: View) => {
    setView(v);
    setSelection(null);
    setFilter("");
    setSelected([]);
    setAdvanced(false);
    setScreen("results");
  };
  async function refresh() {
    setData(await request<ManagementSnapshot>(api + "/management"));
  }
  async function prepare(change: ManagementChange) {
    setBusy(true);
    setMessage("");
    try {
      const result = await request<{
        previewToken: string;
        impact: ManagementImpact;
      }>(iamApi + "/user-management/preview", { change });
      setPreview({ ...result, change, key: crypto.randomUUID() });
    } catch (e) {
      setMessage((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  async function commit() {
    if (!preview) return;
    setBusy(true);
    setMessage("");
    try {
      await request(
        iamApi + "/user-management/save",
        { change: preview.change, previewToken: preview.previewToken },
        preview.key,
      );
      await refresh();
      setPreview(null);
      setEditor(null);
      setMessage(
        "Saved. New access remains pending until independently reviewed.",
      );
    } catch (e) {
      setMessage((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  function startDrag(e: DragEvent, value: Drag) {
    drag.current = value;
    e.dataTransfer.effectAllowed = "copyMove";
    if(value.kind === "people" && value.ids.length===1)e.dataTransfer.setData("application/erpx-person",value.ids[0]);
    e.dataTransfer.setData(
      "application/x-erpx-management",
      JSON.stringify(value),
    );
  }
  function drop(e: DragEvent, target: "group" | "unit", id: string) {
    e.preventDefault();
    const value = drag.current;
    drag.current = null;
    if(value?.kind === "people" && target === "unit" && lifecycle?.canManage) {
      if(value.ids.length!==1){setMessage("Move one person at a time so their access can be reviewed.");return;}
      setMovePerson(value.ids[0]);return;
    }
    if (!value || !(value.kind === "policy" ? data.canManage : canManageGroups)) return;
    if (
      target === "group" &&
      value.kind !== "policy" &&
      data.groups.find((g) => g.id === id)?.kind === "automatic"
    ) {
      changeView("groups");
      select({ kind: "group", id });
      setMessage(
        "Automatic groups use the matching rule shown below. They cannot accept manual members or child groups.",
      );
      return;
    }
    if (value.kind === "people" && target === "group")
      setEditor({ kind: "membership", groupId: id, personIds: value.ids });
    else if (value.kind === "policy")
      setEditor({
        kind: "attachment",
        policyId: value.id,
        target,
        targetId: id,
      });
    else if (value.kind === "group" && target === "group") {
      const g = data.groups.find((g) => g.id === value.id);
      if (g) setEditor({ kind: "group", item: { ...g, parentId: id } });
    } else
      setMessage("Use the HR profile form to change department placement.");
  }
  async function review(decision: "approve" | "reject") {
    setBusy(true);
    setMessage("");
    let completed = 0;
    try {
      for (const id of reviewIds) {
        const candidate = data.candidates.find((c) => c.reviewId === id);
        if (candidate) {
          await request(iamApi + "/reviews/" + id, {
            expectedVersion: candidate.reviewVersion,
            decision,
            reason: "User management: " + decision + " reviewed access",
          });
          completed++;
        }
      }
      setReviewIds([]);
      await refresh();
      setMessage(completed + " access decisions recorded.");
    } catch (e) {
      await refresh();
      setMessage(completed + " decisions recorded. " + (e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const treeUnits = (
    parent: string | null,
    visited: string[] = [],
  ): ReactNode =>
    data.nodes
      .filter(
        (n) =>
          n.parentId === parent ||
          (parent === null && !data.nodes.some((p) => p.id === n.parentId)),
      )
      .filter((n) => !visited.includes(n.id))
      .map((n) => (
        <details key={n.id} open className={styles.branch}>
          <summary>
            <Building2 size={15} />
            <button
              type="button"
              onClick={() => {
                setUnit(n.id);
                changeView("people");
                select({ kind: "unit", id: n.id });
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => drop(e, "unit", n.id)}
            >
              {n.label}
            </button>
          </summary>
          {treeUnits(n.id, [...visited, n.id])}
        </details>
      ));
  const treeGroups = (
    parent: string | null,
    visited: string[] = [],
  ): ReactNode =>
    data.groups
      .filter(
        (g) =>
          !g.archived &&
          (g.parentId === parent ||
            (parent === null && !data.groups.some((p) => p.id === g.parentId))),
      )
      .filter((g) => !visited.includes(g.id))
      .map((g) => (
        <details key={g.id} open className={styles.branch}>
          <summary>
            <Users size={15} />
            <button
              type="button"
              draggable={canManageGroups}
              onDragStart={(e) => startDrag(e, { kind: "group", id: g.id })}
              onDragEnd={() => {
                drag.current = null;
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => drop(e, "group", g.id)}
              onClick={() => {
                changeView("groups");
                select({ kind: "group", id: g.id });
              }}
            >
              {g.name}
              <small>{g.members.length}</small>
            </button>
          </summary>
          {treeGroups(g.id, [...visited, g.id])}
        </details>
      ));
  const nodeIds = new Set(unit ? [unit] : []);
  for (let changed = true; changed;) {
    changed = false;
    for (const n of data.nodes)
      if (n.parentId && nodeIds.has(n.parentId) && !nodeIds.has(n.id)) {
        nodeIds.add(n.id);
        changed = true;
      }
  }
  const people = data.people.filter(
    (p) =>
      (!unit ||
        data.nodes.find((n) => n.id === unit)?.kind === "company" ||
        p.nodes.some((n) => nodeIds.has(n))) &&
      p.name.toLowerCase().includes(filter.toLowerCase()),
  );
  const group =
      selection?.kind === "group"
        ? data.groups.find((g) => g.id === selection.id)
        : null,
    person =
      selection?.kind === "person"
        ? data.people.find((p) => p.id === selection.id)
        : null,
    policy =
      selection?.kind === "policy"
        ? data.policies.find((p) => p.id === selection.id)
        : null,
    attribute =
      selection?.kind === "attribute"
        ? data.attributes.find((a) => a.id === selection.id)
        : null;
  const title =
    person?.name ??
    group?.name ??
    policy?.name ??
    attribute?.label ??
    data.nodes.find((n) => selection?.kind === "unit" && n.id === selection.id)
      ?.label ??
    (compactConsole
      ? view === "attributes" ? "Select a field" : view === "policies" ? "Select a policy" : view === "groups" ? "Select a group" : "Select a person"
      : "Select an item");
  const ancestors = new Set<string>();
  if (selection && ["group", "unit"].includes(selection.kind)) {
    const hierarchy = selection.kind === "group" ? data.groups : data.nodes;
    let id: string | null = selection.id;
    while (id && !ancestors.has(id)) {
      ancestors.add(id);
      id = hierarchy.find((node) => node.id === id)?.parentId ?? null;
    }
  }
  const attachments = data.attachments.filter(
    (a) =>
      !a.archived &&
      (policy
        ? a.policyId === policy.id
        : selection &&
          a.target === selection.kind &&
          ancestors.has(a.targetId)),
  );
  const fieldTarget =
    selection && ["person", "group", "unit"].includes(selection.kind)
      ? (selection.kind as "person" | "group" | "unit")
      : null;
  const visibleGroups = data.groups
    .filter((group) => showArchived || !group.archived)
    .filter((group) => group.name.toLowerCase().includes(filter.toLowerCase()));
  const visiblePolicies = data.policies
    .filter((policy) => showArchived || !policy.archived)
    .filter((policy) => policy.name.toLowerCase().includes(filter.toLowerCase()));
  const visibleAttributes = data.attributes
    .filter((attribute) => showArchived || !attribute.archived)
    .filter((attribute) => !compactConsole || targetFilter === "all" || attribute.target === targetFilter)
    .filter((attribute) => attribute.label.toLowerCase().includes(filter.toLowerCase()));
  const visibleItems = view === "groups" ? visibleGroups : view === "policies" ? visiblePolicies : visibleAttributes;
  const canCreateInView = view === "groups" ? canManageGroups : view === "policies" ? data.canManage : data.canDefine;
  const emptyName = view === "groups" ? "groups" : view === "policies" ? "policies" : "custom attributes";
  const explorerMenu = <details className={styles.advanced}>
    <summary><Settings2 size={15} /> Directory explorer</summary>
    <button onClick={() => { setAdvanced(true); setScreen("tree"); }}>Accounts & connected directories</button>
    <Link href={base + "/people/settings"}>People settings</Link>
    <Link href="/security">Verify MFA</Link>
  </details>;
  return (
    <div className={styles.workspace} data-compact-console={compactConsole || undefined}>
      {lifecycle&&movePerson&&<Dialog open onClose={()=>setMovePerson(null)} title="Assign or transfer person" presentation="responsive-panel">{(()=>{const person=lifecycle.roster.find(p=>p.id===movePerson);return person?<LifecyclePanel base={base} data={lifecycle} person={person} initialKind="transfer"/>:<p>Complete an engagement in this workspace before moving this person.</p>;})()}</Dialog>}
      {(!compactConsole || !views || views.length > 1) && <div className={styles.topbar}>
        {(!views || views.length > 1) && <nav aria-label="User management views">
          {(
            [
              { id: "people", label: "People", Icon: UserRound },
              { id: "groups", label: "Groups", Icon: Users },
              { id: "policies", label: "Access policies", Icon: ShieldCheck },
              {
                id: "attributes",
                label: "Custom attributes",
                Icon: SlidersHorizontal,
              },
            ] as const
          ).filter(({id}) => !views || views.includes(id)).map(({ id, label, Icon }) => (
            <button
              key={id}
              type="button"
              aria-current={!advanced && view === id ? "page" : undefined}
              onClick={() => changeView(id)}
            >
              <Icon size={16} aria-hidden="true" />
              {label}
            </button>
          ))}
        </nav>}
        {!compactConsole && explorerMenu}
      </div>}
      {message && (
        <p role="status" className={styles.notice}>
          {message}
        </p>
      )}
      {advanced ? (
        <>
          <button
            className="button secondary"
            onClick={() => setAdvanced(false)}
          >
            <ArrowLeft size={16} />
            Back to user management
          </button>
          <PeopleAccessExplorer
            {...accounts}
            base={base}
            api={api}
            initialObjectId={accountObjectId}
          />
        </>
      ) : (
        <>
          <div className={styles.intro}>
            <div>
              <h2>
                {view === "people"
                  ? "People & their access"
                  : view === "groups"
                    ? "Bring people together"
                    : view === "policies"
                      ? "Consistent access, clear ownership"
                      : "Fields that fit your organization"}
              </h2>
              <p>
                {view === "people"
                  ? "Select people to update groups, view attributes, or understand their access."
                  : view === "groups"
                    ? "Build a group by choosing members or setting a simple matching rule."
                    : view === "policies"
                      ? "Attach roles to groups or units. Every new grant is independently reviewed."
                      : "Define additional information for people, local groups, and organizational units."}
              </p>
            </div>
            {view === "people" ? (
              <Link className="button secondary" href={base + "/people"}>
                Add or edit a person
              </Link>
            ) : (
              (view === "attributes" ? data.canDefine : view === "groups" ? canManageGroups : data.canManage) && (
                <button
                  className="button"
                  onClick={() =>
                    setEditor({
                      kind:
                        view === "groups"
                          ? "group"
                          : view === "policies"
                            ? "policy"
                            : "attribute",
                    })
                  }
                >
                  <Plus size={16} />
                  New{" "}
                  {view === "groups"
                    ? "group"
                    : view === "policies"
                      ? "policy"
                      : "attribute"}
                </button>
              )
            )}
            {compactConsole && explorerMenu}
          </div>
          {compactConsole ? <nav aria-label="Management navigation" className={styles.mobileSteps}>
            {screen === "detail" && <button type="button" onClick={() => setScreen("results")}><ArrowLeft size={14} aria-hidden="true" /> Back to list</button>}
          </nav> : <nav aria-label="Directory steps" className={styles.mobileSteps}>
            {screen !== "tree" && (
              <button
                onClick={() =>
                  setScreen(screen === "detail" ? "results" : "tree")
                }
              >
                <ArrowLeft size={14} aria-hidden="true" /> Back to{" "}
                {screen === "detail" ? "results" : "hierarchy"}
              </button>
            )}
            <button onClick={() => setScreen("tree")}>Hierarchy</button>
            <ChevronRight size={14} />
            <button onClick={() => setScreen("results")}>Results</button>
            {selection && (
              <>
                <ChevronRight size={14} />
                <button onClick={() => setScreen("detail")}>Details</button>
              </>
            )}
          </nav>}
          <div className={styles.panes} data-screen={screen}>
            {!compactConsole && <aside className={styles.tree} aria-label="Management hierarchy">
              <button
                className={styles.all}
                onClick={() => {
                  setUnit(null);
                  changeView("people");
                }}
              >
                <Users size={16} />
                All people
              </button>
              <h3>Organization</h3>
              {treeUnits(null)}
              <h3>Local groups</h3>
              {treeGroups(null)}
              {!data.groups.some((g) => !g.archived) && (
                <p>Create your first group to organize access.</p>
              )}
              <h3>Connected directories</h3>
              <button className={styles.all} onClick={() => setAdvanced(true)}>
                <GitBranch size={16} />
                Browse read-only directories
              </button>
              <p>Changes here affect ERPX only.</p>
            </aside>}
            <section className={styles.results} aria-label="Management results">
              <label className={styles.search}>
                <Search size={16} aria-hidden="true" />
                <input
                  aria-label={"Search " + view}
                  placeholder={"Search " + view}
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
              </label>
              {view !== "people" && (
                <label>
                  <input
                    type="checkbox"
                    checked={showArchived}
                    onChange={(e) => setShowArchived(e.target.checked)}
                  />{" "}
                  Include archived
                </label>
              )}
              {compactConsole && view === "attributes" && <label className={styles.targetFilter}>
                Target
                <select value={targetFilter} onChange={(event) => setTargetFilter(event.target.value as typeof targetFilter)}>
                  <option value="all">All targets</option>
                  <option value="person">People</option>
                  <option value="group">Groups</option>
                  <option value="unit">Organizational units</option>
                </select>
              </label>}
              {unit && view === "people" && (
                <button onClick={() => setUnit(null)}>
                  Clear unit filter:{" "}
                  {data.nodes.find((n) => n.id === unit)?.label}
                </button>
              )}
              {view === "people" && (
                <>
                  <div className={styles.bulk}>
                    <label>
                      <input
                        type="checkbox"
                        aria-label="Select visible people"
                        checked={
                          people.length > 0 &&
                          people.every((p) => selected.includes(p.id))
                        }
                        onChange={(e) =>
                          setSelected(
                            e.target.checked ? people.map((p) => p.id) : [],
                          )
                        }
                      />
                      Select visible
                    </label>
                    {canManageGroups && (
                      <button
                        disabled={!selected.length}
                        onClick={() =>
                          setEditor({ kind: "membership", personIds: selected })
                        }
                      >
                        Groups ({selected.length})
                      </button>
                    )}
                  </div>
                  {people.map((p) => (
                    <div className={styles.personRow} key={p.id}>
                      <input
                        type="checkbox"
                        aria-label={"Select " + p.name}
                        checked={selected.includes(p.id)}
                        onChange={(e) =>
                          setSelected(
                            e.target.checked
                              ? [...selected, p.id]
                              : selected.filter((id) => id !== p.id),
                          )
                        }
                      />
                      <button
                        draggable={canManageGroups}
                        onDragStart={(e) =>
                          startDrag(e, {
                            kind: "people",
                            ids: selected.includes(p.id) ? selected : [p.id],
                          })
                        }
                        onDragEnd={() => {
                          drag.current = null;
                        }}
                        aria-pressed={selection?.id === p.id}
                        onClick={() => select({ kind: "person", id: p.id })}
                      >
                        <UserRound size={17} />
                        <span>
                          {p.name}
                          <small>
                            {p.userId ? "Linked account" : "No linked account"}
                          </small>
                        </span>
                      </button>
                    </div>
                  ))}
                  {!people.length && <p>No people match this view.</p>}
                </>
              )}
              {view === "groups" &&
                visibleGroups.map((g) => (
                    <button
                      className={styles.item}
                      key={g.id}
                      aria-pressed={selection?.id === g.id}
                      onClick={() => select({ kind: "group", id: g.id })}
                      draggable={canManageGroups && !g.archived}
                      onDragStart={(e) =>
                        startDrag(e, { kind: "group", id: g.id })
                      }
                    >
                      <Users size={18} />
                      <span>
                        {g.name}
                        <small>
                          {g.archived
                            ? "Archived"
                            : g.members.length +
                              " members · " +
                              (g.kind === "automatic"
                                ? "Automatic rule"
                                : "Manual group")}
                        </small>
                      </span>
                    </button>
                  ))}
              {view === "policies" &&
                visiblePolicies.map((p) => (
                    <button
                      className={styles.item}
                      key={p.id}
                      aria-pressed={selection?.id === p.id}
                      draggable={data.canManage && !p.archived}
                      onDragStart={(e) =>
                        startDrag(e, { kind: "policy", id: p.id })
                      }
                      onClick={() => select({ kind: "policy", id: p.id })}
                    >
                      <ShieldCheck size={18} />
                      <span>
                        {p.name}
                        <small>
                          {p.archived
                            ? "Archived"
                            : p.grants.length +
                              " roles · revision " +
                              p.version}
                        </small>
                      </span>
                    </button>
                  ))}
              {view === "attributes" &&
                visibleAttributes.map((a) => (
                    <button
                      className={styles.item}
                      key={a.id}
                      aria-pressed={selection?.id === a.id}
                      onClick={() => select({ kind: "attribute", id: a.id })}
                    >
                      <SlidersHorizontal size={18} />
                      <span>
                        {a.label}
                        <small>
                          {a.archived
                            ? "Archived"
                            : a.target +
                              " · " +
                              a.type +
                              (a.required ? " · Required" : "")}
                        </small>
                      </span>
                    </button>
                  ))}
              {view !== "people" && !visibleItems.length && (
                  <div className={styles.empty}>
                    <CircleHelp size={24} />
                    <p>{filter || (view === "attributes" && targetFilter !== "all")
                      ? `No ${emptyName} match these filters.`
                      : canCreateInView
                        ? `No ${emptyName} yet. Use New to create one.`
                        : `No ${emptyName} are available.`}</p>
                  </div>
                )}
            </section>
            <section className={styles.detail} aria-label="Management details">
              <h2 tabIndex={-1} ref={heading}>
                {title}
              </h2>
              {!selection && (
                <div className={styles.empty}>
                  <Users size={32} />
                  <p>Choose {view === "attributes" ? "a field" : view === "policies" ? "a policy" : view === "groups" ? "a group" : "a person"} to see its details.</p>
                  {view === "people" && <p>Drag people into a manual group, or use the Groups button.</p>}
                </div>
              )}
              {person && (
                <>
                  <p>
                    {person.userId
                      ? "ERPX account linked"
                      : "Create or link an account before granting access."}
                  </p>
                  <Link
                    href={base + "/people/" + person.id}
                    className="button secondary"
                  >
                    Profile & account
                  </Link>
                  <h3>Group membership</h3>
                  <button
                    className="button secondary"
                    onClick={() => {
                      setAccountObjectId("person:" + person.id);
                      setAdvanced(true);
                    }}
                  >
                    Account & direct access
                  </button>
                  {data.groups
                    .filter((g) => g.members.includes(person.id))
                    .map((g) => (
                      <button
                        key={g.id}
                        className={styles.item}
                        onClick={() => {
                          changeView("groups");
                          select({ kind: "group", id: g.id });
                        }}
                      >
                        {g.name}
                        <small>
                          {g.kind === "automatic"
                            ? "Matched by rule"
                            : "Direct or nested membership"}
                        </small>
                      </button>
                    ))}
                  {canManageGroups && (
                    <button
                      className="button secondary"
                      onClick={() =>
                        setEditor({
                          kind: "membership",
                          personIds: [person.id],
                        })
                      }
                    >
                      Change groups
                    </button>
                  )}
                </>
              )}
              {group && (
                <>
                  <p>
                    {group.kind === "automatic"
                      ? "Membership follows this rule; manual additions are disabled."
                      : "Members can be added directly or inherited from child groups."}
                  </p>
                  {group.rule && (
                    <div className={styles.ruleSummary}>
                      <strong>Match {group.rule.mode} conditions</strong>
                      {group.rule.conditions.map((c, i) => (
                        <p key={i}>
                          {data.attributes.find(
                            (a) => "custom:" + a.id === c.field,
                          )?.label ??
                            personRuleFields.find((f) => f.id === c.field)
                              ?.label}{" "}
                          {c.op} {String(c.value ?? "")}
                        </p>
                      ))}
                    </div>
                  )}
                  <p>{group.members.length} effective members</p>
                  <ul>
                    {group.members.map((id) => (
                      <li key={id}>
                        {data.people.find((p) => p.id === id)?.name}
                        <small> · {(group.memberSources?.[id]??[]).join(" · ")}</small>
                        {group.rule && (
                          <small>
                            {group.rule.conditions
                              .filter((condition) =>
                                ruleMatches(
                                  { mode: "all", conditions: [condition] },
                                  data.people.find((p) => p.id === id)?.facts ??
                                    {},
                                ),
                              )
                              .map(
                                (condition) =>
                                  `${data.attributes.find((a) => "custom:" + a.id === condition.field)?.label ?? personRuleFields.find((f) => f.id === condition.field)?.label ?? condition.field}: ${condition.op} ${String(condition.value ?? "")}`,
                              )
                              .join(" · ")}
                          </small>
                        )}
                      </li>
                    ))}
                  </ul>
                  {canManageGroups && (
                    <button
                      className="button secondary"
                      onClick={() => setEditor({ kind: "group", item: group })}
                    >
                      Edit group & parent
                    </button>
                  )}
                </>
              )}
              {policy && (
                <>
                  <p>
                    Revision {policy.version}. Each role becomes a separately
                    reviewed work assignment.
                  </p>
                  {policy.grants.map((g, i) => (
                    <div key={i} className={styles.ruleSummary}>
                      <strong>
                        {roles.find((r) => r.id === g.roleId)?.name ?? g.roleId}
                      </strong>
                      <p>
                        {g.scopeKind} ·{" "}
                        {data.nodes.find((n) => n.id === g.scopeNodeId)
                          ?.label ?? "Company"}
                      </p>
                      <small>
                        Expires{" "}
                        {new Date(g.validUntil).toISOString().slice(0, 10)}
                      </small>
                    </div>
                  ))}
                  {data.canManage && (
                    <div className={styles.actions}>
                      <button
                        className="button secondary"
                        onClick={() =>
                          setEditor({ kind: "policy", item: policy })
                        }
                      >
                        Edit policy
                      </button>
                      <button
                        className="button"
                        disabled={policy.archived}
                        onClick={() =>
                          setEditor({ kind: "attachment", policyId: policy.id })
                        }
                      >
                        Attach policy
                      </button>
                    </div>
                  )}
                </>
              )}
              {attribute && (
                <>
                  <p>{attribute.help || "No help text."}</p>
                  <p>
                    {attribute.target} · {attribute.type} ·{" "}
                    {attribute.required ? "Required" : "Optional"}
                  </p>
                  {attribute.options.length > 0 && (
                    <p>Choices: {attribute.options.join(", ")}</p>
                  )}
                  {data.canDefine && (
                    <button
                      className="button secondary"
                      onClick={() =>
                        setEditor({ kind: "attribute", item: attribute })
                      }
                    >
                      Edit attribute
                    </button>
                  )}
                </>
              )}
              {selection &&
                ["group", "unit", "policy"].includes(selection.kind) && (
                  <>
                    <h3>Attached access policies</h3>
                    {attachments.map((a) => (
                      <div className={styles.ruleSummary} key={a.id}>
                        <strong>
                          {data.policies.find((p) => p.id === a.policyId)?.name}
                        </strong>
                        <p>
                          {selection.kind !== "policy" &&
                            (a.exclusions.some((id) => ancestors.has(id))
                              ? "Excluded from this subtree · "
                              : a.targetId === selection.id
                                ? "Direct · "
                                : "Inherited · ")}
                          {a.target} ·{" "}
                          {data.groups.find((g) => g.id === a.targetId)?.name ??
                            data.nodes.find((n) => n.id === a.targetId)?.label}
                        </p>
                        <small>{a.exclusions.length} subtree exclusions</small>
                        {data.canManage && (
                          <button
                            onClick={() =>
                              setEditor({ kind: "attachment", item: a })
                            }
                          >
                            Edit attachment
                          </button>
                        )}
                      </div>
                    ))}
                    {!attachments.length && (
                      <p>No applicable policy attachments.</p>
                    )}
                    {selection.kind !== "policy" && data.canManage && (
                      <button
                        className="button secondary"
                        onClick={() =>
                          setEditor({
                            kind: "attachment",
                            target: selection.kind as "group" | "unit",
                            targetId: selection.id,
                          })
                        }
                      >
                        Attach policy
                      </button>
                    )}
                  </>
                )}
              {fieldTarget && selection && (
                <AttributeValueForm
                  key={
                    selection.kind +
                    ":" +
                    selection.id +
                    ":" +
                    (data.values.find(
                      (v) =>
                        v.target === fieldTarget && v.objectId === selection.id,
                    )?.version ?? 0)
                  }
                  target={fieldTarget}
                  objectId={selection.id}
                  data={data}
                  onSubmit={prepare}
                  busy={busy}
                />
              )}
            </section>
          </div>
          {(!compactConsole || view === "policies") && <section className={styles.reviewPanel}>
            <div className={styles.intro}>
              <div>
                <h2>Policy access & reviews</h2>
                <p>
                  Membership alone does not grant access. Review each new
                  beneficiary independently.
                </p>
              </div>
              <Link href={base + "/iam/reviews"}>All reviews & audit</Link>
            </div>
            {data.canReview && (
              <div className={styles.actions}>
                <button
                  className="button"
                  disabled={busy || !reviewIds.length}
                  onClick={() => void review("approve")}
                >
                  Approve selected ({reviewIds.length})
                </button>
                <button
                  className="button secondary"
                  disabled={busy || !reviewIds.length}
                  onClick={() => void review("reject")}
                >
                  Reject selected
                </button>
                <Link href="/security">Verify MFA</Link>
              </div>
            )}
            {!data.candidates.length && (
              <p>
                No policy-derived access yet. Attach a policy to a group or unit
                to preview its effect.
              </p>
            )}
            {data.candidates
              .filter((c) => !person || c.personId === person.id)
              .map((c) => (
                <div key={c.id} className={styles.candidate}>
                  {data.canReview && c.status === "pending" && c.reviewId && (
                    <input
                      type="checkbox"
                      aria-label={
                        "Review " +
                        data.people.find((p) => p.id === c.personId)?.name +
                        " " +
                        c.roleId
                      }
                      checked={reviewIds.includes(c.reviewId)}
                      onChange={(e) =>
                        setReviewIds(
                          e.target.checked
                            ? [...reviewIds, c.reviewId!]
                            : reviewIds.filter((id) => id !== c.reviewId),
                        )
                      }
                    />
                  )}
                  <span>
                    <strong>
                      {data.people.find((p) => p.id === c.personId)?.name}
                    </strong>
                    <small>
                      {c.policyName} ·{" "}
                      {roles.find((r) => r.id === c.roleId)?.name ?? c.roleId}
                    </small>
                    <small>{c.explanation}</small>
                  </span>
                  <span className={styles.status} data-status={c.status}>
                    {c.status}
                  </span>
                </div>
              ))}
          </section>}
        </>
      )}
      <Dialog
        open={!!editor}
        title={
          editor
            ? ("item" in editor && editor.item ? "Edit " : "Set up ") +
              editor.kind
            : "Edit"
        }
        presentation="responsive-panel"
        onClose={() => {
          if (!busy) setEditor(null);
        }}
      >
        {editor && (
          <ManagementEditor
            editor={editor}
            data={data}
            roles={roles}
            busy={busy}
            onSubmit={prepare}
          />
        )}{" "}
        {message && <p role="status">{message}</p>}
      </Dialog>
      <Dialog
        open={!!preview}
        title="Review the impact"
        presentation="responsive-panel"
        onClose={() => {
          if (!busy) setPreview(null);
        }}
      >
        {preview && (
          <div className={styles.form}>
            <p>Nothing is saved yet. Review the changes below.</p>
            <p>
              <strong>{preview.change.operation}</strong> ·{" "}
              {preview.change.input.reason}
            </p>
            {Object.entries(preview.impact).map(([key, items]) => (
              <section key={key}>
                <h3>
                  {
                    {
                      membersAdded: "Memberships added",
                      membersRemoved: "Memberships removed",
                      accessRemoved: "Derived access removed",
                      accessPending: "New grants awaiting review",
                      accessBlocked: "Access requiring attention",
                      matchingPeople: "Matching people",
                    }[key]
                  }
                </h3>
                {items.length ? (
                  <ul>
                    {items.map((v, i) => (
                      <li key={i}>{v}</li>
                    ))}
                  </ul>
                ) : (
                  <p>None</p>
                )}
              </section>
            ))}
            <p>
              New access requires a different authorized reviewer. Ownership and
              recovery access use direct assignments.
            </p>
            {message && <p role="alert">{message}</p>}
            <div className={styles.actions}>
              <button
                className="button"
                disabled={busy}
                onClick={() => void commit()}
              >
                {busy ? "Saving…" : "Confirm changes"}
              </button>
              <button
                className="button secondary"
                disabled={busy}
                onClick={() => setPreview(null)}
              >
                Back to editing
              </button>
            </div>
          </div>
        )}
      </Dialog>
    </div>
  );
}

function AttributeValueForm({
  target,
  objectId,
  data,
  onSubmit,
  busy,
}: {
  target: "person" | "group" | "unit";
  objectId: string;
  data: ManagementSnapshot;
  onSubmit: (change: ManagementChange) => Promise<void>;
  busy: boolean;
}) {
  const fields = data.attributes.filter(
      (a) => a.target === target && !a.archived && a.canView !== false,
    ),
    record = data.values.find(
      (v) => v.target === target && v.objectId === objectId,
    ),
    canEdit =
      target === "person"
        ? data.canManagePeople
        : target === "unit"
          ? data.canManageOrganization
          : data.canManageGroups || data.canManage;
  if (!fields.length) return null;
  return (
    <form
      className={styles.form}
      onSubmit={(e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget),
          values: Record<string, unknown> = {};
        for (const a of fields) {
          if (a.canEdit === false) continue;
          const v = f.get(a.id);
          values[a.id] =
            a.type === "checkbox"
              ? v === "on"
              : a.type === "multi-select"
                ? f.getAll(a.id)
                : a.type === "number"
                  ? v === ""
                    ? null
                    : Number(v)
                  : v;
        }
        void onSubmit({
          operation: "values",
          input: {
            target,
            objectId,
            expectedVersion: record?.version ?? 0,
            values,
            inherit: f.getAll("inherit").map(String),
            reason: str(f, "reason"),
          },
        });
      }}
    >
      <h3>Custom attributes</h3>
      {fields.map((a) => (
        <label key={a.id}>
          {a.label}
          {a.required ? " *" : ""}
          {a.type === "checkbox" ? (
            <input
              type="checkbox"
              name={a.id}
              defaultChecked={record?.values[a.id] === true}
              disabled={!canEdit || a.canEdit === false}
            />
          ) : a.type.endsWith("select") ? (
            <select
              name={a.id}
              multiple={a.type === "multi-select"}
              defaultValue={
                (record?.values[a.id] ??
                  (a.type === "multi-select" ? [] : "")) as string | string[]
              }
              required={a.required}
              disabled={!canEdit || a.canEdit === false}
            >
              {a.type === "single-select" && <option value="">Choose…</option>}
              {a.options.map((o) => (
                <option key={o}>{o}</option>
              ))}
            </select>
          ) : (
            <input
              name={a.id}
              type={
                a.type === "number"
                  ? "number"
                  : a.type === "date"
                    ? "date"
                    : "text"
              }
              step="any"
              maxLength={2000}
              required={a.required}
              defaultValue={String(record?.values[a.id] ?? "")}
              disabled={!canEdit || a.canEdit === false}
            />
          )}
          <small>{a.help}{record?.inheritedFrom?.[a.id] ? " · Inherited from "+(data.nodes.find(n=>n.id===record.inheritedFrom![a.id])?.label??"parent organization") : ""}</small>
          {target==="unit"&&canEdit&&a.canEdit!==false&&!record?.inheritedFrom?.[a.id]&&<span><input type="checkbox" name="inherit" value={a.id}/>Use inherited value</span>}
        </label>
      ))}
      {canEdit && (
        <>
          <Reason />
          <button className="button secondary" disabled={busy}>
            Preview attribute changes
          </button>
        </>
      )}
    </form>
  );
}

function ManagementEditor({
  editor,
  data,
  roles,
  busy,
  onSubmit,
}: {
  editor: Editor;
  data: ManagementSnapshot;
  roles: RoleRevision[];
  busy: boolean;
  onSubmit: (change: ManagementChange) => Promise<void>;
}) {
  const [groupKind, setGroupKind] = useState(
      editor.kind === "group" ? (editor.item?.kind ?? "manual") : "manual",
    ),
    [rule, setRule] = useState<MembershipRule>(
      editor.kind === "group" && editor.item?.rule
        ? editor.item.rule
        : {
            mode: "all",
            conditions: [{ field: "worker_type", op: "eq", value: "employee" }],
          },
    ),
    [type, setType] = useState<AttributeDefinition["type"]>(
      editor.kind === "attribute" ? (editor.item?.type ?? "text") : "text",
    ),
    [grants, setGrants] = useState<AccessPolicy["grants"]>(() =>
      editor.kind === "policy"
        ? (editor.item?.grants ?? [
            {
              roleId: "employee",
              expectedRoleRevision:
                roles.find((r) => r.id === "employee")?.revision ?? 1,
              scopeKind: "holding",
              scopeNodeId: null,
              validUntil: Date.now() + 30 * 86400000,
            },
          ])
        : [],
    ),
    [target, setTarget] = useState<"group" | "unit">(
      editor.kind === "attachment"
        ? (editor.item?.target ?? editor.target ?? "group")
        : "group",
    );
  const item = "item" in editor ? editor.item : undefined;
  const ruleFields = [
    ...personRuleFields,
    ...data.attributes
      .filter((a) => a.target === "person" && !a.archived)
      .map((a) => ({ id: "custom:" + a.id, label: a.label, type: a.type })),
  ];
  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget),
      base = {
        id: item?.id,
        expectedVersion: item?.version ?? 0,
        reason: str(f, "reason"),
      },
      archived = f.get("archived") === "on";
    let change: ManagementChange;
    if (editor.kind === "group")
      change = {
        operation: "group",
        input: {
          ...base,
          name: str(f, "name"),
          kind: groupKind,
          parentId: str(f, "parentId") || null,
          scopeNodeId: str(f, "scopeNodeId") || null,
          rule: groupKind === "automatic" ? rule : null,
          archived,
        },
      };
    else if (editor.kind === "attribute")
      change = {
        operation: "attribute",
        input: {
          ...base,
          label: str(f, "label"),
          help: str(f, "help"),
          ownerRole: str(f, "ownerRole") || undefined,
          readRoles: f.getAll("readRoles").map(String),
          writeRoles: f.getAll("writeRoles").map(String),
          branchOverride: f.get("branchOverride") === "on",
          target: str(f, "target") as AttributeDefinition["target"],
          type,
          required: f.get("required") === "on",
          options: str(f, "options")
            .split("\n")
            .map((v) => v.trim())
            .filter(Boolean),
          archived,
        },
      };
    else if (editor.kind === "policy")
      change = {
        operation: "policy",
        input: { ...base, name: str(f, "name"), grants, archived },
      };
    else if (editor.kind === "attachment")
      change = {
        operation: "attachment",
        input: {
          ...base,
          policyId: str(f, "policyId"),
          target,
          targetId: str(f, "targetId"),
          exclusions: f.getAll("exclusions").map(String),
          archived,
        },
      };
    else {
      const group = data.groups.find((g) => g.id === str(f, "groupId"));
      if (!group) return;
      change = {
        operation: "membership",
        input: {
          groupId: group.id,
          expectedVersion: group.version,
          personIds: editor.personIds,
          action: str(f, "action") as "add" | "remove",
          reason: str(f, "reason"),
        },
      };
    }
    void onSubmit(change);
  }
  return (
    <form className={styles.form} onSubmit={submit}>
      {editor.kind === "group" && (
        <>
          <label>
            Group name
            <input
              name="name"
              defaultValue={editor.item?.name}
              required
              minLength={2}
              maxLength={100}
            />
          </label>
          <label>
            Membership
            <select
              value={groupKind}
              onChange={(e) =>
                setGroupKind(e.target.value as "manual" | "automatic")
              }
            >
              <option value="manual">Choose members manually</option>
              <option value="automatic">Automatically match a rule</option>
            </select>
          </label>
          <label>
            Parent group
            <select name="parentId" defaultValue={editor.item?.parentId ?? ""}>
              <option value="">No parent</option>
              {data.groups
                .filter(
                  (g) =>
                    g.kind === "manual" &&
                    !g.archived &&
                    g.id !== editor.item?.id,
                )
                .map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Workspace boundary
            <select
              name="scopeNodeId"
              defaultValue={editor.item?.scopeNodeId ?? ""}
            >
              <option value="">Company</option>
              {data.nodes
                .filter((n) => n.kind !== "company")
                .map((n) => (
                  <option key={n.id} value={n.id}>
                    {n.label}
                  </option>
                ))}
            </select>
          </label>
          {groupKind === "automatic" && (
            <fieldset>
              <legend>Membership rule</legend>
              <label>
                Match
                <select
                  value={rule.mode}
                  onChange={(e) =>
                    setRule({ ...rule, mode: e.target.value as "all" | "any" })
                  }
                >
                  <option value="all">All conditions</option>
                  <option value="any">Any condition</option>
                </select>
              </label>
              {rule.conditions.map((c, i) => {
                const field = ruleFields.find((f) => f.id === c.field),
                  custom = data.attributes.find(
                    (a) => "custom:" + a.id === c.field,
                  ),
                  update = (changes: Partial<typeof c>) =>
                    setRule({
                      ...rule,
                      conditions: rule.conditions.map((old, index) =>
                        index === i ? { ...old, ...changes } : old,
                      ),
                    });
                return (
                  <div className={styles.condition} key={i}>
                    <label>
                      Field
                      <select
                        value={c.field}
                        onChange={(e) => {
                          const next = ruleFields.find(
                            (f) => f.id === e.target.value,
                          );
                          update({
                            field: e.target.value,
                            op:
                              next?.type === "multi-select" ? "contains" : "eq",
                            value: next?.type === "checkbox" ? false : "",
                          });
                        }}
                      >
                        {ruleFields.map((f) => (
                          <option key={f.id} value={f.id}>
                            {f.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      Condition
                      <select
                        value={c.op}
                        onChange={(e) =>
                          update({ op: e.target.value as typeof c.op })
                        }
                      >
                        {[
                          "eq",
                          "neq",
                          "contains",
                          "gt",
                          "gte",
                          "lt",
                          "lte",
                          "empty",
                          "not-empty",
                        ]
                          .filter(
                            (o) =>
                              ["empty", "not-empty"].includes(o) ||
                              (field?.type === "multi-select"
                                ? o === "contains"
                                : ["eq", "neq"].includes(o) ||
                                  (["number", "date"].includes(
                                    field?.type ?? "",
                                  ) &&
                                    ["gt", "gte", "lt", "lte"].includes(o))),
                          )
                          .map((o) => (
                            <option key={o} value={o}>
                              {
                                {
                                  eq: "Equals",
                                  neq: "Does not equal",
                                  contains: "Includes",
                                  gt: "Greater / after",
                                  gte: "At least / on or after",
                                  lt: "Less / before",
                                  lte: "At most / on or before",
                                  empty: "Is empty",
                                  "not-empty": "Is not empty",
                                }[o]
                              }
                            </option>
                          ))}
                      </select>
                    </label>
                    {!["empty", "not-empty"].includes(c.op) && (
                      <label>
                        Value
                        {["branch_id", "department_id"].includes(c.field) ? (
                          <select
                            value={String(c.value ?? "")}
                            onChange={(e) => update({ value: e.target.value })}
                            required
                          >
                            <option value="">Choose a unit…</option>
                            {data.nodes
                              .filter(
                                (n) =>
                                  n.kind ===
                                  (c.field === "branch_id"
                                    ? "branch"
                                    : "department"),
                              )
                              .map((n) => (
                                <option key={n.id} value={n.id}>
                                  {n.label}
                                </option>
                              ))}
                          </select>
                        ) : custom?.type.endsWith("select") ? (
                          <select
                            value={String(c.value ?? "")}
                            onChange={(e) => update({ value: e.target.value })}
                          >
                            <option value="">Choose…</option>
                            {custom.options.map((o) => (
                              <option key={o}>{o}</option>
                            ))}
                          </select>
                        ) : field?.type === "checkbox" ? (
                          <select
                            value={String(c.value ?? false)}
                            onChange={(e) =>
                              update({ value: e.target.value === "true" })
                            }
                          >
                            <option value="true">Yes</option>
                            <option value="false">No</option>
                          </select>
                        ) : (
                          <input
                            type={
                              field?.type === "number"
                                ? "number"
                                : field?.type === "date"
                                  ? "date"
                                  : "text"
                            }
                            step="any"
                            value={String(c.value ?? "")}
                            onChange={(e) =>
                              update({
                                value:
                                  field?.type === "number"
                                    ? Number(e.target.value)
                                    : e.target.value,
                              })
                            }
                          />
                        )}
                      </label>
                    )}
                    <button
                      type="button"
                      disabled={rule.conditions.length === 1}
                      onClick={() =>
                        setRule({
                          ...rule,
                          conditions: rule.conditions.filter((_, j) => j !== i),
                        })
                      }
                    >
                      Remove condition
                    </button>
                  </div>
                );
              })}
              <button
                type="button"
                disabled={rule.conditions.length >= 20}
                onClick={() =>
                  setRule({
                    ...rule,
                    conditions: [
                      ...rule.conditions,
                      { field: "job_title", op: "eq", value: "" },
                    ],
                  })
                }
              >
                Add condition
              </button>
              <p>
                The preview shows everyone matching this rule. New access still
                requires review.
              </p>
            </fieldset>
          )}
        </>
      )}
      {editor.kind === "attribute" && (
        <>
          <label>Data owner role<select name="ownerRole" defaultValue={editor.item?.ownerRole ?? ""}>
            <option value="">Company administration</option>
            {roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
          </select></label>
          <label>Roles allowed to view<select multiple name="readRoles" defaultValue={editor.item?.readRoles ?? []}>
            {roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
          </select><small>Leave empty to retain the existing object view permissions.</small></label>
          <label>Roles allowed to edit<select multiple name="writeRoles" defaultValue={editor.item?.writeRoles ?? []}>
            {roles.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
          </select><small>Editors also need permission to manage this person, group, or unit.</small></label>
          <label><input type="checkbox" name="branchOverride" defaultChecked={editor.item?.branchOverride !== false} />Allow Branch overrides</label>
          <label>
            Label
            <input
              name="label"
              defaultValue={editor.item?.label}
              required
              minLength={2}
            />
          </label>
          <label>
            Applies to
            <select
              name="target"
              defaultValue={editor.item?.target ?? "person"}
            >
              {["person", "group", "unit"].map((t) => (
                <option key={t} value={t}>
                  {
                    {
                      person: "People",
                      group: "Local groups",
                      unit: "Organizational units",
                    }[t]
                  }
                </option>
              ))}
            </select>
          </label>
          <label>
            Field type
            <select
              value={type}
              onChange={(e) => setType(e.target.value as typeof type)}
            >
              {attributeTypes.map((t) => (
                <option key={t}>{t}</option>
              ))}
            </select>
          </label>
          <label>
            Help text
            <input
              name="help"
              defaultValue={editor.item?.help}
              maxLength={500}
            />
          </label>
          {type.endsWith("select") && (
            <label>
              Choices, one per line
              <textarea
                name="options"
                required
                defaultValue={editor.item?.options.join("\n")}
              />
            </label>
          )}
          <label>
            <input
              type="checkbox"
              name="required"
              defaultChecked={editor.item?.required}
            />
            Required when editing attributes
          </label>
        </>
      )}
      {editor.kind === "policy" && (
        <>
          <label>
            Policy name
            <input
              name="name"
              defaultValue={editor.item?.name}
              required
              minLength={2}
            />
          </label>
          {grants.map((g, i) => {
            const update = (v: Partial<typeof g>) =>
              setGrants(
                grants.map((old, j) => (j === i ? { ...old, ...v } : old)),
              );
            return (
              <fieldset key={i}>
                <legend>Role {i + 1}</legend>
                <label>
                  Role
                  <select
                    value={g.roleId}
                    onChange={(e) => {
                      const role = roles.find((r) => r.id === e.target.value)!;
                      update({
                        roleId: role.id,
                        expectedRoleRevision: role.revision,
                      });
                    }}
                  >
                    {roles
                      .filter(
                        (r) =>
                          !r.permissions.includes("company.owner") &&
                          !r.permissions.includes("iam.recovery"),
                      )
                      .map((r) => (
                        <option key={r.id} value={r.id}>
                          {r.name}
                        </option>
                      ))}
                  </select>
                </label>
                <label>
                  Access scope
                  <select
                    value={g.scopeNodeId ?? ""}
                    onChange={(e) => {
                      const n = data.nodes.find((n) => n.id === e.target.value);
                      update({
                        scopeNodeId: n?.id ?? null,
                        scopeKind:
                          (n?.kind as "branch" | "department" | undefined) ??
                          "holding",
                      });
                    }}
                  >
                    <option value="">Company</option>
                    {data.nodes
                      .filter((n) => ["branch", "department"].includes(n.kind))
                      .map((n) => (
                        <option key={n.id} value={n.id}>
                          {n.label} ({n.kind})
                        </option>
                      ))}
                  </select>
                </label>
                <label>
                  Expires on (UTC)
                  <input
                    type="date"
                    required
                    value={new Date(g.validUntil).toISOString().slice(0, 10)}
                    onChange={(e) => {
                      if (e.target.value)
                        update({
                          validUntil: Date.parse(e.target.value + "T23:59:59Z"),
                        });
                    }}
                  />
                </label>
                <button
                  type="button"
                  disabled={grants.length === 1}
                  onClick={() => setGrants(grants.filter((_, j) => j !== i))}
                >
                  Remove role
                </button>
              </fieldset>
            );
          })}
          <button
            type="button"
            disabled={grants.length >= 20}
            onClick={() => setGrants([...grants, { ...grants[0] }])}
          >
            Add another role
          </button>
        </>
      )}
      {editor.kind === "attachment" && (
        <>
          <label>
            Policy
            <select
              name="policyId"
              required
              defaultValue={editor.item?.policyId ?? editor.policyId ?? ""}
            >
              <option value="">Choose a policy</option>
              {data.policies
                .filter((p) => !p.archived)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Attach to
            <select
              value={target}
              onChange={(e) => setTarget(e.target.value as "group" | "unit")}
            >
              <option value="group">Local group</option>
              <option value="unit">Organizational unit</option>
            </select>
          </label>
          <label>
            Target
            <select
              name="targetId"
              key={target}
              required
              defaultValue={editor.item?.targetId ?? editor.targetId ?? ""}
            >
              <option value="">Choose…</option>
              {(target === "group"
                ? data.groups
                    .filter((g) => !g.archived)
                    .map((g) => ({ id: g.id, label: g.name }))
                : data.nodes
              ).map((n) => (
                <option key={n.id} value={n.id}>
                  {n.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Exclude subtrees (optional)
            <select
              name="exclusions"
              multiple
              defaultValue={editor.item?.exclusions ?? []}
            >
              {(target === "group"
                ? data.groups
                    .filter((g) => !g.archived)
                    .map((g) => ({ id: g.id, label: g.name }))
                : data.nodes
              ).map((n) => (
                <option key={n.id} value={n.id}>
                  {n.label}
                </option>
              ))}
            </select>
          </label>
          <p>
            The policy reaches descendants, but its access scopes and expiry do
            not expand.
          </p>
        </>
      )}
      {editor.kind === "membership" && (
        <>
          <p>{editor.personIds.length} selected people</p>
          <label>
            Manual group
            <select name="groupId" required defaultValue={editor.groupId ?? ""}>
              <option value="">Choose…</option>
              {data.groups
                .filter((g) => g.kind === "manual" && !g.archived)
                .map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Action
            <select name="action">
              <option value="add">Add membership (keep other groups)</option>
              <option value="remove">Remove direct membership</option>
            </select>
          </label>
          <p>
            Inherited membership remains while a person belongs to a child
            group.
          </p>
        </>
      )}
      {item && (
        <label>
          <input
            type="checkbox"
            name="archived"
            defaultChecked={item.archived}
          />
          Archive this {editor.kind}
        </label>
      )}
      <Reason />
      <button className="button" disabled={busy}>
        {busy ? "Preparing preview…" : "Preview changes"}
      </button>
    </form>
  );
}
