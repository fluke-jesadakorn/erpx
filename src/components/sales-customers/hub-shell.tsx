import Link from "next/link";
import type { ReactNode } from "react";
import { PageContent } from "@/components/ui/foundation";
import { hubHref, salesViews, customersViews, type HubQuery } from "@/server/modules/sales/public/rules/navigation";
import styles from "./hub.module.css";

export function HubShell({ base, hub, view, access, actions, children, reviewCount }: {
  base: string; hub: "sales" | "customers"; view: string;
  access: Record<string, boolean>; actions?: ReactNode; children: ReactNode; reviewCount?: number | null;
}) {
  const tabs = hub === "sales" ? salesViews : customersViews;
  return <PageContent className={styles.workspace}>
    <header className={styles.header}>
      <div><p className={styles.eyebrow}>Customers &amp; sales</p><h1>{hub === "sales" ? "Sales" : "Customers"}</h1><p className={styles.description}>{hub === "sales" ? "Move the next conversation and sale forward." : "Keep customer information, conversations and service connected."}</p></div>
      <div className={styles.actions}>
        {hub === "sales" && access.reports && <Link className={styles.tool} href={hubHref(base, hub, { view: "reports" })} aria-current={view === "reports" ? "page" : undefined}>Reports</Link>}
        {hub === "sales" && access.pricing && <Link className={styles.tool} href={hubHref(base, hub, { view: "pricing" })} aria-current={view === "pricing" ? "page" : undefined}>Price lists</Link>}
        {hub === "customers" && access.reviews && <Link className={styles.tool} href={hubHref(base, hub, { view: "reviews" })} aria-current={view === "reviews" ? "page" : undefined}>Review customer data{reviewCount !== null && reviewCount !== undefined && reviewCount > 0 && <span className={styles.count}>{reviewCount}</span>}</Link>}
        {actions}
      </div>
    </header>
    <nav className={styles.tabs} aria-label={`${hub === "sales" ? "Sales" : "Customers"} views`}>
      {tabs.filter(([id]) => access[id]).map(([id, label]) => <Link key={id} href={hubHref(base, hub, { view: id })} aria-current={view === id ? "page" : undefined}>{label}</Link>)}
    </nav>
    <div className={styles.content}>{children}</div>
  </PageContent>;
}
export function HubSegments({ base, hub, query, name, values, selected, label }: { base: string; hub: "sales" | "customers"; query: HubQuery; name: string; values: readonly (readonly [string, string])[]; selected: string; label: string }) {
  return <nav className={styles.segments} aria-label={label}>{values.map(([value, text]) => <Link key={value} href={hubHref(base, hub, query, { [name]: value, cursor: undefined, page: undefined, new: undefined, edit: undefined })} aria-current={value === selected ? "page" : undefined}>{text}</Link>)}</nav>;
}
export function HubEmpty({ title, children }: { title: string; children?: ReactNode }) {
  return <div className={styles.empty}><h2>{title}</h2>{children && <p>{children}</p>}</div>;
}
export function HubPagination({ base, hub, query, count, total }: { base: string; hub: "sales" | "customers"; query: HubQuery; count: number; total: number }) {
  const raw = Number(query.page), page = Number.isSafeInteger(raw) && raw > 0 ? raw : 1;
  return <nav aria-label="Pagination" className={styles.pagination}>
    <span>{total} {total === 1 ? "record" : "records"}</span>
    {page > 1 && <Link href={hubHref(base, hub, query, { page: String(page - 1) })}>Previous</Link>}
    {(page - 1) * 30 + count < total && <Link href={hubHref(base, hub, query, { page: String(page + 1) })}>Next</Link>}
  </nav>;
}
