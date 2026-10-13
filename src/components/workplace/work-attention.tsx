import { InlineNotice } from "@/components/ui/foundation";
import { Button } from "@/components/ui/foundation";
import Link from "next/link";
import { ArrowUpRight, CircleCheck, CornerDownRight } from "lucide-react";
import { workItemHref, workTypes } from "@/lib/work_projection/work-item";
import { workTypeTone } from "@/lib/work_projection/work-category-tone";
import type { WorkAttention as Attention } from "@/lib/work_projection/work-attention";
import { workBlocker } from "@/lib/work_projection/work-attention";
import styles from "./work-attention.module.css";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";

export function WorkAttention({
  base,
  attention,
  compact = false,
  rail = false,
  prominent = false,
  loading = false,
  updating = false,
  deferred = false,
  retry,
  returnTo,
  snapshotAt = 0,
}: {
  base: string;
  attention: Attention;
  compact?: boolean;
  rail?: boolean;
  prominent?: boolean;
  loading?: boolean;
  updating?: boolean;
  deferred?: boolean;
  retry?: () => void;
  returnTo?: string;
  snapshotAt?: number;
}) {
  return (
    <section
      className={`${styles.attention} ${compact ? styles.compact : ""} ${rail ? styles.rail : ""} ${prominent ? styles.prominent : ""}`}
      aria-label="Needs you"
      data-attention-state={
        loading || deferred || attention.incomplete
          ? "unknown"
          : attention.total > 0 ? "pending" : "caught-up"
      }
    >
      <header className={styles.header}>
        <div>
          <h2>
            Needs you{" "}
            <span>
              {loading
                ? "…"
                : deferred
                  ? "—"
                  : `${attention.total}${attention.incomplete ? "+" : ""}`}
            </span>
          </h2>
          <p>Your next steps across the workspace</p>
        </div>
        <Link href={`${base}/work?view=actions`} aria-label="View all work">
          <ArrowUpRight size={18} aria-hidden />
        </Link>
      </header>
      {updating && <p className={styles.loading} role="status">Updating current work…</p>}
      {loading && (
        <InlineNotice tone="info" className={styles.loading} role="status">
          Loading current work…
        </InlineNotice>
      )}
      {deferred && !loading && (
        <p className={styles.loading}>
          Work is loaded only when you need it.{" "}
          {retry && (
            <Button appearance="standard" type="submit" variant="ghost" onClick={retry}>
              Load priorities
            </Button>
          )}
        </p>
      )}
      {!deferred && !loading && attention.incomplete && (
        <InlineNotice tone="info" className={styles.notice} role="status">
          Some work may be missing or out of date.{" "}
          {retry && (
            <Button appearance="standard" type="submit" variant="ghost" onClick={retry}>
              Retry work
            </Button>
          )}
        </InlineNotice>
      )}
      {!deferred && !loading && !!attention.items.length && (
        <ul className={styles.items}>
          {attention.items.map((item) => (
            <li key={`${item.type}:${item.id}`} data-tone={workTypeTone[item.type]}>
              <Link href={workItemHref(base, item, returnTo ?? "view=actions")}>
                <span className={styles.itemKind}>
                  {workTypes.find(([type]) => type === item.type)?.[1] ??
                    item.type.replaceAll("_", " ")}
                </span>
                <span className={styles.itemTitle}>{item.title}</span>
                <span className={styles.action}>{item.nextAction ?? item.detail}</span>
                {workBlocker(item) && (
                  <span className={styles.blocker}>Blocked · {workBlocker(item)}</span>
                )}
                {!compact && (
                  <span className={styles.owner}>
                    <CornerDownRight size={14} aria-hidden />
                    {item.flow?.current?.primary?.name ??
                      item.flow?.current?.teamName ??
                      item.responsibleQueue ??
                      "Unassigned"}
                  </span>
                )}
                {!compact && item.dueAt && (
                  <span
                    className={styles.due}
                    data-overdue={snapshotAt > 0 && item.dueAt < snapshotAt}
                  >
                    {snapshotAt > 0 && item.dueAt < snapshotAt ? "Overdue" : "Due"} ·{" "}
                    <TimeText appearance="standard" value={item.dueAt} />
                  </span>
                )}
              </Link>
            </li>
          ))}
        </ul>
      )}
      {!deferred && !loading && !attention.total && !attention.incomplete && (
        <div className={styles.caughtUp}>
          <CircleCheck size={20} aria-hidden />
          <div>
            <strong>No actions for you right now</strong>
            <p>
              {attention.waiting
                ? `${attention.waiting} ${attention.waiting === 1 ? "item is" : "items are"} with others.`
                : "You’re all caught up."}
            </p>
          </div>
        </div>
      )}
      {!compact && (
        <Link className={styles.allWork} href={`${base}/work?view=actions`}>
          View all actions <span aria-hidden>→</span>
        </Link>
      )}
    </section>
  );
}
