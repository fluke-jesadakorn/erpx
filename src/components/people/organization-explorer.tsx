"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useMemo, useState, type DragEvent, type ReactNode } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import {
  BriefcaseBusiness,
  Building2,
  ChevronDown,
  ChevronRight,
  FolderTree,
  GripVertical,
  Layers3,
  List,
  MoreHorizontal,
  Network,
  Search,
  UserRound,
  Users,
} from "lucide-react";
import styles from "./organization-explorer.module.css";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";

/** Only public work contact fields belong in this client-side directory. */
type ExplorerViewData = {
  holdingName: string;
  companies: { id: string; name: string; slug?: string; workEmail?: string | null }[];
  nodes: {
    id: string;
    kind: "branch" | "department" | "team";
    name: string;
    companyId: string;
    branchId: string | null;
    parentId: string | null;
    status?: string;
  }[];
  positions: {
    id: string;
    title: string;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    levelId: string | null;
    reportsToPositionId?: string | null;
    active?: boolean;
  }[];
  people: {
    id: string;
    name: string;
    workEmail: string | null;
    workPhone: string | null;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    positionId: string | null;
    jobTitle?: string | null;
    status?: string;
  }[];
  levels: { id: string; name: string; number: number; companyId: string }[];
};

export type OrganizationExplorerData = HoldingPeopleDirectory;

export type OrganizationSelection = {
  kind:
    "company" | "branch" | "department" | "team" | "position" | "person" | "level" | "unassigned";
  id: string;
};

export type OrganizationMoveIntent =
  | { kind: "person"; id: string; targetKind: "position"; targetId: string }
  | { kind: "position"; id: string; targetKind: "department" | "level"; targetId: string }
  | { kind: "department"; id: string; targetKind: "branch"; targetId: string };

export type OrganizationExplorerContext = {
  company: HoldingPeopleDirectory["companies"][number] | undefined;
  people: HoldingPeopleDirectory["people"];
  positions: HoldingPeopleDirectory["positions"];
  nodes: HoldingPeopleDirectory["nodes"];
  level: HoldingPeopleDirectory["levels"][number] | undefined;
  directory: HoldingPeopleDirectory;
};

export type OrganizationExplorerProps = {
  data: HoldingPeopleDirectory;
  initialCompanyId: string;
  /** Preserve an older deep link while the caller migrates its query keys. */
  initialSelection?: OrganizationSelection;
  holdingName?: string;
  canViewArchived?: boolean;
  archivedPeople?: HoldingPeopleDirectory["people"];
  canManageCompany?: (companyId: string) => boolean;
  canReviewMove?: boolean;
  /** A client wrapper can provide HR or IT detail content for a selected tab. */
  renderDetail?: (
    selection: OrganizationSelection,
    tab: string,
    context: OrganizationExplorerContext,
  ) => ReactNode | undefined;
  /** Limit available tabs for a role. Hidden URL tabs fall back to the first visible tab. */
  visibleTabs?: (selection: OrganizationSelection) => string[] | undefined;
  tabsForSelection?: (selection: OrganizationSelection) => string[];
  headerActions?: ReactNode;
  clearQueryKeys?: string[];
  searchLabel?: string;
  testIdPrefix?: string;
  onAddPersonToTray?: (engagementId: string) => void;
  onPersonDragStateChange?: (engagementId: string | null) => void;
  draftMoveBase?: string;
  selection?: OrganizationSelection;
  onSelectionChange?: (selection: OrganizationSelection) => void;
  tab?: string;
  onTabChange?: (tab: string) => void;
  panelMode?: boolean;
  detailEmpty?: boolean;
  treeSelections?: OrganizationSelection[];
  hiddenKeys?: Set<string>;
  rowControls?: (selection: OrganizationSelection) => ReactNode;
  detailStatus?: ReactNode;
  onRowDragStart?: (event: DragEvent, selection: OrganizationSelection) => void;
  onRowDragOver?: (event: DragEvent, selection: OrganizationSelection) => void;
  onRowDragEnd?: () => void;
  rowDropState?: (selection: OrganizationSelection) => string | undefined;
  onRowDrop?: (event: DragEvent, selection: OrganizationSelection) => void;
};

type MoveSource = Pick<OrganizationMoveIntent, "kind" | "id">;
const dragMime = "application/erpx-people-structure";
const emptyArchived: HoldingPeopleDirectory["people"] = [];
const selectionKeys = [
  "person",
  "position",
  "team",
  "department",
  "branch",
  "level",
  "company",
  "unassigned",
] as const;
const labels: Record<OrganizationSelection["kind"], string> = {
  company: "Company",
  branch: "Branch",
  department: "Department",
  team: "Team",
  position: "Position",
  person: "Person",
  level: "Company level",
  unassigned: "Unassigned people",
};
const icons = {
  company: Building2,
  branch: Building2,
  department: FolderTree,
  team: FolderTree,
  position: BriefcaseBusiness,
  person: UserRound,
  level: Layers3,
  unassigned: Users,
};

function selectionFromQuery(
  query: Pick<URLSearchParams, "get">,
  fallback: string,
  directory: HoldingPeopleDirectory,
  archivedPeople: HoldingPeopleDirectory["people"],
): OrganizationSelection {
  for (const key of selectionKeys) {
    const id = query.get(key);
    if (id)
      return {
        kind: key,
        id:
          key === "person"
            ? ([...directory.people, ...archivedPeople].find(
                (item) => item.id === id || item.personId === id,
              )?.id ?? id)
            : id,
      };
  }
  return { kind: "company", id: fallback };
}

function sourceCompany(data: HoldingPeopleDirectory, source: MoveSource): string | undefined {
  if (source.kind === "person") return data.people.find((item) => item.id === source.id)?.companyId;
  if (source.kind === "position")
    return data.positions.find((item) => item.id === source.id)?.companyId;
  return data.nodes.find((item) => item.id === source.id && item.kind === "department")?.companyId;
}

function targetCompany(
  data: HoldingPeopleDirectory,
  target: OrganizationSelection,
): string | undefined {
  if (target.kind === "position")
    return data.positions.find((item) => item.id === target.id)?.companyId;
  if (target.kind === "level") return data.levels.find((item) => item.id === target.id)?.companyId;
  return data.nodes.find((item) => item.id === target.id)?.companyId;
}

function selectionAncestors(data: ExplorerViewData, item: OrganizationSelection): string[] {
  const person =
    item.kind === "person" ? data.people.find((value) => value.id === item.id) : undefined;
  const position = data.positions.find(
    (value) => value.id === (item.kind === "position" ? item.id : person?.positionId),
  );
  const department = data.nodes.find(
    (value) =>
      value.id ===
      (item.kind === "department" || item.kind === "team"
        ? item.id
        : (position?.departmentId ?? person?.departmentId)),
  );
  const branch = data.nodes.find(
    (value) =>
      value.id ===
      (item.kind === "branch"
        ? item.id
        : (position?.branchId ?? person?.branchId ?? department?.branchId)),
  );
  const companyId =
    item.kind === "company" || item.kind === "unassigned"
      ? item.id
      : (person?.companyId ?? position?.companyId ?? department?.companyId ?? branch?.companyId);
  return [
    companyId && `company:${companyId}`,
    branch && `branch:${branch.id}`,
    department && `${department.kind}:${department.id}`,
    position && `position:${position.id}`,
  ].filter((value): value is string => !!value);
}

/** Shared validation for pointer drag and the keyboard/touch move control. The server must recheck it. */
export function organizationMoveDenial(
  data: HoldingPeopleDirectory,
  source: MoveSource,
  target: OrganizationSelection,
  canManageCompany: (companyId: string) => boolean,
): string | null {
  const companyId = sourceCompany(data, source);
  if (!companyId) return "The item is no longer available.";
  if (!canManageCompany(companyId)) return "You cannot change this company.";
  if (targetCompany(data, target) !== companyId)
    return "Moves between companies need a separate employment change.";
  if (source.kind === "person") {
    if (target.kind !== "position") return "Drop a person on a position.";
    const person = data.people.find((item) => item.id === source.id);
    const position = data.positions.find((item) => item.id === target.id);
    if (!position) return "Choose an active position.";
    if (person?.positionId === target.id) return "This person already holds this position.";
    return null;
  }
  if (source.kind === "position") {
    if (target.kind !== "department" && target.kind !== "level")
      return "Drop a position on a department or level.";
    const position = data.positions.find((item) => item.id === source.id);
    if (!position) return "Choose an active position.";
    if (target.kind === "department" && position.departmentId === target.id)
      return "This position is already in this department.";
    if (target.kind === "level" && position.levelId === target.id)
      return "This position already has this level.";
    if (target.kind === "department") {
      const department = data.nodes.find(
        (item) => item.id === target.id && item.kind === "department",
      );
      if (!department) return "Choose an active department.";
      if (
        !department.branchId ||
        !data.nodes.some((item) => item.id === department.branchId && item.kind === "branch")
      )
        return "Choose a department in an active branch.";
    }
    if (target.kind === "level") {
      const level = data.levels.find((item) => item.id === target.id);
      if (!level || level.number === null || level.number < 1 || level.number > 6)
        return "Choose one of the six active company levels.";
    }
    return null;
  }
  if (target.kind !== "branch") return "Drop a department on a branch.";
  const department = data.nodes.find((item) => item.id === source.id && item.kind === "department");
  if (department?.branchId === target.id) return "This department is already in this branch.";
  if (!data.nodes.some((item) => item.id === target.id && item.kind === "branch"))
    return "Choose an active branch.";
  if (data.nodes.some((item) => item.parentId === source.id))
    return "Move child departments and teams before relocating this department.";
  if (
    department &&
    data.nodes.some(
      (item) =>
        item.id !== department.id &&
        item.kind === "department" &&
        item.branchId === target.id &&
        item.slug === department.slug,
    )
  )
    return "A department with this URL code already exists in that branch.";
  return null;
}

function tabNames(kind: OrganizationSelection["kind"]): string[] {
  if (kind === "person") return ["Profile", "Employment", "Compensation", "History"];
  if (kind === "company" || kind === "branch")
    return ["Overview", "People", "Positions", "Changes", "Settings"];
  if (kind === "department" || kind === "team")
    return ["Overview", "People", "Positions", "Approvals"];
  if (kind === "position") return ["Overview", "People", "Compensation", "History"];
  return ["Overview", "Positions"];
}

export function OrganizationExplorer({
  data: directory,
  initialCompanyId,
  initialSelection,
  holdingName = "Holding",
  canViewArchived = false,
  archivedPeople = emptyArchived,
  canManageCompany = () => false,
  canReviewMove = false,
  renderDetail,
  visibleTabs,
  tabsForSelection,
  headerActions,
  clearQueryKeys = [],
  searchLabel = "Search people and organization",
  testIdPrefix = "people-org",
  onAddPersonToTray,
  onPersonDragStateChange,
  draftMoveBase,
  selection: controlledSelection,
  onSelectionChange,
  tab: controlledTab,
  onTabChange,
  panelMode = false,
  detailEmpty = false,
  treeSelections,
  hiddenKeys,
  rowControls,
  detailStatus,
  onRowDragStart,
  onRowDragOver,
  onRowDrop,
  onRowDragEnd,
  rowDropState,
}: OrganizationExplorerProps) {
  const data: ExplorerViewData = useMemo(
    () => ({
      holdingName,
      companies: directory.companies.map((company) => ({ ...company })),
      nodes: directory.nodes
        .filter(
          (node) =>
            node.kind === "branch" ||
            node.kind === "headquarters" ||
            node.kind === "department" ||
            node.kind === "team",
        )
        .map((node) => ({
          id: node.id,
          kind:
            node.kind === "branch" || node.kind === "headquarters"
              ? ("branch" as const)
              : node.kind === "team"
                ? ("team" as const)
                : ("department" as const),
          name: node.name,
          companyId: node.companyId,
          branchId: node.branchId,
          parentId: node.parentId,
          status: "active",
        })),
      positions: directory.positions.map((position) => ({ ...position, active: true })),
      people: [
        ...directory.people.map((person) => ({
          id: person.id,
          name: person.legalName,
          workEmail: person.workEmail,
          workPhone: person.workPhone,
          companyId: person.companyId,
          branchId: person.branchId,
          departmentId: person.departmentId,
          positionId: person.positionId,
          jobTitle: person.jobTitle,
          status: "current",
        })),
        ...archivedPeople.map((person) => ({
          id: person.id,
          name: person.legalName,
          workEmail: person.workEmail,
          workPhone: person.workPhone,
          companyId: person.companyId,
          branchId: person.branchId,
          departmentId: person.departmentId,
          positionId: person.positionId,
          jobTitle: person.jobTitle,
          status: "archived",
        })),
      ],
      levels: directory.levels.map((level) => ({
        id: level.id,
        name: level.name,
        number: level.number ?? 0,
        companyId: level.companyId,
      })),
    }),
    [directory, holdingName, archivedPeople],
  );
  const router = useRouter();
  const pathname = usePathname();
  const query = useSearchParams();
  const hasQuerySelection = selectionKeys.some((key) => !!query.get(key));
  const selected =
    controlledSelection ??
    (hasQuerySelection
      ? selectionFromQuery(query, initialCompanyId, directory, archivedPeople)
      : (initialSelection ?? { kind: "company", id: initialCompanyId }));
  const explicitSelection =
    hasQuerySelection || (!!initialSelection && initialSelection.kind !== "company");
  const selectedCompany = panelMode ? "all" : (query.get("filterCompany") ?? "all");
  const display = query.get("display") === "list" ? "list" : "structure";
  const status = canViewArchived && query.get("status") === "archived" ? "archived" : "current";
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set([`company:${initialCompanyId}`, ...selectionAncestors(data, selected)]),
  );
  const [dragging, setDragging] = useState<MoveSource | null>(null);
  const [dropOver, setDropOver] = useState("");
  const [dragHint, setDragHint] = useState("");
  const [feedback, setFeedback] = useState("");
  const [openRowMenu, setOpenRowMenu] = useState<string | null>(null);
  const [levelsExpanded, setLevelsExpanded] = useState(selected.kind === "level");

  const companies = data.companies;
  const nodes = data.nodes.filter(
    (item) => item.status !== "archived" && !hiddenKeys?.has(`${item.kind}:${item.id}`),
  );
  const positions = data.positions.filter(
    (item) => item.active !== false && !hiddenKeys?.has(`position:${item.id}`),
  );
  const people = data.people.filter((item) => {
    const archived = item.status === "archived" || item.status === "ended";
    return (
      !hiddenKeys?.has(`person:${item.id}`) &&
      (status === "archived" ? archived : !archived) &&
      (selectedCompany === "all" || item.companyId === selectedCompany)
    );
  });
  const visibleCompanies = companies.filter(
    (item) => selectedCompany === "all" || item.id === selectedCompany,
  );
  const visibleNodes = nodes.filter(
    (item) => selectedCompany === "all" || item.companyId === selectedCompany,
  );
  const visiblePositions = positions.filter(
    (item) => selectedCompany === "all" || item.companyId === selectedCompany,
  );
  const allUnassigned = people.filter(
    (item) =>
      !item.positionId || !visiblePositions.some((position) => position.id === item.positionId),
  );

  function patchUrl(patch: Record<string, string | undefined>, replace = false) {
    const url = new URL(window.location.href);
    for (const [key, value] of Object.entries(patch)) {
      if (value) url.searchParams.set(key, value);
      else url.searchParams.delete(key);
    }
    const href = `${url.pathname}${url.search}${url.hash}`;
    if (replace) router.replace(href, { scroll: false });
    else router.push(href, { scroll: false });
  }

  function select(next: OrganizationSelection) {
    const clear = Object.fromEntries(
      [...selectionKeys, ...clearQueryKeys].map((key) => [key, undefined]),
    );
    const ancestors = selectionAncestors(data, next);
    setExpanded((previous) => new Set([...previous, ...ancestors]));
    setFeedback("");
    setOpenRowMenu(null);
    if (next.kind === "level") setLevelsExpanded(true);
    if (onSelectionChange) {
      onSelectionChange(next);
      return;
    }
    patchUrl({ ...clear, [next.kind]: next.id, tab: undefined });
    requestAnimationFrame(() => document.getElementById("organization-detail-title")?.focus());
  }

  function toggle(key: string) {
    setExpanded((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function sourceFor(selection: OrganizationSelection): MoveSource | null {
    return selection.kind === "person" ||
      selection.kind === "position" ||
      selection.kind === "department"
      ? { kind: selection.kind, id: selection.id }
      : null;
  }

  function canMoveSource(source: MoveSource | null): source is MoveSource {
    const companyId = source && sourceCompany(directory, source);
    return !!source && canReviewMove && !!companyId && canManageCompany(companyId);
  }

  function moveHref(source: MoveSource) {
    const params = new URLSearchParams({ kind: source.kind, id: source.id });
    return `${draftMoveBase ?? `${pathname}/move`}?${params}`;
  }

  function move(source: MoveSource, target: OrganizationSelection) {
    const denied = organizationMoveDenial(directory, source, target, canManageCompany);
    if (denied) {
      setFeedback(denied);
      return;
    }
    if (!canReviewMove) return;
    setFeedback("");
    const params = new URLSearchParams({
      kind: source.kind,
      id: source.id,
      targetKind: target.kind,
      targetId: target.id,
    });
    router.push(`${draftMoveBase ?? `${pathname}/move/review`}?${params}`);
  }

  function row(item: OrganizationSelection, name: string, count?: number, children?: ReactNode) {
    const key = `${item.kind}:${item.id}`;
    if (hiddenKeys?.has(key)) return null;
    const Icon = icons[item.kind];
    const source = sourceFor(item);
    const movable = canMoveSource(source);
    const expandedHere = expanded.has(key);
    const active = selected.kind === item.kind && selected.id === item.id;
    const denied = dragging
      ? organizationMoveDenial(directory, dragging, item, canManageCompany)
      : null;
    const countsPositions =
      item.kind === "branch" ||
      item.kind === "department" ||
      item.kind === "team" ||
      item.kind === "level";
    const countLabel = countsPositions
      ? count === 1
        ? "position"
        : "positions"
      : count === 1
        ? "person"
        : "people";
    return (
      <li className={styles.treeItem} key={key}>
        <div
          className={styles.treeRow}
          data-selected={active}
          data-drop-state={
            rowDropState?.(item) ??
            (dragging
              ? dropOver === key
                ? denied
                  ? "invalid"
                  : "ready"
                : denied
                  ? "blocked"
                  : "available"
              : undefined)
          }
          data-drag-source={!!dragging && dragging.kind === item.kind && dragging.id === item.id}
          data-testid={`${testIdPrefix}-${item.kind}-${item.id}`}
          draggable={movable || (!!onRowDragStart && !!source)}
          onDragStart={(event: DragEvent<HTMLDivElement>) => {
            if (onRowDragStart) {
              onRowDragStart(event, item);
              return;
            }
            if (!source || !movable) return;
            event.stopPropagation();
            event.dataTransfer.setData(dragMime, JSON.stringify(source));
            event.dataTransfer.effectAllowed = "move";
            setFeedback("");
            setDragHint("Move over a highlighted destination to review this change.");
            setDragging(source);
            if (source.kind === "person") onPersonDragStateChange?.(source.id);
          }}
          onDragEnd={(event) => {
            event.stopPropagation();
            onRowDragEnd?.();
            setDragging(null);
            setDropOver("");
            setDragHint("");
            if (source?.kind === "person") onPersonDragStateChange?.(null);
          }}
          onDragOver={(event: DragEvent<HTMLDivElement>) => {
            if (onRowDragOver) {
              onRowDragOver(event, item);
              return;
            }
            if (!event.dataTransfer.types.includes(dragMime)) return;
            event.preventDefault();
            event.stopPropagation();
            event.dataTransfer.dropEffect = denied ? "none" : "move";
            setDropOver(key);
            setFeedback(denied ?? "");
            setDragHint(denied ?? `Release to review the move to ${name}.`);
          }}
          onDragLeave={(event) => {
            if (
              event.relatedTarget instanceof Node &&
              event.currentTarget.contains(event.relatedTarget)
            )
              return;
            setDropOver((current) => (current === key ? "" : current));
          }}
          onDrop={(event: DragEvent<HTMLDivElement>) => {
            if (onRowDrop) {
              onRowDrop(event, item);
              return;
            }
            if (panelMode) return;
            event.preventDefault();
            event.stopPropagation();
            try {
              const input = JSON.parse(event.dataTransfer.getData(dragMime)) as MoveSource;
              if (
                ["person", "position", "department"].includes(input.kind) &&
                typeof input.id === "string"
              )
                move(input, item);
            } catch {
              setFeedback("This item cannot be moved here.");
            }
            setDragging(null);
            setDropOver("");
          }}
        >
          {children ? (
            <Button
              variant="plain"
              type="button"
              className={styles.chevron}
              aria-label={`${expandedHere ? "Collapse" : "Expand"} ${name}`}
              aria-expanded={expandedHere}
              onClick={() => toggle(key)}
            >
              {expandedHere ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            </Button>
          ) : (
            <span className={styles.chevron} aria-hidden="true" />
          )}
          <Button
            variant="plain"
            type="button"
            className={styles.treeButton}
            aria-current={active ? "true" : undefined}
            onClick={() => select(item)}
          >
            <Icon size={16} />
            <span className={styles.treeLabel}>{name}</span>
            {count !== undefined && (
              <span className={styles.count}>{`${count} ${countLabel}`}</span>
            )}
          </Button>
          {rowControls?.(item)}
          {movable && <GripVertical size={14} className={styles.dragHandle} aria-hidden="true" />}
          {movable && (
            <div className={styles.rowActions}>
              <Button
                variant="plain"
                type="button"
                className={styles.rowMenuButton}
                aria-label={`Actions for ${name}`}
                aria-expanded={openRowMenu === key}
                onClick={() => setOpenRowMenu((value) => (value === key ? null : key))}
              >
                <MoreHorizontal size={17} />
              </Button>
              {openRowMenu === key && (
                <div className={styles.rowMenu}>
                  <Link href={moveHref(source)} onClick={() => setOpenRowMenu(null)}>
                    Move {labels[item.kind].toLowerCase()}…
                  </Link>
                  {source.kind === "person" && onAddPersonToTray && (
                    <Button
                      variant="plain"
                      type="button"
                      onClick={() => {
                        onAddPersonToTray(source.id);
                        setOpenRowMenu(null);
                      }}
                    >
                      Add to move tray
                    </Button>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
        {children && expandedHere && (
          <ul className={`${styles.treeList} ${styles.nested}`}>{children}</ul>
        )}
      </li>
    );
  }

  function personRow(person: ExplorerViewData["people"][number]) {
    return row({ kind: "person", id: person.id }, person.name);
  }
  function positionRow(position: ExplorerViewData["positions"][number]) {
    const holders = people.filter((person) => person.positionId === position.id);
    return row(
      { kind: "position", id: position.id },
      position.title,
      holders.length,
      holders.length ? holders.map(personRow) : undefined,
    );
  }
  function departmentRow(department: ExplorerViewData["nodes"][number]): ReactNode {
    const assigned = visiblePositions.filter((position) => position.departmentId === department.id);
    const children = visibleNodes.filter(
      (node) =>
        node.parentId === department.id && (node.kind === "department" || node.kind === "team"),
    );
    const content: ReactNode[] = [...children.map(departmentRow), ...assigned.map(positionRow)];
    return row(
      { kind: department.kind, id: department.id },
      department.name,
      assigned.length,
      content.length ? content : undefined,
    );
  }
  function branchRow(branch: ExplorerViewData["nodes"][number]) {
    const departments = visibleNodes.filter(
      (node) =>
        (node.kind === "department" || node.kind === "team") &&
        (node.branchId === branch.id || node.parentId === branch.id) &&
        !visibleNodes.some(
          (parent) =>
            parent.id === node.parentId && (parent.kind === "department" || parent.kind === "team"),
        ),
    );
    const direct = visiblePositions.filter(
      (position) =>
        position.branchId === branch.id &&
        (!position.departmentId || !visibleNodes.some((node) => node.id === position.departmentId)),
    );
    const children = [...departments.map(departmentRow), ...direct.map(positionRow)];
    return row(
      { kind: "branch", id: branch.id },
      branch.name,
      visiblePositions.filter((position) => position.branchId === branch.id).length,
      children.length ? children : undefined,
    );
  }
  function companyRow(company: ExplorerViewData["companies"][number]) {
    const branches = visibleNodes.filter(
      (node) => node.kind === "branch" && node.companyId === company.id,
    );
    const directDepartments = visibleNodes.filter(
      (node) =>
        (node.kind === "department" || node.kind === "team") &&
        node.companyId === company.id &&
        !node.branchId &&
        !visibleNodes.some(
          (parent) =>
            parent.id === node.parentId && (parent.kind === "department" || parent.kind === "team"),
        ),
    );
    const directPositions = visiblePositions.filter(
      (position) =>
        position.companyId === company.id &&
        !position.branchId &&
        (!position.departmentId || !visibleNodes.some((node) => node.id === position.departmentId)),
    );
    const unassigned = allUnassigned.filter((person) => person.companyId === company.id);
    const children = [
      ...branches.map(branchRow),
      ...directDepartments.map(departmentRow),
      ...directPositions.map(positionRow),
      ...(unassigned.length
        ? [
            row(
              { kind: "unassigned", id: company.id },
              `Unassigned people`,
              unassigned.length,
              unassigned.map(personRow),
            ),
          ]
        : []),
    ];
    return row(
      { kind: "company", id: company.id },
      company.name,
      people.filter((person) => person.companyId === company.id).length,
      children.length ? children : undefined,
    );
  }

  const person = data.people.find((item) => selected.kind === "person" && item.id === selected.id);
  const position = data.positions.find(
    (item) => item.id === (selected.kind === "position" ? selected.id : person?.positionId),
  );
  const node = data.nodes.find((item) => item.id === selected.id);
  const level = data.levels.find(
    (item) => item.id === (selected.kind === "level" ? selected.id : position?.levelId),
  );
  const companyId =
    selected.kind === "company" || selected.kind === "unassigned"
      ? selected.id
      : (person?.companyId ?? position?.companyId ?? node?.companyId ?? level?.companyId);
  const company = data.companies.find((item) => item.id === companyId);
  const levelCompanyId =
    selectedCompany !== "all" ? selectedCompany : (company?.id ?? initialCompanyId);
  const branchId =
    selected.kind === "branch"
      ? selected.id
      : (person?.branchId ?? position?.branchId ?? node?.branchId);
  const branch = data.nodes.find((item) => item.id === branchId && item.kind === "branch");
  const departmentId =
    selected.kind === "department" || selected.kind === "team"
      ? selected.id
      : (person?.departmentId ?? position?.departmentId);
  const department = data.nodes.find(
    (item) => item.id === departmentId && (item.kind === "department" || item.kind === "team"),
  );
  const selectedName =
    person?.name ??
    (selected.kind === "position" ? position?.title : null) ??
    (selected.kind === "level" ? `L${level?.number ?? ""} · ${level?.name ?? ""}` : null) ??
    (selected.kind === "unassigned" ? "Unassigned people" : null) ??
    node?.name ??
    company?.name ??
    data.holdingName;
  const scopePositions = data.positions.filter(
    (item) =>
      (selected.kind === "company" && item.companyId === selected.id) ||
      (selected.kind === "branch" && item.branchId === selected.id) ||
      ((selected.kind === "department" || selected.kind === "team") &&
        item.departmentId === selected.id) ||
      (selected.kind === "level" && item.levelId === selected.id) ||
      (selected.kind === "position" && item.id === selected.id) ||
      (selected.kind === "person" && item.id === person?.positionId),
  );
  const scopePeople = data.people.filter(
    (item) =>
      (status === "archived"
        ? item.status === "archived"
        : item.status !== "archived" && item.status !== "ended") &&
      ((selected.kind === "company" && item.companyId === selected.id) ||
        (selected.kind === "branch" && item.branchId === selected.id) ||
        ((selected.kind === "department" || selected.kind === "team") &&
          item.departmentId === selected.id) ||
        (selected.kind === "level" &&
          scopePositions.some((position) => position.id === item.positionId)) ||
        (selected.kind === "position" && item.positionId === selected.id) ||
        (selected.kind === "person" && item.id === selected.id) ||
        (selected.kind === "unassigned" &&
          item.companyId === selected.id &&
          (!item.positionId ||
            !data.positions.some((position) => position.id === item.positionId)))),
  );
  const vacant = scopePositions.filter(
    (item) =>
      item.active !== false &&
      !data.people.some(
        (person) =>
          person.positionId === item.id &&
          person.status !== "archived" &&
          person.status !== "ended",
      ),
  );
  const context: OrganizationExplorerContext = {
    company: directory.companies.find((item) => item.id === companyId),
    people: [...directory.people, ...archivedPeople].filter((item) =>
      scopePeople.some((person) => person.id === item.id),
    ),
    positions: directory.positions.filter((item) =>
      scopePositions.some((position) => position.id === item.id),
    ),
    nodes: directory.nodes,
    level: directory.levels.find((item) => item.id === level?.id),
    directory,
  };
  const availableTabs = tabsForSelection?.(selected) ?? tabNames(selected.kind);
  const allowedTabs = visibleTabs?.(selected);
  const tabs =
    allowedTabs === undefined
      ? availableTabs
      : allowedTabs.filter((tab) => availableTabs.includes(tab));
  if (tabs.length === 0) tabs.push(availableTabs[0]);
  const currentTab =
    tabs.find((tab) => tab.toLowerCase() === (controlledTab ?? query.get("tab"))?.toLowerCase()) ??
    tabs[0];
  const changeTab = (tab: string) =>
    onTabChange
      ? onTabChange(tab)
      : patchUrl({ tab: tab === tabs[0] ? undefined : tab.toLowerCase() });
  const DetailIcon = icons[selected.kind];
  const source = sourceFor(selected);
  const canMoveSelected = canMoveSource(source);
  const queryText = search.trim().toLocaleLowerCase();
  const searchRows: { selection: OrganizationSelection; name: string; detail: string }[] = [
    ...visibleCompanies.map((item) => ({
      selection: { kind: "company" as const, id: item.id },
      name: item.name,
      detail: "Company",
    })),
    ...visibleNodes.map((item) => ({
      selection: { kind: item.kind, id: item.id },
      name: item.name,
      detail: labels[item.kind],
    })),
    ...visiblePositions.map((item) => ({
      selection: { kind: "position" as const, id: item.id },
      name: item.title,
      detail: companies.find((company) => company.id === item.companyId)?.name ?? "Company",
    })),
    ...people.map((item) => ({
      selection: { kind: "person" as const, id: item.id },
      name: item.name,
      detail: `${item.jobTitle ?? data.positions.find((position) => position.id === item.positionId)?.title ?? "No position"} · ${companies.find((company) => company.id === item.companyId)?.name ?? "Company"} · ${item.workEmail ?? "No work email"}`,
    })),
  ].filter(
    (item) =>
      (!treeSelections ||
        treeSelections.some((s) => s.kind === item.selection.kind && s.id === item.selection.id)) &&
      (!queryText || `${item.name} ${item.detail}`.toLocaleLowerCase().includes(queryText)),
  );

  const crumbs: { selection: OrganizationSelection; name: string }[] = [
    ...(company && selected.kind !== "company"
      ? [{ selection: { kind: "company" as const, id: company.id }, name: company.name }]
      : []),
    ...(branch && selected.kind !== "branch"
      ? [{ selection: { kind: "branch" as const, id: branch.id }, name: branch.name }]
      : []),
    ...(department && selected.kind !== department.kind
      ? [{ selection: { kind: department.kind, id: department.id }, name: department.name }]
      : []),
    ...(position && selected.kind === "person"
      ? [{ selection: { kind: "position" as const, id: position.id }, name: position.title }]
      : []),
  ];

  function defaultDetail() {
    if (selected.kind === "person" && person) {
      if (currentTab === "Profile")
        return (
          <dl className={styles.facts}>
            <div>
              <dt>Full name</dt>
              <dd>{person.name}</dd>
            </div>
            <div>
              <dt>Work email</dt>
              <dd>{person.workEmail ?? "Not provided"}</dd>
            </div>
            <div>
              <dt>Work phone</dt>
              <dd>{person.workPhone ?? "Not provided"}</dd>
            </div>
            <div>
              <dt>Company</dt>
              <dd>{company?.name ?? "—"}</dd>
            </div>
          </dl>
        );
      if (currentTab === "Employment")
        return (
          <dl className={styles.facts}>
            <div>
              <dt>Position</dt>
              <dd>{position?.title ?? "No position assigned"}</dd>
            </div>
            <div>
              <dt>Branch</dt>
              <dd>{branch?.name ?? "Company-wide"}</dd>
            </div>
            <div>
              <dt>Department</dt>
              <dd>{department?.name ?? "Not assigned"}</dd>
            </div>
            <div>
              <dt>Level</dt>
              <dd>{level ? `L${level.number} · ${level.name}` : "Not assigned"}</dd>
            </div>
          </dl>
        );
      if (currentTab === "Compensation")
        return (
          <p className={styles.subtle}>
            Compensation is available to authorized HR and payroll roles.
          </p>
        );
      return (
        <p className={styles.subtle}>Employment history is available to authorized HR roles.</p>
      );
    }
    if (currentTab === "People")
      return (
        <ul className={styles.items}>
          {scopePeople.map((item) => (
            <li key={item.id}>
              <Button
                variant="plain"
                type="button"
                onClick={() => select({ kind: "person", id: item.id })}
              >
                <strong>{item.name}</strong>
                <small>
                  {item.jobTitle ??
                    data.positions.find((position) => position.id === item.positionId)?.title ??
                    "No position"}{" "}
                  · {item.workEmail ?? "No work email"}
                </small>
              </Button>
            </li>
          ))}
          {scopePeople.length === 0 && (
            <li className={styles.empty}>No people in this part of the organization.</li>
          )}
        </ul>
      );
    if (currentTab === "Positions")
      return (
        <ul className={styles.items}>
          {scopePositions.map((item) => (
            <li key={item.id}>
              <Button
                variant="plain"
                type="button"
                onClick={() => select({ kind: "position", id: item.id })}
              >
                <strong>{item.title}</strong>
                <small>
                  {data.people.filter((person) => person.positionId === item.id).length} people ·{" "}
                  {data.levels.find((level) => level.id === item.levelId)?.name ?? "No level"}
                </small>
              </Button>
            </li>
          ))}
          {scopePositions.length === 0 && (
            <li className={styles.empty}>No positions in this part of the organization.</li>
          )}
        </ul>
      );
    if (currentTab === "Changes" || currentTab === "History")
      return (
        <p className={styles.subtle}>
          Changes and approval history appear here when available to your role.
        </p>
      );
    if (currentTab === "Approvals")
      return (
        <p className={styles.subtle}>
          Department approvers appear here when available to your role.
        </p>
      );
    if (currentTab === "Settings")
      return (
        <p className={styles.subtle}>Company settings appear here when available to your role.</p>
      );
    if (currentTab === "Compensation")
      return (
        <p className={styles.subtle}>Position default pay is available to authorized HR roles.</p>
      );
    if (selected.kind === "position" && position)
      return (
        <dl className={styles.facts}>
          <div>
            <dt>Level</dt>
            <dd>{level ? `L${level.number} · ${level.name}` : "Not assigned"}</dd>
          </div>
          <div>
            <dt>Department</dt>
            <dd>{department?.name ?? "Company-wide"}</dd>
          </div>
          <div>
            <dt>Reports to</dt>
            <dd>
              {data.positions.find((item) => item.id === position.reportsToPositionId)?.title ??
                "Company leadership"}
            </dd>
          </div>
          <div>
            <dt>People</dt>
            <dd>{scopePeople.length}</dd>
          </div>
        </dl>
      );
    return (
      <dl className={styles.facts}>
        <div>
          <dt>Company</dt>
          <dd>{company?.name ?? "—"}</dd>
        </div>
        <div>
          <dt>Branches</dt>
          <dd>
            {
              data.nodes.filter((item) => item.kind === "branch" && item.companyId === company?.id)
                .length
            }
          </dd>
        </div>
        <div>
          <dt>{status === "archived" ? "Archived people" : "People"}</dt>
          <dd>{scopePeople.length}</dd>
        </div>
        <div>
          <dt>Positions</dt>
          <dd>{scopePositions.length}</dd>
        </div>
      </dl>
    );
  }
  const customDetail = renderDetail?.(selected, currentTab, context);

  return (
    <section
      className={`${styles.workspace} ${panelMode ? styles.panelWorkspace : ""}`}
      aria-label="People and organization explorer"
    >
      <div className={styles.toolbar}>
        <Field className={styles.search}>
          <Search size={17} />
          <span className={styles.srOnly}>{searchLabel}</span>
          <TextInput
            type="search"
            placeholder="Search people, positions, departments…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </Field>
        {!panelMode && (
          <>
            <div className={styles.switcher} role="group" aria-label="People display">
              <Button
                variant="plain"
                type="button"
                aria-pressed={display === "structure"}
                onClick={() => patchUrl({ display: undefined })}
              >
                <Network size={16} />
                Structure
              </Button>
              <Button
                variant="plain"
                type="button"
                aria-pressed={display === "list"}
                onClick={() => patchUrl({ display: "list" })}
              >
                <List size={16} />
                List
              </Button>
            </div>
            <Field className={styles.filter}>
              Company
              <Select
                value={selectedCompany}
                onChange={(event) => {
                  const companyId = event.target.value;
                  if (companyId === "all") patchUrl({ filterCompany: undefined });
                  else
                    patchUrl({
                      ...Object.fromEntries(selectionKeys.map((key) => [key, undefined])),
                      filterCompany: companyId,
                      company: companyId,
                      tab: undefined,
                    });
                }}
              >
                <option value="all">All in Holding</option>
                {companies.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
            </Field>
            {canViewArchived && (
              <Field className={styles.filter}>
                Status
                <Select
                  value={status}
                  onChange={(event) => {
                    const clear = Object.fromEntries(selectionKeys.map((key) => [key, undefined]));
                    patchUrl({
                      ...clear,
                      company:
                        selectedCompany !== "all"
                          ? selectedCompany
                          : (company?.id ?? initialCompanyId),
                      tab: undefined,
                      status: event.target.value === "archived" ? "archived" : undefined,
                    });
                  }}
                >
                  <option value="current">Current</option>
                  <option value="archived">Archived</option>
                </Select>
              </Field>
            )}
          </>
        )}
        {headerActions}
      </div>
      <div className={styles.layout} data-mobile-detail={explicitSelection}>
        <aside
          className={styles.explorer}
          aria-label={display === "structure" ? "Organization structure" : "People list"}
        >
          <div className={styles.paneHeading}>
            <div>
              <h2>{display === "structure" ? "Organization" : "People"}</h2>
              <span>
                {display === "structure"
                  ? data.holdingName
                  : `${searchRows.filter((item) => item.selection.kind === "person").length} people`}
              </span>
            </div>
            {display === "structure" && (
              <div className={styles.paneTools}>
                <Button
                  variant="plain"
                  type="button"
                  onClick={() =>
                    setExpanded(
                      new Set([
                        ...visibleCompanies.map((item) => `company:${item.id}`),
                        ...visibleNodes.map((item) => `${item.kind}:${item.id}`),
                      ]),
                    )
                  }
                >
                  Expand
                </Button>
                <Button variant="plain" type="button" onClick={() => setExpanded(new Set())}>
                  Collapse
                </Button>
              </div>
            )}
          </div>
          {display === "structure" ? (
            <div className={styles.treeScroll}>
              {treeSelections && !queryText ? (
                <ul className={styles.treeList}>
                  {treeSelections.map((item) => {
                    const title =
                      item.kind === "person"
                        ? data.people.find((p) => p.id === item.id)?.name
                        : item.kind === "position"
                          ? data.positions.find((p) => p.id === item.id)?.title
                          : item.kind === "level"
                            ? data.levels.find((l) => l.id === item.id)?.name
                            : item.kind === "company"
                              ? data.companies.find((c) => c.id === item.id)?.name
                              : data.nodes.find((n) => n.id === item.id)?.name;
                    return row(item, title ?? labels[item.kind]);
                  })}
                  {!treeSelections.length && <li className={styles.empty}>No items here.</li>}
                </ul>
              ) : queryText ? (
                <ul className={styles.treeList}>
                  {searchRows.map((item) => row(item.selection, item.name))}
                  {searchRows.length === 0 && (
                    <li className={styles.empty}>No matching people or organization items.</li>
                  )}
                </ul>
              ) : (
                <>
                  <ul className={styles.treeList}>{visibleCompanies.map(companyRow)}</ul>
                  <div className={styles.groupHeading}>
                    <Button
                      variant="plain"
                      type="button"
                      aria-expanded={levelsExpanded}
                      onClick={() => setLevelsExpanded((value) => !value)}
                    >
                      {levelsExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      Company levels ·{" "}
                      {data.companies.find((item) => item.id === levelCompanyId)?.name ?? "Company"}
                    </Button>
                  </div>
                  {levelsExpanded && (
                    <ul className={styles.treeList}>
                      {data.levels
                        .filter((item) => item.companyId === levelCompanyId)
                        .map((item) =>
                          row(
                            { kind: "level", id: item.id },
                            `L${item.number} · ${item.name}`,
                            visiblePositions.filter((position) => position.levelId === item.id)
                              .length,
                          ),
                        )}
                    </ul>
                  )}
                </>
              )}
            </div>
          ) : (
            <div className={`${styles.treeScroll} ${styles.listScroll}`}>
              <ul className={styles.items}>
                {searchRows
                  .filter((item) => item.selection.kind === "person")
                  .map((item) => {
                    const personSource = sourceFor(item.selection);
                    return (
                      <li key={item.selection.id} className={styles.listItem}>
                        <Button
                          variant="plain"
                          type="button"
                          onClick={() => select(item.selection)}
                        >
                          <strong>{item.name}</strong>
                          <small>{item.detail}</small>
                        </Button>
                        {canMoveSource(personSource) && (
                          <Link
                            className={styles.listMove}
                            href={moveHref(personSource)}
                            aria-label={`Move ${item.name}`}
                          >
                            Move…
                          </Link>
                        )}
                        {canMoveSource(personSource) && onAddPersonToTray && (
                          <Button
                            variant="plain"
                            className={styles.listMove}
                            type="button"
                            onClick={() => onAddPersonToTray(personSource.id)}
                          >
                            Add to tray
                          </Button>
                        )}
                      </li>
                    );
                  })}
                {!searchRows.some((item) => item.selection.kind === "person") && (
                  <li className={styles.empty}>No matching people.</li>
                )}
              </ul>
            </div>
          )}
          <p className={styles.treeHint} role="status">
            {feedback ||
              dragHint ||
              (canReviewMove
                ? "Drag to a valid target, or use Move on the selected item. Every move opens a review."
                : "Select an item to view details.")}
          </p>
        </aside>
        <section className={styles.detail} aria-label="Selected organization details">
          {detailEmpty ? (
            <p className={styles.subtle}>Select a name to view details.</p>
          ) : (
            <>
              {!panelMode && (
                <Button
                  variant="plain"
                  type="button"
                  className={styles.back}
                  onClick={() => {
                    const clear = Object.fromEntries(
                      [...selectionKeys, ...clearQueryKeys].map((key) => [key, undefined]),
                    );
                    patchUrl({ ...clear, tab: undefined });
                  }}
                >
                  ← Organization
                </Button>
              )}
              <nav className={styles.breadcrumb} aria-label="Organization path">
                {crumbs.map((crumb) => (
                  <span key={`${crumb.selection.kind}:${crumb.selection.id}`}>
                    <Button variant="plain" type="button" onClick={() => select(crumb.selection)}>
                      {crumb.name}
                    </Button>
                    <ChevronRight size={12} aria-hidden="true" />
                  </span>
                ))}
                <span aria-current="page">{selectedName}</span>
              </nav>
              <div className={styles.hero}>
                <div className={styles.heroIcon}>
                  <DetailIcon size={23} />
                </div>
                <div className={styles.heroText}>
                  <span className={styles.kicker}>{labels[selected.kind]}</span>
                  <h2
                    id={panelMode ? `${testIdPrefix}-detail-title` : "organization-detail-title"}
                    tabIndex={-1}
                  >
                    {selectedName}
                  </h2>
                  <p>
                    {selected.kind === "person"
                      ? `${position?.title ?? person?.jobTitle ?? "No position"} · ${company?.name ?? "Company"}`
                      : selected.kind === "position"
                        ? `${department?.name ?? "Company-wide"} · ${company?.name ?? "Company"}`
                        : `${scopePeople.length} ${status === "archived" ? "archived people" : "people"} · ${scopePositions.length} positions`}
                  </p>
                </div>
                {canMoveSelected && (
                  <Link
                    className={styles.heroMove}
                    href={moveHref(source)}
                    aria-label={`Move ${selectedName}`}
                  >
                    Move…
                  </Link>
                )}
                {canMoveSelected && source.kind === "person" && onAddPersonToTray && (
                  <Button
                    variant="plain"
                    className={styles.heroMove}
                    type="button"
                    onClick={() => onAddPersonToTray(source.id)}
                  >
                    Add to tray
                  </Button>
                )}
              </div>
              {detailStatus}
              {selected.kind !== "person" && (
                <div className={styles.metrics}>
                  <div className={styles.metric}>
                    <strong>{scopePeople.length}</strong>
                    <span>{status === "archived" ? "Archived people" : "People"}</span>
                  </div>
                  <div className={styles.metric}>
                    <strong>{scopePositions.length}</strong>
                    <span>Positions</span>
                  </div>
                  <div className={styles.metric}>
                    <strong>{vacant.length}</strong>
                    <span>Vacant positions</span>
                  </div>
                </div>
              )}
              <div
                className={styles.tabs}
                role="tablist"
                aria-label="Detail section"
                onKeyDown={(event) => {
                  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
                  const current = tabs.indexOf(currentTab);
                  const index =
                    event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? tabs.length - 1
                        : (current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) %
                          tabs.length;
                  event.preventDefault();
                  changeTab(tabs[index]);
                  (event.currentTarget.children[index] as HTMLButtonElement)?.focus();
                }}
              >
                {tabs.map((tab) => (
                  <Button
                    variant="plain"
                    type="button"
                    role="tab"
                    key={tab}
                    id={`${testIdPrefix}-tab-${tab}`}
                    aria-controls={`${testIdPrefix}-content`}
                    tabIndex={currentTab === tab ? 0 : -1}
                    aria-selected={currentTab === tab}
                    onClick={() => changeTab(tab)}
                  >
                    {tab}
                  </Button>
                ))}
              </div>
              <label className={styles.srOnly} htmlFor={`${testIdPrefix}-section`}>
                Section
              </label>
              <Select
                id={`${testIdPrefix}-section`}
                className={styles.sectionSelect}
                aria-label="Detail section"
                value={currentTab}
                onChange={(event) => changeTab(event.target.value)}
              >
                {tabs.map((tab) => (
                  <option key={tab}>{tab}</option>
                ))}
              </Select>
              <div
                className={styles.content}
                role="tabpanel"
                id={`${testIdPrefix}-content`}
                aria-labelledby={`${testIdPrefix}-tab-${currentTab}`}
              >
                {customDetail !== undefined ? customDetail : defaultDetail()}
              </div>
            </>
          )}{" "}
        </section>
      </div>
    </section>
  );
}
