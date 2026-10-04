"use client";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TextArea, TextInput } from "@/components/ui/foundation";

import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type DragEvent,
  type ReactNode,
} from "react";
import { useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import {
  ArrowRight,
  BriefcaseBusiness,
  Building2,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock3,
  FileCog,
  FolderTree,
  GripVertical,
  Layers3,
  Link2,
  MoveRight,
  Network,
  Plus,
  Search,
  ShieldCheck,
  Users,
  UserRound,
} from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import { retirementInstant, retirementLocalTime } from "@/lib/retirement";
import { formatUiDateTime } from "@/lib/ui-format";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { OrganizationPerson, OrganizationPosition } from "@/lib/organization";
import type { DirectoryAutomationData } from "@/lib/directory-operations";
import type { PeopleLifecycleData, LifecycleInput, LifecyclePreview } from "@/lib/people-lifecycle";
import type { TechnologyData } from "@/lib/technology";
import type {
  GpoRule,
  GpoRuleInput,
  GpoRulePreview,
  GpoRuleTarget,
  GpoRulesOverview,
} from "@/lib/gpo-rules";
import type {
  PositionMoveInput,
  PositionMovePreview,
  PositionMoveRecord,
} from "@/lib/position-moves";
import { DirectoryWorkspace } from "@/components/people/directory-workspace";
import {
  OrganizationExplorer,
  type OrganizationSelection,
} from "@/components/people/organization-explorer";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import { PositionConsole } from "./position-console";
import { ProvisioningWorkspace } from "./provisioning-workspace";
import styles from "./organization-access-workspace.module.css";

type Selection = {
  kind:
    | "company"
    | "branch"
    | "department"
    | "position"
    | "employee"
    | "level"
    | "directory"
    | "unassigned";
  id: string;
  connectionId?: string;
};
type DragItem = { kind: "employee" | "position" | "gpo"; id: string; connectionId?: string };
export type OrganizationAccessEdit =
  | {
      kind: "gpo";
      target: GpoRuleTarget;
      gpoId?: string;
      connectionId?: string;
      rule?: GpoRule;
      remove?: boolean;
    }
  | { kind: "position"; positionId: string; targetKind: "department" | "level"; targetId: string }
  | { kind: "employee"; engagementId: string; positionId: string; departmentId?: string };
type Edit = OrganizationAccessEdit;
type Drawer =
  "detail" | "library" | "directory" | "provisioning" | "preparation" | "operations" | null;
const dragMime = "application/erpx-organization-access";
const mobileQuery = "(max-width:760px)";
function subscribeMobile(listener: () => void) {
  const media = window.matchMedia(mobileQuery);
  media.addEventListener("change", listener);
  return () => media.removeEventListener("change", listener);
}
function readMobile() {
  return window.matchMedia(mobileQuery).matches;
}
function serverMobile() {
  return false;
}
const icons = {
  company: Building2,
  branch: Building2,
  department: FolderTree,
  position: BriefcaseBusiness,
  employee: UserRound,
  level: Layers3,
  directory: Network,
  unassigned: Users,
};
const kindLabels = {
  company: "Company",
  branch: "Branch",
  department: "Department",
  position: "Position",
  employee: "Employee",
  level: "Company level",
  directory: "Directory object",
  unassigned: "Unassigned employees",
};
const initialRules: GpoRulesOverview = {
  rules: [],
  effectiveByPosition: {},
  jobs: [],
  tasks: [],
  canManage: false,
};
function time(value: number, timeZone = "Asia/Bangkok") {
  return `${formatUiDateTime(value, timeZone)} · ${timeZone}`;
}
function same(a: Selection, b: Selection) {
  return a.kind === b.kind && a.id === b.id && a.connectionId === b.connectionId;
}
function targetFor(selection: Selection): GpoRuleTarget | null {
  return selection.kind === "position" ? { kind: "position", id: selection.id } : null;
}
function selectionFromQuery(
  query: Pick<URLSearchParams, "get">,
  organization: OrganizationAccessData,
): Selection {
  const position = query.get("position"),
    employee = query.get("employee"),
    person = query.get("person");
  if (position) return { kind: "position", id: position };
  if (person || employee)
    return {
      kind: "employee",
      id:
        organization.people.find(
          (item) => item.person_id === person || item.engagement_id === person,
        )?.engagement_id ??
        employee ??
        person!,
    };
  for (const kind of ["department", "level", "branch"] as const) {
    const id = query.get(kind);
    if (id) return { kind, id };
  }
  const object = query.get("object");
  if (object)
    return { kind: "directory", id: object, connectionId: query.get("connection") ?? undefined };
  if (query.get("unassigned")) return { kind: "unassigned", id: "unassigned" };
  if (query.get("view") === "directory") return { kind: "directory", id: "directory" };
  if (query.get("view") === "employees" && organization.people.length)
    return { kind: "employee", id: organization.people[0].engagement_id };
  return {
    kind: "company",
    id:
      query.get("company") ??
      organization.nodes.find((node) => node.kind === "company")?.id ??
      "company",
  };
}
function sharedDirectory(
  organization: OrganizationAccessData,
  companyName: string,
): HoldingPeopleDirectory {
  const company = organization.nodes.find((node) => node.kind === "company");
  const companyId =
    company?.id ??
    organization.positions[0]?.company_id ??
    organization.people[0]?.company_id ??
    "company";
  const activeNodes = organization.nodes.filter(
    (node) =>
      node.status === "active" &&
      ["branch", "headquarters", "department", "team"].includes(node.kind),
  );
  return {
    companies: [{ id: companyId, slug: company?.slug ?? "company", name: companyName }],
    nodes: activeNodes.map((node) => ({
      id: node.id,
      companyId,
      kind: node.kind as "branch" | "headquarters" | "department" | "team",
      slug: node.slug,
      name: node.display_name,
      parentId: node.kind === "department" || node.kind === "team" ? node.branch_id : companyId,
      branchId: node.kind === "branch" || node.kind === "headquarters" ? null : node.branch_id,
    })),
    levels: organization.levels.map((level) => ({
      id: level.id,
      companyId,
      code: `L${level.level_number}`,
      name: level.name,
      number: level.level_number,
    })),
    positions: organization.positions
      .filter((position) => position.active)
      .map((position) => ({
        id: position.id,
        companyId: position.company_id,
        branchId: position.branch_id,
        departmentId: position.department_id,
        title: position.title,
        levelId: position.level_id,
        reportsToPositionId: position.reports_to_position_id,
        isCeo: !!position.is_ceo,
      })),
    people: organization.people
      .filter((person) => person.status !== "ended")
      .map((person) => ({
        id: person.engagement_id,
        personId: person.person_id,
        engagementId: person.engagement_id,
        companyId: person.company_id,
        branchId: person.branch_id,
        departmentId: person.department_id,
        positionId: person.position_id,
        legalName: person.legal_name,
        jobTitle: person.job_title,
        workEmail: null,
        workPhone: null,
      })),
  };
}
function sharedSelection(selection: Selection, companyId: string): OrganizationSelection {
  if (selection.kind === "employee") return { kind: "person", id: selection.id };
  if (selection.kind === "directory") return { kind: "company", id: companyId };
  if (selection.kind === "company") return { kind: "company", id: companyId };
  if (selection.kind === "unassigned") return { kind: "unassigned", id: companyId };
  return { kind: selection.kind, id: selection.id };
}
function selectionAncestors(
  organization: OrganizationAccessData,
  selection?: Selection,
): Set<string> {
  const expanded = new Set(["company:company"]);
  if (!selection) return expanded;
  const person =
    selection.kind === "employee"
      ? organization.people.find((person) => person.engagement_id === selection.id)
      : undefined;
  const position = organization.positions.find(
    (position) =>
      position.id === (selection.kind === "position" ? selection.id : person?.position_id),
  );
  const department = organization.nodes.find(
    (node) =>
      node.id ===
      (selection.kind === "department"
        ? selection.id
        : (position?.department_id ?? person?.department_id)),
  );
  const branchId = position?.branch_id ?? department?.branch_id ?? person?.branch_id;
  if (branchId) expanded.add(`branch:${branchId}`);
  if (position?.department_id) expanded.add(`department:${position.department_id}`);
  if (person?.position_id) expanded.add(`position:${person.position_id}`);
  if (person && !position) expanded.add("unassigned:unassigned");
  if (selection.kind === "directory") expanded.add("directory");
  return expanded;
}
function Section({
  title,
  icon,
  children,
  count,
}: {
  title: string;
  icon: ReactNode;
  children: ReactNode;
  count?: number;
}) {
  return (
    <details open className={styles.section}>
      <summary>
        <span>
          {icon}
          {title}
          {count !== undefined && <span className={styles.badge}>{count}</span>}
        </span>
        <ChevronRight size={15} />
      </summary>
      <div className={styles.sectionBody}>{children}</div>
    </details>
  );
}

export function OrganizationAccessWorkspace({
  base,
  companyName,
  organization,
  lifecycle,
  automation,
  technology,
  gpoRules = initialRules,
  canApply,
  initialSelection,
  initialSection,
}: {
  base: string;
  companyName: string;
  organization: OrganizationAccessData;
  lifecycle: PeopleLifecycleData;
  automation: DirectoryAutomationData;
  technology: TechnologyData;
  gpoRules?: GpoRulesOverview;
  canApply: boolean;
  initialSelection?: Selection;
  initialSection?: string;
}) {
  const router = useRouter();
  const query = useSearchParams();
  const mobile = useSyncExternalStore(subscribeMobile, readMobile, serverMobile);
  const selected = selectionFromQuery(query, organization);
  const directory = sharedDirectory(organization, companyName);
  const companyId = directory.companies[0].id;
  const legacyDirectoryView = selected.kind === "directory";
  const selectionKey = `${selected.kind}:${selected.id}:${selected.connectionId ?? ""}`;
  const [previousSelectionKey, setPreviousSelectionKey] = useState(selectionKey);
  const section = query.get("section");
  const [previousSection, setPreviousSection] = useState(section);
  const [search, setSearch] = useState("");
  const [policySearch, setPolicySearch] = useState("");
  const [connectionId, setConnectionId] = useState(automation.connections[0]?.id ?? "");
  const [collapsed, setCollapsed] = useState<string[]>(() => {
    const expanded = selectionAncestors(organization, initialSelection);
    return [
      "directory",
      "unassigned:unassigned",
      ...organization.nodes
        .filter((node) => node.kind === "branch" || node.kind === "department")
        .map((node) => `${node.kind}:${node.id}`),
      ...organization.positions.map((position) => `position:${position.id}`),
    ].filter((key) => !expanded.has(key));
  });
  const [drawer, setDrawer] = useState<Drawer>(
    initialSection === "operations"
      ? "operations"
      : initialSelection?.kind === "directory"
        ? "detail"
        : null,
  );
  const [edit, setEdit] = useState<Edit | null>(null);
  const [dragging, setDragging] = useState<DragItem | null>(null);
  const [dropOver, setDropOver] = useState<string | null>(null);
  const [dragHint, setDragHint] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [moves, setMoves] = useState<PositionMoveRecord[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [cancelMove, setCancelMove] = useState<PositionMoveRecord | null>(null);
  const [retryTarget, setRetryTarget] = useState<{
    kind: "rule" | "job";
    id: string;
    version: number;
    name: string;
  } | null>(null);
  const [retryReason, setRetryReason] = useState("");
  const [retryReviewed, setRetryReviewed] = useState(false);
  const [closingTask, setClosingTask] = useState<GpoRulesOverview["tasks"][number] | null>(null);
  const [closureReason, setClosureReason] = useState("");
  // Selection follows the URL; refreshing data must not reset local forms or search.
  if (previousSelectionKey !== selectionKey) {
    setPreviousSelectionKey(selectionKey);
    const ancestors = selectionAncestors(organization, selected);
    setCollapsed((values) => values.filter((key) => !ancestors.has(key)));
    if (mobile && selected.kind === "directory") setDrawer("detail");
  }
  if (previousSection !== section) {
    setPreviousSection(section);
    if (section === "operations") setDrawer("operations");
    else if (previousSection === "operations")
      setDrawer((current) => (current === "operations" ? null : current));
  }
  const root = "/api/v1" + base;
  const positions = organization.positions.filter((p) => p.active);
  const people = organization.people.filter((p) => p.status !== "ended");
  const nodes = organization.nodes.filter((n) => n.status === "active");
  const unassigned = people.filter(
    (p) => !p.position_id || !positions.some((position) => position.id === p.position_id),
  );
  const connection = automation.connections.find((c) => c.id === connectionId);
  const library = (connection?.inventory?.objects ?? []).filter(
    (o) =>
      o.kind === "gpo" && o.name.toLocaleLowerCase().includes(policySearch.toLocaleLowerCase()),
  );
  const person = people.find(
    (p) => p.engagement_id === selected.id && selected.kind === "employee",
  );
  const position = positions.find(
    (p) => p.id === (selected.kind === "position" ? selected.id : person?.position_id),
  );
  const node = nodes.find((n) => n.id === selected.id);
  const level = organization.levels.find(
    (l) => l.id === (selected.kind === "level" ? selected.id : position?.level_id),
  );
  const directoryConnection = automation.connections.find((c) => c.id === selected.connectionId);
  const directoryObject = directoryConnection?.inventory?.objects.find((o) => o.id === selected.id);
  const scopePositions = positions.filter(
    (p) =>
      selected.kind === "company" ||
      (selected.kind === "branch" && p.branch_id === selected.id) ||
      (selected.kind === "department" && p.department_id === selected.id) ||
      (selected.kind === "level" && p.level_id === selected.id) ||
      p.id === position?.id,
  );
  const scopePeople =
    selected.kind === "employee"
      ? person
        ? [person]
        : []
      : selected.kind === "unassigned"
        ? unassigned
        : people.filter(
            (p) =>
              selected.kind === "company" ||
              (selected.kind === "branch" && p.branch_id === selected.id) ||
              (selected.kind === "department" && p.department_id === selected.id) ||
              scopePositions.some((position) => position.id === p.position_id),
          );
  const currentRules =
    selected.kind === "position" || selected.kind === "employee"
      ? (gpoRules.effectiveByPosition[position?.id ?? ""] ?? [])
      : gpoRules.rules.filter(
          (r) =>
            r.active &&
            (selected.kind === "company" ||
              (r.target.kind === selected.kind && r.target.id === selected.id)),
        );
  const groupedPolicies = Array.from(
    currentRules
      .reduce((map, rule) => {
        const key = rule.connectionId + ":" + rule.gpoId;
        map.set(key, [...(map.get(key) ?? []), rule]);
        return map;
      }, new Map<string, GpoRule[]>())
      .values(),
  );
  const scopeCases = lifecycle.cases.filter((record) =>
    scopePeople.some((person) => person.engagement_id === record.engagement_id),
  );
  const scopeJobs = gpoRules.jobs.filter(
    (j) =>
      (!person || j.person_id === person.person_id) &&
      scopePositions.some((p) => p.id === j.position_id),
  );
  const selectionName =
    person?.legal_name ??
    (selected.kind === "position" ? position?.title : null) ??
    (selected.kind === "level"
      ? `Level ${level?.level_number ?? ""} · ${level?.name ?? ""}`
      : null) ??
    node?.display_name ??
    directoryObject?.name ??
    (selected.kind === "unassigned"
      ? "Unassigned employees"
      : selected.kind === "directory"
        ? "Directory objects"
        : companyName);
  const Icon = icons[selected.kind];
  const selectionTarget = targetFor(selected);
  const objectName = (id: string, connId: string) =>
    automation.connections.find((c) => c.id === connId)?.inventory?.objects.find((o) => o.id === id)
      ?.name ?? id;
  const nodeName = (id: string | null) => nodes.find((n) => n.id === id)?.display_name ?? "Company";
  const positionName = (id: string | null) =>
    positions.find((p) => p.id === id)?.title ?? "Not assigned";
  const personName = (id: string | null) =>
    people.find((p) => p.person_id === id)?.legal_name ?? "Not assigned";

  useEffect(() => {
    if (!organization.canManage) return;
    let active = true;
    const refresh = async () => {
      try {
        const response = await workFetch(root + "/people/organization/position-moves", {
          cache: "no-store",
        });
        const value = await response.json();
        if (response.ok && active)
          setMoves(Array.isArray(value.data) ? value.data : (value.data?.moves ?? []));
      } catch {
        /* Existing page remains available; command failures have their own error state. */
      }
    };
    void refresh();
    const interval = setInterval(refresh, 15000);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [organization.canManage, root, organization.positions]);
  const pending =
    moves.some((m) => m.status === "scheduled") ||
    scopeCases.some((record) =>
      ["scheduled", "waiting_it", "pending_review"].includes(record.status),
    ) ||
    scopeJobs.some((j) => ["queued", "running", "retry"].includes(j.status));
  useEffect(() => {
    if (!pending) return;
    const timer = setInterval(() => router.refresh(), 15000);
    return () => clearInterval(timer);
  }, [pending, router]);

  function choose(next: Selection) {
    const ancestors = selectionAncestors(organization, next);
    setCollapsed((values) => values.filter((key) => !ancestors.has(key)));
    const query = new URLSearchParams({ view: "organization" });
    if (next.kind !== "company")
      query.set(
        next.kind === "employee" ? "person" : next.kind === "directory" ? "object" : next.kind,
        next.id,
      );
    if (next.connectionId) query.set("connection", next.connectionId);
    window.history.replaceState(null, "", base + "/technology?" + query.toString());
    if (next.kind === "directory" && window.matchMedia("(max-width:760px)").matches)
      setDrawer("detail");
  }
  function toggle(key: string) {
    setCollapsed((values) =>
      values.includes(key) ? values.filter((v) => v !== key) : [...values, key],
    );
  }
  function openEdit(next: Edit) {
    setError("");
    setNotice("");
    setEdit(next);
  }
  function reviewRetry(target: NonNullable<typeof retryTarget>) {
    setRetryTarget(target);
    setRetryReason("");
    setRetryReviewed(false);
    setError("");
  }
  function completed(message: string) {
    setNotice(message);
    setEdit(null);
    router.refresh();
  }
  function denial(item: DragItem, target: Selection): string | null {
    if (item.kind === "gpo")
      return !gpoRules.canManage
        ? "Your current role cannot manage GPO rules."
        : !targetFor(target)
          ? "Drop a GPO on a Position."
          : null;
    if (item.kind === "employee")
      return !organization.canMovePeople
        ? "Your current role cannot transfer employees."
        : !["position", "department"].includes(target.kind)
          ? "Drop an employee on a Position or Department."
          : target.kind === "position" &&
              people.find((p) => p.engagement_id === item.id)?.position_id === target.id
            ? "This employee already holds this position."
            : null;
    return !organization.canManage
      ? "Your current role cannot move positions."
      : !["department", "level"].includes(target.kind)
        ? "Drop a Position on a Department or Level."
        : null;
  }
  function drop(item: DragItem, target: Selection) {
    const invalid = denial(item, target);
    if (invalid) {
      setNotice(invalid);
      return;
    }
    if (item.kind === "gpo")
      openEdit({
        kind: "gpo",
        target: targetFor(target)!,
        gpoId: item.id,
        connectionId: item.connectionId,
      });
    else if (item.kind === "employee")
      openEdit({
        kind: "employee",
        engagementId: item.id,
        positionId: target.kind === "position" ? target.id : "",
        departmentId: target.kind === "department" ? target.id : undefined,
      });
    else
      openEdit({
        kind: "position",
        positionId: item.id,
        targetKind: target.kind as "department" | "level",
        targetId: target.id,
      });
  }
  function dragProps(item: DragItem, allowed: boolean) {
    return {
      draggable: allowed,
      onDragStart: (event: DragEvent) => {
        event.dataTransfer.setData(dragMime, JSON.stringify(item));
        event.dataTransfer.effectAllowed = "move";
        setDragging(item);
      },
      onDragEnd: () => {
        setDragging(null);
        setDropOver(null);
        setDragHint(null);
      },
    };
  }
  function dropProps(target: Selection) {
    const key = target.kind + ":" + target.id;
    return {
      "data-drop": dropOver === key && !!dragging && !denial(dragging, target),
      "data-invalid": !!dragging && !!denial(dragging, target),
      onDragOver: (event: DragEvent) => {
        if (event.dataTransfer.types.includes(dragMime)) {
          event.preventDefault();
          event.stopPropagation();
          event.dataTransfer.dropEffect = dragging && !denial(dragging, target) ? "move" : "none";
          setDragHint(
            dragging
              ? (denial(dragging, target) ??
                  `Release to preview ${dragging.kind === "gpo" ? "policy assignment" : "the move"}.`)
              : null,
          );
          setDropOver(key);
        }
      },
      onDragLeave: () => setDropOver(null),
      onDrop: (event: DragEvent) => {
        event.preventDefault();
        event.stopPropagation();
        try {
          const value = JSON.parse(event.dataTransfer.getData(dragMime)) as DragItem;
          if (["employee", "position", "gpo"].includes(value.kind) && typeof value.id === "string")
            drop(value, target);
        } catch {
          setNotice("This item cannot be moved here.");
        }
        setDragging(null);
        setDropOver(null);
        setDragHint(null);
      },
    };
  }
  function row(
    target: Selection,
    name: string,
    count?: number,
    children?: ReactNode,
    dragItem?: DragItem,
  ) {
    const RowIcon = icons[target.kind],
      key = target.kind + ":" + target.id,
      expanded = !collapsed.includes(key),
      active = same(selected, target);
    const canDrag =
      dragItem?.kind === "employee"
        ? organization.canMovePeople
        : dragItem?.kind === "position"
          ? organization.canManage
          : gpoRules.canManage;
    return (
      <li
        className={styles.treeItem}
        key={target.connectionId ? `${key}:${target.connectionId}` : key}
      >
        <div
          className={styles.treeRow}
          data-testid={`org-${target.kind === "employee" ? "person" : target.kind}-${target.id}`}
          data-selected={active}
          {...dropProps(target)}
          {...(dragItem ? dragProps(dragItem, canDrag) : {})}
        >
          {children ? (
            <Button
              variant="ghost"
              type="button"
              className={`${styles.iconButton} ${styles.chevron}`}
              aria-label={`${expanded ? "Collapse" : "Expand"} ${name}`}
              aria-expanded={expanded}
              onClick={() => toggle(key)}
            >
              {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            </Button>
          ) : (
            <span className={styles.treeSpace} />
          )}
          <Button
            variant="plain"
            type="button"
            className={styles.treeButton}
            aria-current={active ? "true" : undefined}
            onClick={() => choose(target)}
          >
            <RowIcon size={15} />
            <span className={styles.treeLabel}>{name}</span>
            {count !== undefined && <span className={styles.treeCount}>{count}</span>}
          </Button>
        </div>
        {children && expanded && (
          <ul className={`${styles.treeList} ${styles.nested}`}>{children}</ul>
        )}
      </li>
    );
  }
  const personRow = (p: OrganizationPerson) =>
    row({ kind: "employee", id: p.engagement_id }, p.legal_name, undefined, undefined, {
      kind: "employee",
      id: p.engagement_id,
    });
  function positionRow(p: OrganizationPosition) {
    const holders = people.filter((person) => person.position_id === p.id);
    return row(
      { kind: "position", id: p.id },
      p.title,
      holders.length,
      holders.length ? holders.map(personRow) : undefined,
      { kind: "position", id: p.id },
    );
  }
  function departmentRow(id: string) {
    const department = nodes.find((n) => n.id === id)!;
    const list = positions.filter((p) => p.department_id === id);
    return row(
      { kind: "department", id },
      department.display_name,
      list.length,
      list.length ? list.map(positionRow) : undefined,
    );
  }
  const searchResults = (() => {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return [];
    const entries: { selection: Selection; name: string; count?: number; drag?: DragItem }[] = [
      ...nodes
        .filter((n) => n.kind === "branch" || n.kind === "department")
        .map((n) => ({
          selection: { kind: n.kind as "branch" | "department", id: n.id },
          name: n.display_name,
        })),
      ...positions.map((p) => ({
        selection: { kind: "position" as const, id: p.id },
        name: p.title,
        drag: { kind: "position" as const, id: p.id },
      })),
      ...people.map((p) => ({
        selection: { kind: "employee" as const, id: p.engagement_id },
        name: p.legal_name,
        drag: { kind: "employee" as const, id: p.engagement_id },
      })),
      ...organization.levels.map((l) => ({
        selection: { kind: "level" as const, id: l.id },
        name: `Level ${l.level_number} · ${l.name}`,
      })),
      ...automation.connections.flatMap((c) =>
        (c.inventory?.objects ?? []).map((o) => ({
          selection: { kind: "directory" as const, id: o.id, connectionId: c.id },
          name: o.name,
        })),
      ),
    ];
    return entries.filter((item) => item.name.toLocaleLowerCase().includes(query));
  })();
  const explorer = (
    <aside className={styles.explorer} aria-label="Organization explorer">
      <div className={styles.paneHead}>
        <div className={styles.paneTitle}>
          <h2>Explorer</h2>
          <span>Organization & identity</span>
        </div>
        <Field className={styles.search}>
          <Search size={15} />
          <span className={styles.srOnly}>Search organization</span>
          <TextInput
            type="search"
            placeholder="Find a team, position, or person…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </Field>
      </div>
      <div className={styles.scroll}>
        {search ? (
          <ul className={styles.treeList}>
            {searchResults.map((item) =>
              row(item.selection, item.name, item.count, undefined, item.drag),
            )}
            {!searchResults.length && (
              <li className={styles.empty}>No matching items. Try a name or position.</li>
            )}
          </ul>
        ) : (
          <>
            <ul className={styles.treeList}>
              {row(
                { kind: "company", id: "company" },
                companyName,
                people.length,
                <>
                  {nodes
                    .filter((n) => n.kind === "branch")
                    .map((branch) => {
                      const departments = nodes.filter(
                          (n) => n.kind === "department" && n.branch_id === branch.id,
                        ),
                        direct = positions.filter(
                          (p) => p.branch_id === branch.id && !p.department_id,
                        );
                      return row(
                        { kind: "branch", id: branch.id },
                        branch.display_name,
                        positions.filter((p) => p.branch_id === branch.id).length,
                        <>
                          {departments.map((dept) => departmentRow(dept.id))}
                          {direct.map(positionRow)}
                        </>,
                      );
                    })}
                  {nodes
                    .filter((n) => n.kind === "department" && !n.branch_id)
                    .map((dept) => departmentRow(dept.id))}
                  {positions.filter((p) => !p.branch_id && !p.department_id).map(positionRow)}
                  {unassigned.length > 0 &&
                    row(
                      { kind: "unassigned", id: "unassigned" },
                      "Unassigned employees",
                      unassigned.length,
                      unassigned.map(personRow),
                    )}
                </>,
              )}
            </ul>
            <div className={styles.groupHeading}>
              <Button
                variant="plain"
                type="button"
                aria-expanded={!collapsed.includes("levels")}
                onClick={() => toggle("levels")}
              >
                {collapsed.includes("levels") ? (
                  <ChevronRight size={13} />
                ) : (
                  <ChevronDown size={13} />
                )}
                Company levels
              </Button>
            </div>
            {!collapsed.includes("levels") && (
              <ul className={styles.treeList}>
                {organization.levels.map((l) =>
                  row(
                    { kind: "level", id: l.id },
                    `L${l.level_number} · ${l.name}`,
                    positions.filter((p) => p.level_id === l.id).length,
                  ),
                )}
              </ul>
            )}
            <div className={styles.groupHeading}>
              <Button
                variant="plain"
                type="button"
                aria-expanded={!collapsed.includes("directory")}
                onClick={() => toggle("directory")}
              >
                {collapsed.includes("directory") ? (
                  <ChevronRight size={13} />
                ) : (
                  <ChevronDown size={13} />
                )}
                Directory objects
              </Button>
            </div>
            {!collapsed.includes("directory") &&
              (automation.connections.length ? (
                automation.connections.map((c) => (
                  <div key={c.id}>
                    <div className={styles.groupHeading}>{c.name}</div>
                    <ul className={styles.treeList}>
                      {(c.inventory?.objects ?? []).map((o) =>
                        row({ kind: "directory", id: o.id, connectionId: c.id }, o.name),
                      )}
                    </ul>
                    {!c.inventory?.objects.length && (
                      <p className={styles.hint}>No inventory synced.</p>
                    )}
                  </div>
                ))
              ) : (
                <p className={styles.hint}>
                  Directory inventory is unavailable in this role or no connection is configured.
                </p>
              ))}
          </>
        )}
      </div>
      <p className={styles.hint} role="status">
        <GripVertical size={12} />{" "}
        {mobile
          ? "Select a person, position, or level to review available actions."
          : (dragHint ??
            "Drag people, positions, or GPOs to a target. Every change is reviewed before confirmation.")}
      </p>
    </aside>
  );

  const libraryPanel = (
    <aside className={styles.library} aria-label="GPO library">
      <div className={styles.paneHead}>
        <div className={styles.paneTitle}>
          <h2>GPO library</h2>
          <ShieldCheck size={16} />
        </div>
        <div className={styles.libraryFilters}>
          <Field>
            Directory connection
            <Select value={connectionId} onChange={(event) => setConnectionId(event.target.value)}>
              <option value="" disabled>
                Choose a connection
              </option>
              {automation.connections.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field className={styles.search}>
            <Search size={14} />
            <span className={styles.srOnly}>Search GPO library</span>
            <TextInput
              type="search"
              placeholder="Search policies…"
              value={policySearch}
              onChange={(event) => setPolicySearch(event.target.value)}
            />
          </Field>
        </div>
      </div>
      <div className={styles.libraryCount}>
        {library.length} policies · {gpoRules.canManage ? "Drag to assign" : "Read only"}
      </div>
      <div className={styles.scroll}>
        {library.map((gpo) => (
          <article
            key={gpo.id}
            className={styles.policyCard}
            data-testid={`gpo-library-${gpo.id}`}
            data-dragging={dragging?.id === gpo.id}
            {...dragProps({ kind: "gpo", id: gpo.id, connectionId }, gpoRules.canManage)}
          >
            <FileCog size={18} />
            <div>
              <strong>{gpo.name}</strong>
              <small>Group Policy Object</small>
              <Button
                variant="plain"
                type="button"
                disabled={!gpoRules.canManage}
                onClick={() =>
                  openEdit({
                    kind: "gpo",
                    target: selectionTarget ?? { kind: "position", id: "" },
                    gpoId: gpo.id,
                    connectionId,
                  })
                }
              >
                Assign policy <span aria-hidden="true">↗</span>
              </Button>
            </div>
            <GripVertical size={13} />
          </article>
        ))}
        {library.length === 0 && (
          <div className={styles.empty}>
            <FileCog size={25} />
            <strong>{policySearch ? "No matching policies" : "No GPOs available"}</strong>
            {connection
              ? "Sync this connection’s Directory inventory to see available policies."
              : "Choose a Directory connection to browse its policies."}
          </div>
        )}
      </div>
      <p className={styles.hint}>
        Assign existing policies directly to Positions. Each mapping needs a user filter group and
        computer OU.
      </p>
    </aside>
  );

  const detailPanel = (
    <section className={styles.detail} aria-label="Selected item details">
      <div className={styles.breadcrumb}>
        <Button
          variant="plain"
          type="button"
          onClick={() => choose({ kind: "company", id: "company" })}
        >
          {companyName}
        </Button>
        {position?.branch_id && (
          <>
            <ChevronRight size={12} />
            <Button
              variant="plain"
              type="button"
              onClick={() => choose({ kind: "branch", id: position.branch_id! })}
            >
              {nodeName(position.branch_id)}
            </Button>
          </>
        )}
        {position?.department_id && (
          <>
            <ChevronRight size={12} />
            <Button
              variant="plain"
              type="button"
              onClick={() => choose({ kind: "department", id: position.department_id! })}
            >
              {nodeName(position.department_id)}
            </Button>
          </>
        )}
        <ChevronRight size={12} />
        <span>{kindLabels[selected.kind]}</span>
      </div>
      <div className={styles.detailHero}>
        <div className={styles.heroIcon}>
          <Icon size={23} />
        </div>
        <div>
          <span className={styles.kicker}>{kindLabels[selected.kind]}</span>
          <h2>{selectionName}</h2>
          <p>
            {selected.kind === "level"
              ? "Policies apply to matching positions across the Company."
              : selected.kind === "department"
                ? "People, positions, and inherited access in one place."
                : selected.kind === "directory"
                  ? (directoryObject?.dn ??
                    "Manage synchronized identity objects and Directory operations.")
                  : position
                    ? `Level ${position.level_number} · ${nodeName(position.department_id)}`
                    : "Explore your organization and shape access around it."}
          </p>
        </div>
      </div>
      <div className={styles.actions}>
        {person && (
          <Button
            variant="primary"
            type="button"
            className={styles.button}
            disabled={!organization.canMovePeople}
            title={
              !organization.canMovePeople
                ? "Requires HR permission to transfer employees"
                : undefined
            }
            onClick={() =>
              openEdit({
                kind: "employee",
                engagementId: person.engagement_id,
                positionId: person.position_id ?? "",
              })
            }
          >
            <MoveRight size={14} />
            Transfer employee
          </Button>
        )}
        {selected.kind === "position" && position && (
          <>
            <Button
              variant="primary"
              type="button"
              className={styles.button}
              disabled={!organization.canManage}
              title={
                !organization.canManage
                  ? "Requires organization and HR management permissions"
                  : undefined
              }
              onClick={() =>
                openEdit({
                  kind: "position",
                  positionId: position.id,
                  targetKind: "department",
                  targetId: position.department_id ?? "",
                })
              }
            >
              <MoveRight size={14} />
              Move position
            </Button>
            <Button
              variant="primary"
              type="button"
              className={styles.button}
              disabled={!organization.canManage}
              onClick={() =>
                openEdit({
                  kind: "position",
                  positionId: position.id,
                  targetKind: "level",
                  targetId: position.level_id,
                })
              }
            >
              <Layers3 size={14} />
              Change level
            </Button>
          </>
        )}
        {selectionTarget && (
          <Button
            variant="primary"
            type="button"
            className={styles.button}
            disabled={!gpoRules.canManage}
            title={!gpoRules.canManage ? "Requires Company IT administrator permission" : undefined}
            onClick={() => openEdit({ kind: "gpo", target: selectionTarget, connectionId })}
          >
            <Plus size={14} />
            Assign GPO
          </Button>
        )}
        {selected.kind === "directory" && automation.connections.length > 0 && (
          <Button
            variant="primary"
            type="button"
            className={styles.button}
            aria-label="Open Directory operations"
            onClick={() => setDrawer("directory")}
          >
            <Network size={14} />
            Manage Directory object
          </Button>
        )}
      </div>
      {selected.kind !== "directory" && (
        <div className={styles.metrics}>
          <div className={styles.metric}>
            <strong>{scopePeople.length}</strong>
            <span>{selected.kind === "position" ? "Current holders" : "Employees"}</span>
          </div>
          <div className={styles.metric}>
            <strong>{scopePositions.length}</strong>
            <span>Positions</span>
          </div>
          <div className={styles.metric}>
            <strong>{groupedPolicies.length}</strong>
            <span>{position ? "Effective GPOs" : "GPO policies"}</span>
          </div>
        </div>
      )}
      {selected.kind === "directory" ? (
        <Section title="Directory details" icon={<Network size={15} />}>
          <dl className={styles.facts}>
            <div>
              <dt>Connection</dt>
              <dd>{directoryConnection?.name ?? "Choose an object in Explorer"}</dd>
            </div>
            <div>
              <dt>Object type</dt>
              <dd>{directoryObject?.kind ?? "—"}</dd>
            </div>
            {directoryObject && (
              <>
                <div>
                  <dt>Distinguished name</dt>
                  <dd>{directoryObject.dn}</dd>
                </div>
                <div>
                  <dt>Parent</dt>
                  <dd>
                    {directoryObject.parentId
                      ? objectName(directoryObject.parentId, selected.connectionId!)
                      : "Root"}
                  </dd>
                </div>
              </>
            )}
          </dl>
        </Section>
      ) : (
        <>
          {(person || position) && (
            <Section title="Placement" icon={<BriefcaseBusiness size={15} />}>
              <dl className={styles.facts}>
                <div>
                  <dt>Department</dt>
                  <dd>{nodeName(person?.department_id ?? position?.department_id ?? null)}</dd>
                </div>
                <div>
                  <dt>Branch</dt>
                  <dd>{nodeName(person?.branch_id ?? position?.branch_id ?? null)}</dd>
                </div>
                <div>
                  <dt>{person ? "Actual level" : "Position level"}</dt>
                  <dd>
                    Level {person?.level_number ?? position?.level_number} ·{" "}
                    {organization.levels.find(
                      (l) => l.id === (person?.level_id ?? position?.level_id),
                    )?.name ?? "Unassigned"}
                  </dd>
                </div>
                <div>
                  <dt>{person ? "Reporting manager" : "Reports to position"}</dt>
                  <dd>
                    {person
                      ? personName(person.manager_person_id)
                      : positionName(position?.reports_to_position_id ?? null)}
                  </dd>
                </div>
              </dl>
              {person?.issues.length ? (
                <p className={styles.error}>{person.issues.join(" ")}</p>
              ) : null}
            </Section>
          )}
          <Section
            title="Position GPO tasks"
            icon={<ShieldCheck size={15} />}
            count={
              gpoRules.tasks.filter((task) => scopePositions.some((p) => p.id === task.positionId))
                .length
            }
          >
            <div className={styles.policyList}>
              {gpoRules.tasks
                .filter((task) => scopePositions.some((p) => p.id === task.positionId))
                .map((task) => (
                  <article className={styles.recordRow} key={task.positionId}>
                    <div>
                      <strong>
                        {task.positionName} · {task.status.replaceAll("_", " ")}
                      </strong>
                      <small>
                        {task.verifiedCount}/{task.mappingCount} GPO configurations verified in
                        Directory
                      </small>
                      {task.reason && <p className={styles.muted}>{task.reason}</p>}
                      <div className={styles.actions}>
                        <Link
                          className={styles.source}
                          href={`${base}/technology?view=organization&position=${encodeURIComponent(task.positionId)}`}
                        >
                          Open Position GPO editor
                        </Link>
                        {gpoRules.canManage &&
                          task.status !== "not_required" &&
                          task.mappingCount === 0 && (
                            <Button
                              variant="plain"
                              type="button"
                              className={styles.source}
                              onClick={() => {
                                setClosingTask(task);
                                setClosureReason("");
                              }}
                            >
                              GPO not required
                            </Button>
                          )}
                      </div>
                    </div>
                  </article>
                ))}
            </div>
          </Section>
          <Section
            title={
              person ? "Effective policies" : position ? "Effective policies" : "Policy inheritance"
            }
            icon={<ShieldCheck size={15} />}
            count={groupedPolicies.length}
          >
            <div className={styles.policyList}>
              {groupedPolicies.map((rules) => {
                const first = rules[0];
                const jobs = scopeJobs.filter(
                  (job) =>
                    job.desired_action === "add" &&
                    job.status !== "superseded" &&
                    scopePeople.some(
                      (holder) =>
                        holder.engagement_id === job.engagement_id &&
                        holder.position_id === job.position_id,
                    ) &&
                    rules.some(
                      (rule) =>
                        rule.revision === job.revision &&
                        rule.connectionId === job.connection_id &&
                        (job.rule_id
                          ? rule.id === job.rule_id
                          : rule.legacyPositionId === job.position_id),
                    ),
                );
                const verified = rules.every(
                  (rule) => rule.directoryStatus === "directory_applied",
                );
                const blocked = rules.some((rule) => rule.directoryStatus === "blocked");
                const endpoints =
                  jobs.length > 0 &&
                  jobs.every(
                    (job) => job.status === "directory_applied" && job.endpoint_observed_at,
                  );
                return (
                  <article
                    className={styles.effectivePolicy}
                    key={first.connectionId + first.gpoId}
                  >
                    <div className={styles.policyTop}>
                      <strong>{objectName(first.gpoId, first.connectionId)}</strong>
                      <span
                        className={styles.badge}
                        data-tone={blocked ? "warning" : verified ? "good" : undefined}
                      >
                        {blocked ? "Needs attention" : verified ? "Config verified" : "Pending"}
                      </span>
                    </div>
                    <div className={styles.sources}>
                      {rules.map((rule) => (
                        <Button
                          variant="plain"
                          type="button"
                          key={rule.id}
                          className={styles.source}
                          title="View policy source"
                          onClick={() => choose(rule.target)}
                        >
                          <Link2 size={10} />
                          {rule.target.kind === "position"
                            ? "Position"
                            : rule.target.kind === "level"
                              ? "Level"
                              : "Department"}{" "}
                          · {rule.targetName}
                        </Button>
                      ))}
                    </div>
                    <div className={styles.policyStatus}>
                      <span>
                        {verified ? <CheckCircle2 size={11} /> : <Clock3 size={11} />}Directory
                        config {verified ? "verified" : "pending"}
                      </span>
                      {jobs.length > 0 && (
                        <span>
                          <Users size={11} />
                          {
                            jobs.filter(
                              (job) =>
                                job.status === "directory_applied" && job.directory_observed_at,
                            ).length
                          }
                          /{jobs.length} recent deliveries verified
                        </span>
                      )}
                      <span>
                        <MonitorIcon />
                        Endpoint {endpoints ? "confirmed" : "unconfirmed"}
                      </span>
                    </div>
                    {rules.map((rule) =>
                      rule.directoryError ? (
                        <p key={rule.id} className={styles.muted}>
                          {rule.directoryError}
                        </p>
                      ) : null,
                    )}
                    {gpoRules.canManage && (
                      <div className={styles.sources}>
                        {rules
                          .filter(
                            (rule) =>
                              rule.target.kind === selected.kind && rule.target.id === selected.id,
                          )
                          .map((rule) => (
                            <span key={rule.id} className={styles.actions}>
                              <Button
                                variant="plain"
                                type="button"
                                className={styles.source}
                                onClick={() => openEdit({ kind: "gpo", target: rule.target, rule })}
                              >
                                Edit direct rule
                              </Button>
                              <Button
                                variant="plain"
                                type="button"
                                className={styles.source}
                                onClick={() => {
                                  setConnectionId(rule.connectionId);
                                  choose({
                                    kind: "directory",
                                    id: rule.gpoId,
                                    connectionId: rule.connectionId,
                                  });
                                  setDrawer("directory");
                                }}
                              >
                                Edit GPO settings
                              </Button>
                              <Button
                                variant="plain"
                                type="button"
                                className={styles.source}
                                onClick={() =>
                                  openEdit({ kind: "gpo", target: rule.target, rule, remove: true })
                                }
                              >
                                Remove direct rule
                              </Button>
                              {rule.directoryStatus === "blocked" && (
                                <Button
                                  variant="plain"
                                  type="button"
                                  className={styles.source}
                                  onClick={() =>
                                    reviewRetry({
                                      kind: "rule",
                                      id: rule.id,
                                      version: rule.version,
                                      name: objectName(rule.gpoId, rule.connectionId),
                                    })
                                  }
                                >
                                  Retry configuration
                                </Button>
                              )}
                            </span>
                          ))}
                      </div>
                    )}
                  </article>
                );
              })}
              {!groupedPolicies.length && (
                <div className={styles.empty}>
                  <ShieldCheck size={23} />
                  <strong>No GPO rules here yet</strong>Assign an existing policy to a Position.
                  Employee delivery is tracked separately.
                </div>
              )}
            </div>
            {position && (
              <p className={styles.muted}>
                Each mapped policy is assigned directly to this Position.
              </p>
            )}
          </Section>
          <Section
            title={person ? "Access preparation" : "People & positions"}
            icon={<Users size={15} />}
            count={person ? undefined : scopePeople.length}
          >
            {person ? (
              <>
                <p className={styles.muted}>
                  HR placement and Directory delivery are tracked separately. A linked Directory
                  account is not required to schedule HR changes.
                </p>
                <div className={styles.actions}>
                  <Button
                    variant="primary"
                    type="button"
                    className={styles.button}
                    disabled={!automation.connections.length && !lifecycle.canPrepare}
                    onClick={() => setDrawer("provisioning")}
                  >
                    Review employee resources <ArrowRight size={13} />
                  </Button>
                  <UiActionLink
                    variant="primary"
                    className={styles.button}
                    href={`${base}/people/${encodeURIComponent(person.person_id)}`}
                  >
                    Employee profile
                  </UiActionLink>
                </div>
              </>
            ) : (
              <>
                {scopePeople.map((p) => (
                  <div
                    className={styles.personRow}
                    key={p.engagement_id}
                    {...dragProps(
                      { kind: "employee", id: p.engagement_id },
                      organization.canMovePeople,
                    )}
                  >
                    <span className={styles.avatar}>
                      {p.legal_name
                        .split(" ")
                        .slice(0, 2)
                        .map((s) => s[0])
                        .join("")}
                    </span>
                    <Button
                      variant="plain"
                      type="button"
                      onClick={() => choose({ kind: "employee", id: p.engagement_id })}
                    >
                      <strong>{p.legal_name}</strong>
                      <small>
                        {positionName(p.position_id)} · L{p.level_number ?? "—"}
                      </small>
                    </Button>
                    <ChevronRight size={13} />
                  </div>
                ))}
                {scopePeople.length === 0 && (
                  <div className={styles.empty}>
                    <Users size={22} />
                    <strong>{position ? "Vacant position" : "No employees here"}</strong>
                    {position
                      ? "This position is ready for an employee assignment."
                      : "Employees will appear when positions are assigned."}
                  </div>
                )}
                {!position &&
                  scopePositions
                    .filter((p) => !people.some((person) => person.position_id === p.id))
                    .map((p) => (
                      <div className={styles.personRow} key={p.id}>
                        <BriefcaseBusiness size={17} />
                        <Button
                          variant="plain"
                          type="button"
                          onClick={() => choose({ kind: "position", id: p.id })}
                        >
                          <strong>{p.title}</strong>
                          <small>Vacant · Level {p.level_number}</small>
                        </Button>
                        <ChevronRight size={13} />
                      </div>
                    ))}
                {position && lifecycle.canPrepare && (
                  <div className={styles.actions}>
                    <Button
                      variant="primary"
                      type="button"
                      className={styles.button}
                      onClick={() => setDrawer("preparation")}
                    >
                      Prepare position resources <ArrowRight size={13} />
                    </Button>
                  </div>
                )}
              </>
            )}
          </Section>
          {(moves.length > 0 || scopeJobs.length > 0 || scopeCases.length > 0) && (
            <Section title="Changes & delivery" icon={<Clock3 size={15} />}>
              {moves
                .filter((move) => scopePositions.some((p) => p.id === move.position_id))
                .map((move) => (
                  <article className={styles.recordRow} key={move.id}>
                    <Clock3 size={16} />
                    <div style={{ flex: 1 }}>
                      <strong>
                        {move.definition.targetKind === "level"
                          ? "Level change"
                          : "Department move"}{" "}
                        · {positionName(move.position_id)}
                      </strong>
                      <small>
                        {move.status.replaceAll("_", " ")} · {time(move.effective_at)}
                      </small>
                      {move.last_error && <p className={styles.muted}>{move.last_error}</p>}
                    </div>
                    {move.status === "scheduled" && organization.canManage && (
                      <Button
                        variant="plain"
                        type="button"
                        className={styles.source}
                        onClick={() => setCancelMove(move)}
                      >
                        Cancel
                      </Button>
                    )}
                    {move.status === "needs_review" && organization.canManage && (
                      <Button
                        variant="plain"
                        type="button"
                        className={styles.source}
                        onClick={() =>
                          openEdit({
                            kind: "position",
                            positionId: move.position_id,
                            targetKind: move.definition.targetKind,
                            targetId: move.definition.targetId,
                          })
                        }
                      >
                        Review again
                      </Button>
                    )}
                  </article>
                ))}
              {scopeCases.map((record) => (
                <article className={styles.recordRow} key={record.id}>
                  <UserRound size={15} />
                  <div>
                    <strong>
                      {personName(record.person_id)} · {record.kind}
                    </strong>
                    <small>
                      {record.status.replaceAll("_", " ")} · {time(record.effective_at)}
                    </small>
                    {record.last_error && <p className={styles.muted}>{record.last_error}</p>}
                  </div>
                  <Link className={styles.source} href={`${base}/work?type=people_lifecycle`}>
                    Track change
                  </Link>
                </article>
              ))}
              {scopeJobs.map((job) => (
                <article className={styles.recordRow} key={job.id}>
                  <ShieldCheck size={15} />
                  <div>
                    <strong>
                      {personName(job.person_id)} · {job.status.replaceAll("_", " ")}
                    </strong>
                    <small>
                      Directory:{" "}
                      {job.directory_observed_at
                        ? job.status === "directory_applied"
                          ? "verified"
                          : job.status === "superseded"
                            ? "previously verified"
                            : "last verified; needs review"
                        : "pending"}{" "}
                      · Endpoint:{" "}
                      {job.endpoint_observed_at
                        ? job.status === "directory_applied"
                          ? "confirmed"
                          : job.status === "superseded"
                            ? "previously confirmed"
                            : "last confirmed; needs review"
                        : "unconfirmed"}
                    </small>
                    {job.last_error && <p className={styles.muted}>{job.last_error}</p>}
                  </div>
                  {gpoRules.canManage && ["blocked", "retry"].includes(job.status) && (
                    <Button
                      variant="plain"
                      type="button"
                      className={styles.source}
                      onClick={() =>
                        reviewRetry({
                          kind: "job",
                          id: job.id,
                          version: job.version,
                          name: personName(job.person_id),
                        })
                      }
                    >
                      Retry delivery
                    </Button>
                  )}
                </article>
              ))}
            </Section>
          )}
        </>
      )}
    </section>
  );
  const contextualRoster = lifecycle.roster.filter((p) =>
    person
      ? p.engagementId === person.engagement_id
      : scopePeople.some((person) => person.engagement_id === p.engagementId),
  );
  return (
    <div
      className={styles.workspace}
      role="region"
      aria-label="Organization & Access"
      data-testid="organization-access-workspace"
    >
      <header className={styles.toolbar}>
        <div className={styles.heading}>
          <Network size={21} />
          <div>
            <strong>Organization & Access</strong>
            <small>
              {positions.length} positions · {people.length} employees ·{" "}
              {organization.levels.length} levels
            </small>
          </div>
        </div>
      </header>
      {notice && (
        <p className={styles.notice} role="status">
          {notice}
        </p>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {legacyDirectoryView ? (
        <div className={styles.layout}>
          {explorer}
          {detailPanel}
        </div>
      ) : (
        <div className={styles.sharedSurface}>
          <OrganizationExplorer
            data={directory}
            initialCompanyId={companyId}
            initialSelection={sharedSelection(selected, companyId)}
            holdingName={companyName}
            tabsForSelection={() => ["Policies", "Directory", "Delivery"]}
            testIdPrefix="org"
            searchLabel="Search organization"
            clearQueryKeys={["employee", "object", "unassigned", "connection"]}
            headerActions={
              <div className={styles.actions}>
                <Button
                  variant="primary"
                  type="button"
                  className={styles.button}
                  aria-label="Open GPO library"
                  onClick={() => setDrawer("library")}
                >
                  <FileCog size={14} />
                  GPO library
                </Button>
                {automation.connections.length > 0 && (
                  <Button
                    variant="primary"
                    type="button"
                    className={styles.button}
                    aria-label="Open Directory operations"
                    onClick={() => setDrawer("directory")}
                  >
                    <Network size={14} />
                    Directory operations
                  </Button>
                )}
                {lifecycle.canPrepare && (
                  <Button
                    variant="primary"
                    type="button"
                    className={styles.button}
                    aria-label="Open Work and delivery"
                    onClick={() => setDrawer("operations")}
                  >
                    <Clock3 size={14} />
                    Work & delivery
                  </Button>
                )}
              </div>
            }
            renderDetail={(_selection, tab) =>
              tab === "Policies" ? (
                <div className={styles.sharedDetail}>{detailPanel}</div>
              ) : tab === "Directory" ? (
                <div className={styles.itSection}>
                  <p className={styles.muted}>
                    Directory objects and GPO delivery are managed by IT. HR placement remains
                    available independently.
                  </p>
                  <DirectoryWorkspace
                    base={base}
                    initial={automation}
                    initialConnectionId={connectionId || undefined}
                  />
                </div>
              ) : (
                <div className={styles.itSection}>
                  <div className={styles.metrics}>
                    <div className={styles.metric}>
                      <strong>{scopeCases.length}</strong>
                      <span>HR changes in scope</span>
                    </div>
                    <div className={styles.metric}>
                      <strong>{scopeJobs.length}</strong>
                      <span>GPO deliveries in scope</span>
                    </div>
                    <div className={styles.metric}>
                      <strong>
                        {
                          scopeJobs.filter(
                            (job) => job.status === "blocked" || job.status === "retry",
                          ).length
                        }
                      </strong>
                      <span>Need attention</span>
                    </div>
                  </div>
                  <div className={styles.actions}>
                    <Button
                      variant="primary"
                      type="button"
                      className={styles.button}
                      onClick={() => setDrawer("operations")}
                    >
                      Open work & delivery <ArrowRight size={14} />
                    </Button>
                    {person && (
                      <Button
                        variant="primary"
                        type="button"
                        className={styles.button}
                        onClick={() => setDrawer("provisioning")}
                      >
                        Employee resources <ArrowRight size={14} />
                      </Button>
                    )}
                  </div>
                  {scopeJobs.map((job) => (
                    <article className={styles.recordRow} key={job.id}>
                      <ShieldCheck size={15} />
                      <div>
                        <strong>
                          {personName(job.person_id)} · {job.status.replaceAll("_", " ")}
                        </strong>
                        <small>
                          Directory {job.directory_observed_at ? "verified" : "pending"} · Endpoint{" "}
                          {job.endpoint_observed_at ? "confirmed" : "unconfirmed"}
                        </small>
                        {job.last_error && <p className={styles.muted}>{job.last_error}</p>}
                      </div>
                      {gpoRules.canManage && ["blocked", "retry"].includes(job.status) && (
                        <Button
                          variant="plain"
                          type="button"
                          className={styles.source}
                          onClick={() =>
                            reviewRetry({
                              kind: "job",
                              id: job.id,
                              version: job.version,
                              name: personName(job.person_id),
                            })
                          }
                        >
                          Retry delivery
                        </Button>
                      )}
                    </article>
                  ))}
                  {scopeJobs.length === 0 && (
                    <p className={styles.muted}>No policy deliveries for this selection.</p>
                  )}
                </div>
              )
            }
          />
        </div>
      )}
      <div className={styles.srOnly} role="status" aria-live="polite">
        {dragging
          ? `Moving ${dragging.kind}. ${dragging.kind === "gpo" ? "Drop on a Level, Department, or Position." : dragging.kind === "position" ? "Drop on a Department or Level." : "Drop on a Position or Department."}`
          : ""}
      </div>
      <Dialog
        open={drawer !== null && (drawer !== "detail" || mobile)}
        onClose={() => setDrawer(null)}
        title={
          drawer === "library"
            ? "GPO library"
            : drawer === "directory"
              ? "Directory operations"
              : drawer === "preparation"
                ? "Position preparation"
                : drawer === "operations"
                  ? "Work & delivery"
                  : drawer === "provisioning"
                    ? "Employee resources"
                    : selectionName
        }
        presentation={
          drawer === "directory" ||
          drawer === "provisioning" ||
          drawer === "preparation" ||
          drawer === "operations"
            ? "fullscreen"
            : "people-drawer"
        }
      >
        <div className={styles.drawerContent}>
          {drawer === "detail" && detailPanel}
          {drawer === "library" && libraryPanel}
          {drawer === "directory" && (
            <DirectoryWorkspace
              key={selected.connectionId ?? "directory"}
              base={base}
              initial={automation}
              initialConnectionId={selected.connectionId}
              initialObjectId={selected.kind === "directory" ? selected.id : undefined}
            />
          )}
          {drawer === "preparation" && position && (
            <PositionConsole
              base={base}
              data={{
                ...lifecycle,
                positions: lifecycle.positions.filter((p) => p.id === position.id),
              }}
              positionId={position.id}
            />
          )}
          {(drawer === "provisioning" || drawer === "operations") && (
            <ProvisioningWorkspace
              key={drawer}
              base={base}
              lifecycle={
                drawer === "provisioning" ? { ...lifecycle, roster: contextualRoster } : lifecycle
              }
              data={technology}
              canApply={canApply}
              section={drawer === "operations" ? "operations" : "people"}
            />
          )}
        </div>
      </Dialog>
      {edit && (
        <OrganizationActionDialog
          key={JSON.stringify(edit)}
          edit={edit}
          organization={organization}
          automation={automation}
          root={root}
          onClose={() => setEdit(null)}
          onComplete={completed}
        />
      )}
      <Dialog
        open={closingTask !== null}
        onClose={() => {
          if (!busy) setClosingTask(null);
        }}
        title="GPO not required"
      >
        <div className={styles.form}>
          <p>
            Close the GPO task for <strong>{closingTask?.positionName}</strong>. A material Position
            change will reopen it.
          </p>
          <Field>
            Reason
            <TextArea
              minLength={3}
              maxLength={1000}
              value={closureReason}
              onChange={(event) => setClosureReason(event.target.value)}
            />
          </Field>
          {error && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
          <div className={styles.actions}>
            <Button
              variant="primary"
              type="button"
              className={styles.button}
              onClick={() => setClosingTask(null)}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              type="button"
              className={styles.primary}
              disabled={busy || closureReason.trim().length < 3}
              onClick={async () => {
                if (!closingTask) return;
                setBusy(true);
                setError("");
                try {
                  await command(root + "/technology/gpo-rules/not-required", {
                    positionId: closingTask.positionId,
                    expectedVersion: closingTask.version,
                    reason: closureReason,
                  });
                  setClosingTask(null);
                  setNotice("Position GPO task closed as not required.");
                  router.refresh();
                } catch (e) {
                  setError(e instanceof Error ? e.message : "Unable to close task.");
                } finally {
                  setBusy(false);
                }
              }}
            >
              Confirm not required
            </Button>
          </div>
        </div>
      </Dialog>
      <Dialog
        open={retryTarget !== null}
        onClose={() => {
          if (!busy) setRetryTarget(null);
        }}
        title={
          retryTarget?.kind === "rule" ? "Retry Directory configuration" : "Retry employee delivery"
        }
      >
        <div className={styles.form}>
          <p>
            Retry <strong>{retryTarget?.name}</strong>. Directory state will be verified again;
            endpoint confirmation remains separate.
          </p>
          {retryReviewed ? (
            <div className={styles.preview}>
              <strong>Review retry</strong>
              <p>{retryReason}</p>
              <p>The existing policy targets will be used. No HR placement changes are included.</p>
            </div>
          ) : (
            <Field>
              Reason
              <TextArea
                minLength={3}
                maxLength={1000}
                value={retryReason}
                onChange={(event) => setRetryReason(event.target.value)}
              />
            </Field>
          )}
          {error && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
          <div className={styles.actions}>
            {retryReviewed && (
              <Button
                variant="primary"
                type="button"
                className={styles.button}
                disabled={busy}
                onClick={() => setRetryReviewed(false)}
              >
                Back
              </Button>
            )}
            <Button
              variant="primary"
              type="button"
              className={styles.primary}
              disabled={busy || retryReason.trim().length < 3}
              onClick={async () => {
                if (!retryReviewed) {
                  setRetryReviewed(true);
                  return;
                }
                if (!retryTarget) return;
                setBusy(true);
                setError("");
                try {
                  await command(
                    root +
                      (retryTarget.kind === "rule"
                        ? "/technology/gpo-rules/retry-config"
                        : "/technology/position-gpo/retry"),
                    {
                      id: retryTarget.id,
                      expectedVersion: retryTarget.version,
                      reason: retryReason,
                    },
                  );
                  setRetryTarget(null);
                  setNotice(
                    "Directory retry queued. Verification status will update as delivery completes.",
                  );
                  router.refresh();
                } catch (e) {
                  setError(e instanceof Error ? e.message : "Unable to queue retry.");
                } finally {
                  setBusy(false);
                }
              }}
            >
              {retryReviewed ? "Confirm retry" : "Preview retry"}
            </Button>
          </div>
        </div>
      </Dialog>
      <Dialog
        open={cancelMove !== null}
        onClose={() => setCancelMove(null)}
        title="Cancel scheduled position change"
      >
        <div className={styles.form}>
          <p>
            Cancel the change to <strong>{positionName(cancelMove?.position_id ?? null)}</strong>{" "}
            scheduled for {cancelMove ? time(cancelMove.effective_at) : ""}. The current placement
            will stay in effect.
          </p>
          <div className={styles.actions}>
            <Button
              variant="primary"
              type="button"
              className={styles.button}
              onClick={() => setCancelMove(null)}
            >
              Keep change
            </Button>
            <Button
              variant="primary"
              type="button"
              className={styles.primary}
              disabled={busy}
              onClick={async () => {
                if (!cancelMove) return;
                setBusy(true);
                try {
                  await command(
                    root +
                      "/people/organization/position-moves/" +
                      encodeURIComponent(cancelMove.id) +
                      "/cancel",
                    { expectedVersion: cancelMove.version },
                  );
                  setMoves((values) =>
                    values.map((move) =>
                      move.id === cancelMove.id ? { ...move, status: "cancelled" } : move,
                    ),
                  );
                  setCancelMove(null);
                  setNotice("Scheduled position change cancelled.");
                  router.refresh();
                } catch (e) {
                  setError(e instanceof Error ? e.message : "Unable to cancel the change.");
                  setCancelMove(null);
                } finally {
                  setBusy(false);
                }
              }}
            >
              Confirm cancellation
            </Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
function MonitorIcon() {
  return (
    <span
      aria-hidden="true"
      style={{
        display: "inline-block",
        width: 10,
        height: 8,
        border: "1px solid currentColor",
        borderRadius: 2,
      }}
    />
  );
}

async function command<T>(
  path: string,
  body: unknown,
  idempotencyKey = crypto.randomUUID(),
): Promise<T> {
  const response = await workFetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": idempotencyKey },
    body: JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok)
    throw new Error(
      result.error?.message ?? result.message ?? "The change could not be completed.",
    );
  return result.data as T;
}

export function OrganizationActionDialog({
  edit,
  organization,
  automation,
  root,
  onClose,
  onComplete,
}: {
  edit: Edit;
  organization: OrganizationAccessData;
  automation: DirectoryAutomationData;
  root: string;
  onClose: () => void;
  onComplete: (message: string) => void;
}) {
  const [targetKind, setTargetKind] = useState<GpoRuleTarget["kind"]>(
    edit.kind === "gpo"
      ? edit.target.kind
      : edit.kind === "position"
        ? edit.targetKind
        : "position",
  );
  const [targetId, setTargetId] = useState(
    edit.kind === "gpo"
      ? edit.target.id
      : edit.kind === "position"
        ? edit.targetId
        : edit.positionId,
  );
  const [connectionId, setConnectionId] = useState(
    edit.kind === "gpo"
      ? (edit.rule?.connectionId ?? edit.connectionId ?? automation.connections[0]?.id ?? "")
      : "",
  );
  const [gpoId, setGpoId] = useState(
    edit.kind === "gpo" ? (edit.rule?.gpoId ?? edit.gpoId ?? "") : "",
  );
  const [timezone, setTimezone] = useState("Asia/Bangkok");
  const [effectiveLocal, setEffectiveLocal] = useState(() =>
    retirementLocalTime(Date.now() + 3600000, "Asia/Bangkok"),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<
    | {
        kind: "gpo";
        data: GpoRulePreview;
        input: GpoRuleInput | { id: string; expectedVersion: number; reason: string };
      }
    | { kind: "position"; data: PositionMovePreview; input: PositionMoveInput }
    | { kind: "employee"; data: LifecyclePreview; input: LifecycleInput }
    | null
  >(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const [draftFields, setDraftFields] = useState<Record<string, string>>({});
  const receipt = useRef<{ fingerprint: string; key: string } | null>(null);
  const errorRef = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (error) {
      errorRef.current?.focus();
      errorRef.current?.scrollIntoView({ block: "start", behavior: "instant" });
    }
  }, [error]);
  const position = organization.positions.find(
    (p) => p.id === (edit.kind === "position" ? edit.positionId : ""),
  );
  const employee = organization.people.find(
    (p) => p.engagement_id === (edit.kind === "employee" ? edit.engagementId : ""),
  );
  const holders = organization.people.filter(
    (p) => p.position_id === position?.id && p.status !== "ended",
  );
  const objects =
    automation.connections.find((c) => c.id === connectionId)?.inventory?.objects ?? [];
  const removing = edit.kind === "gpo" && edit.remove;
  const targetOptions =
    targetKind === "level"
      ? organization.levels.map((l) => ({ id: l.id, name: `Level ${l.level_number} · ${l.name}` }))
      : targetKind === "department"
        ? organization.nodes
            .filter((n) => n.kind === "department" && n.status === "active")
            .map((n) => ({ id: n.id, name: n.display_name }))
        : organization.positions
            .filter(
              (p) =>
                p.active &&
                (edit.kind !== "employee" ||
                  !edit.departmentId ||
                  p.department_id === edit.departmentId),
            )
            .map((p) => ({
              id: p.id,
              name: [
                p.title,
                organization.nodes.find((n) => n.id === p.department_id)?.display_name,
                organization.nodes.find((n) => n.id === p.branch_id)?.display_name,
              ]
                .filter(Boolean)
                .join(" · "),
            }));
  const levelName = (id: string | null) =>
    organization.levels.find((l) => l.id === id)?.name ?? "Unassigned";
  const departmentName = (id: string | null) =>
    organization.nodes.find((n) => n.id === id)?.display_name ?? "Company";
  const managerName = (id: string | null) =>
    organization.people.find((p) => p.person_id === id)?.legal_name ?? "Unassigned";
  const policyObjectName = (id: string | undefined, connection: string | undefined) =>
    !id
      ? "Not assigned"
      : (automation.connections
          .find((c) => c.id === connection)
          ?.inventory?.objects.find((o) => o.id === id)?.name ?? id);
  const title =
    edit.kind === "gpo"
      ? removing
        ? "Remove GPO rule"
        : edit.rule
          ? "Edit GPO rule"
          : "Assign GPO policy"
      : edit.kind === "position"
        ? targetKind === "level"
          ? "Change position level"
          : "Move position & employees"
        : "Transfer employee";
  const targetName = targetOptions.find((t) => t.id === targetId)?.name ?? "Choose a target";
  async function send<T>(path: string, body: unknown): Promise<T> {
    const fingerprint = path + JSON.stringify(body);
    if (receipt.current?.fingerprint !== fingerprint)
      receipt.current = { fingerprint, key: crypto.randomUUID() };
    const value = await command<T>(root + path, body, receipt.current.key);
    receipt.current = null;
    return value;
  }
  async function submitPreview(form: HTMLFormElement) {
    setBusy(true);
    setError("");
    const fields = new FormData(form),
      reason = String(fields.get("reason") ?? "");
    setDraftFields(
      Object.fromEntries(Array.from(fields.entries()).map(([key, value]) => [key, String(value)])),
    );
    try {
      if (edit.kind === "gpo") {
        const input =
          removing && edit.rule
            ? { id: edit.rule.id, expectedVersion: edit.rule.version, reason }
            : {
                id: edit.rule?.id,
                target: { kind: "position" as const, id: targetId },
                expectedVersion: edit.rule?.version ?? 0,
                connectionId,
                gpoId,
                userFilterGroupId: String(fields.get("group") ?? ""),
                computerTargetOuId: String(fields.get("ou") ?? ""),
                reason,
              };
        const data = await send<GpoRulePreview>(
          "/technology/gpo-rules/" + (removing ? "remove-preview" : "preview"),
          input,
        );
        setPreview({ kind: "gpo", data, input });
      } else if (edit.kind === "position" && position) {
        const input: PositionMoveInput = {
          positionId: position.id,
          expectedVersion: position.version,
          targetKind: targetKind as "department" | "level",
          targetId,
          effectiveAt: retirementInstant(effectiveLocal, timezone),
          timezone,
          reportsToPositionId: String(fields.get("reportsTo") ?? "") || null,
          managerAssignments: holders.map((p) => ({
            engagementId: p.engagement_id,
            managerPersonId: String(fields.get("manager:" + p.engagement_id) ?? "") || null,
          })),
          reason,
        };
        const data = await send<PositionMovePreview>(
          "/people/organization/position-moves/preview",
          input,
        );
        setPreview({ kind: "position", data, input });
      } else if (edit.kind === "employee" && employee) {
        const input: LifecycleInput = {
          expectedVersion: 0,
          kind: employee.status === "onboarding" ? "onboarding" : "transfer",
          personId: employee.person_id,
          engagementId: employee.engagement_id,
          positionId: targetId,
          managerPersonId: String(fields.get("manager") ?? "") || null,
          acceptRecommendedPay: false,
          acceptRecommendedLeave: fields.get("leave") === "on",
          effectiveAt: retirementInstant(effectiveLocal, timezone),
          timezone,
          reason,
          departureReason: "other",
          clearanceOwners: { equipment: null, payroll: null, finance: null },
          lastWorkingDate: null,
        };
        const data = await send<LifecyclePreview>("/people/lifecycle/placements/preview", input);
        setPreview({ kind: "employee", data, input });
        setAcknowledged(false);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to preview this change.");
    } finally {
      setBusy(false);
    }
  }
  async function confirm() {
    if (!preview) return;
    setBusy(true);
    setError("");
    try {
      if (preview.kind === "gpo") {
        await send("/technology/gpo-rules/" + (removing ? "remove" : "publish"), {
          ...preview.input,
          previewToken: preview.data.token,
        });
        onComplete(
          removing
            ? "GPO rule removed. Policies inherited from other sources remain in effect; Directory changes are queued."
            : "GPO mapping published. Directory configuration and employee delivery are tracked separately.",
        );
      } else if (preview.kind === "position") {
        await send("/people/organization/position-moves/submit", {
          ...preview.input,
          previewToken: preview.data.previewToken,
          expiresAt: preview.data.expiresAt,
        });
        onComplete(
          `Position change scheduled for ${time(preview.input.effectiveAt, timezone)}. All ${preview.data.affectedCount} employees will move together.`,
        );
      } else {
        await send("/people/lifecycle/placements/submit", {
          ...preview.input,
          previewToken: preview.data.token,
          ...(preview.data.legacyCaseToSupersede
            ? {
                supersedeLegacyCase: {
                  id: preview.data.legacyCaseToSupersede.id,
                  expectedVersion: preview.data.legacyCaseToSupersede.version,
                },
              }
            : {}),
        });
        onComplete(
          `Employee transfer scheduled for ${time(preview.input.effectiveAt, timezone)}. Directory preparation follows HR placement.`,
        );
      }
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Unable to confirm this change. Refresh the preview and try again.",
      );
    } finally {
      setBusy(false);
    }
  }
  const ready = preview?.kind === "gpo" || preview?.data.ready;
  return (
    <Dialog
      open
      onClose={() => {
        if (!busy) onClose();
      }}
      title={title}
      presentation="people-drawer"
    >
      <div className={`${styles.form} ${styles.actionDrawer}`}>
        {error && (
          <p ref={errorRef} tabIndex={-1} className={styles.error} role="alert">
            {error}
          </p>
        )}
        <div className={styles.breadcrumb}>
          <span className={styles.badge} data-tone={!preview ? "good" : undefined}>
            1 · Configure
          </span>
          <ChevronRight size={12} />
          <span className={styles.badge} data-tone={preview ? "good" : undefined}>
            2 · Preview & confirm
          </span>
        </div>
        {!preview ? (
          <form
            className={styles.form}
            onSubmit={(event) => {
              event.preventDefault();
              void submitPreview(event.currentTarget);
            }}
          >
            <p className={styles.muted}>
              {edit.kind === "position"
                ? `${position?.title ?? "Position"} · ${holders.length} non-ended placements, including scheduled starts. This change applies to the position and every holder together.`
                : edit.kind === "employee"
                  ? `${employee?.legal_name ?? "Employee"} · Choose the destination, actual manager, and effective date.`
                  : removing
                    ? "Remove this source only. The preview shows access retained through other rules."
                    : "Choose a Position and Directory targets. Multiple GPOs may be mapped to one Position."}
            </p>
            {edit.kind === "gpo" && !removing && (
              <Field>
                Apply to
                <Select
                  value={targetKind}
                  disabled={!!edit.rule}
                  onChange={(event) => {
                    setTargetKind(event.target.value as GpoRuleTarget["kind"]);
                    setTargetId("");
                  }}
                >
                  <option value="position">Position</option>
                </Select>
              </Field>
            )}
            {!removing && (
              <Field>
                {targetKind === "level"
                  ? "Target level"
                  : targetKind === "department"
                    ? "Target department"
                    : "Target position"}
                <Select
                  data-testid="organization-target-select"
                  required
                  value={targetId}
                  disabled={edit.kind === "gpo" && !!edit.rule}
                  onChange={(event) => setTargetId(event.target.value)}
                >
                  <option value="">Choose {targetKind}</option>
                  {targetOptions.map((option) => (
                    <option key={option.id} value={option.id}>
                      {option.name}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            {edit.kind === "employee" && edit.departmentId && (
              <p className={styles.muted}>
                Positions in {departmentName(edit.departmentId)}. Select the destination position
                before continuing.
              </p>
            )}
            {edit.kind === "gpo" && !removing && (
              <>
                <Field>
                  Directory connection
                  <Select
                    required
                    value={connectionId}
                    onChange={(event) => {
                      setConnectionId(event.target.value);
                      setGpoId("");
                    }}
                  >
                    <option value="">Choose connection</option>
                    {automation.connections.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field>
                  Group Policy Object
                  <Select required value={gpoId} onChange={(event) => setGpoId(event.target.value)}>
                    <option value="">Choose GPO</option>
                    {objects
                      .filter((o) => o.kind === "gpo")
                      .map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.name}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field>
                  User security filter group
                  <Select
                    key={connectionId + "group"}
                    required
                    name="group"
                    defaultValue={draftFields.group ?? edit.rule?.userFilterGroupId ?? ""}
                  >
                    <option value="">Choose group</option>
                    {objects
                      .filter((o) => o.kind === "group")
                      .map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.name}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field>
                  Computer target OU
                  <Select
                    key={connectionId + "ou"}
                    required
                    name="ou"
                    defaultValue={draftFields.ou ?? edit.rule?.computerTargetOuId ?? ""}
                  >
                    <option value="">Choose organizational unit</option>
                    {objects
                      .filter((o) => o.kind === "ou")
                      .map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.name}
                        </option>
                      ))}
                  </Select>
                </Field>
              </>
            )}
            {edit.kind === "position" && position && (
              <>
                <Field>
                  Reports to position
                  <Select
                    name="reportsTo"
                    defaultValue={draftFields.reportsTo ?? position.reports_to_position_id ?? ""}
                  >
                    <option value="">No parent position</option>
                    {organization.positions
                      .filter((p) => p.active && p.id !== position.id)
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title} · L{p.level_number}
                        </option>
                      ))}
                  </Select>
                </Field>
                {holders.length > 0 && (
                  <details open className={styles.section}>
                    <summary>
                      <span>Actual reporting managers · {holders.length}</span>
                      <ChevronRight size={14} />
                    </summary>
                    <div className={styles.form}>
                      {holders.map((p) => (
                        <Field key={p.engagement_id}>
                          {p.legal_name}
                          <Select
                            name={"manager:" + p.engagement_id}
                            defaultValue={
                              draftFields["manager:" + p.engagement_id] ?? p.manager_person_id ?? ""
                            }
                          >
                            <option value="">No manager</option>
                            {organization.people
                              .filter((m) => m.status === "active" && m.person_id !== p.person_id)
                              .map((m) => (
                                <option key={m.engagement_id} value={m.person_id}>
                                  {m.legal_name} · L{m.level_number ?? "—"}
                                </option>
                              ))}
                          </Select>
                        </Field>
                      ))}
                    </div>
                  </details>
                )}
              </>
            )}
            {edit.kind === "employee" && employee && (
              <>
                <Field>
                  Actual reporting manager
                  <Select
                    name="manager"
                    defaultValue={draftFields.manager ?? employee.manager_person_id ?? ""}
                  >
                    <option value="">No manager</option>
                    {organization.people
                      .filter((p) => p.status === "active" && p.person_id !== employee.person_id)
                      .map((p) => (
                        <option key={p.engagement_id} value={p.person_id}>
                          {p.legal_name} · L{p.level_number ?? "—"}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field orientation="inline">
                  <TextInput
                    type="checkbox"
                    name="leave"
                    defaultChecked={draftFields.reason ? draftFields.leave === "on" : true}
                  />
                  Assign the destination Level’s leave default at the effective date.
                </Field>
              </>
            )}
            {edit.kind !== "gpo" && (
              <div className={styles.formGrid}>
                <Field>
                  Timezone
                  <Select value={timezone} onChange={(event) => setTimezone(event.target.value)}>
                    <option>Asia/Bangkok</option>
                    <option>UTC</option>
                    <option>Asia/Singapore</option>
                  </Select>
                </Field>
                <Field>
                  Effective date & time
                  <TextInput
                    type="datetime-local"
                    required
                    value={effectiveLocal}
                    onChange={(event) => setEffectiveLocal(event.target.value)}
                  />
                </Field>
              </div>
            )}
            <Field>
              Reason
              <TextArea
                name="reason"
                defaultValue={draftFields.reason ?? ""}
                required
                minLength={3}
                maxLength={1000}
                placeholder="Explain this change for the review history"
              />
            </Field>
            <div className={styles.actions}>
              <Button variant="primary" type="button" className={styles.button} onClick={onClose}>
                Cancel
              </Button>
              <Button clientManaged variant="primary" type="submit" className={styles.primary} disabled={busy}>
                {busy ? "Preparing preview…" : "Preview change"}
                <ArrowRight size={14} />
              </Button>
            </div>
          </form>
        ) : (
          <>
            <div className={styles.preview}>
              <h3>
                {preview.kind === "gpo"
                  ? removing
                    ? "Review rule removal"
                    : "Review policy assignment"
                  : "Review scheduled change"}
              </h3>
              {preview.kind === "gpo" ? (
                <>
                  <p>
                    <strong>{preview.data.targetName}</strong> · {preview.data.positions.length}{" "}
                    positions · {preview.data.holders.length} current employees
                  </p>
                  <Table className={styles.previewTable}>
                    <thead>
                      <tr>
                        <th>Policy target</th>
                        <th>Before</th>
                        <th>After</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(
                        [
                          ["GPO", "gpoId"],
                          ["User filter group", "userFilterGroupId"],
                          ["Computer OU", "computerTargetOuId"],
                        ] as const
                      ).map(([label, field]) => (
                        <tr key={field}>
                          <td>{label}</td>
                          <td>
                            {policyObjectName(
                              edit.kind === "gpo" ? edit.rule?.[field] : undefined,
                              edit.kind === "gpo" ? edit.rule?.connectionId : undefined,
                            )}
                          </td>
                          <td>
                            {"connectionId" in preview.input
                              ? policyObjectName(preview.input[field], preview.input.connectionId)
                              : "Direct rule removed"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                  <p>
                    {preview.data.changes.add.length} membership additions ·{" "}
                    {preview.data.changes.remove.length} removals ·{" "}
                    {preview.data.changes.retained.length} retained through other rules
                  </p>
                  {preview.data.inheritedByFuturePositions && (
                    <p>This mapping applies to the selected Position.</p>
                  )}
                  <p>
                    {preview.data.holders.filter((p) => !p.accountIds.length).length} employees
                    awaiting Directory account linkage. HR placement remains available.
                  </p>
                  <p>
                    Directory delivery starts after confirmation. Endpoint application remains
                    unconfirmed.
                  </p>
                  {preview.data.positions.length > 0 && (
                    <details>
                      <summary>Affected positions</summary>
                      <ul>
                        {preview.data.positions.map((p) => (
                          <li key={p.id}>{p.title}</li>
                        ))}
                      </ul>
                    </details>
                  )}
                </>
              ) : preview.kind === "position" ? (
                <>
                  <p>
                    <strong>{position?.title}</strong> · {preview.data.affectedCount} employees ·{" "}
                    {time(preview.input.effectiveAt, timezone)}
                  </p>
                  <Table className={styles.previewTable}>
                    <thead>
                      <tr>
                        <th>Placement</th>
                        <th>Before</th>
                        <th>After</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr>
                        <td>Branch</td>
                        <td>{departmentName(preview.data.before.branchId)}</td>
                        <td>{departmentName(preview.data.after.branchId)}</td>
                      </tr>
                      <tr>
                        <td>Department</td>
                        <td>{departmentName(preview.data.before.departmentId)}</td>
                        <td>{departmentName(preview.data.after.departmentId)}</td>
                      </tr>
                      <tr>
                        <td>Position level</td>
                        <td>{levelName(preview.data.before.levelId)}</td>
                        <td>{levelName(preview.data.after.levelId)}</td>
                      </tr>
                      <tr>
                        <td>Reports to</td>
                        <td>
                          {organization.positions.find(
                            (p) => p.id === preview.data.before.reportsToPositionId,
                          )?.title ?? "Not assigned"}
                        </td>
                        <td>
                          {organization.positions.find(
                            (p) => p.id === preview.data.after.reportsToPositionId,
                          )?.title ?? "Not assigned"}
                        </td>
                      </tr>
                    </tbody>
                  </Table>
                  {preview.data.people.map((p) => (
                    <div key={p.engagementId}>
                      <strong>{p.name}</strong>
                      <p>
                        Actual level: {levelName(p.before.levelId)} → {levelName(p.after.levelId)}
                        <br />
                        Manager: {managerName(p.before.managerPersonId)} →{" "}
                        {managerName(p.after.managerPersonId)}
                        <br />
                        {p.leaveNote}
                        {p.leaveEffectiveFrom ? ` · Effective ${p.leaveEffectiveFrom}` : ""}
                      </p>
                    </div>
                  ))}
                  <div>
                    <strong>Current GPO impact</strong>
                    <p>
                      {preview.data.policyChanges.added.length} added ·{" "}
                      {preview.data.policyChanges.removed.length} removed ·{" "}
                      {preview.data.policyChanges.retained.length} retained
                    </p>
                    {(["added", "removed", "retained"] as const).map((kind) =>
                      preview.data.policyChanges[kind].map((policy, index) => (
                        <p key={kind + index}>
                          <strong>{kind[0].toUpperCase() + kind.slice(1)}:</strong>{" "}
                          {automation.connections
                            .find((c) => c.id === policy.connectionId)
                            ?.inventory?.objects.find((o) => o.id === policy.gpoId)?.name ??
                            policy.gpoId}
                          <br />
                          Sources:{" "}
                          {(kind === "removed" ? policy.beforeSources : policy.afterSources)
                            .map((source) =>
                              source.kind === "level"
                                ? `Level · ${levelName(source.id)}`
                                : source.kind === "department"
                                  ? `Department · ${departmentName(source.id)}`
                                  : `Position · ${organization.positions.find((p) => p.id === source.id)?.title ?? source.id}`,
                            )
                            .join(", ")}
                        </p>
                      )),
                    )}
                    <p>
                      Directory delivery follows the HR effective date. Policies are rechecked when
                      the change takes effect.
                    </p>
                  </div>
                  <p>
                    Salary stays unchanged. The whole change stops for a new preview if holders,
                    placement, or manager relationships change before the effective date.
                  </p>
                  {preview.data.blockers.map((blocker, index) => (
                    <p key={index} role="alert">
                      {blocker}
                    </p>
                  ))}
                </>
              ) : (
                <>
                  <p>
                    <strong>{employee?.legal_name}</strong> ·{" "}
                    {time(preview.input.effectiveAt, timezone)}
                  </p>
                  <Table className={styles.previewTable}>
                    <thead>
                      <tr>
                        <th>Placement</th>
                        <th>Before</th>
                        <th>After</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr>
                        <td>Position</td>
                        <td>
                          {organization.positions.find((p) => p.id === employee?.position_id)
                            ?.title ?? "Unassigned"}
                        </td>
                        <td>{targetName}</td>
                      </tr>
                      <tr>
                        <td>Manager</td>
                        <td>{managerName(employee?.manager_person_id ?? null)}</td>
                        <td>{managerName(preview.input.managerPersonId)}</td>
                      </tr>
                    </tbody>
                  </Table>
                  <p>
                    {
                      organization.people.filter(
                        (person) =>
                          person.position_id === preview.input.positionId &&
                          person.status !== "ended",
                      ).length
                    }{" "}
                    people already hold the destination position. This placement adds a holder; it
                    does not replace anyone.
                  </p>
                  {preview.data.recommendation && (
                    <p>
                      Leave: {preview.data.recommendation.annualDays ?? "—"} annual days ·{" "}
                      {preview.data.recommendation.sickDays ?? "—"} sick days ·{" "}
                      {preview.input.acceptRecommendedLeave
                        ? "Apply at effective date"
                        : "Keep current entitlement"}
                    </p>
                  )}
                  <p>
                    Salary stays unchanged. HR proceeds on the effective date; Directory delivery is
                    tracked separately.
                  </p>
                  {preview.data.blockers.map((blocker, index) => (
                    <p key={index} role="alert">
                      {blocker}
                    </p>
                  ))}
                  {preview.data.legacyCaseToSupersede && (
                    <Field orientation="inline">
                      <TextInput
                        type="checkbox"
                        checked={acknowledged}
                        onChange={(event) => setAcknowledged(event.target.checked)}
                      />
                      Replace the older {preview.data.legacyCaseToSupersede.kind} request (
                      {preview.data.legacyCaseToSupersede.status.replaceAll("_", " ")}).
                    </Field>
                  )}
                </>
              )}
            </div>
            <div className={styles.actions}>
              <Button
                variant="primary"
                type="button"
                className={styles.button}
                disabled={busy}
                onClick={() => {
                  setPreview(null);
                  setError("");
                }}
              >
                Back to edit
              </Button>
              <Button
                variant="primary"
                type="button"
                className={styles.primary}
                disabled={
                  busy ||
                  !ready ||
                  (preview.kind === "employee" &&
                    !!preview.data.legacyCaseToSupersede &&
                    !acknowledged)
                }
                onClick={() => void confirm()}
              >
                {busy
                  ? "Confirming…"
                  : removing
                    ? "Confirm removal"
                    : preview.kind === "gpo"
                      ? "Publish GPO rule"
                      : "Confirm scheduled change"}
              </Button>
            </div>
          </>
        )}
      </div>
    </Dialog>
  );
}
