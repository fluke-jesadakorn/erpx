import Link from "@erpx/workplace/components/workspace-link";
import type { ReactNode } from "react";
import { ActionLink, Button, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { hubHref, queryValue, type HubQuery } from "@erpx/feature-sales/lib/navigation";
import { HubForm } from "@erpx/feature-sales/components/customers/hub-form";
import styles from "@erpx/feature-sales/components/customers/hub.module.css";

export function HubFilters({ query, placeholder, filters = [] }: { query: HubQuery; placeholder: string; filters?: { name: string; label: string; values: readonly (readonly [string, string])[] }[] }) {
  const controlled = new Set(["q", "cursor", "page", "new", "edit", "detailKind", "detailId", "detailArgs", "detailPage", ...filters.map(filter => filter.name)]);
  return <form className={styles.filters} role="search">
    {Object.entries(query).filter(([key, value]) => !controlled.has(key) && typeof value === "string").map(([key, value]) => <input key={key} type="hidden" name={key} value={String(value)} />)}
    <TextInput appearance="standard" type="search" name="q" aria-label={placeholder} placeholder={placeholder} defaultValue={queryValue(query, "q")} />
    {filters.map(filter => <Select appearance="standard" key={filter.name} name={filter.name} aria-label={filter.label} defaultValue={queryValue(query, filter.name)}>{filter.values.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select>)}
    <Button appearance="standard" variant="secondary" type="submit">Apply</Button>
  </form>;
}
export function HubTable({ label, columns, children }: { label: string; columns: string[]; children: ReactNode }) {
  return <TableFrame appearance="standard" label={label} className={styles.tableFrame}><Table className={styles.table}><thead><tr>{columns.map(column => <th key={column} scope="col">{column}</th>)}</tr></thead><tbody>{children}</tbody></Table></TableFrame>;
}
export function HubCreate({ base, hub, query, label, title = label, children }: { base: string; hub: "sales" | "customers"; query: HubQuery; label: string; title?: string; children?: ReactNode }) {
  return children ? <HubForm key={`${query.new}-${query.edit}-${query.orderId}`} title={title} label={label} initialOpen>{children}</HubForm> : <ActionLink appearance="standard" variant="primary" href={hubHref(base, hub, query, { new: "1", edit: undefined })} scroll={false}>{label}</ActionLink>;
}
export function HubCursor({ base, hub, query, page }: { base: string; hub: "sales" | "customers"; query: HubQuery; page: { items: unknown[]; nextCursor: string | null; previousCursor: string | null } }) {
  return <nav className={styles.pagination} aria-label="Pagination"><span>{page.items.length} records on this page</span>{query.cursor && <Link href={hubHref(base, hub, query, { cursor: undefined })}>First page</Link>}{page.previousCursor && <Link href={hubHref(base, hub, query, { cursor: page.previousCursor })}>Previous</Link>}{page.nextCursor && <Link href={hubHref(base, hub, query, { cursor: page.nextCursor })}>Next</Link>}</nav>;
}
export function hubPage<T>(items: T[], query: HubQuery) {
  const raw = Number(queryValue(query, "page", "1")), page = Number.isSafeInteger(raw) && raw > 0 ? raw : 1;
  return items.slice((page - 1) * 30, page * 30);
}
