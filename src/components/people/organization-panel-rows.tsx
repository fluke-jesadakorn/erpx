"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useEffect, useState, type DragEvent, type ReactNode } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Inbox } from "lucide-react";
import { workFetch } from "@/lib/work-fetch";
import {
  draftKey,
  organizationPath,
  type DraftDefinition,
  type DraftItem,
} from "@/lib/organization-draft";
import {
  childSelections,
  hiddenDraftKeys,
  peopleExplorerTabs,
  projectedDirectory,
  selectPanel,
  trayKindsForParents,
  type ExplorerPath,
} from "@/lib/people-explorer";
import type { peopleExplorerDetail } from "@/server/people-explorer-detail";
import {
  OrganizationExplorer,
  type OrganizationSelection,
  type OrganizationExplorerContext,
} from "./organization-explorer";
import {
  PeopleOrganizationDetail,
  type PeopleOrganizationProps,
} from "./people-organization-workspace";
import styles from "./organization-panel-rows.module.css";

type Source = Pick<DraftItem, "kind" | "id">;
type Target = { kind: "branch" | "department" | "level" | "position"; id: string };
type Detail = Awaited<ReturnType<typeof peopleExplorerDetail>>;
export type PanelControls = {
  selected: Source[];
  dragging: Source[];
  over: string;
  select: (source: Source, multiple?: boolean) => void;
  place: (sources: Source[], target?: Target) => void;
  restore: (sources: Source[]) => void;
  end: () => void;
  start: (event: DragEvent, source: Source) => void;
  receive: (event: DragEvent, target?: Target, returnToSource?: boolean) => void;
  dragOver: (event: DragEvent, key: string, target?: Target) => void;
  denial: (sources: Source[], target: Target) => string;
};
type Paths = { source: ExplorerPath; destination: ExplorerPath; tray: ExplorerPath };
type Role = keyof Paths;
const sourceFor = (s: OrganizationSelection): Source | null =>
  ["person", "position", "department"].includes(s.kind) ? (s as Source) : null;
const targetFor = (s: OrganizationSelection): Target | null =>
  ["branch", "department", "level", "position"].includes(s.kind) ? (s as Target) : null;
function DetailContent({
  people,
  selection,
  tab,
  context,
}: {
  people: PeopleOrganizationProps;
  selection: OrganizationSelection;
  tab: string;
  context: OrganizationExplorerContext;
}) {
  const [result, setResult] = useState<{ key: string; detail: Detail } | null>(null),
    [error, setError] = useState<{ key: string; message: string } | null>(null);
  const query = useSearchParams(),
    archived = query.get("status") === "archived";
  const key = `${selection.kind}:${selection.id}:${tab}:${archived}`;
  useEffect(() => {
    const controller = new AbortController();
    let alive = true;
    const params = new URLSearchParams({
      kind: selection.kind,
      id: selection.id,
      tab,
      archived: String(archived),
    });
    void workFetch(`/api/v1${people.base}/people/explorer-detail?${params}`, {
      signal: controller.signal,
    })
      .then(async (response) => {
        const payload = await response.json();
        if (!response.ok) throw new Error(payload.error?.message ?? "Unable to load details.");
        if (alive) {
          setResult({ key, detail: payload.data });
          setError(null);
        }
      })
      .catch((e) => {
        if (alive && !controller.signal.aborted) setError({ key, message: e.message });
      });
    return () => {
      alive = false;
      controller.abort();
    };
  }, [people.base, selection.kind, selection.id, tab, key, archived]);
  if (!result || result.key !== key)
    return (
      <p role={error?.key === key ? "alert" : "status"}>
        {error?.key === key ? error.message : "Loading details…"}
      </p>
    );
  return (
    <PeopleOrganizationDetail
      key={key}
      {...people}
      {...result.detail}
      selection={selection}
      tab={tab}
      context={context}
    />
  );
}
export function OrganizationPanelRows({
  people,
  definition,
  controls,
}: {
  people: PeopleOrganizationProps;
  definition: DraftDefinition;
  controls?: PanelControls;
}) {
  const query = useSearchParams(),
    router = useRouter(),
    archived = people.canManage && query.get("status") === "archived";
  const company: OrganizationSelection = { kind: "company", id: people.currentCompanyId };
  const storageKey = `erpx:people-panels:v1:${people.base}:${people.currentCompanyId}:${people.actorId}${archived ? ":archived" : ""}`;
  const [paths, setPaths] = useState<Paths>({
      source: [{ selection: company, tab: "" }],
      destination: [{ selection: company, tab: "" }],
      tray: [{ selection: null, tab: "" }],
    }),
    [restored, setRestored] = useState(false);
  const [modes, setModes] = useState<Record<number, { panel: Role; pair: string }>>({});
  const [allTray, setAllTray] = useState(false);
  const directory = {
    ...people.data,
    people: archived ? people.archivedPeople : people.data.people,
  };
  const destination = projectedDirectory(directory, people.accessOrganization, definition);
  const projected = people.accessOrganization
    ? projectedAccess(people.accessOrganization, definition)
    : null;
  const sourceHidden = hiddenDraftKeys(directory, definition, true),
    destinationHidden = hiddenDraftKeys(destination, definition, false);
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      try {
        const stored = JSON.parse(sessionStorage.getItem(storageKey) ?? "null");
        if (
          stored &&
          ["source", "destination", "tray"].every(
            (role) =>
              Array.isArray(stored.paths?.[role]) &&
              stored.paths[role].length > 0 &&
              stored.paths[role].length <= 30 &&
              stored.paths[role].every(
                (p: { selection?: OrganizationSelection; tab?: string }) =>
                  p &&
                  typeof p.tab === "string" &&
                  (!p.selection ||
                    (typeof p.selection.id === "string" &&
                      [
                        "company",
                        "branch",
                        "department",
                        "team",
                        "position",
                        "person",
                        "level",
                        "unassigned",
                      ].includes(p.selection.kind))),
              ),
          )
        ) {
          setPaths(stored.paths);
          setModes(
            Object.fromEntries(
              Object.entries(stored.modes ?? {}).filter(
                ([row, value]) =>
                  /^(?:[0-9]|[12][0-9])$/.test(row) &&
                  value &&
                  typeof value === "object" &&
                  "panel" in value &&
                  ["source", "tray", "destination"].includes(String(value.panel)) &&
                  "pair" in value &&
                  ["source-tray", "tray-destination", "source-destination"].includes(
                    String(value.pair),
                  ),
              ),
            ) as Record<number, { panel: Role; pair: string }>,
          );
          setAllTray(stored.allTray === true);
        } else {
          const kinds = [
            "person",
            "position",
            "team",
            "department",
            "branch",
            "level",
            "company",
            "unassigned",
          ] as const;
          const kind = kinds.find((kind) => query.get(kind));
          if (kind) {
            let id = query.get(kind)!;
            if (kind === "person")
              id =
                [...people.data.people, ...people.archivedPeople].find(
                  (p) => p.id === id || p.personId === id || p.engagementId === id,
                )?.engagementId ?? id;
            setPaths((p) => ({ ...p, source: selectPanel(p.source, 0, { kind, id }, directory) }));
          }
        }
      } catch {
        /* Unavailable session storage keeps an in-memory workspace. */
      }
      setRestored(true);
    });
    return () => cancelAnimationFrame(frame);
    // Restore once for this actor and Company; panel navigation is local state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storageKey]);
  useEffect(() => {
    if (restored)
      try {
        sessionStorage.setItem(storageKey, JSON.stringify({ paths, modes, allTray }));
      } catch {}
  }, [storageKey, paths, modes, allTray, restored]);
  function choose(role: Role, row: number, selection: OrganizationSelection) {
    setPaths((p) => ({
      ...p,
      [role]: selectPanel(
        p[role],
        row,
        selection,
        role === "destination" ? destination : directory,
      ),
    }));
    requestAnimationFrame(() =>
      document.getElementById(`people-${role}-${row}-detail-title`)?.focus({ preventScroll: true }),
    );
  }
  function mode(row: number, patch: Partial<{ panel: Role; pair: string }>) {
    setModes((m) => ({
      ...m,
      [row]: { ...(m[row] ?? { panel: "source", pair: "source-tray" }), ...patch },
    }));
  }
  const rows = Math.max(paths.source.length, paths.destination.length, paths.tray.length);
  const parked = definition.items.filter((i) => i.parked);
  function itemCompany(s: OrganizationSelection) {
    return s.kind === "company" || s.kind === "unassigned"
      ? s.id
      : s.kind === "person"
        ? directory.people.find((p) => p.engagementId === s.id)?.companyId
        : s.kind === "position"
          ? directory.positions.find((p) => p.id === s.id)?.companyId
          : s.kind === "level"
            ? directory.levels.find((l) => l.id === s.id)?.companyId
            : directory.nodes.find((n) => n.id === s.id)?.companyId;
  }
  function rowControls(s: OrganizationSelection, role: Role): ReactNode {
    const source = sourceFor(s),
      target = targetFor(s),
      local = itemCompany(s) === people.currentCompanyId;
    if (!controls || !local || query.get("status") === "archived") return null;
    const staged = source && definition.items.find((i) => draftKey(i) === draftKey(source));
    const reason = target
      ? controls.denial(controls.dragging.length ? controls.dragging : controls.selected, target)
      : "";
    return (
      <span className={styles.rowActions}>
        {source && (
          <TextInput
            type="checkbox"
            aria-label={`Select ${label(s)}`}
            checked={controls.selected.some((i) => draftKey(i) === draftKey(source))}
            onChange={() => controls.select(source, true)}
          />
        )}
        {source && !staged?.parked && (
          <Button variant="plain" type="button" onClick={() => controls.place([source])}>
            Park
          </Button>
        )}
        {source && staged && (
          <Button variant="plain" type="button" onClick={() => controls.restore([source])}>
            Undo change
          </Button>
        )}
        {role === "destination" && target && (
          <Button
            variant="plain"
            type="button"
            disabled={!!reason}
            title={reason || "Move selected items here"}
            data-testid={`target-${s.kind}:${s.id}`}
            onDragOver={(e) => controls.dragOver(e, `${s.kind}:${s.id}`, target)}
            onDrop={(e) => controls.receive(e, target)}
            onClick={() => controls.place(controls.selected, target)}
          >
            Move here
          </Button>
        )}
      </span>
    );
  }
  function label(s: OrganizationSelection) {
    return s.kind === "person"
      ? (directory.people.find((p) => p.engagementId === s.id)?.legalName ?? "Employee")
      : s.kind === "position"
        ? (directory.positions.find((p) => p.id === s.id)?.title ?? "Position")
        : s.kind === "company"
          ? (directory.companies.find((c) => c.id === s.id)?.name ?? "Company")
          : s.kind === "level"
            ? (directory.levels.find((l) => l.id === s.id)?.name ?? "Level")
            : (directory.nodes.find((n) => n.id === s.id)?.name ?? "Organization");
  }
  function status(s: OrganizationSelection) {
    const item = definition.items.find((i) => draftKey(i) === draftKey(s as Source));
    const original = people.accessOrganization,
      source = sourceFor(s);
    if (!original || !source) return null;
    const before =
      source.kind === "person"
        ? original.people.find((p) => p.engagement_id === source.id)
        : source.kind === "position"
          ? original.positions.find((p) => p.id === source.id)
          : original.nodes.find((n) => n.id === source.id);
    const after =
      source.kind === "person"
        ? projected?.people.find((p) => p.engagement_id === source.id)
        : source.kind === "position"
          ? projected?.positions.find((p) => p.id === source.id)
          : projected?.nodes.find((n) => n.id === source.id);
    const changed =
      before &&
      after &&
      (
        [
          "branch_id",
          "department_id",
          "position_id",
          "level_id",
          "manager_person_id",
          "reports_to_position_id",
        ] as const
      ).some(
        (key) =>
          (key in before ? before[key as keyof typeof before] : undefined) !==
          (key in after ? after[key as keyof typeof after] : undefined),
      );
    if (!item && !changed) return null;
    return (
      <div className={styles.changeStatus}>
        <strong>{item?.parked ? "In Tray" : "Pending change"}</strong>
        <dl>
          <div>
            <dt>Current location</dt>
            <dd>{organizationPath(original, source.kind, source.id)}</dd>
          </div>
          <div>
            <dt>Proposed location</dt>
            <dd>
              {item?.parked
                ? "Tray — choose a destination"
                : organizationPath(projected!, source.kind, source.id)}
            </dd>
          </div>
        </dl>
      </div>
    );
  }
  return (
    <div className={styles.workspace} data-testid="organization-panel-rows">
      <div className={styles.trayToolbar}>
        <strong>
          {controls
            ? `${definition.items.length} draft changes · ${parked.length} in Tray`
            : "Read-only organization explorer"}
        </strong>
        {people.canManage && (
          <Field>
            Status{" "}
            <Select
              aria-label="People status"
              value={archived ? "archived" : "current"}
              onChange={(e) => {
                const params = new URLSearchParams(query);
                if (e.target.value === "archived") params.set("status", "archived");
                else params.delete("status");
                router.replace(`${people.base}/people?${params}`, { scroll: false });
              }}
            >
              {" "}
              <option value="current">Current</option>
              <option value="archived">Archived</option>
            </Select>
          </Field>
        )}
        <Button
          variant="plain"
          type="button"
          onClick={() => {
            setAllTray((v) => !v);
            mode(0, { panel: "tray", pair: "source-tray" });
          }}
        >
          <Inbox size={16} />
          {allTray ? "Show trays by type" : "Open all Tray"} ({parked.length})
        </Button>
      </div>
      {Array.from({ length: rows }, (_, row) => {
        const visible = modes[row] ?? { panel: "source", pair: "source-tray" };
        const kinds = trayKindsForParents(
          row === 0
            ? []
            : [
                paths.source[row - 1]?.selection,
                paths.destination[row - 1]?.selection,
                paths.tray[row - 1]?.selection,
              ],
        );
        const trayItems = parked.filter((i) => (row === 0 && allTray) || kinds.includes(i.kind));
        const title = kinds
          .map((kind) =>
            kind === "person" ? "User" : kind === "position" ? "Position" : "Organization",
          )
          .join(" / ");
        return (
          <section className={styles.row} key={row} data-testid={`organization-panel-row-${row}`}>
            {row > 0 && (
              <h3 className={styles.rowTitle}>
                {title} workspace · {row + 1}
                <Button
                  variant="plain"
                  type="button"
                  onClick={() =>
                    setPaths((p) => ({
                      source: p.source.slice(0, row),
                      destination: p.destination.slice(0, row),
                      tray: p.tray.slice(0, row),
                    }))
                  }
                >
                  Close this row and below
                </Button>
              </h3>
            )}
            <nav className={styles.mobileTabs} aria-label={`Panels in row ${row + 1}`}>
              {(["source", "tray", "destination"] as const).map((role) => (
                <Button
                  variant="plain"
                  key={role}
                  type="button"
                  aria-pressed={visible.panel === role}
                  onClick={() => mode(row, { panel: role })}
                >
                  {role === "tray"
                    ? `Tray (${trayItems.length})`
                    : role === "source"
                      ? "Source"
                      : "Destination"}
                </Button>
              ))}
            </nav>
            <Field className={styles.pair}>
              Visible panels
              <Select
                aria-label={`Visible panels row ${row + 1}`}
                value={visible.pair}
                onChange={(e) => mode(row, { pair: e.target.value })}
              >
                <option value="source-tray">Source + Tray</option>
                <option value="tray-destination">Tray + Destination</option>
                <option value="source-destination">Source + Destination</option>
              </Select>
            </Field>
            <div className={styles.panels} data-panel={visible.panel} data-pair={visible.pair}>
              {(["source", "tray", "destination"] as const).map((role) => {
                const state = paths[role][row],
                  selection = state?.selection ?? company;
                const parentSelection = paths[role][row - 1]?.selection;
                const data = role === "destination" ? destination : directory;
                const children =
                  role === "tray"
                    ? trayItems.map((i) => ({ kind: i.kind, id: i.id }))
                    : row === 0
                      ? undefined
                      : parentSelection
                        ? childSelections(data, parentSelection)
                        : [];
                const actualChildren =
                  role === "tray" &&
                  parentSelection &&
                  parked.some((i) => draftKey(i) === draftKey(parentSelection as Source))
                    ? [
                        ...new Map(
                          [...(children ?? []), ...childSelections(directory, parentSelection)].map(
                            (s) => [draftKey(s as Source), s],
                          ),
                        ).values(),
                      ]
                    : children;
                const local = itemCompany(selection) === people.currentCompanyId;
                const tabs = peopleExplorerTabs(
                  selection.kind,
                  local && people.canManage,
                  local && people.canReadPay,
                );
                return (
                  <section
                    key={role}
                    className={`${styles.panel} ${styles[role]}`}
                    aria-label={`${role === "source" ? "Source" : role === "tray" ? "Tray" : "Destination"} panel row ${row + 1}`}
                    onDragOver={(e) => {
                      if (role === "tray") controls?.dragOver(e, "tray");
                      else if (role === "source") controls?.dragOver(e, "source");
                    }}
                    onDrop={(e) => {
                      if (role === "tray") controls?.receive(e);
                      else if (role === "source") controls?.receive(e, undefined, true);
                    }}
                    data-over={controls?.over === role}
                  >
                    <header>
                      <h3>
                        {role === "tray"
                          ? `${title} tray · ${trayItems.length}`
                          : role === "source"
                            ? "Source"
                            : "Destination"}
                      </h3>
                      {parentSelection && (
                        <nav aria-label={`${role} breadcrumb row ${row + 1}`}>
                          {paths[role].slice(0, row).map(
                            (p, index) =>
                              p.selection && (
                                <Button
                                  variant="plain"
                                  type="button"
                                  key={index}
                                  onClick={() => choose(role, index, p.selection!)}
                                >
                                  {label(p.selection)} ›
                                </Button>
                              ),
                          )}
                        </nav>
                      )}
                    </header>
                    <OrganizationExplorer
                      data={data}
                      initialCompanyId={people.currentCompanyId}
                      holdingName={people.holdingName}
                      selection={selection}
                      onSelectionChange={(s) => choose(role, row, s)}
                      tab={state?.tab ?? ""}
                      onTabChange={(tab) =>
                        setPaths((p) => ({
                          ...p,
                          [role]: p[role].map((s, i) => (i === row ? { ...s, tab } : s)),
                        }))
                      }
                      panelMode
                      detailEmpty={!state?.selection}
                      treeSelections={actualChildren}
                      hiddenKeys={
                        role === "source"
                          ? sourceHidden
                          : role === "destination"
                            ? destinationHidden
                            : undefined
                      }
                      testIdPrefix={`people-${role}-${row}`}
                      searchLabel={
                        role === "source"
                          ? "Search source"
                          : role === "destination"
                            ? "Search destinations"
                            : `Search ${title.toLowerCase()} tray`
                      }
                      visibleTabs={() => tabs}
                      tabsForSelection={() => tabs}
                      rowControls={(s) => rowControls(s, role)}
                      detailStatus={status(selection)}
                      onRowDragStart={
                        controls
                          ? (e, s) => {
                              const source = sourceFor(s);
                              if (
                                source &&
                                itemCompany(s) === people.currentCompanyId &&
                                query.get("status") !== "archived"
                              )
                                controls.start(e, source);
                              else e.preventDefault();
                            }
                          : undefined
                      }
                      onRowDragEnd={controls?.end}
                      rowDropState={
                        controls && role === "destination"
                          ? (s) => {
                              const target = targetFor(s);
                              if (!target || !controls.dragging.length) return undefined;
                              return controls.denial(controls.dragging, target)
                                ? "blocked"
                                : controls.over === `${s.kind}:${s.id}`
                                  ? "ready"
                                  : "available";
                            }
                          : undefined
                      }
                      onRowDrop={
                        controls && role === "destination"
                          ? (e, s) => {
                              const target = targetFor(s);
                              if (target) controls.receive(e, target);
                            }
                          : undefined
                      }
                      onRowDragOver={
                        controls && role === "destination"
                          ? (e, s) => {
                              const target = targetFor(s);
                              if (target) controls.dragOver(e, `${s.kind}:${s.id}`, target);
                            }
                          : undefined
                      }
                      renderDetail={(s, tab, context) =>
                        ["People", "Positions"].includes(tab) ||
                        ["level", "unassigned"].includes(s.kind) ||
                        (s.kind === "position" &&
                          tab === "Overview" &&
                          !people.canManageStructure) ? undefined : (
                          <DetailContent
                            people={people}
                            selection={s}
                            tab={tab}
                            context={context}
                          />
                        )
                      }
                    />
                  </section>
                );
              })}
            </div>
          </section>
        );
      })}
    </div>
  );
}
import { projectOrganization as projectedAccess } from "@/lib/organization-draft";
