"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Dialog } from "@erpx/ui/components/dialog";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import Link from "@erpx/workplace/components/workspace-link";
import { useCallback, useEffect, useRef, useState } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { DailyPlanView } from "@erpx/server-work_projection/public/daily-plan";
import styles from "@erpx/feature-work_projection/components/today/daily-plan.module.css";

type PlanItem = DailyPlanView["items"][number];

function todayInBangkok() {
  const parts = new Intl.DateTimeFormat("en-US-u-ca-gregory", {
    timeZone: "Asia/Bangkok",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date());
  const value = (part: string) => parts.find((item) => item.type === part)?.value ?? "";
  return `${value("year")}-${value("month")}-${value("day")}`;
}

function shiftDate(date: string, days: number) {
  return new Date(Date.parse(`${date}T00:00:00Z`) + days * 86_400_000).toISOString().slice(0, 10);
}

function referenceKey(item: { type: string; id: string; variant: string }) {
  return JSON.stringify([item.type, item.id, item.variant]);
}

function draftKey(base: string, ownerKey: string, date: string) {
  return `erpx:daily-plan-draft:${base}:${ownerKey}:${date}`;
}

function timeValue(minute: number | null) {
  return minute === null
    ? ""
    : `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

function plannedFields(items: PlanItem[]) {
  return items.map(({ type, id, variant, startMinute, plannedMinutes, sourceUpdatedAt }) => ({
    type,
    id,
    variant,
    startMinute,
    plannedMinutes,
    sourceUpdatedAt,
  }));
}

async function parseResponse<T>(response: Response): Promise<T> {
  const body = (await response.json()) as { data?: T; error?: { message?: string } };
  if (!response.ok || body.data === undefined)
    throw new Error(body.error?.message ?? "The plan could not be loaded. Try again.");
  return body.data;
}

export function DailyPlan({ base, initialDate, initialOpen = false }: { base: string; initialDate?: string; initialOpen?: boolean }) {
  const [expanded, setExpanded] = useState(initialOpen);
  const [date, setDate] = useState(() =>
    initialDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDate) ? initialDate : todayInBangkok(),
  );
  const [view, setView] = useState<DailyPlanView | null>(null);
  const [items, setItems] = useState<PlanItem[]>([]);
  const [search, setSearch] = useState("");
  const [selection, setSelection] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const attempt = useRef<{ signature: string; key: string } | null>(null);

  const load = useCallback(
    async (selectedDate: string, signal?: AbortSignal) => {
      setLoading(true);
      setError("");
      setMessage("");
      setView(null);
      try {
        const response = await workFetch(
          `/api/v1${base}/daily-plan?date=${encodeURIComponent(selectedDate)}`,
          { signal },
        );
        const next = await parseResponse<DailyPlanView>(response);
        if (signal?.aborted) return;
        setView(next);
        let restored: PlanItem[] | null = null;
        try {
          const text = sessionStorage.getItem(draftKey(base, next.ownerKey, selectedDate));
          if (text) {
            const draft = JSON.parse(text) as { items?: unknown[] };
            if (Array.isArray(draft.items) && draft.items.length <= 40) {
              const visible = new Map(
                [...next.choices, ...next.items.map((item) => item.work)].map(
                  (work) => [referenceKey(work), work] as const,
                ),
              );
              restored = draft.items.flatMap((raw) => {
                if (!raw || typeof raw !== "object") return [];
                const item = raw as Record<string, unknown>;
                const work = visible.get(
                  referenceKey({
                    type: String(item.type),
                    id: String(item.id),
                    variant: String(item.variant),
                  }),
                );
                if (!work) return [];
                const startMinute =
                  item.startMinute === null || Number.isInteger(item.startMinute)
                    ? (item.startMinute as number | null)
                    : null;
                const plannedMinutes =
                  item.plannedMinutes === null || Number.isInteger(item.plannedMinutes)
                    ? (item.plannedMinutes as number | null)
                    : null;
                return [
                  {
                    type: work.type,
                    id: work.id,
                    variant: work.variant,
                    startMinute,
                    plannedMinutes,
                    sourceUpdatedAt: work.updatedAt,
                    work,
                  },
                ];
              });
            }
          }
        } catch {
          /* Invalid local drafts are ignored. */
        }
        setItems(restored ?? next.items);
        if (restored)
          setMessage("Your unsaved plan was restored on this device. Review it before saving.");
        setSelection("");
        attempt.current = null;
      } catch (cause) {
        if (signal?.aborted) return;
        setError(cause instanceof Error ? cause.message : "The plan could not be loaded.");
      } finally {
        if (!signal?.aborted) setLoading(false);
      }
    },
    [base],
  );

  useEffect(() => {
    const controller = new AbortController();
    const timer = setTimeout(() => void load(date, controller.signal), 0);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [date, load]);

  useEffect(() => {
    const openFromHash = () => {
      if (window.location.hash === "#plan") setExpanded(true);
    };
    const timer = setTimeout(openFromHash, 0);
    window.addEventListener("hashchange", openFromHash);
    window.addEventListener("erpx:open-plan", openFromHashOrAction);
    function openFromHashOrAction() {
      setExpanded(true);
    }
    return () => {
      clearTimeout(timer);
      window.removeEventListener("hashchange", openFromHash);
      window.removeEventListener("erpx:open-plan", openFromHashOrAction);
    };
  }, []);

  useEffect(() => {
    if (!view || view.date !== date) return;
    const key = draftKey(base, view.ownerKey, date);
    const changed =
      view.hiddenCount > 0 ||
      JSON.stringify(plannedFields(items)) !== JSON.stringify(plannedFields(view.items));
    try {
      if (changed) sessionStorage.setItem(key, JSON.stringify({ items: plannedFields(items) }));
      else sessionStorage.removeItem(key);
    } catch {
      /* Planning still works when browser storage is unavailable. */
    }
  }, [base, date, items, view]);

  const dirty =
    !!view &&
    view.date === date &&
    (view.hiddenCount > 0 ||
      JSON.stringify(plannedFields(items)) !== JSON.stringify(plannedFields(view.items)));
  const choices = (view?.choices ?? [])
    .filter(
      (choice) =>
        !items.some((item) => referenceKey(item) === referenceKey(choice)) &&
        `${choice.title} ${choice.detail}`.toLowerCase().includes(search.toLowerCase()),
    )
    .slice(0, 80);
  const totalMinutes = items.reduce((sum, item) => sum + (item.plannedMinutes ?? 0), 0);
  const dateLabel = new Intl.DateTimeFormat("en-GB-u-ca-gregory", {
    timeZone: "Asia/Bangkok",
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(`${date}T12:00:00Z`));

  function move(index: number, direction: -1 | 1) {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= items.length) return;
    setItems((current) => {
      const next = [...current];
      [next[index], next[nextIndex]] = [next[nextIndex], next[index]];
      return next;
    });
    setMessage("");
  }

  function changeItem(index: number, patch: Partial<PlanItem>) {
    setItems((current) => current.map((item, i) => (i === index ? { ...item, ...patch } : item)));
    setMessage("");
  }

  function add() {
    const work = view?.choices.find((choice) => referenceKey(choice) === selection);
    if (!work || items.some((item) => referenceKey(item) === selection)) return;
    setItems((current) => [
      ...current,
      {
        type: work.type,
        id: work.id,
        variant: work.variant,
        startMinute: null,
        plannedMinutes: null,
        sourceUpdatedAt: work.updatedAt,
        work,
      },
    ]);
    setSelection("");
    setSearch("");
    setMessage("");
  }

  async function save() {
    if (!view || view.date !== date || !dirty || saving) return;
    const payload = JSON.stringify({
      date,
      expectedVersion: view.version,
      items: plannedFields(items),
    });
    if (attempt.current?.signature !== payload)
      attempt.current = { signature: payload, key: crypto.randomUUID() };
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const response = await workFetch(`/api/v1${base}/daily-plan`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: payload,
      });
      await parseResponse<{ date: string; version: number }>(response);
      try {
        sessionStorage.removeItem(draftKey(base, view.ownerKey, date));
      } catch {
        /* Optional local backup. */
      }
      await load(date);
      setMessage("Plan saved. Work status and deadlines are unchanged.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The plan could not be saved.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className={styles.plan} id="plan" tabIndex={-1} aria-labelledby="daily-plan-title">
      <div className={styles.header}>
        <div>
          <h3 id="daily-plan-title">
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className={styles.toggle}
              aria-expanded={expanded}
              aria-controls="daily-plan-editor"
              onClick={() => setExpanded((open) => !open)}
            >
              Plan my day <span aria-hidden="true">{expanded ? "−" : "+"}</span>
            </Button>
          </h3>
          <p>
            {expanded
              ? "Choose your priorities and time blocks. Your plan is private to you."
              : loading
                ? "Loading your plan…"
                : `${dateLabel} · ${items.length} ${items.length === 1 ? "priority" : "priorities"} · ${totalMinutes} min planned${dirty ? " · Unsaved changes" : ""}`}
          </p>
        </div>

      </div>
      <Dialog appearance="standard" open={expanded} onClose={() => setExpanded(false)} title="Plan my day" finalFocus={() => document.querySelector<HTMLButtonElement>("#plan button")} presentation="form-drawer" dismissDisabled={saving}>{expanded && (
          <div className={styles.dateControls}>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              aria-label="Previous day"
              onClick={() => setDate(shiftDate(date, -1))}
            >
              ←
            </Button>
            <Field appearance="standard">
              Date{" "}
              <TextInput appearance="standard"
                type="date"
                value={date}
                onChange={(event) => {
                  if (/^\d{4}-\d{2}-\d{2}$/.test(event.target.value)) setDate(event.target.value);
                }}
              />
            </Field>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              aria-label="Next day"
              onClick={() => setDate(shiftDate(date, 1))}
            >
              →
            </Button>
          </div>
        )}<div id="daily-plan-editor" className={styles.editor}>
        {loading && (
          <InlineNotice tone="info" className={styles.muted} role="status">
            Loading your plan…
          </InlineNotice>
        )}
        {!loading && view && (
          <>
            {view.sourcesUnavailable && (
              <InlineNotice tone="info" className={styles.notice} role="status">
                Some work sources are temporarily unavailable. Refresh before changing affected
                work.
              </InlineNotice>
            )}
            {view.hiddenCount > 0 && (
              <InlineNotice tone="info" className={styles.notice} role="status">
                {view.hiddenCount} planned item{view.hiddenCount === 1 ? " is" : "s are"} no longer
                visible in this work role. Saving removes them from this plan.
              </InlineNotice>
            )}
            <ol className={styles.items}>
              {items.map((item, index) => (
                <li key={referenceKey(item)} className={styles.item}>
                  <div className={styles.itemTop}>
                    <span className={styles.priority}>Priority {index + 1}</span>
                    <div className={styles.move}>
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        aria-label={`Move ${item.work.title} up`}
                        disabled={index === 0}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </Button>
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        aria-label={`Move ${item.work.title} down`}
                        disabled={index === items.length - 1}
                        onClick={() => move(index, 1)}
                      >
                        ↓
                      </Button>
                      <Button appearance="standard"
                        variant="ghost"
                        type="button"
                        aria-label={`Remove ${item.work.title} from plan`}
                        onClick={() => {
                          setItems((current) => current.filter((_, i) => i !== index));
                          setMessage("");
                        }}
                      >
                        Remove
                      </Button>
                    </div>
                  </div>
                  <Link className={styles.workTitle} href={`${base}${item.work.href}`}>
                    {item.work.title}
                  </Link>
                  <p className={styles.workStatus}>
                    {item.work.status.replaceAll("_", " ")}
                    {item.work.dueAt !== null && item.work.dueAt !== undefined
                      ? ` · Due ${new Intl.DateTimeFormat("en-GB-u-ca-gregory", { timeZone: "Asia/Bangkok", day: "numeric", month: "short", year: "numeric" }).format(new Date(item.work.dueAt))}`
                      : ""}
                  </p>
                  <div className={styles.timeFields}>
                    <Field appearance="standard">
                      Start time{" "}
                      <TextInput appearance="standard"
                        type="time"
                        value={timeValue(item.startMinute)}
                        onChange={(event) => {
                          const value = event.target.value;
                          changeItem(index, {
                            startMinute: value
                              ? Number(value.slice(0, 2)) * 60 + Number(value.slice(3, 5))
                              : null,
                            plannedMinutes:
                              value && item.plannedMinutes === null ? 60 : item.plannedMinutes,
                          });
                        }}
                      />
                    </Field>
                    <Field appearance="standard">
                      Minutes planned{" "}
                      <TextInput appearance="standard"
                        type="number"
                        min="1"
                        max="1440"
                        step="15"
                        value={item.plannedMinutes ?? ""}
                        onChange={(event) =>
                          changeItem(index, {
                            plannedMinutes: event.target.value ? Number(event.target.value) : null,
                            startMinute: event.target.value ? item.startMinute : null,
                          })
                        }
                      />
                    </Field>
                  </div>
                </li>
              ))}
            </ol>
            {!items.length && (
              <p className={styles.empty}>
                No work planned yet. Add a task or request you can access.
              </p>
            )}
            <div className={styles.add}>
              <Field appearance="standard">
                Find work{" "}
                <TextInput appearance="standard"
                  type="search"
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search your work"
                />
              </Field>
              <Field appearance="standard">
                Work item{" "}
                <Select appearance="standard" value={selection} onChange={(event) => setSelection(event.target.value)}>
                  <option value="">Choose work to plan</option>
                  {choices.map((choice) => (
                    <option key={referenceKey(choice)} value={referenceKey(choice)}>
                      {choice.title} · {choice.status.replaceAll("_", " ")}
                    </option>
                  ))}
                </Select>
              </Field>
              <Button appearance="standard" variant="ghost" type="button" disabled={!selection} onClick={add}>
                Add to day
              </Button>
            </div>
            <div className={styles.footer}>
              <p>
                {items.length} item{items.length === 1 ? "" : "s"} · {totalMinutes} min planned
              </p>
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={!dirty || saving}
                onClick={() => void save()}
              >
                {saving ? "Saving…" : "Save plan"}
              </Button>
            </div>
          </>
        )}
      </div></Dialog>
      {error && (
        <InlineNotice tone="danger" className={styles.error} role="alert">
          {error}{" "}
          <Button appearance="standard" variant="ghost" type="button" onClick={() => void load(date)}>
            Reload plan
          </Button>
        </InlineNotice>
      )}
      {message && (
        <InlineNotice tone="info" className={styles.success} role="status">
          {message}
        </InlineNotice>
      )}
    </section>
  );
}
