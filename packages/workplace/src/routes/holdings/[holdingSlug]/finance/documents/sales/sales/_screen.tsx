import {DestinationFilterForm} from "@erpx/ui/components/destination-filter-form";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import Link from "@erpx/workplace/components/workspace-link";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { SalesInvoiceEditor, IssueCustomerReceipt } from "@erpx/feature-finance/components/sales-controls";
import { Button, Card, Field, Select, TextInput, Table, TableFrame, InlineNotice, FormActions } from "@erpx/ui/components/foundation";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {salesWorkspace,salesDefaultsWorkspace} from '@erpx/server-finance/public/sales';
import {commerceOverview} from '@erpx/server-commerce/public/commerce';
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { ApiError } from "@erpx/server-runtime/public/http";
import styles from "@erpx/feature-finance/components/sales-workspace.module.css";

export default async function SalesPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | undefined>>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), search = await searchParams;
    const data = await salesWorkspace(actor, search).catch(error => pageError(error, holdingSlug));
    const tab = data.query.tab, local = !!actor.branchId || !!actor.departmentId;
    const setup = !local ? await salesDefaultsWorkspace(actor) : null;
    const approved = setup?.editions.filter(row => setup.usableDefaults.includes(row.id)) ?? [];
    const canSales = hasPermission(actor, "commerce.sales"), canInvoice = hasPermission(actor, "finance.write") && !local;
    let orderOptions = null, commerceSetupRequired = false;
    if (search.new && ["quotations", "orders"].includes(tab) && canSales) {
        try {
            orderOptions = await commerceOverview(actor);
        }
        catch (error) {
            if (error instanceof ApiError && error.code === "COMMERCE_SETUP_REQUIRED")
                commerceSetupRequired = true;
            else
                return pageError(error, holdingSlug);
        }
    }
    const url = (change: Record<string, string>) => {
        const query = new URLSearchParams(Object.entries(search).filter((pair): pair is [
            string,
            string
        ] => !!pair[1]));
        query.delete("cursor");
        query.delete("new");
        for (const [key, value] of Object.entries(change)) {
            if (value)
                query.set(key, value);
            else
                query.delete(key);
        }
        return canonicalUiHref(`${base}/finance/sales?${query}`);
    };
    const statuses = ["draft", "submitted", "confirmed", "partially_fulfilled", "fulfilled", "converted", "cancelled"];
    const rows = data.page?.items ?? [];
    const metric = (label: string, kind: "awaiting" | "overdue" | "received") => <Card as="section" appearance="standard" className={styles.metric}><h2>{label}</h2>{(kind === "received" ? data.received : data.summary).length ? (kind === "received" ? data.received : data.summary).map(row => <Link key={String(row.currency)} href={url({ tab: kind === "received" ? "receipts" : "invoices", currency: String(row.currency), queue: kind, status: "" })}><span>{String(row.currency)}</span><span>{minorToDecimal(Number(row[`${kind === "awaiting" ? "awaiting" : kind}_minor`]))}</span></Link>) : <span>—</span>}</Card>;
    return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} active="finance-sales" title="Sales & collections" description="Quote, deliver, invoice and collect customer payments." actions={<Link href={canonicalUiHref(`${base}/finance/sales/settings`)}>Sales settings</Link>}>
    <div className={styles.workspace}><div className={styles.metrics}>{metric("Awaiting payment", "awaiting")}{metric("Overdue", "overdue")}{metric("Received", "received")}</div>

      <DestinationFilterForm className={styles.filters}><input type="hidden" name="tab" value={tab}/><Field appearance="standard">Search<TextInput appearance="standard" name="q" placeholder="Customer or document number" defaultValue={search.q ?? ""}/></Field><Field appearance="standard">From<TextInput appearance="standard" type="date" name="startDate" defaultValue={search.startDate}/></Field><Field appearance="standard">To<TextInput appearance="standard" type="date" name="endDate" defaultValue={search.endDate}/></Field><Field appearance="standard">Status<Select appearance="standard" name="status" defaultValue={search.status ?? ""}><option value="">All statuses</option>{statuses.map(status => <option key={status} value={status}>{status.replaceAll("_", " ")}</option>)}</Select></Field><Button appearance="standard" type="submit">Search</Button></DestinationFilterForm>
      {(search.queue || search.currency) && <p>Showing {search.queue ?? "all documents"} · {search.currency ?? "all currencies"} <Link href={url({ queue: "", currency: "" })}>Clear amount filter</Link></p>}
      {!local && !approved.length && <InlineNotice tone="warning">Sales account defaults need independent Finance approval. <Link href={canonicalUiHref(`${base}/finance/sales/settings`)}>Configure sales defaults</Link>. <Link href={canonicalUiHref(`${base}/finance/setup`)}>Open Finance setup</Link>.</InlineNotice>}
      {setup?.issues.map(issue => <InlineNotice key={issue.currency} tone="warning">{issue.currency}: {issue.message} <Link href={canonicalUiHref(`${base}/finance/setup`)}>Review accounts</Link> · <Link href={canonicalUiHref(`${base}/finance/sales/settings`)}>Sales defaults</Link></InlineNotice>)}
      {false}
      <FormActions>{canSales && <Link href={url({ tab: "quotations", new: "quotation" })}>New quotation</Link>}{canSales && true && <Link href={url({ new: "order" })}>New sales order</Link>}{canInvoice && approved.length > 0 && false && <Link href={url({ new: "invoice" })}>Invoice completed services</Link>}<Link href={canonicalUiHref(`${base}/commerce?view=products`)}>Items & services</Link><Link href={canonicalUiHref(`${base}/commerce?view=parties&kind=customer`)}>Customers</Link></FormActions>
      {data.permissionRequired ? <InlineNotice tone="warning">A Commerce read assignment is required to view quotations and sales orders.</InlineNotice> : <><TableFrame appearance="standard" label="Sales documents"><Table><thead><tr><th>Document</th><th>Customer</th><th>Date</th><th>Status</th><th>Total</th>{false}</tr></thead><tbody>{rows.map(row => { const href = canonicalUiHref(`${base}/${"commerce/orders"}/${row.id}`); return <tr key={String(row.id)}><td>{href ? <Link href={href}>{String(row.number)}</Link> : <>{String(row.number)}{canInvoice && <IssueCustomerReceipt base={base} paymentId={String(row.id)}/>}</>}</td><td>{String(row.party_name)}</td><td>{String(row.date ?? row.order_date)}</td><td><StatusBadge appearance="standard" status={String(row.status ?? row.state)}/></td><td>{String(row.currency)} {minorToDecimal(Number(row.total_minor ?? Number(row.subtotal_minor) + Number(row.tax_minor)))}</td>{false}</tr>; })}</tbody></Table></TableFrame>{!rows.length && <p>No documents match these filters.</p>}{data.page && <ListPagination nextCursor={data.page.nextCursor} previousCursor={data.page.previousCursor} query={search} count={rows.length}/>}</>}
      {false}
      {search.new === "invoice" && setup && canInvoice && approved.length > 0 && <SalesInvoiceEditor base={base} parties={setup.options.parties} defaults={approved}/>}
    </div>
  </FinanceWorkspace></FeaturePage>;
}
