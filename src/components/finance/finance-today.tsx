import Link from "next/link";
import { ArrowRight, CheckCircle2 } from "lucide-react";
import { formatMoneyUnits } from "@/lib/company-money";
import { financeWork, type FinanceWorkView } from "@/lib/finance-work";
import { workItemHref } from "@/lib/work-item";
import type { FinanceAttention } from "@/server/finance-workspace";
import type { WorkSnapshot } from "@/server/work-items";
import styles from "./finance-workspace.module.css";

export function FinanceToday({ base, attention, work, view, query, page = 1 }: { base: string; attention: FinanceAttention[]; work: WorkSnapshot; view: FinanceWorkView; query: string; page?: number }) {
  const items = financeWork(work.items, view, query);
  const currentPage = Number.isSafeInteger(page) && page > 0 ? page : 1;
  const rows = items.slice((currentPage - 1) * 20, currentPage * 20);
  const href = (values: Record<string, string | number>) => `${base}/finance?${new URLSearchParams(Object.entries(values).map(([key, value]) => [key, String(value)]))}`;
  const returnTo = href({ view, q: query, page: currentPage });
  const signals = attention.filter(item => ["receivable-overdue", "payable-due", "payment-review", "bank-unmatched"].includes(item.id));
  return <>
    <section className={styles.section} aria-labelledby="finance-work-title">
      <div className={styles.sectionHead}><div><h2 id="finance-work-title">Your next steps</h2><p className="muted">Each row is one record. Its current workflow determines who can act.</p></div></div>
      <nav className={styles.workViews} aria-label="Finance responsibilities">{([["actions", "My actions"], ["waiting", "Waiting"], ["team", "Team & setup"]] as const).map(([key, label]) => <Link key={key} href={href({ view: key, q: query })} aria-current={view === key ? "page" : undefined}>{label}<span>{financeWork(work.items, key).length}</span></Link>)}</nav>
      <form className="business-form" method="get"><input type="hidden" name="view" value={view}/><label>Find work<input name="q" defaultValue={query} placeholder="Document, party or next step"/></label><button className="button secondary" type="submit">Search</button>{query && <Link href={href({ view })}>Clear search</Link>}</form>
      {work.unavailableSources.length > 0 && <p className="alert" role="status">Some work sources are temporarily unavailable. These counts may be incomplete. Open the owning page to continue or refresh this view.</p>}
      {rows.length ? <div className={styles.priorityList}>{rows.map(item => <Link className={styles.priority} key={`${item.type}:${item.id}`} href={workItemHref(base, item, returnTo)}>
        <div><span className="eyebrow">{item.type.replaceAll("_", " ")} · {item.status.replaceAll("_", " ")}</span><h3>{item.title}</h3><p>{item.nextAction ?? item.detail}</p><small className="muted">{item.responsibleQueue ?? item.flow?.current?.teamName ?? "Open record for responsibility"}{item.blocker ? ` · ${item.blocker}` : ""}</small></div><ArrowRight size={18} aria-hidden="true"/>
      </Link>)}</div> : <div className={styles.empty}><CheckCircle2 size={24} aria-hidden="true"/><h3>{query ? "No work matches this search" : view === "actions" ? "You have no pending Finance actions" : "No records in this view"}</h3><p>Use Documents, Cash or Accounting to review Company records.</p></div>}
      {items.length > 20 && <nav className="form-actions" aria-label="Finance work pages">{currentPage > 1 && <Link className="button secondary" href={href({ view, q: query, page: currentPage - 1 })}>Previous</Link>}<span className="muted">Page {currentPage} · {items.length} records</span>{currentPage * 20 < items.length && <Link className="button secondary" href={href({ view, q: query, page: currentPage + 1 })}>Next</Link>}</nav>}
    </section>
    <section className={styles.section} aria-labelledby="finance-cash-status-title"><h2 id="finance-cash-status-title">Company cash & reconciliation</h2>
    <section className={styles.summaryStrip} aria-label="Company cash and reconciliation status">{signals.map(item => <Link key={item.id} href={`${base}/${item.href}`}>
      <span className="muted">{item.label}</span><strong>{item.count}</strong>
      <small>{item.amounts?.length ? item.amounts.map(amount => formatMoneyUnits(String(amount.amountMinor), amount.currency, 2)).join(" · ") : item.count ? "Open records →" : "No outstanding items"}</small>
    </Link>)}</section>
    </section>
    {attention.some(item => item.id === "period-close" && item.count > 0) && <section className={styles.panel}><div className={styles.sectionHead}><div><h2>Period close needs attention</h2><p className="muted">Review blockers and retained evidence before independent approval.</p></div><Link href={`${base}/finance/close`}>Open period close →</Link></div></section>}
  </>;
}
