"use client";
import { useState, useRef, useEffect } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Search, List, Network, UserRound } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import dynamic from "next/dynamic";
const LifecyclePanel=dynamic(()=>import("./lifecycle-workspace").then(module=>module.LifecyclePanel));
import { hasLifecycleCaseDetail, type PeopleLifecycleData } from "@/lib/people-lifecycle";
import { positionGpoStatus } from "@/components/people/gpo-status";
import type { OrganizationOverview } from "@/lib/organization";
import { PeopleEditor } from "@/components/modules/people-editor";
import { useHydrated } from "@/components/ui/use-hydrated";
import styles from "./people-directory.module.css";

export type PeopleDirectoryRow = {
  retirementDate?: string | null;
  retired?: boolean | null;
  id: string;
  name: string;
  email: string;
  jobTitle: string;
  status: string;
  personId: string | null;
  userId: string | null;
  engagementId: string | null;
  positionId: string | null;
  managerPersonId: string | null;
  accounts?: { id: string; name: string; email: string }[];
};
export type PeopleDirectoryEditor = {
  person?: Record<string, unknown>;
  member?: { id: string; name: string; email: string };
  nodes: Record<string, unknown>[];
  levels: Record<string, unknown>[];
  people: Record<string, unknown>[];
  organization: OrganizationOverview;
  levelManagedSeparately: boolean;
};
export function PeopleDirectory({
  base,
  rows,
  lifecycle,
  hasExplorer,
  selectedRow,
  initialPositionId,
  editor,
  canManage = false,
  queryPrefix = "",
}: {
  base: string;
  queryPrefix?: string;
  rows: PeopleDirectoryRow[];
  lifecycle: PeopleLifecycleData | null;
  hasExplorer?: boolean;
  selectedRow?: PeopleDirectoryRow | null;
  initialCaseId?: string;
  initialMemberId?: string;
  initialPositionId?: string;
  editor?: PeopleDirectoryEditor | null;
  canManage?: boolean;
}) {
  const rawSearch = useSearchParams();
  const ready = useHydrated();
  const search = {get:(key:string)=>rawSearch.get(queryPrefix+key)};
  const router = useRouter();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [pendingSelected, setPendingSelected] = useState<string | null>(null);
  useEffect(()=>()=>{if(timer.current) clearTimeout(timer.current);},[]);
  const patchUrl = (patch: Record<string,string|undefined>, replace=false) => {
    const url = new URL(window.location.href);
    for (const [key,value] of Object.entries(patch)) { if (value) url.searchParams.set(queryPrefix+key,value); else url.searchParams.delete(queryPrefix+key); }
    if (timer.current) clearTimeout(timer.current);
    if (Object.keys(patch).some(key=>["q","retirement","display"].includes(key))) url.searchParams.delete(queryPrefix+"cursor");
    const navigate=()=>router[replace ? "replace" : "push"](`${url.pathname}${url.search}`,{scroll:false});
    if (replace) timer.current=setTimeout(navigate,300); else navigate();
  };
  const casePerson = lifecycle?.cases.find(c=>c.id === search.get("case"))?.person_id;
  const query = search.get("q") ?? "";
  useEffect(()=>{if(searchInput.current)searchInput.current.value=query;},[query]);
  const setQuery = (value:string) => patchUrl({q:value || undefined},true);
  const mode = hasExplorer && search.get("display") === "explorer" ? "explorer" : "list";
  const setMode = (value:string) => patchUrl({display:value === "list" ? undefined : value});
  const detailRows = selectedRow ? [...rows.filter(p=>p.id!==selectedRow.id),selectedRow] : rows;
  const routeSelected = detailRows.find(p=>p.id === search.get("person") || (casePerson ? p.personId === casePerson : !!search.get("member") && (p.userId === search.get("member") || p.accounts?.some(a=>a.id === search.get("member")))))?.id ?? null;
  const selected = pendingSelected && pendingSelected !== routeSelected ? pendingSelected : routeSelected;
  const setSelected = (value:string|null) => patchUrl({person:value ?? undefined,case:undefined,member:undefined,position:undefined});
  const [targetPosition,setTargetPosition] = useState(initialPositionId);
  const retirementFilter = search.get("retirement") ?? "all";
  const setRetirementFilter = (value:string) => patchUrl({retirement:value === "all" ? undefined : value});
  const retirement = (person:PeopleDirectoryRow) => person.retirementDate ? {cutoffEffective:person.retired,definition:{lastWorkingDate:person.retirementDate}} : undefined;
  const person = detailRows.find(p=>p.id===selected), filtered = rows;
  const editorId = String(editor?.person?.id ?? editor?.member?.id ?? "");
  const editorReady = !!person && !!editorId && [person.personId, person.userId, person.id].includes(editorId);
  const closePerson = () => {
    setPendingSelected(null);
    setSelected(null);
    setTargetPosition(undefined);
  };
  const returnToDirectory = () => {
    const params = new URLSearchParams(rawSearch);
    for (const key of ["person", "case", "member", "position"])
      params.delete(queryPrefix + key);
    const queryString = params.toString();
    return `${base}/people${queryString ? `?${queryString}` : ""}`;
  };
  return (
    <div className={styles.workspace}>
      <div className={styles.toolbar}>
        <label className={styles.search}>
          <Search size={17} />
          <input
            aria-label="Search people"
            placeholder="Search name, email, or position"
            ref={searchInput}
            defaultValue={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <select className="input" aria-label="Retirement status" value={retirementFilter} onChange={e=>setRetirementFilter(e.target.value)}><option value="all">All people</option><option value="upcoming">Upcoming retirement</option><option value="retired">Retired</option></select>
        {hasExplorer && (
          <div className={styles.switcher} aria-label="People display">
            <button
              aria-pressed={mode === "list"}
              onClick={() => setMode("list")}
            >
              <List size={16} /> List
            </button>
            <button
              aria-pressed={mode === "explorer"}
              onClick={() => setMode("explorer")}
            >
              <Network size={16} /> Explorer
            </button>
          </div>
        )}
      </div>
      {
        <section>
          <div className={styles.listHead}>
            <strong>People</strong>
            <span>{filtered.length} people and linked accounts on this page</span>
          </div>
          {!filtered.length && (
            <p className={styles.empty}>No matching people.</p>
          )}
          {filtered.map((p) => (
            <button
              key={p.id}
              className={styles.row}
              disabled={!ready}
              onClick={() => {
                setPendingSelected(p.id);
                setSelected(p.id);
              }}
              draggable={!!p.personId && !!lifecycle?.canManage}
              onDragStart={(e) =>
                e.dataTransfer.setData("application/erpx-person", p.id)
              }
            >
              <span className={styles.avatar}>
                <UserRound size={19} />
              </span>
              <span className={styles.name}>
                <strong>{p.name}</strong>
                <span>
                  {p.jobTitle || "Profile setup pending"} ·{" "}
                  {p.email || "No work email"}
                </span>
              </span>
              <span className={styles.status}>{retirement(p) ? retirement(p)!.cutoffEffective ? "Retired" : `Retires ${retirement(p)!.definition.lastWorkingDate}` : p.status}</span>
              <span className={styles.account}>
                {(p.accounts?.length ?? 0) > 1
                  ? `${p.accounts!.length} accounts linked`
                  : p.userId
                    ? "Account linked"
                    : "No verified account"}
              </span>
            </button>
          ))}
        </section>
      }
      {mode === "explorer" && lifecycle?.canManage && (
        <section>
          <h2>Assign or transfer to a position</h2>
          <p className="muted">
            Drag a person here, or open their details and choose Start a change.
            Both open the same preview. HR can choose any active position;
            GPO preparation continues separately in Technology.
          </p>
          <div className={styles.targets}>
            {lifecycle.positions
              .filter((p) => p.active)
              .map((p) => (
                <div
                  key={p.id}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    e.preventDefault();
                    const id = e.dataTransfer.getData(
                      "application/erpx-person",
                    );
                    if (rows.some((r) => r.id === id && r.engagementId)) {
                      setSelected(id);
                      setTargetPosition(p.id);
                    }
                  }}
                >
                  <strong>{p.title}</strong>
                  <span>{positionGpoStatus(p.gpo)}</span>
                </div>
              ))}
          </div>
        </section>
      )}
      <Dialog
        open={!!person}
        title={editor?.member && person ? `Complete ${person.name}` : person && canManage ? `Edit ${person.name}` : person?.name ?? "Person"}
        onClose={closePerson}
        presentation="responsive-panel"
      >
        {person && (
          <div className={styles.detail}>
            <button type="button" className="button secondary" onClick={closePerson}>
              ← Back to people
            </button>
            {canManage && !editorReady ? (
              <p role="status">Preparing the employee editor…</p>
            ) : editor && (person.personId || editor.member) ? (
              <PeopleEditor
                key={person.personId ?? editor.member?.id}
                alwaysOpen
                lifecycleManaged
                base={base}
                person={editor.person}
                member={editor.member}
                nodes={editor.nodes}
                levels={editor.levels}
                people={editor.people}
                organization={editor.organization}
                gpoPositions={lifecycle?.positions}
                levelManagedSeparately={editor.levelManagedSeparately}
                returnTo={returnToDirectory()}
              />
            ) : (
              <>
            <p>
              {person.jobTitle} · {person.email}
            </p>
            <p>
              {person.status} ·{" "}
              {person.userId
                ? "Verified account linked"
                : "Identity verification required before account linking"}
            </p>
            {person.userId && <Link className="button secondary" href={`${base}/profiles/${encodeURIComponent(person.userId)}`}>View member profile</Link>}
            {(person.accounts?.length ?? 0) > 1 && (
              <div>
                <strong>Verified accounts</strong>
                <ul>
                  {person.accounts!.map((account) => (
                    <li key={account.id}>
                      {account.name} · {account.email || "Directory account"}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {person.personId ? (
              <Link
                className="button secondary"
                href={`${base}/people/${encodeURIComponent(person.personId)}`}
              >
                View profile, engagement & account details
              </Link>
            ) : (
              <Link
                className="button secondary"
                href={`${base}/people?member=${encodeURIComponent(person.userId ?? person.id)}`}
              >
                Complete employee profile
              </Link>
            )}
              </>
            )}
            {lifecycle && person.personId && person.engagementId && (
              <LifecyclePanel
                key={person.id + String(targetPosition)}
                base={base}
                data={lifecycle}
                person={{
                  id: person.personId,
                  name: person.name,
                  status: person.status,
                  engagementId: person.engagementId,
                  positionId: person.positionId,
                  managerPersonId: person.managerPersonId,
                }}
                initialCase={lifecycle.cases.filter(hasLifecycleCaseDetail).find(
                  (c) =>
                    c.id === search.get("case") &&
                    c.person_id === person.personId,
                )}
                initialPositionId={targetPosition}
              />
            )}
          </div>
        )}
      </Dialog>
    </div>
  );
}
