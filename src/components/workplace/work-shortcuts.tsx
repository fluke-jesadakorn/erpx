"use client";

import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { useLayoutEffect, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpRight,
  BriefcaseBusiness,
  CheckCheck,
  ClipboardList,
  Headset,
  Laptop,
  PackageCheck,
  Plus,
  Receipt,
  ShoppingCart,
  Truck,
  Users,
  CalendarDays,
} from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import { useHydrated } from "@/components/ui/use-hydrated";
import {
  defaultWorkCardIds,
  newWorkHref,
  workCardCatalog,
  type WorkActionCard,
  type WorkCardId,
  type WorkCardPreferences,
} from "@/lib/work-cards";
import type { WorkSummary } from "@/server/work-summary";
import styles from "./work-cards.module.css";

const icons = {
  tasks: ClipboardList,
  expenses: Receipt,
  leave: CalendarDays,
  services: Headset,
  equipment: Laptop,
  purchases: ShoppingCart,
  approvals: CheckCheck,
  team: Users,
  counts: PackageCheck,
  transfers: Truck,
};
export function WorkShortcuts({
  base,
  current,
  summary,
  refresh,
  onSaved,
}: {
  base: string;
  current: string;
  summary: WorkSummary;
  refresh: () => Promise<void>;
  onSaved: (preferences: WorkCardPreferences) => void;
}) {
  const ready = useHydrated(),
    [open, setOpen] = useState(false),
    [all, setAll] = useState(false),
    [expanded, setExpanded] = useState(false);
  const pinned =
    summary.preferences.pinnedIds ?? defaultWorkCardIds(summary.cards);
  const cards = all
    ? summary.cards
    : pinned.flatMap((id) => summary.cards.filter((c) => c.id === id));
  const expandButton = useRef<HTMLButtonElement>(null);
  return (
    <section className={styles.shortcuts} aria-label="Your shortcuts">
      <div className={styles.header}>
        <div>
          <h2>Your shortcuts</h2>
          <p>Your everyday actions.</p>
        </div>
        <div className={styles.buttons}>
          <button
            className="button ghost"
            disabled={!ready || summary.preferencesUnavailable}
            onClick={() => setOpen(true)}
          >
            Customize
          </button>
          <button
            className="button secondary"
            disabled={!ready}
            aria-expanded={all}
            onClick={() => setAll((v) => !v)}
          >
            {all ? "Pinned shortcuts" : "All actions"}
          </button>
        </div>
      </div>
      {summary.preferencesUnavailable && (
        <p role="status">
          Your saved shortcuts are unavailable. Showing the default arrangement.{" "}
          <button className="button ghost" onClick={() => void refresh()}>
            Retry shortcuts
          </button>
        </p>
      )}
      {!cards.length && (
        <p className={styles.empty}>
          No shortcuts pinned for this workspace. Use All actions or Customize
          to choose what appears here.
        </p>
      )}
      <div
        className={`${styles.grid} ${!all && !expanded ? styles.collapsed : ""}`}
      >
        {cards.map((card) => (
          <ShortcutCard
            key={card.id}
            base={base}
            current={current}
            card={card}
          />
        ))}
      </div>
      {!all && cards.length > 4 && (
        <button
          ref={expandButton}
          className={`button ghost ${styles.mobileMore}`}
          aria-expanded={expanded}
          onClick={() => {
            setExpanded(!expanded);
            expandButton.current?.focus();
          }}
        >
          {expanded
            ? "Show fewer shortcuts"
            : `Show all ${cards.length} shortcuts`}
        </button>
      )}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Customize shortcuts"
      >
        <ShortcutEditor
          base={base}
          cards={summary.cards}
          initial={summary.preferences}
          defaults={defaultWorkCardIds(summary.cards)}
          onSaved={onSaved}
          close={() => setOpen(false)}
        />
      </Dialog>
    </section>
  );
}
function ShortcutCard({
  base,
  current,
  card,
}: {
  base: string;
  current: string;
  card: WorkActionCard;
}) {
  const Icon = icons[card.icon] ?? BriefcaseBusiness;
  return (
    <article className={styles.card} aria-label={card.title}>
      <Link className={styles.mainLink} href={card.href}>
        <div className={styles.cardHead}>
          <span className={styles.icon}>
            <Icon size={21} aria-hidden />
          </span>
          <ArrowUpRight size={17} aria-hidden />
        </div>
        <h3>{card.title}</h3>
        {card.incomplete ? (
          <p className={styles.partial}>Summary incomplete</p>
        ) : (
          <p className={styles.counts}>
            {!!card.toDo && <span><strong>{card.toDo}</strong> To do</span>}
            {!!card.waiting && <span><strong>{card.waiting}</strong> Waiting</span>}
            {!card.toDo && !card.waiting && <span>No pending work</span>}
          </p>
        )}
        {card.blocked > 0 && (
          <p className={styles.blocked}>
            {card.blocked} {card.blocked === 1 ? "item needs" : "items need"}{" "}
            attention
          </p>
        )}
        <span className={styles.openLabel}>
          View work <span aria-hidden>→</span>
        </span>
      </Link>
      {card.create && (
        <Link
          className={styles.newLink}
          aria-label={
            card.create.kind === "conversation"
              ? card.create.label
              : `New ${card.create.label.toLowerCase()}`
          }
          href={newWorkHref(base, card.create, current)}
        >
          <Plus size={16} aria-hidden />
          {card.create.kind === "conversation"
            ? "Start from a conversation"
            : "New"}
        </Link>
      )}
    </article>
  );
}
function ShortcutEditor({
  base,
  cards,
  initial,
  defaults,
  close,
  onSaved,
}: {
  base: string;
  cards: WorkActionCard[];
  initial: WorkCardPreferences;
  defaults: WorkCardId[];
  close: () => void;
  onSaved: (p: WorkCardPreferences) => void;
}) {
  const [ids, setIds] = useState<WorkCardId[]>(initial.pinnedIds ?? defaults),
    [version, setVersion] = useState(initial.version),
    [reset, setReset] = useState(false);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [conflict, setConflict] = useState(false),
    [reviewed, setReviewed] = useState(false);
  const [latest, setLatest] = useState<WorkCardPreferences | null>(null);
  const request = useRef<{ body: string; key: string } | null>(null);
  const editor = useRef<HTMLDivElement>(null),
    focusTarget = useRef<WorkCardId | "available" | null>(null);
  useLayoutEffect(() => {
    const target = focusTarget.current;
    if (!target) return;
    focusTarget.current = null;
    const selector =
      target === "available"
        ? "[data-available] button:not(:disabled), [data-reset]"
        : `[data-card-id="${target}"] button:not(:disabled)`;
    editor.current?.querySelector<HTMLButtonElement>(selector)?.focus();
  }, [ids]);
  const title = (id: WorkCardId) =>
    workCardCatalog.find((c) => c.id === id)!.title;
  function change(next: WorkCardId[], target?: WorkCardId | "available") {
    focusTarget.current = target ?? null;
    setIds(next);
    setReset(false);
  }
  function move(index: number, offset: number) {
    const next = [...ids];
    [next[index], next[index + offset]] = [next[index + offset], next[index]];
    change(next, ids[index]);
  }
  async function readLatest() {
    const response = await workFetch(`/api/v1${base}/work/card-preferences`, {
      cache: "no-store",
    });
    const body = (await response.json()) as {
      data: WorkCardPreferences;
      error?: { message?: string };
    };
    if (!response.ok)
      throw Error(
        body.error?.message ?? "Could not load the latest shortcuts.",
      );
    setLatest(body.data);
    setVersion(body.data.version);
    setReviewed(false);
  }
  async function save() {
    setBusy(true);
    setError("");
    const body = JSON.stringify({
      pinnedIds: reset ? null : ids,
      expectedVersion: version,
    });
    if (request.current?.body !== body)
      request.current = { body, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/work/card-preferences`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": request.current.key,
        },
        body,
      });
      const result = (await response.json()) as {
        data: WorkCardPreferences;
        error?: { message?: string };
      };
      if (!response.ok) {
        if (response.status === 409) {
          setConflict(true);
          setReviewed(false);
          await readLatest();
        }
        throw Error(result.error?.message ?? "Could not save your shortcuts.");
      }
      onSaved(result.data);
      close();
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Could not save. Your arrangement is preserved.",
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <div ref={editor} className={styles.editor}>
      <p>
        Choose up to six shortcuts. Your order is saved across devices for this
        company.
      </p>
      <fieldset disabled={busy}>
        <legend>Pinned shortcuts · {ids.length}/6</legend>
        {!ids.length && <p>No shortcuts selected.</p>}
        <ol className={styles.pinList}>
          {ids.map((id, index) => (
            <li key={id} data-card-id={id}>
              <div>
                <strong>{title(id)}</strong>
                {!cards.some((c) => c.id === id) && (
                  <small>Kept for another scope or when available again.</small>
                )}
              </div>
              <div className={styles.buttons}>
                <button
                  className="icon-button"
                  type="button"
                  disabled={index === 0}
                  aria-label={`Move ${title(id)} up`}
                  onClick={() => move(index, -1)}
                >
                  <ArrowUp size={17} />
                </button>
                <button
                  className="icon-button"
                  type="button"
                  disabled={index === ids.length - 1}
                  aria-label={`Move ${title(id)} down`}
                  onClick={() => move(index, 1)}
                >
                  <ArrowDown size={17} />
                </button>
                <button
                  className="button ghost"
                  type="button"
                  aria-label={`Unpin ${title(id)}`}
                  onClick={() =>
                    change(
                      ids.filter((x) => x !== id),
                      ids[index + 1] ?? ids[index - 1] ?? "available",
                    )
                  }
                >
                  Unpin
                </button>
              </div>
            </li>
          ))}
        </ol>
        <h3>Available actions</h3>
        <div className={styles.choices} data-available>
          {cards
            .filter((c) => !ids.includes(c.id))
            .map((c) => (
              <button
                type="button"
                className="button secondary"
                key={c.id}
                disabled={ids.length >= 6}
                onClick={() => change([...ids, c.id], c.id)}
              >
                Pin {c.title}
              </button>
            ))}
        </div>
        <button
          className="button ghost"
          type="button"
          data-reset
          onClick={() => {
            setIds(defaults);
            setReset(true);
          }}
        >
          Reset to default
        </button>
        {reset && <p role="status">Save to use the default arrangement.</p>}
      </fieldset>
      {error && <p role="alert">{error}</p>}
      {conflict && (
        <div className={styles.conflict}>
          <p>Your proposed arrangement is preserved.</p>
          {latest ? (
            <>
              <p>
                Latest saved arrangement:{" "}
                {(latest.pinnedIds ?? defaults).map(title).join(", ") ||
                  "No pinned shortcuts"}
                .
              </p>
              <label>
                <input
                  type="checkbox"
                  checked={reviewed}
                  onChange={(e) => setReviewed(e.target.checked)}
                />{" "}
                I reviewed the latest arrangement and want to save mine
              </label>
            </>
          ) : (
            <button
              className="button secondary"
              onClick={() =>
                void readLatest().catch((e) => setError(String(e.message)))
              }
            >
              Load latest arrangement
            </button>
          )}
        </div>
      )}
      <div className={styles.buttons}>
        <button
          className="button"
          disabled={busy || (conflict && (!latest || !reviewed))}
          onClick={() => void save()}
        >
          {busy ? "Saving…" : "Save"}
        </button>
        <button className="button secondary" disabled={busy} onClick={close}>
          Cancel
        </button>
      </div>
    </div>
  );
}
