"use client";
import { Dialog } from "@/components/ui/dialog";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import {
  useEffect,
  useId,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
  type PointerEvent,
  type KeyboardEvent,
} from "react";
import { useRouter, useSearchParams } from "next/navigation";
import dynamic from "next/dynamic";
import {
  ArrowLeft,
  BriefcaseBusiness,
  ChevronDown,
  ChevronRight,
  GripVertical,
  MoreHorizontal,
  Search,
  Settings2,
  Users,
  X,
} from "lucide-react";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { workFetch } from "@/lib/work-fetch";
import { draftKey, type DraftDefinition, type DraftItem } from "@/lib/organization-draft";
import { peopleExplorerTabs, projectedDirectory } from "@/lib/people-explorer";
import type { peopleExplorerDetail } from "@/server/people-explorer-detail";
import type { peopleNewPersonData } from "@/server/people-hr-settings";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import type { OrganizationExplorerContext, OrganizationSelection } from "./organization-explorer";
import type { PanelControls } from "./organization-panel-rows";
import {
  PeopleOrganizationDetail,
  type PeopleOrganizationProps,
} from "./people-organization-workspace";
import { usePeopleIntegrationData } from "./people-integration-data";
import styles from "./people-structure-workspace.module.css";

const PeopleEditor = dynamic(() =>
  import("@/components/modules/people-editor").then((module) => module.PeopleEditor),
);
const OrganizationUnits = dynamic(() =>
  import("@/components/modules/organization-workspace").then((module) => module.OrganizationUnits),
);
const PositionEditor = dynamic(() =>
  import("@/components/modules/organization-workspace").then((module) => module.PositionEditor),
);
const PeopleHrSettings = dynamic(() =>
  import("./people-hr-settings").then((module) => module.PeopleHrSettings),
);
const PeopleTeamSchedule = dynamic(() =>
  import("./people-team-schedule").then((module) => module.PeopleTeamSchedule),
);
const PeopleWorkingTime = dynamic(() =>
  import("./people-team-schedule").then((module) => module.PeopleWorkingTime),
);
const PeoplePositionWorkingTime = dynamic(() =>
  import("./people-team-schedule").then((module) => module.PeoplePositionWorkingTime),
);

type Source = Pick<DraftItem, "kind" | "id">;
type Target = { kind: "branch" | "department" | "level" | "position"; id: string };
type Detail = Awaited<ReturnType<typeof peopleExplorerDetail>>;
const selectionKinds = [
  "person",
  "position",
  "department",
  "team",
  "branch",
  "company",
  "level",
  "unassigned",
] as const;
const overlayMedia = "(max-width: 1250px)";
const subscribeOverlay = (change: () => void) => {
  const media = window.matchMedia(overlayMedia);
  media.addEventListener("change", change);
  return () => media.removeEventListener("change", change);
};

function label(data: HoldingPeopleDirectory, selection: OrganizationSelection) {
  if (selection.kind === "person")
    return data.people.find((p) => p.engagementId === selection.id)?.legalName ?? "Employee";
  if (selection.kind === "position")
    return data.positions.find((p) => p.id === selection.id)?.title ?? "Position";
  if (selection.kind === "company" || selection.kind === "unassigned")
    return data.companies.find((c) => c.id === selection.id)?.name ?? "Company";
  if (selection.kind === "level")
    return data.levels.find((l) => l.id === selection.id)?.name ?? "Level";
  return data.nodes.find((n) => n.id === selection.id)?.name ?? "Unit";
}
function companyOf(data: HoldingPeopleDirectory, selection: OrganizationSelection) {
  if (selection.kind === "company" || selection.kind === "unassigned") return selection.id;
  if (selection.kind === "person")
    return data.people.find((p) => p.engagementId === selection.id)?.companyId;
  if (selection.kind === "position")
    return data.positions.find((p) => p.id === selection.id)?.companyId;
  if (selection.kind === "level") return data.levels.find((l) => l.id === selection.id)?.companyId;
  return data.nodes.find((n) => n.id === selection.id)?.companyId;
}
function descendants(data: HoldingPeopleDirectory, id: string) {
  const ids = new Set([id]);
  let growing = true;
  while (growing) {
    growing = false;
    for (const node of data.nodes)
      if (node.parentId && ids.has(node.parentId) && !ids.has(node.id)) {
        ids.add(node.id);
        growing = true;
      }
  }
  return ids;
}
function contextFor(
  data: HoldingPeopleDirectory,
  selection: OrganizationSelection,
): OrganizationExplorerContext {
  const companyId = companyOf(data, selection),
    ids = descendants(data, selection.id);
  const positions = data.positions.filter((p) =>
    selection.kind === "unassigned"
      ? false
      : selection.kind === "position"
        ? p.id === selection.id
        : selection.kind === "person"
          ? p.id === data.people.find((p) => p.engagementId === selection.id)?.positionId
          : selection.kind === "branch"
            ? p.branchId === selection.id
            : selection.kind === "department" || selection.kind === "team"
              ? !!p.departmentId && ids.has(p.departmentId)
              : selection.kind === "level"
                ? p.levelId === selection.id
                : p.companyId === companyId,
  );
  const people = data.people.filter((p) =>
    selection.kind === "person"
      ? p.engagementId === selection.id
      : selection.kind === "position"
        ? p.positionId === selection.id
        : selection.kind === "branch"
          ? p.branchId === selection.id
          : selection.kind === "department" || selection.kind === "team"
            ? !!p.departmentId && ids.has(p.departmentId)
            : selection.kind === "unassigned"
              ? p.companyId === companyId && !p.positionId
              : selection.kind === "level"
                ? positions.some((position) => position.id === p.positionId)
                : p.companyId === companyId,
  );
  return {
    company: data.companies.find((c) => c.id === companyId),
    people,
    positions,
    nodes: data.nodes.filter((n) => n.companyId === companyId),
    level: data.levels.find((l) => l.id === selection.id),
    directory: data,
  };
}

function PublicFacts({
  selection,
  context,
  archived,
}: {
  selection: OrganizationSelection;
  context: OrganizationExplorerContext;
  archived: boolean;
}) {
  const position = context.directory.positions.find((p) => p.id === selection.id);
  const values =
    selection.kind === "position"
      ? [
          [
            "Department",
            context.nodes.find((n) => n.id === position?.departmentId)?.name ?? "Company-wide",
          ],
          [
            "Level",
            context.directory.levels.find((l) => l.id === position?.levelId)?.name ?? "Unassigned",
          ],
          [
            "Reports to",
            context.directory.positions.find((p) => p.id === position?.reportsToPositionId)
              ?.title ?? "Company leadership",
          ],
          [
            "People",
            `${context.people.length} ${archived ? "archived" : context.people.length ? "current" : "· Vacant"}`,
          ],
        ]
      : [
          ["Company", context.company?.name ?? "Company"],
          ["People", String(context.people.length)],
          ["Positions", String(context.positions.length)],
        ];
  return (
    <dl className={styles.facts}>
      {values.map(([name, value]) => (
        <div key={name}>
          <dt>{name}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function LazyDetail({
  people,
  selection,
  section,
  context,
}: {
  people: PeopleOrganizationProps;
  selection: OrganizationSelection;
  section: string;
  context: OrganizationExplorerContext;
}) {
  const query = useSearchParams(),
    archived = query.get("status") === "archived";
  const filters = new URLSearchParams();
  if (section === "Changes")
    for (const name of ["changeId", "changeState", "changeQuery", "changePage"]) {
      const value = query.get(name);
      if (value !== null) filters.set(name, value);
    }
  const changeContext = filters.toString();
  const key = `${selection.kind}:${selection.id}:${section}:${archived}:${changeContext}`;
  const [result, setResult] = useState<{ key: string; detail: Detail } | null>(null);
  const [error, setError] = useState<{ key: string; message: string } | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    let alive = true;
    const params = new URLSearchParams({
      kind: selection.kind,
      id: selection.id,
      tab: section,
      archived: String(archived),
    });
    for (const [name, value] of new URLSearchParams(changeContext)) params.set(name, value);
    void workFetch(`/api/v1${people.base}/people/explorer-detail?${params}`, {
      signal: controller.signal,
    })
      .then(async (response) => {
        const payload = await response.json();
        if (!response.ok) throw new Error(payload.error?.message ?? "Unable to load this section.");
        if (alive) {
          setResult({ key, detail: payload.data });
          setError(null);
        }
      })
      .catch((error) => {
        if (alive && !controller.signal.aborted) setError({ key, message: error.message });
      });
    return () => {
      alive = false;
      controller.abort();
    };
  }, [
    people.base,
    people.data,
    selection.kind,
    selection.id,
    section,
    archived,
    key,
    attempt,
    changeContext,
  ]);
  if (error?.key === key)
    return (
      <div role="alert">
        {error.message}
        <Button variant="plain" type="button" onClick={() => setAttempt((a) => a + 1)}>
          Retry
        </Button>
      </div>
    );
  if (result?.key !== key) return <p role="status">Loading details…</p>;
  return (
    <>
      <PeopleOrganizationDetail
        {...people}
        {...result.detail}
        selection={selection}
        tab={section}
        context={context}
      />
      {section === "Overview" &&
        (!result.detail.organization ||
          selection.kind === "level" ||
          selection.kind === "unassigned") && (
          <PublicFacts selection={selection} context={context} archived={archived} />
        )}
    </>
  );
}

function Accordion({
  title,
  summary,
  initiallyOpen,
  children,
  onOpen,
}: {
  title: string;
  summary: string;
  initiallyOpen?: boolean;
  children: ReactNode;
  onOpen: () => void;
}) {
  const [open, setOpen] = useState(!!initiallyOpen);
  return (
    <details
      className={styles.section}
      open={open}
      onToggle={(event) => {
        const next = event.currentTarget.open;
        setOpen(next);
        if (next && !open) onOpen();
      }}
    >
      <summary>
        <span>
          {title}
          <small>{summary}</small>
        </span>
        <ChevronDown size={16} />
      </summary>
      {open && <div className={styles.sectionBody}>{children}</div>}
    </details>
  );
}

function trapFocus(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const elements = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, [tabindex="0"]',
    ),
  ).filter((el) => el.getClientRects().length > 0);
  const first = elements[0],
    last = elements.at(-1);
  if (
    event.shiftKey &&
    (document.activeElement === first || document.activeElement === event.currentTarget)
  ) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}

function NewPersonEditor({
  base,
  positionId,
  kind = "person",
}: {
  base: string;
  positionId?: string;
  kind?: string;
}) {
  const resource = usePeopleIntegrationData<Awaited<ReturnType<typeof peopleNewPersonData>>>(
    `/api/v1${base}/people/hr-settings?editor=person`,
  );
  if (resource.error)
    return (
      <p role="alert">
        {resource.error}
        <Button variant="plain" type="button" onClick={() => void resource.reload()}>
          Retry
        </Button>
      </p>
    );
  if (!resource.data) return <p role="status">Loading person form…</p>;
  const { editorNodes, staffOptions, organization, lifecycle } = resource.data;
  if (kind === "position")
    return organization ? (
      <PositionEditor base={base} data={organization} />
    ) : (
      <p>Position setup is unavailable in this workspace.</p>
    );
  if (kind === "unit")
    return organization ? (
      <OrganizationUnits base={base} data={organization} />
    ) : (
      <p>Unit setup is unavailable in this workspace.</p>
    );
  return (
    <PeopleEditor
      base={base}
      nodes={editorNodes}
      levels={staffOptions.levels}
      people={staffOptions.people}
      organization={organization ?? undefined}
      gpoPositions={lifecycle.positions}
      initialPositionId={positionId}
      initialOpen
      alwaysOpen
      returnTo={`${base}/people`}
    />
  );
}

function ItemActions({
  name,
  grip,
  onMove,
  onAddPerson,
}: {
  name: string;
  grip?: ReactNode;
  onMove?: () => void;
  onAddPerson?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId(),
    ref = useRef<HTMLSpanElement>(null),
    trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    ref.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const dismiss = (event: Event) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("focusin", dismiss);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("focusin", dismiss);
    };
  }, [open]);
  function select(action: () => void) {
    setOpen(false);
    trigger.current?.focus();
    action();
  }
  return (
    <span ref={ref} className={styles.itemActions}>
      {grip}
      <Button
        variant="plain"
        ref={trigger}
        className={styles.actionTrigger}
        type="button"
        aria-label={`Actions for ${name}`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <MoreHorizontal size={18} />
      </Button>
      {open && (
        <span
          id={id}
          className={styles.actionMenu}
          role="menu"
          aria-label={`Actions for ${name}`}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
              trigger.current?.focus();
            }
            if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
              event.preventDefault();
              const items = Array.from(
                event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'),
              );
              const index = items.indexOf(document.activeElement as HTMLElement);
              items[
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? items.length - 1
                    : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length
              ]?.focus();
            }
          }}
        >
          {onMove && (
            <Button
              variant="plain"
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => select(onMove)}
            >
              Move…
            </Button>
          )}
          {onAddPerson && (
            <Button
              variant="plain"
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => select(onAddPerson)}
            >
              Add person
            </Button>
          )}
        </span>
      )}
    </span>
  );
}

export function PeopleStructureWorkspace({
  people,
  definition,
  controls,
  changeBar,
  extraChanges,
}: {
  people: PeopleOrganizationProps;
  definition: DraftDefinition;
  controls?: PanelControls;
  changeBar?: ReactNode;
  extraChanges?: ReactNode;
}) {
  const query = useSearchParams(),
    router = useRouter();
  const archived = people.canManage && query.get("status") === "archived";
  const data = projectedDirectory(
    { ...people.data, people: archived ? people.archivedPeople : people.data.people },
    people.accessOrganization,
    definition,
  );
  const rawKind = selectionKinds.find((kind) => query.get(kind));
  const selectedId =
    rawKind === "person"
      ? (data.people.find(
          (p) => p.engagementId === query.get(rawKind) || p.personId === query.get(rawKind),
        )?.engagementId ?? query.get(rawKind))
      : rawKind
        ? query.get(rawKind)
        : null;
  const selection: OrganizationSelection | null =
    rawKind && selectedId ? { kind: rawKind, id: selectedId } : null;
  const unitParts = query.get("unit")?.split(":");
  const unitParam = unitParts ? [unitParts[0], unitParts.slice(1).join(":")] : undefined;
  const unit: OrganizationSelection =
    unitParam &&
    ["company", "branch", "department", "team", "level", "unassigned"].includes(unitParam[0]) &&
    unitParam[1]
      ? { kind: unitParam[0] as OrganizationSelection["kind"], id: unitParam[1] }
      : selection &&
          ["company", "branch", "department", "team", "level", "unassigned"].includes(
            selection.kind,
          )
        ? selection
        : { kind: "company", id: people.currentCompanyId };
  const context = contextFor(data, unit),
    detailContext = selection ? contextFor(data, selection) : null;
  const term = (query.get("q") ?? "").trim().toLocaleLowerCase();
  const showOccupancy = !archived && unit.kind !== "unassigned";
  const occupancy =
    showOccupancy && ["filled", "vacant"].includes(query.get("occupancy") ?? "")
      ? query.get("occupancy")
      : "all";
  const [expanded, setExpanded] = useState<Set<string>>(
    () =>
      new Set([
        people.currentCompanyId,
        ...people.data.nodes
          .filter(
            (n) =>
              n.companyId === people.currentCompanyId &&
              (n.kind === "branch" || n.kind === "headquarters"),
          )
          .map((n) => n.id),
      ]),
  );
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [railOpen, setRailOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [destination, setDestination] = useState("");
  const [touchDrag, setTouchDrag] = useState<{ source: Source; target: Target | null } | null>(
    null,
  );
  const suppressTouchClick = useRef(false);
  const drawerRef = useRef<HTMLElement>(null),
    returnFocus = useRef<HTMLElement | null>(null);
  const canOpenHrSettings = people.canOpenHrSettings ?? people.canManage;
  const panel = query.get("panel"),
    settingsOpen = panel === "settings" && canOpenHrSettings;
  const addKind = query.get("new");
  const adding =
    (addKind === "person" && people.canManage) ||
    (["position", "unit"].includes(addKind ?? "") && people.canManageStructure);
  const detailOpen =
    (adding || !!selection) && !settingsOpen && panel !== "schedule" && panel !== "move";
  const moveOpen =
    panel === "move" &&
    query.get("kind") &&
    query.get("id") &&
    ["person", "position", "department"].includes(query.get("kind")!)
      ? { kind: query.get("kind") as Source["kind"], id: query.get("id")! }
      : panel === "move" &&
          selection &&
          ["person", "position", "department"].includes(selection.kind)
        ? (selection as Source)
        : null;
  const overlay = useSyncExternalStore(
    subscribeOverlay,
    () => window.matchMedia(overlayMedia).matches,
    () => false,
  );
  const backgroundInert = !!moveOpen || (overlay && (detailOpen || settingsOpen));
  function update(patch: Record<string, string | null>, replace = false) {
    const params = new URLSearchParams(window.location.search);
    for (const [key, value] of Object.entries(patch))
      if (value === null) params.delete(key);
      else params.set(key, value);
    window.history[replace ? "replaceState" : "pushState"](
      null,
      "",
      `${people.base}/people${params.size ? `?${params}` : ""}`,
    );
  }
  function selectUnit(next: OrganizationSelection) {
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      unit: `${next.kind}:${next.id}`,
      panel: null,
      section: null,
      tab: null,
    });
    setRailOpen(false);
  }
  function backToStructure() {
    const departmentId = query.get("department");
    const next = departmentId
      ? {
          kind:
            data.nodes.find((node) => node.id === departmentId)?.kind === "team"
              ? "team"
              : "department",
          id: departmentId,
        }
      : unit;
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      unit: `${next.kind}:${next.id}`,
      panel: null,
      view: null,
      request: null,
      tab: null,
    });
  }
  function choose(next: OrganizationSelection) {
    returnFocus.current = document.activeElement as HTMLElement;
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      [next.kind]: next.id,
      panel: null,
      tab: null,
    });
  }
  function closeDetail() {
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      panel: null,
      section: null,
      tab: null,
      new: null,
      editor: null,
    });
    returnFocus.current?.focus();
  }
  function addPerson(positionId: string) {
    returnFocus.current = document.activeElement as HTMLElement;
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      new: "person",
      position: positionId,
      panel: null,
      tab: null,
    });
  }
  useEffect(() => {
    if (detailOpen || settingsOpen) drawerRef.current?.focus({ preventScroll: true });
  }, [detailOpen, settingsOpen, selectedId]);
  function toggle(id: string) {
    setExpanded((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  function canMove(source: Source) {
    return !!controls && !archived && companyOf(data, source) === people.currentCompanyId;
  }
  function openMove(source: Source) {
    returnFocus.current = document.activeElement as HTMLElement;
    setDestination("");
    update({
      ...Object.fromEntries(selectionKinds.map((kind) => [kind, null])),
      panel: "move",
      [source.kind]: source.id,
    });
  }
  function endMove() {
    setDestination("");
    if (panel === "move")
      update({ panel: null, kind: null, id: null, targetKind: null, targetId: null });
    returnFocus.current?.focus();
  }
  const change = (source: Source) =>
    definition.items.some((i) => draftKey(i) === draftKey(source)) ||
    (source.kind === "person" &&
      people.data.people.some(
        (p) =>
          p.engagementId === source.id &&
          (p.positionId !== data.people.find((n) => n.engagementId === source.id)?.positionId ||
            p.departmentId !==
              data.people.find((n) => n.engagementId === source.id)?.departmentId ||
            p.branchId !== data.people.find((n) => n.engagementId === source.id)?.branchId),
      ));
  function targetProps(target: Target) {
    const key = `${target.kind}:${target.id}`;
    return {
      "data-people-drop": key,
      "data-drop-state":
        controls?.over === key || touchDrag?.target?.id === target.id ? "active" : undefined,
      onDragOver: (event: React.DragEvent) => controls?.dragOver(event, key, target),
      onDrop: (event: React.DragEvent) => controls?.receive(event, target),
    };
  }
  function startDrag(event: React.DragEvent<HTMLButtonElement>, source: Source) {
    const card = event.currentTarget.closest<HTMLElement>("[data-people-drag-card]");
    if (card) {
      const bounds = card.getBoundingClientRect();
      event.dataTransfer.setDragImage(
        card,
        Math.max(0, Math.min(bounds.width, event.clientX - bounds.left)),
        Math.max(0, Math.min(bounds.height, event.clientY - bounds.top)),
      );
    }
    controls?.start(event, source);
  }
  function touchMove(event: PointerEvent<HTMLButtonElement>) {
    if (!touchDrag) return;
    const node = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>("[data-people-drop]");
    const [kind, ...idParts] = (node?.dataset.peopleDrop ?? "").split(":");
    const id = idParts.join(":");
    const target = id ? { kind: kind as Target["kind"], id } : null;
    setTouchDrag((current) =>
      current
        ? {
            ...current,
            target: target && !controls?.denial([current.source], target) ? target : null,
          }
        : null,
    );
    if (event.clientY < 90) window.scrollBy(0, -20);
    else if (event.clientY > window.innerHeight - 70) window.scrollBy(0, 20);
  }
  function actions(source: Source, addToPositionId?: string) {
    const movable = canMove(source);
    if (!movable && !addToPositionId) return null;
    return (
      <ItemActions
        name={label(data, source)}
        onMove={movable ? () => openMove(source) : undefined}
        onAddPerson={addToPositionId ? () => addPerson(addToPositionId) : undefined}
        grip={
          movable && (
            <Button
              variant="plain"
              className={styles.grip}
              type="button"
              draggable
              aria-label={`Drag ${label(data, source)}`}
              title="Drag to move; press Enter to choose a destination"
              onClick={() => {
                if (suppressTouchClick.current) {
                  suppressTouchClick.current = false;
                  return;
                }
                openMove(source);
              }}
              onDragStart={(event) => startDrag(event, source)}
              onDragEnd={() => controls?.end()}
              onPointerDown={(event) => {
                if (event.pointerType !== "mouse") {
                  event.currentTarget.setPointerCapture(event.pointerId);
                  setTouchDrag({ source, target: null });
                }
              }}
              onPointerMove={touchMove}
              onPointerUp={() => {
                if (touchDrag?.target) {
                  suppressTouchClick.current = true;
                  controls?.place([touchDrag.source], touchDrag.target);
                }
                setTouchDrag(null);
              }}
              onPointerCancel={(event) => {
                if (event.pointerType !== "mouse") {
                  setTouchDrag(null);
                  controls?.end();
                }
              }}
            >
              <GripVertical size={16} />
            </Button>
          )
        }
      />
    );
  }
  function unitNode(
    node: HoldingPeopleDirectory["nodes"][number],
    depth: number,
    ancestors = new Set<string>(),
    inheritedMatch = false,
  ): ReactNode {
    if (ancestors.has(node.id)) return null;
    const nextAncestors = new Set([...ancestors, node.id]);
    const kind = node.kind === "headquarters" ? "branch" : node.kind;
    const item: OrganizationSelection = { kind, id: node.id };
    const children = data.nodes.filter(
      (n) =>
        (n.id !== node.id && n.parentId === node.id) ||
        (kind === "branch" &&
          n.branchId === node.id &&
          ["department", "team"].includes(n.kind) &&
          !data.nodes.some(
            (parent) => parent.id === n.parentId && ["department", "team"].includes(parent.kind),
          )),
    );
    const ownMatch =
      inheritedMatch ||
      node.name.toLocaleLowerCase().includes(term) ||
      data.companies
        .find((c) => c.id === node.companyId)
        ?.name.toLocaleLowerCase()
        .includes(term);
    const nodeContext = contextFor(data, item);
    const matches =
      !term ||
      ownMatch ||
      nodeContext.positions.some((p) => p.title.toLocaleLowerCase().includes(term)) ||
      nodeContext.people.some((p) =>
        `${p.legalName} ${p.jobTitle} ${p.workEmail ?? ""}`.toLocaleLowerCase().includes(term),
      );
    if (!matches) return null;
    return (
      <div key={node.id}>
        <div
          className={styles.unitRow}
          style={{ paddingLeft: Math.min(depth, 4) * 16 }}
          data-people-drag-card={kind}
          data-selected={unit.id === node.id || undefined}
          data-testid={`people-${kind}-${node.id}`}
          {...(["branch", "department"].includes(kind) ? targetProps(item as Target) : {})}
        >
          {children.length ? (
            <Button
              variant="plain"
              type="button"
              className={styles.expand}
              aria-label={`${expanded.has(node.id) ? "Collapse" : "Expand"} ${node.name}`}
              aria-expanded={expanded.has(node.id)}
              onClick={() => toggle(node.id)}
            >
              {expanded.has(node.id) ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            </Button>
          ) : (
            <span className={styles.expand} />
          )}
          <Button
            variant="plain"
            type="button"
            className={styles.unitName}
            aria-pressed={unit.id === node.id}
            onClick={() => selectUnit(item)}
          >
            {node.name}
            {kind === "department" && change({ kind, id: node.id }) && (
              <span className={styles.dot} title="Pending change" />
            )}
          </Button>
          {kind === "department" && actions({ kind, id: node.id })}
        </div>
        {(expanded.has(node.id) || !!term) &&
          children.map((child) => unitNode(child, depth + 1, nextAncestors, !!ownMatch))}
      </div>
    );
  }
  const holdersByPosition = new Map<string, HoldingPeopleDirectory["people"]>();
  for (const person of data.people)
    if (person.positionId)
      holdersByPosition.set(person.positionId, [
        ...(holdersByPosition.get(person.positionId) ?? []),
        person,
      ]);
  const scopePositions =
    unit.kind === "unassigned" ? [] : term ? data.positions : context.positions;
  // Archived holders describe past assignments; empty current positions are not archive vacancies.
  const matchingPositions = scopePositions.filter(
    (p) =>
      (!archived || holdersByPosition.has(p.id)) &&
      (!term ||
        `${p.title} ${data.nodes.find((n) => n.id === p.departmentId)?.name ?? ""} ${data.nodes.find((n) => n.id === p.branchId)?.name ?? ""} ${data.companies.find((c) => c.id === p.companyId)?.name ?? ""}`
          .toLocaleLowerCase()
          .includes(term) ||
        (holdersByPosition.get(p.id) ?? []).some((person) =>
          `${person.legalName} ${person.workEmail ?? ""}`.toLocaleLowerCase().includes(term),
        )),
  );
  const filledCount = matchingPositions.filter((p) => holdersByPosition.has(p.id)).length;
  const occupancyCounts = {
    all: matchingPositions.length,
    filled: filledCount,
    vacant: matchingPositions.length - filledCount,
  };
  const visiblePositions = matchingPositions.filter(
    (p) => occupancy === "all" || holdersByPosition.has(p.id) === (occupancy === "filled"),
  );
  const loosePeople =
    occupancy === "all"
      ? (term && unit.kind !== "unassigned" ? data.people : context.people).filter(
          (p) =>
            !p.positionId &&
            (!term ||
              `${p.legalName} ${p.jobTitle} ${p.workEmail ?? ""}`
                .toLocaleLowerCase()
                .includes(term)),
        )
      : [];
  const groupMap = new Map<
    string,
    {
      key: string;
      companyId: string;
      branchId: string | null;
      departmentId: string | null;
      positions: HoldingPeopleDirectory["positions"];
    }
  >();
  for (const position of visiblePositions) {
    const key = JSON.stringify([position.companyId, position.branchId, position.departmentId]);
    const group = groupMap.get(key) ?? {
      key,
      companyId: position.companyId,
      branchId: position.branchId,
      departmentId: position.departmentId,
      positions: [],
    };
    group.positions.push(position);
    groupMap.set(key, group);
  }
  const groupNames = (group: {
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
  }) => [
    data.companies.find((c) => c.id === group.companyId)?.name ?? "Company",
    data.nodes.find((n) => n.id === group.branchId)?.name ?? "",
    data.nodes.find((n) => n.id === group.departmentId)?.name ?? "",
  ];
  const groups = [...groupMap.values()].sort((a, b) => {
    const left = groupNames(a),
      right = groupNames(b);
    return (
      left[0].localeCompare(right[0]) ||
      left[1].localeCompare(right[1]) ||
      left[2].localeCompare(right[2]) ||
      a.key.localeCompare(b.key)
    );
  });
  const visiblePeopleCount = visiblePositions.reduce(
    (count, position) => count + (holdersByPosition.get(position.id)?.length ?? 0),
    loosePeople.length,
  );
  const visibleCompanies = data.companies.filter(
    (c) =>
      !term ||
      c.name.toLocaleLowerCase().includes(term) ||
      data.nodes.some((n) => n.companyId === c.id && n.name.toLocaleLowerCase().includes(term)) ||
      data.positions.some(
        (p) => p.companyId === c.id && p.title.toLocaleLowerCase().includes(term),
      ) ||
      data.people.some(
        (p) =>
          p.companyId === c.id &&
          `${p.legalName} ${p.jobTitle} ${p.workEmail ?? ""}`.toLocaleLowerCase().includes(term),
      ),
  );
  const targets: Target[] = !moveOpen
    ? []
    : moveOpen.kind === "person"
      ? data.positions
          .filter((p) => p.companyId === people.currentCompanyId)
          .map((p) => ({ kind: "position", id: p.id }))
      : moveOpen.kind === "position"
        ? [
            ...data.nodes
              .filter((n) => n.companyId === people.currentCompanyId && n.kind === "department")
              .map((n) => ({ kind: "department" as const, id: n.id })),
            ...data.levels
              .filter((l) => l.companyId === people.currentCompanyId)
              .map((l) => ({ kind: "level" as const, id: l.id })),
          ]
        : data.nodes
            .filter((n) => n.companyId === people.currentCompanyId && n.kind === "branch")
            .map((n) => ({ kind: "branch", id: n.id }));
  const selectedTarget = targets.find((t) => `${t.kind}:${t.id}` === destination);
  const denial =
    moveOpen && selectedTarget
      ? controls?.denial([moveOpen], selectedTarget)
      : "Choose a destination.";
  function personRow(person: HoldingPeopleDirectory["people"][number], positionTitle?: string) {
    const pending = change({ kind: "person", id: person.engagementId });
    const jobTitle =
      person.jobTitle.trim().toLocaleLowerCase() !== positionTitle?.trim().toLocaleLowerCase()
        ? person.jobTitle
        : "";
    return (
      <div
        key={person.engagementId}
        className={styles.person}
        data-people-drag-card="person"
        data-testid={`people-person-${person.engagementId}`}
        data-pending={change({ kind: "person", id: person.engagementId }) || undefined}
      >
        <PersonAvatar name={person.legalName} identity={person.personId} small />
        <Button
          variant="plain"
          type="button"
          className={styles.personName}
          onClick={() => choose({ kind: "person", id: person.engagementId })}
        >
          {person.legalName}
          {(pending || jobTitle) && <small>{pending ? "Pending change" : jobTitle}</small>}
        </Button>
        {actions({ kind: "person", id: person.engagementId })}
      </div>
    );
  }
  const localDetail = selection && companyOf(data, selection) === people.currentCompanyId;
  const detailSections = selection
    ? peopleExplorerTabs(
        selection.kind,
        !!localDetail && people.canManage,
        !!localDetail && people.canReadPay,
      ).filter((tab) => !["People", "Positions", "Settings"].includes(tab))
    : [];
  if (
    selection &&
    localDetail &&
    people.canManage &&
    ["company", "branch"].includes(selection.kind) &&
    ["spending", "legacy"].includes(query.get("view") ?? "")
  )
    detailSections.push("Settings");
  if (
    selection &&
    localDetail &&
    people.canManage &&
    !archived &&
    ["person", "position"].includes(selection.kind)
  )
    detailSections.splice(1, 0, "Working time");
  const sectionSummary = (section: string) =>
    section === "Profile"
      ? "Name and contact details"
      : section === "Employment"
        ? "Assignment, manager, and lifecycle"
        : section === "Working time"
          ? "Work policy, holidays, and roster"
          : section === "Compensation"
            ? selection?.kind === "position"
              ? "Recommended salary"
              : "Pay group and effective compensation"
            : section === "History"
              ? "Recorded employment changes"
              : section === "Approvals"
                ? "Department authority and routing"
                : "Assignment and reporting";
  return (
    <section className={styles.workspace} data-testid="people-structure-workspace">
      <header className={styles.toolbar} inert={backgroundInert}>
        <div className={styles.search}>
          <Search size={18} />
          <TextInput
            type="search"
            aria-label="Search people, positions, and units"
            placeholder="Search people, positions, and units"
            value={query.get("q") ?? ""}
            onChange={(event) => update({ q: event.target.value || null }, true)}
          />
        </div>
        <div className={styles.topActions}>
          {people.canManage && (
            <div className={styles.addMenu}>
              <Button
                variant="primary"
                type="button"
                aria-expanded={addOpen}
                onClick={() => setAddOpen((open) => !open)}
              >
                + Add
              </Button>
              {addOpen && (
                <div
                  aria-label="Add to organization"
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      setAddOpen(false);
                    }
                  }}
                >
                  {["person", ...(people.canManageStructure ? ["position", "unit"] : [])].map(
                    (kind) => (
                      <Button
                        variant="plain"
                        type="button"
                        key={kind}
                        onClick={() => {
                          returnFocus.current = document.activeElement as HTMLElement;
                          update({ new: kind, panel: null });
                          setAddOpen(false);
                        }}
                      >
                        Add {kind}
                      </Button>
                    ),
                  )}
                </div>
              )}
            </div>
          )}
          {canOpenHrSettings && (
            <Button
              variant="plain"
              type="button"
              onClick={() => {
                returnFocus.current = document.activeElement as HTMLElement;
                update({ panel: "settings", section: null, new: null });
              }}
            >
              <Settings2 size={17} />
              HR settings
            </Button>
          )}
        </div>
      </header>
      <div className={styles.layout} data-detail-open={detailOpen || settingsOpen || undefined}>
        <Button
          variant="plain"
          type="button"
          className={styles.unitPicker}
          inert={backgroundInert}
          aria-expanded={railOpen}
          onClick={() => setRailOpen((value) => !value)}
        >
          <Users size={17} />
          Choose unit · {label(data, unit)}
          <ChevronDown size={16} />
        </Button>
        <aside
          className={styles.rail}
          inert={backgroundInert}
          data-mobile-open={railOpen || undefined}
          data-testid="people-organization-rail"
          aria-label="Organization units"
        >
          <div className={styles.railTitle}>
            <strong>Units</strong>
            {people.canManage && (
              <Select
                aria-label="People status"
                value={archived ? "archived" : "current"}
                onChange={(event) => {
                  const params = new URLSearchParams(query);
                  if (event.target.value === "archived") params.set("status", "archived");
                  else params.delete("status");
                  router.push(`${people.base}/people?${params}`, { scroll: false });
                }}
              >
                <option value="current">Current</option>
                <option value="archived">Archived</option>
              </Select>
            )}
          </div>
          {visibleCompanies.map((company) => (
            <div key={company.id} className={styles.company}>
              <div
                className={styles.unitRow}
                data-selected={(unit.kind === "company" && unit.id === company.id) || undefined}
              >
                <Button
                  variant="plain"
                  type="button"
                  className={styles.expand}
                  aria-expanded={expanded.has(company.id)}
                  aria-label={`${expanded.has(company.id) ? "Collapse" : "Expand"} ${company.name}`}
                  onClick={() => toggle(company.id)}
                >
                  {expanded.has(company.id) ? (
                    <ChevronDown size={16} />
                  ) : (
                    <ChevronRight size={16} />
                  )}
                </Button>
                <Button
                  variant="plain"
                  type="button"
                  className={styles.unitName}
                  aria-pressed={unit.kind === "company" && unit.id === company.id}
                  onClick={() => selectUnit({ kind: "company", id: company.id })}
                >
                  {company.name}
                </Button>
              </div>
              {(expanded.has(company.id) || !!term) && (
                <>
                  {data.nodes
                    .filter(
                      (n) =>
                        n.companyId === company.id &&
                        n.kind !== "company" &&
                        (!n.parentId ||
                          !data.nodes.some((p) => p.id === n.parentId && p.kind !== "company")) &&
                        (!n.branchId || n.kind === "branch" || n.kind === "headquarters"),
                    )
                    .map((node) => unitNode(node, 1))}
                  <Button
                    variant="plain"
                    type="button"
                    className={styles.unassigned}
                    aria-pressed={unit.kind === "unassigned" && unit.id === company.id}
                    onClick={() => selectUnit({ kind: "unassigned", id: company.id })}
                  >
                    Without position ·{" "}
                    {data.people.filter((p) => p.companyId === company.id && !p.positionId).length}
                  </Button>
                </>
              )}
            </div>
          ))}
        </aside>
        <div className={styles.main} inert={backgroundInert}>
          {panel === "schedule" ? (
            <PeopleTeamSchedule
              base={people.base}
              departmentId={
                query.get("department") ??
                (unit.kind === "department" || unit.kind === "team" ? unit.id : undefined)
              }
              initialWeek={query.get("week") ?? undefined}
              initialView={query.get("view") ?? undefined}
              requestId={query.get("request") ?? undefined}
              onBack={backToStructure}
            />
          ) : (
            <>
              <div className={styles.mainHeading}>
                <div>
                  <span className={styles.eyebrow}>{label(data, unit)}</span>
                  <h2>
                    {archived
                      ? "Archived people"
                      : unit.kind === "unassigned"
                        ? "People without a position"
                        : term
                          ? "Search results"
                          : "Positions and people"}
                  </h2>
                  <p data-testid="people-result-counts" aria-live="polite">
                    {visiblePositions.length} positions · {visiblePeopleCount}{" "}
                    {archived ? "archived people" : "people"}
                  </p>
                </div>
                <Button variant="plain" type="button" onClick={() => choose(unit)}>
                  Unit details
                </Button>
                {["department", "team"].includes(unit.kind) && (
                  <Button
                    variant="plain"
                    type="button"
                    onClick={() => update({ panel: "schedule", department: unit.id })}
                  >
                    Open team schedule
                  </Button>
                )}
              </div>
              {showOccupancy && (
                <div className={styles.occupancy} role="group" aria-label="Position occupancy">
                  {(["all", "filled", "vacant"] as const).map((filter) => (
                    <Button
                      variant="plain"
                      type="button"
                      key={filter}
                      aria-pressed={occupancy === filter}
                      onClick={() => update({ occupancy: filter === "all" ? null : filter })}
                    >
                      {filter === "all" ? "All" : filter === "filled" ? "Filled" : "Vacant"}
                      <span>{occupancyCounts[filter]}</span>
                    </Button>
                  ))}
                </div>
              )}
              {!controls && <p className={styles.readOnly}>Read-only organization explorer</p>}
              {groups.map((group) => {
                const department = data.nodes.find((n) => n.id === group.departmentId),
                  [companyName, branchName] = groupNames(group);
                const title = department?.name ?? (branchName ? "Branch-wide" : "Company-wide");
                const groupSelection: OrganizationSelection = department
                  ? { kind: department.kind === "team" ? "team" : "department", id: department.id }
                  : group.branchId
                    ? { kind: "branch", id: group.branchId }
                    : { kind: "company", id: group.companyId };
                return (
                  <section
                    className={styles.group}
                    key={group.key}
                    data-testid="people-position-group"
                    data-company-id={group.companyId}
                    data-branch-id={group.branchId ?? undefined}
                    data-department-id={group.departmentId ?? undefined}
                    {...(department?.kind === "department"
                      ? targetProps({ kind: "department", id: department.id })
                      : {})}
                  >
                    <div className={styles.groupHeading}>
                      <h3 className={styles.groupTitle}>
                        <Button
                          variant="plain"
                          type="button"
                          className={styles.groupName}
                          aria-label={title}
                          aria-expanded={!collapsed.has(group.key)}
                          onClick={() =>
                            setCollapsed((old) => {
                              const next = new Set(old);
                              if (next.has(group.key)) next.delete(group.key);
                              else next.add(group.key);
                              return next;
                            })
                          }
                        >
                          {collapsed.has(group.key) ? (
                            <ChevronRight size={16} />
                          ) : (
                            <ChevronDown size={16} />
                          )}
                          <span>
                            {title}
                            <small>{[companyName, branchName].filter(Boolean).join(" · ")}</small>
                          </span>
                        </Button>
                      </h3>
                      <span className={styles.groupCounts} data-testid="people-group-counts">
                        {group.positions.length} positions ·{" "}
                        {group.positions.reduce(
                          (count, position) =>
                            count + (holdersByPosition.get(position.id)?.length ?? 0),
                          0,
                        )}{" "}
                        people
                      </span>
                      <Button variant="plain" type="button" onClick={() => choose(groupSelection)}>
                        Details
                      </Button>
                    </div>
                    {!collapsed.has(group.key) &&
                      group.positions.map((position) => {
                        const holders = holdersByPosition.get(position.id) ?? [],
                          canAdd =
                            people.canManage &&
                            position.companyId === people.currentCompanyId &&
                            !archived;
                        return (
                          <article
                            key={position.id}
                            className={styles.position}
                            data-people-drag-card="position"
                            data-testid={`people-position-${position.id}`}
                            data-occupancy={
                              archived ? undefined : holders.length ? "filled" : "vacant"
                            }
                            data-pending={
                              change({ kind: "position", id: position.id }) || undefined
                            }
                            {...targetProps({ kind: "position", id: position.id })}
                          >
                            <div className={styles.positionHeading}>
                              <BriefcaseBusiness size={18} />
                              <Button
                                variant="plain"
                                type="button"
                                className={styles.positionName}
                                onClick={() => choose({ kind: "position", id: position.id })}
                              >
                                {position.title}
                                <small>
                                  {archived ? "Previously reported to" : "Reports to"}{" "}
                                  {data.positions.find((p) => p.id === position.reportsToPositionId)
                                    ?.title ?? "Company leadership"}
                                  {change({ kind: "position", id: position.id }) &&
                                    " · Pending change"}
                                </small>
                              </Button>
                            </div>
                            <div className={styles.holders}>
                              {holders.map((person) => personRow(person, position.title))}
                              {!holders.length && !archived && (
                                <span className={styles.vacant}>Vacant position</span>
                              )}
                            </div>
                            <div className={styles.positionActions}>
                              {!holders.length && canAdd && (
                                <Button
                                  variant="plain"
                                  type="button"
                                  className={styles.addPerson}
                                  onClick={() => addPerson(position.id)}
                                >
                                  + Add person
                                </Button>
                              )}
                              {actions(
                                { kind: "position", id: position.id },
                                holders.length && canAdd ? position.id : undefined,
                              )}
                            </div>
                          </article>
                        );
                      })}
                  </section>
                );
              })}
              {!!loosePeople.length && (
                <section className={styles.group} data-testid="people-unassigned">
                  <div className={styles.groupHeading}>
                    <h3 className={styles.groupTitle}>Without a position</h3>
                    <span className={styles.groupCounts}>{loosePeople.length} people</span>
                  </div>
                  <div className={styles.loosePeople}>
                    {loosePeople.map((person) => personRow(person))}
                  </div>
                </section>
              )}
              {!visiblePositions.length && !loosePeople.length && (
                <p className={styles.empty}>
                  {archived
                    ? "No archived people in this selection."
                    : unit.kind === "unassigned"
                      ? term
                        ? "No unassigned people match your search."
                        : "Everyone in this company has a position."
                      : occupancy === "filled"
                        ? "No filled positions in this selection."
                        : occupancy === "vacant"
                          ? "No vacant positions in this selection."
                          : term
                            ? "No people, positions, or units match your search."
                            : "No positions or people in this unit."}
                </p>
              )}
            </>
          )}
        </div>
        {(detailOpen || settingsOpen) && (
          <aside
            ref={drawerRef}
            className={styles.detail}
            data-testid="people-detail"
            tabIndex={-1}
            role={overlay ? "dialog" : undefined}
            aria-modal={overlay || undefined}
            aria-label={
              settingsOpen
                ? "HR settings"
                : `${selection ? label(data, selection) : "People"} details`
            }
            onKeyDown={(event) => {
              if (overlay) trapFocus(event);
              if (event.key === "Escape") {
                event.stopPropagation();
                closeDetail();
              }
            }}
          >
            <div className={styles.detailHeading}>
              <div>
                <span className={styles.eyebrow}>
                  {settingsOpen ? "Company administration" : adding ? "People" : selection?.kind}
                </span>
                <h2>
                  {settingsOpen
                    ? "HR settings"
                    : adding
                      ? `Add ${addKind}`
                      : selection && label(data, selection)}
                </h2>
              </div>
              <Button
                variant="plain"
                type="button"
                aria-label="Close details"
                onClick={closeDetail}
              >
                <X size={20} />
              </Button>
            </div>
            {settingsOpen ? (
              <PeopleHrSettings
                base={people.base}
                initialTopic={query.get("section") ?? undefined}
                initialAction={
                  query.get("new") === "leave-policy"
                    ? "policy"
                    : (query.get("editor") ?? undefined)
                }
                onTopicChange={(topic) => update({ section: topic, new: null, editor: null })}
                onClose={closeDetail}
              />
            ) : adding ? (
              <NewPersonEditor
                key={addKind}
                kind={addKind ?? undefined}
                base={people.base}
                positionId={query.get("position") ?? undefined}
              />
            ) : (
              selection &&
              detailContext && (
                <>
                  {canMove(selection as Source) &&
                    ["person", "position", "department"].includes(selection.kind) && (
                      <Button
                        variant="plain"
                        type="button"
                        onClick={() => openMove(selection as Source)}
                      >
                        Move…
                      </Button>
                    )}
                  {["department", "team"].includes(selection.kind) && (
                    <Button
                      variant="secondary"
                      type="button"
                      onClick={() => update({ panel: "schedule", department: selection.id })}
                    >
                      Open team schedule
                    </Button>
                  )}
                  {detailSections.map((section, index) => (
                    <Accordion
                      key={`${selection.kind}:${selection.id}:${section}:${query.get("tab") ?? panel}`}
                      title={section}
                      summary={sectionSummary(section)}
                      initiallyOpen={
                        query.get("tab")
                          ? query.get("tab")?.toLocaleLowerCase() === section.toLocaleLowerCase()
                          : panel === "history"
                            ? section === "Changes"
                            : index === 0
                      }
                      onOpen={() => update({ tab: section.toLocaleLowerCase() }, true)}
                    >
                      {section === "Working time" ? (
                        selection.kind === "person" ? (
                          <PeopleWorkingTime base={people.base} engagementId={selection.id} />
                        ) : (
                          <PeoplePositionWorkingTime base={people.base} positionId={selection.id} />
                        )
                      ) : (
                        <LazyDetail
                          people={people}
                          selection={selection}
                          section={section}
                          context={detailContext}
                        />
                      )}
                    </Accordion>
                  ))}
                </>
              )
            )}
          </aside>
        )}
      </div>
      <div inert={backgroundInert}>
        {extraChanges}
        {!!definition.items.length && (
          <details className={styles.pending} data-testid="people-pending-changes">
            <summary>
              {definition.items.length} pending changes
              {definition.items.some((i) => i.parked) && " · Choose remaining destinations"}
            </summary>
            {definition.items.map((item) => (
              <div key={draftKey(item)}>
                <Button variant="plain" type="button" onClick={() => choose(item)}>
                  {label(data, item)}
                </Button>
                <span>{item.parked ? "Choose a destination" : "Pending review"}</span>
                <Button variant="plain" type="button" onClick={() => openMove(item)}>
                  Move…
                </Button>
                <Button variant="plain" type="button" onClick={() => controls?.restore([item])}>
                  Undo change
                </Button>
              </div>
            ))}
          </details>
        )}
      </div>
      {changeBar && (
        <div className={styles.stickyChanges} inert={backgroundInert}>
          {changeBar}
        </div>
      )}
      {touchDrag && (
        <div className={styles.touchStatus} role="status">
          Moving {label(data, touchDrag.source)} ·{" "}
          {touchDrag.target
            ? `Release over ${label(data, touchDrag.target)}`
            : "Drag onto a destination"}
        </div>
      )}
      {moveOpen && controls && (
        <Dialog
          open
          onClose={endMove}
          title={`Move ${label(data, moveOpen)}`}
          className={styles.moveChooser}
          bodyClassName="contents"
          showHeader={false}
          data-testid="people-move-chooser"
        >
          <div className={styles.detailHeading}>
            <h2 id="people-move-title">Move {label(data, moveOpen)}</h2>
            <Button variant="plain" type="button" aria-label="Close move" onClick={endMove}>
              <X size={20} />
            </Button>
          </div>
          <p>
            {moveOpen.kind === "position"
              ? "Current holders move with this position."
              : moveOpen.kind === "department"
                ? "Positions and people move with this department."
                : "Choose the new position."}{" "}
            Changes stay in your private draft until review.
          </p>
          <Field>
            Move destination
            <Select
              data-autofocus
              value={destination}
              onChange={(event) => setDestination(event.target.value)}
            >
              <option value="">Choose a destination</option>
              {targets.map((target) => (
                <option key={`${target.kind}:${target.id}`} value={`${target.kind}:${target.id}`}>
                  {target.kind} · {label(data, target)}
                  {target.kind === "position"
                    ? ` · ${data.nodes.find((n) => n.id === data.positions.find((p) => p.id === target.id)?.departmentId)?.name ?? "Company-wide"}`
                    : ""}
                </option>
              ))}
            </Select>
          </Field>
          {destination && denial && <p role="status">{denial}</p>}
          <div className={styles.moveActions}>
            <Button variant="plain" type="button" onClick={endMove}>
              <ArrowLeft size={16} />
              Cancel
            </Button>
            <Button
              variant="primary"
              type="button"
              disabled={!selectedTarget || !!denial}
              onClick={() => {
                if (selectedTarget) {
                  controls.place([moveOpen], selectedTarget);
                  endMove();
                }
              }}
            >
              Move here
            </Button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
