"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Select, TextInput } from "@/components/ui/foundation";

import { FeatureWidget, WidgetGrid } from "@/components/features/feature-page";

import { lastWorkChange, scheduleWorkRefresh } from "@/lib/work-refresh";

import { PageHeader } from "@/components/ui/primitives";
import { workFetch } from "@/lib/work-fetch";
import { workResponsibilities, workTypes, workViews } from "@/lib/work-item";
import type { WorkPageSummary } from "@/server/modules/work_projection/public/work-summary";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigationState } from "./navigation-counts";
import { NewWork } from "./new-work";
import { WorkList } from "./work-list";
import { ForMe } from "./for-me";
import { WorkReviewPanel } from "./work-review-panel";
import type { WorkItem } from "@/lib/work-item";

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
  const { publishWork, shortcutStorageKey } = useNavigationState();
  useEffect(() => {
    if (!shortcutStorageKey) return;
    const key = `erpx:work-position:${shortcutStorageKey}:${base}:${current}`;
    let frame = 0;
    try {
      const saved = JSON.parse(sessionStorage.getItem(key) ?? "null") as { y: number; href: string } | null;
      if (saved) frame = requestAnimationFrame(() => {
        const link = [...document.querySelectorAll<HTMLAnchorElement>("#workspace-content a")].find(a => a.href === saved.href);
        link?.focus({ preventScroll: true });
        window.scrollTo({ top: saved.y });
      });
    } catch { /* An unavailable storage area does not prevent queue navigation. */ }
    const remember = (event: MouseEvent) => {
      const link = (event.target as Element)?.closest?.("a");
      if (!link || !link.closest("#workspace-content")) return;
      try { sessionStorage.setItem(key, JSON.stringify({ y: window.scrollY, href: link.href })); } catch { /* Optional context recovery. */ }
    };
    document.addEventListener("click", remember, true);
    return () => { cancelAnimationFrame(frame); document.removeEventListener("click", remember, true); };
  }, [base, current, shortcutStorageKey]);
  const [summary, setSummary] = useState(initial),
    [error, setError] = useState("");
  const [review, setReview] = useState<WorkItem | null>(null);
  const tracking = new URLSearchParams(current).get("tracking") ?? "all";
  function viewHref(id: string, trackingView?: string) {
    return `?${new URLSearchParams({ view: id, ...(type !== "all" ? { type } : {}), ...(q ? { q } : {}), ...(trackingView && trackingView !== "all" ? { tracking: trackingView } : {}) })}`;
  }
  const pending = useRef<Promise<void> | null>(null),
    controller = useRef<AbortController | null>(null);
  const refresh = useCallback((): Promise<void> => {
    if (pending.current && !controller.current?.signal.aborted) return pending.current;
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
          if (!abort.signal.aborted && [401, 403, 404].includes(response.status)) {
            setReview(null);
            setSummary((s) => ({
              ...s,
              cards: [],
              items: [],
              actionCount: null,
              viewCounts: {},
              relationships: [],
              list: { offset: 0, total: 0, hasMore: false },
              capabilities: { creation: [], teamQueue: false, unavailable: [] },
            }));
          }
          throw Error(body.error?.message ?? "Work could not be refreshed.");
        }
        if (!abort.signal.aborted) {
          setSummary(body.data);
          setError("");
        }
      } catch (e) {
        if (!abort.signal.aborted)
          setError(e instanceof Error ? e.message : "Work could not be refreshed.");
      } finally {
        if (controller.current === abort) pending.current = null;
      }
    })();
    return pending.current;
  }, [base, current, initial.list.offset]);
  useEffect(() => {
    const changed = () => {
      controller.current?.abort();
      void refresh();
    };
    const unsubscribe = scheduleWorkRefresh(changed);
    const restored = (event: PageTransitionEvent) => {
      if (event.persisted) changed();
    };
    window.addEventListener("pageshow", restored);
    // Fresh SSR data is already authoritative; only restored/invalidated snapshots need a read.
    const timer =
      Date.now() - initial.generatedAt > 30_000 || initial.generatedAt < lastWorkChange()
        ? setTimeout(() => void refresh(), 0)
        : undefined;
    return () => {
      clearTimeout(timer);
      controller.current?.abort();
      unsubscribe();
      window.removeEventListener("pageshow", restored);
    };
  }, [refresh, initial.generatedAt]);
  useEffect(() => {
    const timer = setTimeout(() => publishWork(summary.actionCount, {
      team: summary.capabilities.teamQueue || summary.relationships.includes("team"),
      routing: !!summary.viewCounts.routing,
    }), 0);
    return () => clearTimeout(timer);
  }, [summary, publishWork]);
  const pageHref = (offset: number) => {
    const query = new URLSearchParams(current);
    query.set("offset", String(offset));
    return `?${query}`;
  };
  const items = summary.items,
    selected = items;
  const unavailable = [
    ...new Set([...summary.unavailableSources, ...summary.capabilities.unavailable]),
  ];
  return (
    <>
      <PageHeader appearance="standard"
        title="My work"
        description="Act, decide and follow progress across the workspace."
        action={
          <div className="ui-form-actions">
            <UiActionLink appearance="standard" variant="secondary" href={`${base}/calendar`}>
              Calendar
            </UiActionLink>
            <NewWork base={base} creation={summary.capabilities.creation} current={current} />
          </div>
        }
      />
      {error && <InlineNotice tone="info" role="status">{error} Updates may be out of date. </InlineNotice>}
      <div id="work-results" style={{ scrollMarginTop: "100px" }}>
        {!!unavailable.length && (
          <InlineNotice tone="info" role="status">
            Temporarily unavailable: {unavailable.join(", ")}. Your other work is shown.{" "}
            <Button appearance="standard" type="submit" variant="ghost" onClick={() => void refresh()}>
              Retry
            </Button>
          </InlineNotice>
        )}
        {view === "for-me" ? (
          <ForMe base={base} cards={summary.cards} capabilities={summary.capabilities} />
        ) : (
          <WidgetGrid pageKey="work">
            <FeatureWidget
              id="work-queue"
              title={workViews.find(([id]) => id === view)?.[1] ?? "Work queue"}
            >
              {view === "tracking" && (
                <nav className="ui-form-actions" aria-label="Tracking filters">
                  {[
                    ["all", "All tracking"],
                    ["requests", "My requests"],
                    ["waiting", "Waiting"],
                    ["completed", "Completed"],
                  ].map(([id, label]) => (
                    <Link
                      key={id}
                      data-slot="button" className={`button ui-standard-control ${tracking === id ? "secondary" : "ghost"}`}
                      href={viewHref("tracking", id)}
                      aria-current={tracking === id ? "page" : undefined}
                    >
                      {label}
                    </Link>
                  ))}
                </nav>
              )}
              <form className="ui-filter-bar" action={`${base}/work`}>
                <TextInput appearance="standard"
                  type="hidden"
                  name={view === "legacy" ? "queue" : "view"}
                  value={view === "legacy" ? queue : view}
                />
                {view === "tracking" && (
                  <TextInput appearance="standard" type="hidden" name="tracking" value={tracking} />
                )}
                <Select appearance="standard"
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
                        summary.relationships.includes(id as "assigned" | "approval" | "team"),
                    )
                    .map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                </Select>
                <TextInput appearance="standard"
                  className="input"
                  type="search"
                  name="q"
                  aria-label="Search work"
                  placeholder="Search your work…"
                  defaultValue={q}
                />
                <Select appearance="standard" className="input" name="type" aria-label="Work type" defaultValue={type}>
                  {workTypes
                    .filter(
                      ([id]) =>
                        (id === "all" || id === type || summary.availableTypes.includes(id)) &&
                        (!beta ||
                          [
                            "all",
                            "staff_task",
                            "expense",
                            "service_request",
                            "equipment_request",
                            "purchase_request",
                          ].includes(id)),
                    )
                    .map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                </Select>
                <Button appearance="standard" type="submit" variant="secondary">
                  Search
                </Button>
                {(q || type !== "all" || responsibility !== "all") && (
                  <UiActionLink appearance="standard"
                    variant="ghost"
                    href={`?${view === "legacy" ? "queue=" + queue : "view=" + view}${view === "tracking" && tracking !== "all" ? "&tracking=" + tracking : ""}`}
                  >
                    Clear filters
                  </UiActionLink>
                )}
              </form>
              {view === "routing" && (
                <p className="muted">
                  These workflows need an eligible responsible person or corrected approval setup.
                  Open the record and review its Flow before reassigning with a reason. Completed
                  decisions stay unchanged.{" "}
                  <Link href={`${base}/work/routing`}>Review Company source routing</Link>
                </p>
              )}
              {view === "team" && (
                <p className="muted">
                  On this page: {selected.filter((i) => i.category === "setup").length} setup items
                  · {selected.filter((i) => i.category !== "setup").length} team items
                </p>
              )}
              {view === "team" ? (
                [
                  ...new Set(
                    selected.map((i) =>
                      i.category === "setup"
                        ? (i.setupReason ?? "Setup needs an owner")
                        : "Team operations",
                    ),
                  ),
                ].map((reason) => (
                  <section key={reason}>
                    <h2>{reason}</h2>
                    <WorkList
                      base={base}
                      items={selected.filter(
                        (i) =>
                          (i.category === "setup"
                            ? (i.setupReason ?? "Setup needs an owner")
                            : "Team operations") === reason,
                      )}
                      returnTo={current}
                      onReview={setReview}
                    />
                  </section>
                ))
              ) : (
                <WorkList
                  base={base}
                  items={selected}
                  returnTo={current}
                  queue={
                    view === "completed"
                      ? "completed"
                      : view === "waiting" || view === "tracking"
                        ? "waiting"
                        : view === "actions" || view === "approvals"
                          ? "todo"
                          : queue
                  }
                  filtered={!!q || type !== "all" || responsibility !== "all"}
                  waiting={summary.viewCounts.waiting}
                  incomplete={!!error || !!unavailable.length}
                  onReview={setReview}
                />
              )}
              <nav className="ui-form-actions" aria-label="Work pagination">
                {summary.list.offset > 0 && (
                  <UiActionLink appearance="standard"
                    variant="secondary"
                    href={pageHref(Math.max(0, summary.list.offset - 30))}
                    prefetch={false}
                  >
                    Previous
                  </UiActionLink>
                )}
                <span>{summary.list.total} matching items</span>
                {summary.list.hasMore && (
                  <UiActionLink appearance="standard"
                    variant="secondary"
                    href={pageHref(summary.list.offset + 30)}
                    prefetch={false}
                  >
                    Next
                  </UiActionLink>
                )}
              </nav>
            </FeatureWidget>
          </WidgetGrid>
        )}
      </div>
      <WorkReviewPanel
        base={base}
        item={review}
        returnTo={current}
        onClose={() => setReview(null)}
        onDecided={async () => {
          setReview(null);
          await refresh();
        }}
      />
    </>
  );
}
