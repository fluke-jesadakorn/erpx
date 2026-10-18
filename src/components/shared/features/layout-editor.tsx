"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import {
  Children,
  cloneElement,
  Fragment,
  createContext,
  isValidElement,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import Link from "@/components/workplace/workspace-link";
import { registerUnsavedWork } from "@/lib/work_projection/context-switch";
import { resizeGrid } from "@/lib/shared/widget-geometry";

import { GripVertical, Settings2, EyeOff, ArrowUp, ArrowDown } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import type { NavigationContext } from "@/lib/work_projection/workplace-navigation";
import {
  allowedWidgets,
  layoutDifferences,
  reconcileLayout,
  resetWidgetToOrganization,
  type FeatureDefinition,
  type LayoutItem,
  type LayoutSnapshot,
  type PageLayout,
} from "@/lib/work_projection/page-layouts-view";
import { workFetch } from "@/lib/work_projection/work-fetch";

import styles from "@/components/shared/features/features.module.css";

import { InsideWidget, PageDrawer, WidgetRenderer, type FeatureWidgetProps } from "@/components/shared/features/widget-context";
import { FeatureWidget } from "@/components/shared/features/feature-widget";
type GridContext = {
  layout: PageLayout;
  editing: boolean;
  target: string | null;
  patch: (id: string, patch: Partial<LayoutItem>) => void;
  move: (id: string, offset: number) => void;
  drop: (id: string, target: string) => void;
};
const Grid = createContext<GridContext | null>(null);

function flatten(children: ReactNode, keyPath = ""): ReactNode[] {
  return Children.toArray(children).flatMap((child) => {
    if (!isValidElement(child)) return [child];
    const key = `${keyPath}${child.key ?? ""}`;
    return isValidElement<{ children?: ReactNode }>(child) && child.type === Fragment
      ? flatten(child.props.children, `${key}/`)
      : [cloneElement(child, { key })];
  });
}
const empty: LayoutSnapshot = {
  personal: null,
  organization: null,
  version: 0,
  organizationVersion: 0,
  canPublish: false,
};
export function LayoutGrid({
  children,
  className,
  pageKey,
  navigation,
  feature,
  target,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  pageKey: string;
  navigation: NavigationContext;
  feature: FeatureDefinition;
  target: string | null;
}) {
  const definitions = useMemo(() => allowedWidgets(pageKey, navigation), [pageKey, navigation]);
  const pageDrawer = useContext(PageDrawer);
  const [snapshot, setSnapshot] = useState(empty),
    [draft, setDraft] = useState<PageLayout | null>(null),
    [busy, setBusy] = useState(false),
    [loaded, setLoaded] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [publishing, setPublishing] = useState(false);
  const api = `/api/v1${navigation.workspaceBase === "/account" ? "/me" : navigation.workspaceBase}/page-layouts/${encodeURIComponent(pageKey)}`;
  useEffect(() => {
    if (!definitions.length) return;
    const controller = new AbortController();
    workFetch(api, { signal: controller.signal, cache: "no-store" })
      .then(async (r) => {
        const body = await r.json();
        if (!r.ok) throw Error(body.error?.message ?? "Could not load layout.");
        setSnapshot(body.data);
        setLoaded(true);
      })
      .catch((e) => {
        if (!controller.signal.aborted) setError(e.message);
      });
    return () => controller.abort();
  }, [api, definitions.length]);
  const layout = reconcileLayout(draft ?? snapshot.personal ?? snapshot.organization, definitions);
  function patch(id: string, patch: Partial<LayoutItem>) {
    setDraft({
      ...layout,
      widgets: layout.widgets.map((w) => (w.id === id ? { ...w, ...patch } : w)),
    });
  }
  function move(id: string, offset: number) {
    const widgets = [...layout.widgets],
      from = widgets.findIndex((w) => w.id === id),
      to = Math.max(0, Math.min(widgets.length - 1, from + offset));
    if (from < 0) return;
    const [item] = widgets.splice(from, 1);
    widgets.splice(to, 0, item);
    setDraft({ ...layout, widgets });
    setNotice(`${definitions.find((w) => w.id === id)?.title ?? "Widget"} moved.`);
  }
  function drop(id: string, targetId: string) {
    const from = layout.widgets.findIndex((w) => w.id === id),
      to = layout.widgets.findIndex((w) => w.id === targetId);
    if (from >= 0 && to >= 0) move(id, to - from);
  }
  const retry = useRef<{ body: string; key: string } | null>(null);
  async function save(action: "save" | "reset" | "publish") {
    if (busy) return;
    setBusy(true);
    setError("");
    const body = JSON.stringify({
      action,
      layout: action === "reset" ? null : layout,
      expectedVersion: action === "publish" ? snapshot.organizationVersion : snapshot.version,
    });
    const command =
      retry.current?.body === body ? retry.current : { body, key: crypto.randomUUID() };
    retry.current = command;
    try {
      const r = await workFetch(api, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": command.key,
        },
        body,
      });
      const result = await r.json();
      if (!r.ok) throw Error(result.error?.message ?? "Could not save layout.");
      setSnapshot(result.data);
      setDraft(null);
      setPublishing(false);
      retry.current = null;
      setNotice(
        action === "publish"
          ? "Organization layout published. Personal layouts stay unchanged."
          : action === "reset"
            ? "Personal layout reset to organization layout."
            : "Layout saved.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save layout.");
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    if (!draft) return;
    const unregister = registerUnsavedWork(() => true);
    const prevent = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", prevent);
    return () => {
      unregister();
      window.removeEventListener("beforeunload", prevent);
    };
  }, [draft]);
  useEffect(() => {
    if (!draft) return;
    const confirmNavigation = (event: MouseEvent) => {
      const link = (event.target as HTMLElement | null)?.closest("a[href]");
      if (!link || link.getAttribute("target") === "_blank") return;
      if (!window.confirm("Discard your unsaved layout changes?")) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    document.addEventListener("click", confirmNavigation, true);
    return () => document.removeEventListener("click", confirmNavigation, true);
  }, [draft]);
  const differences = layoutDifferences(
    draft ?? snapshot.personal,
    snapshot.organization,
    definitions,
  );
  function closeDrawer() {
    if (draft && !window.confirm("Discard your unsaved layout changes?")) return;
    setDraft(null);
    setPublishing(false);
    setError("");
    pageDrawer?.setOpen(false);
  }
  function resetWidget(id: string) {
    setDraft(resetWidgetToOrganization(draft ?? layout, id, snapshot.organization, definitions));
  }
  const nodes = flatten(children);
  const widgetOrder = (id: string) => {
    const index = layout.widgets.findIndex((widget) => widget.id === id);
    return index < 0 ? layout.widgets.length : index;
  };
  // Sort only declared sibling widgets. Stable keys preserve their form state.
  const sortable = nodes.filter(
    (n) => isValidElement<{ id: string }>(n) && n.type === FeatureWidget,
  );
  sortable.sort(
    (a, b) =>
      widgetOrder((a as React.ReactElement<{ id: string }>).props.id) -
      widgetOrder((b as React.ReactElement<{ id: string }>).props.id),
  );
  let index = 0;
  const ordered = nodes.map((n) =>
    isValidElement(n) && n.type === FeatureWidget
      ? (() => {
          const widget = sortable[index++] as React.ReactElement<{ id: string }>;
          return cloneElement(widget, { key: widget.props.id });
        })()
      : n,
  );
  return (
    <Grid value={{ layout, editing: !!draft, target, patch, move, drop }}>
      <WidgetRenderer value={renderEditableWidget}>
      {feature.secondaryTabs.length === 0 && definitions.length > 0 && (
        <div className={styles.customize} data-layout-controls>
          {draft ? (
            <>
              <strong>{publishing ? "Organization layout" : "Personal layout"}</strong>
              <Button
                type="submit"
                variant="plain"
                disabled={busy}
                onClick={() => void save(publishing ? "publish" : "save")}
              >
                Save
              </Button>
              <Button
                type="submit"
                variant="plain"
                disabled={busy}
                onClick={() => {
                  setDraft(null);
                  setPublishing(false);
                  setError("");
                }}
              >
                Cancel
              </Button>
              <details>
                <summary>Restore widget</summary>
                {layout.widgets
                  .filter((widget) => widget.hidden)
                  .map((widget) => (
                    <Button
                      type="submit"
                      variant="plain"
                      key={widget.id}
                      onClick={() => patch(widget.id, { hidden: false })}
                    >
                      {definitions.find((definition) => definition.id === widget.id)?.title}
                    </Button>
                  ))}
                {!layout.widgets.some((widget) => widget.hidden) && (
                  <span>All widgets are visible</span>
                )}
              </details>
            </>
          ) : (
            <>
              <Button
                type="submit"
                variant="plain"
                disabled={!loaded}
                onClick={() => setDraft(layout)}
              >
                <Settings2 size={15} /> Customize layout
              </Button>
              {snapshot.personal && (
                <Button
                  type="submit"
                  variant="plain"
                  disabled={busy}
                  onClick={() => void save("reset")}
                >
                  Reset to organization layout
                </Button>
              )}
              {snapshot.canPublish && (
                <Button
                  type="submit"
                  variant="plain"
                  disabled={!loaded}
                  onClick={() => {
                    setPublishing(true);
                    setDraft(reconcileLayout(snapshot.organization, definitions));
                  }}
                >
                  Edit organization layout
                </Button>
              )}
            </>
          )}
          {notice && <span role="status">{notice}</span>}
        </div>
      )}
      {feature.secondaryTabs.length > 0 && pageDrawer && (
        <Dialog
          id="people-tools-drawer"
          open={pageDrawer.open}
          onClose={closeDrawer}
          title="People views and layout"
          presentation="people-drawer"
        >
          <div className={styles.drawerContent} data-layout-controls>
            <section className={styles.drawerSection}>
              <h3>More views</h3>
              <nav aria-label="More People views" className={styles.moreViews}>
                {feature.secondaryTabs.map((tab) => (
                  <Link
                    key={tab.id}
                    href={tab.href}
                    aria-current={tab.id === feature.activeId ? "page" : undefined}
                  >
                    <span>{tab.label}</span>
                    {tab.id === feature.activeId && <small>Current</small>}
                  </Link>
                ))}
              </nav>
            </section>
            <section className={styles.drawerSection}>
              <div className={styles.drawerSectionHeading}>
                <div>
                  <h3>Page widgets</h3>
                  <p>
                    {publishing
                      ? "Editing the organization baseline"
                      : differences.hasPersonalOverride
                        ? `${differences.changeCount} personal ${differences.changeCount === 1 ? "change" : "changes"}`
                        : "Using organization layout"}
                  </p>
                </div>
                {snapshot.canPublish && !publishing && (
                  <Button
                    variant="plain"
                    type="button"
                    disabled={!loaded}
                    onClick={() => {
                      setPublishing(true);
                      setDraft(reconcileLayout(snapshot.organization, definitions));
                    }}
                  >
                    Edit organization layout
                  </Button>
                )}
              </div>
              {!definitions.length && <p>No configurable widgets on this page.</p>}
              <fieldset className={styles.widgetSettings} disabled={!loaded || busy}>
                {layout.widgets.map((item) => {
                  const definition = definitions.find((widget) => widget.id === item.id);
                  const difference = differences.widgets.find((widget) => widget.id === item.id);
                  if (!definition) return null;
                  return (
                    <article key={item.id} className={styles.widgetSetting}>
                      <div className={styles.widgetSettingTitle}>
                        <strong>{definition.title}</strong>
                        <Field orientation="inline">
                          <TextInput
                            type="checkbox"
                            checked={!item.hidden}
                            onChange={(event) => patch(item.id, { hidden: !event.target.checked })}
                          />
                          Visible
                        </Field>
                      </div>
                      {difference && (
                        <div className={styles.differenceBadges}>
                          {difference.badges.map((badge) => (
                            <span key={badge}>{badge}</span>
                          ))}
                        </div>
                      )}
                      <div className={styles.widgetSettingControls}>
                        <span>Position</span>
                        <Button
                          variant="plain"
                          type="button"
                          aria-label={`Move ${definition.title} up`}
                          onClick={() => move(item.id, -1)}
                        >
                          <ArrowUp size={14} />
                        </Button>
                        <Button
                          variant="plain"
                          type="button"
                          aria-label={`Move ${definition.title} down`}
                          onClick={() => move(item.id, 1)}
                        >
                          <ArrowDown size={14} />
                        </Button>
                        <Field>
                          Width
                          <Select
                            aria-label={`${definition.title} width`}
                            value={item.columns}
                            onChange={(event) =>
                              patch(item.id, { columns: Number(event.target.value) })
                            }
                          >
                            {[3, 4, 6, 8, 12].map((columns) => (
                              <option key={columns} value={columns}>
                                {columns}/12
                              </option>
                            ))}
                          </Select>
                        </Field>
                        <Field>
                          Height
                          <Select
                            aria-label={`${definition.title} height`}
                            value={item.height === null ? "auto" : "fixed"}
                            onChange={(event) =>
                              patch(item.id, {
                                height: event.target.value === "auto" ? null : (item.height ?? 320),
                              })
                            }
                          >
                            <option value="auto">Auto</option>
                            <option value="fixed">Fixed</option>
                          </Select>
                        </Field>
                        {item.height !== null && (
                          <Field>
                            Pixels
                            <TextInput
                              aria-label={`${definition.title} height in pixels`}
                              type="number"
                              min={160}
                              max={1600}
                              step={20}
                              value={item.height}
                              onChange={(event) =>
                                patch(item.id, {
                                  height: Math.max(160, Math.min(1600, Number(event.target.value))),
                                })
                              }
                            />
                          </Field>
                        )}
                      </div>
                      <Button
                        variant="plain"
                        type="button"
                        className={styles.resetWidget}
                        onClick={() => resetWidget(item.id)}
                      >
                        Reset widget
                      </Button>
                    </article>
                  );
                })}
              </fieldset>
            </section>
            {error && (
              <div className={styles.error} role="alert">
                {error}{" "}
                <Button variant="plain" type="button" onClick={closeDrawer}>
                  Close
                </Button>
              </div>
            )}
            {notice && (
              <p role="status" className={styles.drawerNotice}>
                {notice}
              </p>
            )}
            <div className={styles.drawerActions}>
              <Button
                variant="primary"
                type="button"
                disabled={busy || !draft}
                onClick={() => void save(publishing ? "publish" : "save")}
              >
                Save
              </Button>
              <Button variant="secondary" type="button" disabled={busy} onClick={closeDrawer}>
                Cancel
              </Button>
              {!publishing && (
                <Button
                  variant="plain"
                  type="button"
                  disabled={busy || !snapshot.personal}
                  onClick={() => void save("reset")}
                >
                  Reset all to organization layout
                </Button>
              )}
            </div>
          </div>
        </Dialog>
      )}
      {error && (
        <div className={styles.error} role="alert">
          {error}{" "}
          <Button
            type="submit"
            variant="plain"
            onClick={() => {
              setBusy(true);
              workFetch(api, { cache: "no-store" })
                .then(async (r) => {
                  const b = await r.json();
                  if (!r.ok) throw Error(b.error?.message);
                  setSnapshot(b.data);
                  setDraft(null);
                  setLoaded(true);
                  setError("");
                })
                .catch((e) => setError(e.message))
                .finally(() => setBusy(false));
            }}
          >
            Reload saved layout
          </Button>
        </div>
      )}
      <div
        {...props}
        className={`${styles.grid} ${className ?? ""}`}
        data-widget-grid
        data-page-key={pageKey}
      >
        {ordered}
      </div>
      </WidgetRenderer>
    </Grid>
  );
}
function renderEditableWidget(props: FeatureWidgetProps) {
  return <EditableFeatureWidget {...props} />;
}
function EditableFeatureWidget({
  id,
  title,
  children,
  className = "",
  columns = 12,
  ...props
}: HTMLAttributes<HTMLElement> & {
  id: string;
  title: string;
  columns?: number;
}) {
  const nested = useContext(InsideWidget);
  const grid = useContext(Grid),
    ref = useRef<HTMLElement>(null),
    start = useRef<{
      x: number;
      y: number;
      width: number;
      columns: number;
      height: number;
    } | null>(null);
  const item = grid?.layout.widgets.find((w) => w.id === id) ?? {
    id,
    columns,
    height: null,
    hidden: false,
  };
  const layoutIndex = grid?.layout.widgets.findIndex((widget) => widget.id === id) ?? -1;
  const targeted = !!grid?.target && (id === grid.target || id.endsWith(`-${grid.target}`));
  useEffect(() => {
    if (targeted) {
      ref.current?.scrollIntoView({ block: "start" });
      ref.current?.focus({ preventScroll: true });
    }
  }, [targeted]);
  if (nested)
    return (
      <section {...props} id={id} className={className}>
        <h3>{title}</h3>
        {children}
      </section>
    );
  const hidden = item.hidden && !grid?.editing && !targeted;
  return (
    <section
      {...props}
      ref={ref}
      id={id}
      tabIndex={-1}
      hidden={hidden}
      data-feature-widget={id}
      className={`${styles.widget} ${className}`}
      style={
        {
          ...props.style,
          order: layoutIndex < 0 ? (grid?.layout.widgets.length ?? 0) : layoutIndex,
          gridColumn: `span ${item.columns}`,
          height: item.height ?? undefined,
        } as CSSProperties
      }
      onDragOver={(e) => {
        if (grid?.editing && e.dataTransfer.types.includes("application/x-erpx-layout"))
          e.preventDefault();
      }}
      onDrop={(e) => {
        if (!grid?.editing) return;
        const source = e.dataTransfer.getData("application/x-erpx-layout");
        if (source) {
          e.preventDefault();
          e.stopPropagation();
          grid.drop(source, id);
        }
      }}
    >
      <header className={styles.widgetHeading}>
        <h2>{title}</h2>
        {grid?.editing && (
          <div className={styles.widgetTools}>
            <Button
              type="submit"
              variant="plain"
              draggable
              style={{ touchAction: "none" }}
              onPointerDown={(e) => {
                if (e.pointerType !== "mouse") {
                  e.preventDefault();
                  e.currentTarget.setPointerCapture(e.pointerId);
                }
              }}
              onPointerUp={(e) => {
                if (e.pointerType !== "mouse") {
                  const target = document
                    .elementFromPoint(e.clientX, e.clientY)
                    ?.closest("[data-feature-widget]")
                    ?.getAttribute("data-feature-widget");
                  if (target) grid.drop(id, target);
                }
              }}
              aria-label={`Move ${title}`}
              onDragStart={(e) => e.dataTransfer.setData("application/x-erpx-layout", id)}
              onKeyDown={(e) => {
                if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                  e.preventDefault();
                  grid.move(id, e.key === "ArrowUp" ? -1 : 1);
                }
              }}
            >
              <GripVertical size={16} />
            </Button>
            <Button
              type="submit"
              variant="plain"
              aria-label={`Move ${title} up`}
              onClick={() => grid.move(id, -1)}
            >
              <ArrowUp size={14} />
            </Button>
            <Button
              type="submit"
              variant="plain"
              aria-label={`Move ${title} down`}
              onClick={() => grid.move(id, 1)}
            >
              <ArrowDown size={14} />
            </Button>
            <Field>
              Width
              <Select
                aria-label={`${title} width`}
                value={item.columns}
                onChange={(e) => grid.patch(id, { columns: Number(e.target.value) })}
              >
                {[3, 4, 6, 8, 12].map((n) => (
                  <option key={n} value={n}>
                    {n}/12
                  </option>
                ))}
              </Select>
            </Field>
            <Button
              type="submit"
              variant="plain"
              aria-label={`${item.hidden ? "Show" : "Hide"} ${title}`}
              onClick={() => grid.patch(id, { hidden: !item.hidden })}
            >
              <EyeOff size={14} />
              {item.hidden ? "Show" : "Hide"}
            </Button>
            <Button type="submit" variant="plain" onClick={() => grid.patch(id, { height: null })}>
              Auto height
            </Button>
          </div>
        )}
      </header>
      {item.hidden && grid?.editing && <p>Hidden in your saved layout</p>}
      <InsideWidget value={true}>
        <div data-slot="widget-content" className={styles.widgetContent}>{children}</div>
      </InsideWidget>
      {grid?.editing && (
        <Button
          type="submit"
          variant="plain"
          className={styles.resize}
          aria-label={`Resize ${title}`}
          onPointerDown={(e) => {
            e.preventDefault();
            e.currentTarget.setPointerCapture(e.pointerId);
            const box = ref.current!.getBoundingClientRect();
            start.current = {
              x: e.clientX,
              y: e.clientY,
              width: ref.current!.parentElement!.getBoundingClientRect().width,
              columns: item.columns,
              height: item.height ?? box.height,
            };
          }}
          onPointerMove={(e) => {
            const s = start.current;
            if (!s) return;
            grid.patch(
              id,
              resizeGrid(s.columns, s.height, e.clientX - s.x, e.clientY - s.y, s.width, "corner", {
                minColumns: 3,
                minHeight: 160,
                maxHeight: 1600,
              }),
            );
          }}
          onPointerUp={() => {
            start.current = null;
          }}
          onPointerCancel={() => {
            start.current = null;
          }}
          onKeyDown={(e) => {
            if (e.key.startsWith("Arrow")) {
              e.preventDefault();
              grid.patch(id, {
                columns: Math.min(
                  12,
                  Math.max(
                    3,
                    item.columns + (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0),
                  ),
                ),
                height: Math.min(
                  1600,
                  Math.max(
                    160,
                    (item.height ?? ref.current!.offsetHeight) +
                      (e.key === "ArrowDown" ? 24 : e.key === "ArrowUp" ? -24 : 0),
                  ),
                ),
              });
            }
          }}
        >
          ◢
        </Button>
      )}
    </section>
  );
}
