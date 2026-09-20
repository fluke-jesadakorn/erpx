"use client";
import { FeatureTabBar,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";

import { lastWorkChange,scheduleWorkRefresh } from "@/lib/work-refresh";

import { PageHeader } from "@/components/ui/primitives";
import { workFetch } from "@/lib/work-fetch";
import { workResponsibilities,workTypes,workViews } from "@/lib/work-item";
import type { WorkPageSummary } from "@/server/work-summary";
import Link from "next/link";
import { useCallback,useEffect,useRef,useState } from "react";
import { useNavigationCounts } from "./navigation-counts";
import { NewWork } from "./new-work";
import { WorkList } from "./work-list";
import { WorkShortcuts } from "./work-shortcuts";
import styles from "./work-dashboard.module.css";

export function WorkDashboard({
  base,
  beta,
  initial,
  queue,
  view,
  type,
  q,
  responsibility,
  current,
}: {
  base: string;
  beta: boolean;
  initial: WorkPageSummary;
  queue: string;
  view: string;
  type: string;
  q: string;
  responsibility: string;
  current: string;
}) {
  const { publishWork } = useNavigationCounts();
  const activeTab = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    const tab = activeTab.current;
    const bar = tab?.parentElement;
    if (!tab || !bar) return;
    const tabBounds = tab.getBoundingClientRect();
    const barBounds = bar.getBoundingClientRect();
    // Keep the selected view visible without moving the page vertically.
    if (tabBounds.right > barBounds.right) bar.scrollLeft += tabBounds.right - barBounds.right;
    else if (tabBounds.left < barBounds.left) bar.scrollLeft += tabBounds.left - barBounds.left;
  }, [view]);
  const [summary, setSummary] = useState(initial),
    [error, setError] = useState("");
  const pending = useRef<Promise<void> | null>(null),
    controller = useRef<AbortController | null>(null);
  const refresh = useCallback((): Promise<void> => {
    if (pending.current && !controller.current?.signal.aborted)
      return pending.current;
    const abort = new AbortController();
    controller.current = abort;
    pending.current = (async () => {
      try {
        const params = new URLSearchParams(current);
        params.set("paged", "1");
        params.set("offset", String(initial.list.offset));
        const response = await workFetch(`/api/v1${base}/work/summary?${params}`, {
          cache: "no-store",
          signal: abort.signal,
        });
        const body = (await response.json()) as {
          data: WorkPageSummary;
          error?: { message?: string };
        };
        if (!response.ok) {
          if (!abort.signal.aborted && [401, 403, 404].includes(response.status))
            setSummary((s) => ({
              ...s,
              cards: [],
              items: [],
              actionCount:null, viewCounts:{},relationships:[],list:{offset:0,total:0,hasMore:false},
              capabilities: { creation: [], teamQueue: false, unavailable: [] },
            }));
          throw Error(body.error?.message ?? "Work could not be refreshed.");
        }
        if (!abort.signal.aborted) {
          setSummary(body.data);
          setError("");
        }
      } catch (e) {
        if (!abort.signal.aborted)
          setError(
            e instanceof Error ? e.message : "Work could not be refreshed.",
          );
      } finally {
        if (controller.current === abort) pending.current = null;
      }
    })();
    return pending.current;
  }, [base,current,initial.list.offset]);
  useEffect(() => {
    const changed = () => { controller.current?.abort(); void refresh(); };
    const unsubscribe = scheduleWorkRefresh(changed);
    const restored = (event: PageTransitionEvent) => { if (event.persisted) changed(); };
    window.addEventListener("pageshow", restored);
    // Fresh SSR data is already authoritative; only restored/invalidated snapshots need a read.
    const timer = Date.now() - initial.generatedAt > 30_000 || initial.generatedAt < lastWorkChange()
      ? setTimeout(() => void refresh(), 0) : undefined;
    return () => { clearTimeout(timer); controller.current?.abort(); unsubscribe(); window.removeEventListener("pageshow", restored); };
  }, [refresh, initial.generatedAt]);
  useEffect(() => {
    const timer = setTimeout(() => publishWork(summary.actionCount), 0);
    return () => clearTimeout(timer);
  }, [summary, publishWork]);
  const pageHref=(offset:number)=>{const query=new URLSearchParams(current);query.set("offset",String(offset));return `?${query}`;};
  const items = summary.items,
    selected = items;
  const unavailable = [
    ...new Set([
      ...summary.unavailableSources,
      ...summary.capabilities.unavailable,
    ]),
  ];
  return (
    <>
      <PageHeader
        title="Your work"
        description="Your next steps, across the workspace."
        action={
          <NewWork
            base={base}
            creation={summary.capabilities.creation}
            current={current}
          />
        }
      />
      {error && (
        <p role="status">
          {error} Updates may be out of date.{" "}
        </p>
      )}
      <div id="work-results" style={{ scrollMarginTop: "100px" }}>
        <FeatureTabBar className={styles.tabs} aria-label="Work queues">
          {workViews.map(([id, label]) => (
            <Link
              key={id}
              ref={view === id ? activeTab : undefined}
              href={`?${new URLSearchParams({ view: id, ...(responsibility !== "all" ? { responsibility } : {}), ...(type !== "all" ? { type } : {}), ...(q ? { q } : {}) })}`}
              aria-current={view === id ? "page" : undefined}
            >
              {label}{" "}
              {summary.viewCounts[id] != null && (
                <span className={styles.count}>{summary.viewCounts[id]}</span>
              )}
            </Link>
          ))}
        </FeatureTabBar>
        {!!unavailable.length && (
          <p role="status">
            Temporarily unavailable: {unavailable.join(", ")}. Your other work
            is shown.{" "}
            <button className="button ghost" onClick={() => void refresh()}>
              Retry
            </button>
          </p>
        )}
        <WidgetGrid pageKey="work"><FeatureWidget id="work-queue" title="Work queue"><form className="work-filters" action={`${base}/work`}>
          <input type="hidden" name={view === "legacy" ? "queue" : "view"} value={view === "legacy" ? queue : view} />
          <select
            className="input"
            name="responsibility"
            aria-label="Responsibility"
            defaultValue={responsibility}
          >
            {workResponsibilities
              .filter(
                ([id]) =>
                  id === "all" ||
                  id === "requested" ||
                  id === responsibility ||
                  (id === "team" && summary.capabilities.teamQueue) ||
                  summary.relationships.includes(
                      id as "assigned" | "approval" | "team",
                    ),
              )
              .map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
          </select>
          <input
            className="input"
            type="search"
            name="q"
            aria-label="Search work"
            placeholder="Search your work…"
            defaultValue={q}
          />
          <select
            className="input"
            name="type"
            aria-label="Work type"
            defaultValue={type}
          >
            {workTypes
              .filter(
                ([id]) =>
                  !beta ||
                  [
                    "all",
                    "staff_task",
                    "expense",
                    "service_request",
                    "equipment_request",
                    "purchase_request",
                  ].includes(id),
              )
              .map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
          </select>
          <button className="button secondary">Search</button>
          {(q || type !== "all" || responsibility !== "all") && (
            <Link className="button ghost" href={`?${view === "legacy" ? "queue=" + queue : "view=" + view}`}>
              Clear filters
            </Link>
          )}
        </form>
        {view === "routing" && <p className="muted">These workflows need an eligible responsible person or corrected approval setup. Open the record and review its Flow before reassigning with a reason. Completed decisions stay unchanged. <Link href={`${base}/work/routing`}>Review Company source routing</Link></p>}
        {view === "team" && <p className="muted">On this page: {selected.filter(i => i.category === "setup").length} setup items · {selected.filter(i => i.category !== "setup").length} team items</p>}
        {view === "team" ? [...new Set(selected.map(i => i.category === "setup" ? i.setupReason ?? "Setup needs an owner" : "Team operations"))].map(reason => <section key={reason}><h2>{reason}</h2><WorkList base={base} items={selected.filter(i => (i.category === "setup" ? i.setupReason ?? "Setup needs an owner" : "Team operations") === reason)} returnTo={current} /></section>) :
        <WorkList base={base} items={selected} returnTo={current} queue={queue} filtered={!!q || type !== "all" || responsibility !== "all"} waiting={summary.viewCounts.waiting} incomplete={!!error || !!unavailable.length} />}
        <nav className="form-actions" aria-label="Work pagination">
          {summary.list.offset>0&&<Link className="button secondary" href={pageHref(Math.max(0,summary.list.offset-30))} prefetch={false}>Previous</Link>}
          <span>{summary.list.total} matching items</span>
          {summary.list.hasMore&&<Link className="button secondary" href={pageHref(summary.list.offset+30)} prefetch={false}>Next</Link>}
        </nav></FeatureWidget></WidgetGrid>
      </div>
      <WorkShortcuts
        base={base}
        current={current}
        summary={summary}
        refresh={refresh}
        onSaved={(preferences) => {
          controller.current?.abort();
          setSummary((s) => ({
            ...s,
            preferences,
            preferencesUnavailable: false,
          }));
        }}
      />
    </>
  );
}
