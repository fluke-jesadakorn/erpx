"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, InlineNotice, Panel, RecordList, Select, TextInput } from "@/components/ui/foundation";

import { useState, useRef, useEffect } from "react";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useRouter } from "@/lib/shared/destination-router";
import { Search, List, Network, UserRound } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import {DirectoryLifecyclePanel} from "./directory-lifecycle-panel";
import { hasLifecycleCaseDetail, type PeopleLifecycleData } from "@/lib/people_payroll/people-lifecycle";
import { positionGpoStatus } from "@/components/people_payroll/gpo-status";
import type { OrganizationOverview } from "@/lib/organization/organization";
import { PeopleEditor } from "@/components/people_payroll/people-editor";
import { useHydrated } from "@/components/ui/use-hydrated";
import styles from "./people-directory-workspace.module.css";
import {peopleLifecyclePurchased} from "@/lib/people_payroll/purchase-controls";

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
export function PeopleDirectoryWorkspace({
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
  const search = { get: (key: string) => rawSearch.get(queryPrefix + key) };
  const router = useRouter();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [pendingSelected, setPendingSelected] = useState<string | null>(null);
  const [retainedPerson, setRetainedPerson] = useState<string | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const patchUrl = (patch: Record<string, string | undefined>, replace = false) => {
    const url = new URL(window.location.href);
    for (const [key, value] of Object.entries(patch)) {
      if (value) url.searchParams.set(queryPrefix + key, value);
      else url.searchParams.delete(queryPrefix + key);
    }
    if (timer.current) clearTimeout(timer.current);
    if (Object.keys(patch).some((key) => ["q", "retirement", "display"].includes(key)))
      url.searchParams.delete(queryPrefix + "cursor");
    const navigate = () =>
      router[replace ? "replace" : "push"](`${url.pathname}${url.search}`, { scroll: false });
    if (replace) timer.current = setTimeout(navigate, 300);
    else navigate();
  };
  const casePerson = lifecycle?.cases.find((c) => c.id === search.get("case"))?.person_id;
  const query = search.get("q") ?? "";
  useEffect(() => {
    if (searchInput.current) searchInput.current.value = query;
  }, [query]);
  const setQuery = (value: string) => patchUrl({ q: value || undefined }, true);
  const mode = hasExplorer && search.get("display") === "explorer" ? "explorer" : "list";
  const setMode = (value: string) => patchUrl({ display: value === "list" ? undefined : value });
  const detailRows = selectedRow
    ? [...rows.filter((p) => p.id !== selectedRow.id), selectedRow]
    : rows;
  const routeSelected =
    detailRows.find(
      (p) =>
        p.id === search.get("person") ||
        (casePerson
          ? p.personId === casePerson
          : !!search.get("member") &&
            (p.userId === search.get("member") ||
              p.accounts?.some((a) => a.id === search.get("member")))),
    )?.id ?? null;
  const selected =
    pendingSelected && pendingSelected !== routeSelected ? pendingSelected : routeSelected;
  const setSelected = (value: string | null) => {
    if (!ownerSwitchAllowed()) return false;
    patchUrl({person: value ?? undefined, case: undefined, member: undefined, position: undefined});
    return true;
  };
  const [targetPosition, setTargetPosition] = useState(initialPositionId);
  const retirementFilter = search.get("retirement") ?? "all";
  const setRetirementFilter = (value: string) =>
    patchUrl({ retirement: value === "all" ? undefined : value });
  const retirement = (person: PeopleDirectoryRow) =>
    person.retirementDate
      ? { cutoffEffective: person.retired, definition: { lastWorkingDate: person.retirementDate } }
      : undefined;
  const person = detailRows.find((p) => p.id === selected),
    filtered = rows;
  const selectedMember = search.get("member");
  const editorOwner = person && selectedMember && (person.userId === selectedMember || person.accounts?.some(account => account.id === selectedMember)) ? `member:${selectedMember}` : person?.personId ?? person?.userId ?? person?.id ?? "";
  const editorId = String(editor?.person?.id ?? editor?.member?.id ?? "");
  const editorReady =
    !!person && !!editorId && [person.personId, person.userId, person.id].includes(editorId);
  const detailRoot = useRef<HTMLDivElement>(null);
  const [confirmDiscard,setConfirmDiscard]=useState(false), [closeError, setCloseError] = useState("");
  function ownerSwitchAllowed() {
    const root = detailRoot.current;
    if (root?.querySelector('[data-directory-command-pending="true"], [data-directory-command-busy="true"]')) {setCloseError("Reopen and recover the original People request before selecting another person or placement.");return false;}
    if (root?.querySelector('[data-people-draft][data-work-dirty="true"], [data-directory-owner-dirty="true"]')) {setCloseError("Reopen and keep or explicitly discard the retained People form before selecting another person or placement.");return false;}
    return true;
  }
  const canTransfer = !!lifecycle?.canManage && peopleLifecyclePurchased(lifecycle.purchases, "transfer");
  const finishClosePerson = () => {
    setPendingSelected(null);
    patchUrl({person: undefined, case: undefined, member: undefined, position: undefined});
    setTargetPosition(undefined);
  };
  const closePerson = (discard = false) => {
    const root = detailRoot.current;
    if (root?.querySelector('[data-directory-command-pending="true"]')) {setCloseError("Recover the original People request before closing these details."); return;}
    if (!discard && root?.querySelector('[data-people-draft][data-work-dirty="true"], [data-directory-owner-dirty="true"]')) {setConfirmDiscard(true); return;}
    if (discard) root?.querySelectorAll('[data-people-draft]').forEach(form => window.dispatchEvent(new CustomEvent("erpx:record-saved", {detail: {root: form}})));
    setConfirmDiscard(false); setCloseError(""); finishClosePerson();
  };
  const retainPerson = () => {
    if (detailRoot.current?.querySelector('[data-directory-command-busy="true"]')) {setCloseError("Wait for the current People request before closing its retained details.");return;}
    if (person) {setRetainedPerson(person.id);setConfirmDiscard(false);setCloseError("");}
  };
  const returnToDirectory = () => {
    const params = new URLSearchParams(rawSearch);
    for (const key of ["person", "case", "member", "position"]) params.delete(queryPrefix + key);
    const queryString = params.toString();
    return canonicalUiHref(`${base}/people${queryString ? `?${queryString}` : ""}`);
  };
  return (
    <div className={styles.workspace} data-ui-appearance="standard">
      <div className={styles.toolbar}>
        <Field appearance="standard" className={styles.search}>
          <Search size={17} />
          <TextInput appearance="standard"
            type="search"
            aria-label="Search people"
            placeholder="Search name, email, or position"
            ref={searchInput}
            defaultValue={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </Field>
        <Select appearance="standard"
          
          aria-label="Retirement status"
          value={retirementFilter}
          onChange={(e) => setRetirementFilter(e.target.value)}
        >
          <option value="all">All people</option>
          <option value="upcoming">Upcoming retirement</option>
          <option value="retired">Retired</option>
        </Select>
        {hasExplorer && (
          <div className={styles.switcher} role="group" aria-label="People display">
            <Button appearance="standard"
              type="submit"
              variant="plain"
              aria-pressed={mode === "list"}
              onClick={() => setMode("list")}
            >
              <List size={16} /> List
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="plain"
              aria-pressed={mode === "explorer"}
              onClick={() => setMode("explorer")}
            >
              <Network size={16} /> Explorer
            </Button>
          </div>
        )}
      </div>
      {
        <RecordList as="section">
          <div className={styles.listHead}>
            <strong>People</strong>
            <span>{filtered.length} people and linked accounts on this page</span>
          </div>
          {!filtered.length && <p className={styles.empty}>No matching people.</p>}
          {filtered.map((p) => (
            <Button appearance="standard"
              type="submit"
              variant="plain"
              key={p.id}
              className={styles.row}
              disabled={!ready}
              onClick={() => {
                if (setSelected(p.id)) setPendingSelected(p.id);
              }}
              draggable={!!p.personId && canTransfer}
              onDragStart={(e) => e.dataTransfer.setData("application/erpx-person", p.id)}
            >
              <span className={styles.avatar}>
                <UserRound size={19} />
              </span>
              <span className={styles.name}>
                <strong>{p.name}</strong>
                <span>
                  {p.jobTitle || "Profile setup pending"} · {p.email || "No work email"}
                </span>
              </span>
              <span className={styles.status}>
                {retirement(p)
                  ? retirement(p)!.cutoffEffective
                    ? "Retired"
                    : `Retires ${retirement(p)!.definition.lastWorkingDate}`
                  : p.status}
              </span>
              <span className={styles.account}>
                {(p.accounts?.length ?? 0) > 1
                  ? `${p.accounts!.length} accounts linked`
                  : p.userId
                    ? "Account linked"
                    : "No verified account"}
              </span>
            </Button>
          ))}
        </RecordList>
      }
      {mode === "explorer" && canTransfer && (
        <Panel appearance="standard">
          <h2>Assign or transfer to a position</h2>
          <p className="muted">
            Drag a person here, or open their details and choose Start a change. Both open the same
            preview. HR can choose any active position; GPO preparation continues separately in
            Technology.
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
                    const id = e.dataTransfer.getData("application/erpx-person");
                    if (rows.some((r) => r.id === id && r.engagementId)) {
                      if (setSelected(id)) {setPendingSelected(id);setTargetPosition(p.id);}
                    }
                  }}
                >
                  <strong>{p.title}</strong>
                  <span>{positionGpoStatus(p.gpo)}</span>
                </div>
              ))}
          </div>
        </Panel>
      )}
      {person && retainedPerson === person.id && <InlineNotice><p>The employee form and any original request remain in these retained details.</p>{closeError && <p role="alert">{closeError}</p>}<Button appearance="standard" type="button" variant="secondary" onClick={() => setRetainedPerson(null)}>Reopen retained person details</Button></InlineNotice>}
      <Dialog appearance="standard"
        retainFormContents
        open={!!person && retainedPerson !== person.id}
        title={
          editor?.member && person
            ? `Complete ${person.name}`
            : person && canManage
              ? `Edit ${person.name}`
              : (person?.name ?? "Person")
        }
        onClose={() => closePerson()}
        presentation="form-drawer"
      >
        {person && (
          <div className={styles.detail} ref={detailRoot}>
            {closeError && <InlineNotice tone="warning">{closeError}</InlineNotice>}
            <Button appearance="standard" variant="secondary" type="button" onClick={() => closePerson()}>
              ← Back to people
            </Button>
            {canManage && !editorReady ? (
              <p role="status">Preparing the employee editor…</p>
            ) : editor && (person.personId || editor.member) ? (
              <PeopleEditor
                appearance="standard"
                onCloseRetained={retainPerson}
                commandScope={{ownerKey: editorOwner, source: JSON.stringify([base, selectedRow, editor, lifecycle, canManage]), canManage}}
                purchases={lifecycle?.purchases}
                key={editorOwner}
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
                {person.userId && (
                  <UiActionLink appearance="standard"
                    variant="secondary"
                    href={canonicalUiHref(`${base}/profiles/${encodeURIComponent(person.userId)}`)}
                  >
                    View member profile
                  </UiActionLink>
                )}
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
                  <UiActionLink appearance="standard"
                    variant="secondary"

                    href={canonicalUiHref(`${base}/people/${encodeURIComponent(person.personId)}`)}
                  >
                    View profile, engagement & account details
                  </UiActionLink>
                ) : (
                  <UiActionLink appearance="standard"
                    variant="secondary"

                    href={canonicalUiHref(`${base}/people?member=${encodeURIComponent(person.userId ?? person.id)}`)}
                  >
                    Complete employee profile
                  </UiActionLink>
                )}
              </>
            )}
            {lifecycle && person.personId && person.engagementId && (
              <DirectoryLifecyclePanel
                parentOpen={retainedPerson !== person.id}
                onCloseRetained={retainPerson}
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
                initialCase={lifecycle.cases
                  .filter(hasLifecycleCaseDetail)
                  .find((c) => c.id === search.get("case") && c.person_id === person.personId)}
                initialPositionId={targetPosition}
              />
            )}
          </div>
        )}
      </Dialog>
      <Dialog appearance="standard" open={confirmDiscard} title="Discard People changes?" onClose={()=>setConfirmDiscard(false)}><p>Your edited employee form has unsaved changes.</p><div className="ui-form-actions"><Button appearance="standard" type="button" variant="secondary" onClick={()=>setConfirmDiscard(false)}>Keep editing</Button><Button appearance="standard" type="button" variant="danger" onClick={()=>closePerson(true)}>Discard changes</Button></div></Dialog>
    </div>
  );
}
