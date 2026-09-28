import Link from "next/link";
import { ArrowUpRight, CircleCheck, CornerDownRight } from "lucide-react";
import { workItemHref, workTypes } from "@/lib/work-item";
import { workTypeTone } from "@/lib/work-category-tone";
import type { WorkAttention as Attention } from "@/lib/work-attention";
import { workBlocker } from "@/lib/work-attention";
import styles from "./work-attention.module.css";

export function WorkAttention({ base, attention, compact = false, prominent = false, loading = false, deferred = false, retry, returnTo, snapshotAt = 0 }: {
  base: string;
  attention: Attention;
  compact?: boolean;
  prominent?: boolean;
  loading?: boolean;
  deferred?: boolean;
  retry?: () => void;
  returnTo?: string;
  snapshotAt?: number;
}) {
  return <section className={`${styles.attention} ${compact ? styles.compact : ""} ${prominent ? styles.prominent : ""}`} aria-label="Needs you">
    <header className={styles.header}>
      <div><h2>Needs you <span>{loading ? "…" : deferred ? "—" : `${attention.total}${attention.incomplete ? "+" : ""}`}</span></h2><p>Your next steps across the workspace</p></div>
      <Link href={`${base}/work?view=actions`} aria-label="View all work"><ArrowUpRight size={18} aria-hidden /></Link>
    </header>
    {loading && <p className={styles.loading} role="status">Loading current work…</p>}
    {deferred && !loading && <p className={styles.loading}>Work is loaded only when you need it. {retry && <button onClick={retry}>Load priorities</button>}</p>}
    {!deferred && !loading && attention.incomplete && <p className={styles.notice} role="status">Some work may be missing or out of date. {retry && <button onClick={retry}>Retry work</button>}</p>}
    {!deferred && !loading && !!attention.items.length && <ul className={styles.items}>{attention.items.map(item => <li key={`${item.type}:${item.id}`} data-tone={workTypeTone[item.type]}>
      <Link href={workItemHref(base, item, returnTo ?? "view=actions")}>
        <span className={styles.itemKind}>{workTypes.find(([type]) => type === item.type)?.[1] ?? item.type.replaceAll("_", " ")}</span>
        <span className={styles.itemTitle}>{item.title}</span>
        <span className={styles.action}>{item.nextAction ?? item.detail}</span>
        {workBlocker(item) && <span className={styles.blocker}>Blocked · {workBlocker(item)}</span>}
        {!compact && <span className={styles.owner}><CornerDownRight size={14} aria-hidden />{item.flow?.current?.primary?.name ?? item.flow?.current?.teamName ?? item.responsibleQueue ?? "Unassigned"}</span>}
        {!compact && item.dueAt && <span className={styles.due} data-overdue={snapshotAt > 0 && item.dueAt < snapshotAt}>{snapshotAt > 0 && item.dueAt < snapshotAt ? "Overdue" : "Due"} · {new Date(item.dueAt).toLocaleDateString("en-GB", { timeZone: "Asia/Bangkok" })}</span>}
      </Link>
    </li>)}</ul>}
    {!deferred && !loading && !attention.total && !attention.incomplete && <div className={styles.caughtUp}><CircleCheck size={20} aria-hidden /><div><strong>No actions for you right now</strong><p>{attention.waiting ? `${attention.waiting} ${attention.waiting === 1 ? "item is" : "items are"} with others.` : "You’re all caught up."}</p></div></div>}
    {!compact && <Link className={styles.allWork} href={`${base}/work?view=actions`}>View all actions <span aria-hidden>→</span></Link>}
  </section>;
}
