import Link from "next/link";
import { ArrowUpRight, CircleCheck, CornerDownRight } from "lucide-react";
import type { WorkAttention as Attention } from "@/lib/work-attention";
import { workBlocker } from "@/lib/work-attention";
import styles from "./work-attention.module.css";

export function WorkAttention({ base, attention, compact = false, loading = false, deferred = false, retry, returnTo }: {
  base: string;
  attention: Attention;
  compact?: boolean;
  loading?: boolean;
  deferred?: boolean;
  retry?: () => void;
  returnTo?: string;
}) {
  return <section className={`${styles.attention} ${compact ? styles.compact : ""}`} aria-label="Needs attention">
    <header className={styles.header}>
      <div><h2>Needs attention <span>{loading ? "…" : deferred ? "—" : `${attention.total}${attention.incomplete ? "+" : ""}`}</span></h2><p>Across your workspace</p></div>
      <Link href={`${base}/work?queue=all`} aria-label="View all work"><ArrowUpRight size={18} aria-hidden /></Link>
    </header>
    {loading && <p className={styles.loading} role="status">Loading current work…</p>}
    {deferred && !loading && <p className={styles.loading}>Work is loaded only when you need it. {retry && <button onClick={retry}>Load priorities</button>}</p>}
    {!deferred && !loading && attention.incomplete && <p className={styles.notice} role="status">Some work may be missing or out of date. {retry && <button onClick={retry}>Retry work</button>}</p>}
    {!deferred && !loading && !!attention.items.length && <ul className={styles.items}>{attention.items.map(item => <li key={`${item.type}:${item.id}`}>
      <Link href={`${base}${item.href}${returnTo ? `?${new URLSearchParams({ return: returnTo })}` : ""}`}>
        <span className={styles.itemTitle}>{item.title}</span>
        <span className={styles.action} data-blocked={!!workBlocker(item)}>{workBlocker(item) ?? item.nextAction ?? item.detail}</span>
        {!compact && <span className={styles.owner}><CornerDownRight size={14} aria-hidden />{item.flow?.current?.primary?.name ?? item.flow?.current?.teamName ?? item.responsibleQueue ?? "Unassigned"}</span>}
      </Link>
    </li>)}</ul>}
    {!deferred && !loading && !attention.total && !attention.incomplete && <div className={styles.caughtUp}><CircleCheck size={20} aria-hidden /><div><strong>No actions for you right now</strong><p>{attention.waiting ? `${attention.waiting} ${attention.waiting === 1 ? "item is" : "items are"} with others.` : "You’re all caught up."}</p></div></div>}
    {!compact && <Link className={styles.allWork} href={`${base}/work?queue=all`}>Open Work <span aria-hidden>→</span></Link>}
  </section>;
}
