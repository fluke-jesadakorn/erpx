"use client";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import type { WorkSummary } from "@/server/work-summary";
import { queues } from "@/lib/work-queue";
import { filterWork, workResponsibilities, workTypes } from "@/lib/work-item";
import { PageHeader } from "@/components/ui/primitives";
import { WorkList } from "./work-list";
import { NewWork } from "./new-work";
import { WorkShortcuts } from "./work-shortcuts";
import styles from "./pages.module.css";

export function WorkDashboard({
  base,
  beta,
  initial,
  queue,
  type,
  q,
  responsibility,
  current,
}: {
  base: string;
  beta: boolean;
  initial: WorkSummary;
  queue: string;
  type: string;
  q: string;
  responsibility: string;
  current: string;
}) {
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
        const response = await fetch(`/api/v1${base}/work/summary`, {
          cache: "no-store",
          signal: abort.signal,
        });
        const body = (await response.json()) as {
          data: WorkSummary;
          error?: { message?: string };
        };
        if (!response.ok) {
          if ([401, 403, 404].includes(response.status))
            setSummary((s) => ({
              ...s,
              cards: [],
              items: [],
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
  }, [base]);
  useEffect(() => {
    const visible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    const restored = (event: PageTransitionEvent) => {
      if (event.persisted) void refresh();
    };
    window.addEventListener("focus", visible);
    document.addEventListener("visibilitychange", visible);
    window.addEventListener("pageshow", restored);
    window.addEventListener("erpx:work-changed", visible);
    // A restored App Router page can carry an older server snapshot.
    visible();
    return () => {
      controller.current?.abort();
      window.removeEventListener("focus", visible);
      document.removeEventListener("visibilitychange", visible);
      window.removeEventListener("pageshow", restored);
      window.removeEventListener("erpx:work-changed", visible);
    };
  }, [refresh]);
  const items = summary.items,
    selected = filterWork(items, queue, type, q, responsibility);
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
          <button className="button ghost" onClick={() => void refresh()}>
            Retry work
          </button>
        </p>
      )}
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
      <div id="work-results" style={{ scrollMarginTop: "100px" }}>
        <nav className={styles.tabs} aria-label="Work queues">
          {queues.map(([id, label]) => (
            <Link
              key={id}
              href={`?${new URLSearchParams({ queue: id, ...(responsibility !== "all" ? { responsibility } : {}), ...(type !== "all" ? { type } : {}), ...(q ? { q } : {}) })}`}
              aria-current={queue === id ? "page" : undefined}
            >
              {label}
              <span>
                {filterWork(items, id, type, "", responsibility).length}
              </span>
            </Link>
          ))}
        </nav>
        {!!unavailable.length && (
          <p role="status">
            Temporarily unavailable: {unavailable.join(", ")}. Your other work
            is shown.{" "}
            <button className="button ghost" onClick={() => void refresh()}>
              Retry
            </button>
          </p>
        )}
        <form className="work-filters" action={`${base}/work`}>
          <input type="hidden" name="queue" value={queue} />
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
                  items.some((i) =>
                    i.relationships?.includes(
                      id as "assigned" | "approval" | "team",
                    ),
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
          {(q || type !== "all") && (
            <Link className="button ghost" href={`?queue=${queue}`}>
              Clear filters
            </Link>
          )}
        </form>
        <WorkList base={base} items={selected} returnTo={current} />
      </div>
    </>
  );
}
