import { commercePurchaseControls } from "@erpx/server-commerce/public/purchase-controls";
import { CommerceEnvironment,CommercePurchaseProvider } from "@erpx/feature-commerce/components/purchase-context";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { Button, Field, InlineNotice, Select } from "@erpx/ui/components/foundation";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { CRMConsole } from "@erpx/feature-crm/components/crm-console";
import { CustomerFollowupForm } from "@erpx/feature-customer_360/components/customer-followups";
import { SalesPricingConsole } from "@erpx/feature-sales/components/sales-pricing-console";
import { AnalyticsReport } from "@erpx/feature-runtime/components/executive/analytics-report";
import { OrderEditor } from "@erpx/workplace/components/work-editors";
import { queryValue, type HubQuery } from "@erpx/server-sales/public/rules/navigation";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { commerceSalesDocuments, getOrder, workOrderRows } from "@erpx/server-commerce/public/commerce";
import {salesEditorOptions,type SalesEditorOptions} from "@erpx/server-commerce/public/sales-editor";
import {salesPostingDocumentReadiness} from "@erpx/server-finance/public/sales-posting-setup";
import { crmWorkspace } from "@erpx/server-crm/public/crm";
import { crmIdentities } from "@erpx/server-crm/public/crm-identities";
import { crmAnalytics, analyticsOptions, defaultAnalyticsRange } from "@erpx/server-crm/public/crm-marketing-report";
import { customerFollowupWorkspace } from "@erpx/server-sales/public/customer-followups";
import { ApiError } from "@erpx/server-runtime/public/http";
import { pricingWorkspace } from "@erpx/server-sales/public/sales-pricing";
import { salesToday } from "@erpx/server-sales/public/sales-customers";
import { HubEmpty, HubPagination, HubSegments } from "@erpx/feature-sales/components/customers/hub-shell";
import { HubCreate, HubCursor, HubFilters, HubTable, hubPage } from "@erpx/feature-sales/components/customers/hub-list";
import styles from "@erpx/feature-sales/components/customers/hub.module.css";

export async function SalesTodayView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const data = await salesToday(actor, query), rows = hubPage(data.items, query), newForm = query.new === "1";
  const source = newForm && query.orderId ? await customerFollowupWorkspace(actor, queryValue(query, "orderId")) : null;
  const orders = newForm && !source ? (await workOrderRows(actor)).filter(row => ["quote", "sales"].includes(String(row.kind))) : [];
  return <>
    <div className={styles.filterRow}><HubSegments base={base} hub="sales" query={query} name="owner" selected={queryValue(query, "owner", "mine")} label="Work ownership" values={[["mine", "My work"], ["team", "Team"]]} />{(data.canCreateFollowup||newForm) && <HubCreate base={base} hub="sales" query={query} label="New follow-up">{newForm && (source?.order ? <CustomerFollowupForm base={base} order={source.order} recipients={source.recipients} purchase={source.purchase} inlineCreate /> : <form className={styles.section} data-work-dirty="false"><input type="hidden" name="view" value="today" /><input type="hidden" name="new" value="1" /><Field appearance="standard">Customer quotation or order<Select appearance="standard" name="orderId" required defaultValue=""><option value="">Choose source</option>{orders.map(order => <option key={String(order.id)} value={String(order.id)}>{String(order.number)} · {String(order.party_name)}</option>)}</Select></Field><Button appearance="standard" variant="primary" type="submit">Continue</Button></form>)}</HubCreate>}</div>
    <HubFilters query={query} placeholder="Search tasks and customers" filters={[{ name: "kind", label: "Task type", values: [["all", "All tasks"], ...(data.canReadCrm ? [["crm", "CRM & activities"], ["followups", "Follow-ups"]] as const : []), ...(data.canReadDocuments ? [["documents", "Sales documents"]] as const : [])] }, { name: "status", label: "Task status", values: [["", "Open tasks"], ["all", "All statuses"]] }]} />
    {data.unavailable.length > 0 && <InlineNotice tone="warning" role="status">Unable to load {data.unavailable.join(", ")}. Refresh to retry; the available tasks remain below.</InlineNotice>}
    {rows.length ? <HubTable label="Sales work" columns={["Next action", "Status", "Responsible", "Due"]}>{rows.map(row => <tr key={`${row.type}-${row.id}`}><td><Link className={styles.recordName} href={`${base}${row.href}`}>{row.title}</Link><span className={styles.meta}>{row.nextAction}{row.blocker ? ` · ${row.blocker}` : ""}</span></td><td data-label="Status"><StatusBadge appearance="standard" status={row.status} /></td><td data-label="Responsible">{row.responsibleQueue || "Unassigned"}</td><td data-label="Due" className={row.dueAt && row.dueAt < data.observedAt ? styles.overdue : undefined}>{row.dueAt ? <TimeText appearance="standard" value={row.dueAt} options={{ year: "numeric", month: "short", day: "numeric" }} /> : "No date"}</td></tr>)}</HubTable> : <HubEmpty title={data.unavailable.length ? "Some tasks are unavailable" : "No tasks in this view"}>Try Team or adjust the filters.</HubEmpty>}
    <HubPagination base={base} hub="sales" query={query} count={rows.length} total={data.items.length} />
  </>;
}
export async function SalesPipelineView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const data = await crmWorkspace(actor), search = queryValue(query, "q").trim().toLowerCase(), stage = queryValue(query, "stage");
  const items = data.items.filter(row => (!stage || String(row.stage ?? row.status) === stage) && (!search || `${row.title} ${row.customer_name} ${row.owner_name}`.toLowerCase().includes(search))), rows = hubPage(items, query);
  const identities = data.canCreate && query.new === "1" ? await crmIdentities(actor) : undefined;
  return <>
    <div className={styles.filterRow}><HubFilters query={query} placeholder="Search opportunities and customers" filters={[{ name: "stage", label: "Pipeline stage", values: [["", "All stages"], ["open", "Lead"], ["discovery", "Discovery"], ["proposal", "Proposal"], ["negotiation", "Negotiation"], ["won", "Won"], ["lost", "Lost"], ["disqualified", "Disqualified"]] }]} />{data.canCreate && (data.purchase?.newWork!==false||query.new==="1") && <HubCreate base={base} hub="sales" query={query} label="New lead">{identities && <CRMConsole base={base} data={data} identityData={identities} inlineCapture />}</HubCreate>}</div>
    {rows.length ? <HubTable label="Sales pipeline" columns={["Opportunity / customer", "Stage", "Owner", "Next date", "Value"]}>{rows.map(row => <tr key={String(row.id)}><td><Link className={styles.recordName} href={canonicalUiHref(`${base}/crm/${row.id}`)}>{String(row.title)}</Link><span className={styles.meta}>{String(row.customer_name ?? "Prospect")}</span></td><td data-label="Stage"><StatusBadge appearance="standard" status={String(row.stage ?? row.status)} /></td><td data-label="Owner">{String(row.owner_name ?? "Owner unavailable")}</td><td data-label="Next date">{String(row.close_date ?? row.due_date ?? "No date")}</td><td data-label="Value">{row.expected_minor !== null && row.expected_minor !== undefined ? <MoneyAmount units={String(row.expected_minor)} legacyScale={2} currency={String(row.currency)} /> : "—"}</td></tr>)}</HubTable> : <HubEmpty title="No opportunities match">Adjust your search or stage.</HubEmpty>}
    <HubPagination base={base} hub="sales" query={query} count={rows.length} total={items.length} />
  </>;
}
export async function SalesDocumentsView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const kind = queryValue(query, "type", "quote") === "sales" ? "sales" : "quote", status = queryValue(query, "status");
  const page = await commerceSalesDocuments(actor, {view:kind,q:queryValue(query,"q"),cursor:queryValue(query,"cursor")||undefined,status});
  const editing = queryValue(query, "edit"), detail = editing ? await getOrder(actor, editing) : null;
  if (detail && detail.order.kind !== kind) throw new ApiError(404, "ORDER_NOT_FOUND", "Sales document is not available in this view.");
  const purchase = await commercePurchaseControls(actor, {type:"kind",kind}), readiness=await salesPostingDocumentReadiness(actor,kind),selected=query.new==="1"||!!editing;
  const options:SalesEditorOptions=selected&&readiness.ready&&hasPermission(actor,"commerce.sales")&&purchase?.newWork!==false?await salesEditorOptions(actor,kind):{products:[],parties:[],warehouses:[],nodes:[],priceLists:[],priceListItems:[],currency:""};
  const editorPurchase=readiness.ready?purchase:purchase?{...purchase,newWork:false}:{newWork:false,settleExisting:false,history:true,revision:"sales-posting-required",convertNewWork:false,stockNewWork:false,stockSettleExisting:false,financeSettleExisting:false,receivingSettleExisting:false};
  const editorOrder=detail?Object.fromEntries(["id","version","party_id","price_list_id","warehouse_id","discount_reason","order_date","due_date","note"].map(field=>[field,detail.order[field]??null])):undefined,editorLines=detail?.lines.map(line=>Object.fromEntries(["product_id","quantity_milli","price_minor","tax_bps"].map(field=>[field,line[field]??null])));
  return <CommerceEnvironment purchase={purchase}>
    {!readiness.ready&&<InlineNotice role="status">Prepare and independently approve service Sales posting defaults before preparing a quotation or order. <Link href={canonicalUiHref(`${base}/customers-sales/sales/settings/posting`)}>Prepare service Sales posting</Link>. Goods with stock and cost posting require separate goods setup.</InlineNotice>}
    <div className={styles.filterRow}><HubSegments base={base} hub="sales" query={query} name="type" selected={kind} label="Sales document type" values={[["quote", "Quotations"], ["sales", "Sales orders"]]} />{hasPermission(actor, "commerce.sales") && (selected||purchase?.newWork !== false&&readiness.ready) && <HubCreate base={base} hub="sales" query={query} label={editing ? "Edit draft" : kind === "quote" ? "New quotation" : "New sales order"}>{selected&&<CommercePurchaseProvider purchase={editorPurchase}><OrderEditor base={base} options={options} kind={kind} order={editorOrder} lines={editorLines} initialOpen initialPartyId={queryValue(query, "customerId")} /></CommercePurchaseProvider>}</HubCreate>}</div>
    {hasPermission(actor,"commerce.sales")&&<p><Link href={canonicalUiHref(`${base}/customers-sales/sales/settings/services`)}>Service catalogue</Link></p>}
    <HubFilters query={query} placeholder="Search number or customer" filters={[{ name: "status", label: "Document status", values: [["", "All statuses"], ["draft", "Draft"], ["submitted", "Submitted"], ["confirmed", "Confirmed"], ["partially_fulfilled", "Partially fulfilled"], ["fulfilled", "Fulfilled"], ["converted", "Converted"], ["cancelled", "Cancelled"]] }]} />
    {page.items.length ? <HubTable label={kind === "quote" ? "Quotations" : "Sales orders"} columns={["Document / customer", "Status", "Date", "Value"]}>{page.items.map(row => <tr key={String(row.id)}><td><Link className={styles.recordName} href={canonicalUiHref(`${base}/commerce/orders/${row.id}?view=${kind}`)}>{String(row.number)}</Link><span className={styles.meta}>{String(row.party_name)}</span></td><td data-label="Status"><StatusBadge appearance="standard" status={String(row.state)} /></td><td data-label="Date">{String(row.order_date)}</td><td data-label="Value"><MoneyAmount units={(BigInt(String(row.subtotal_minor)) + BigInt(String(row.tax_minor))).toString()} legacyScale={2} currency={String(row.currency)} /></td></tr>)}</HubTable> : <HubEmpty title="No sales documents match">Adjust your search or create a document.</HubEmpty>}
    <HubCursor base={base} hub="sales" query={query} page={page} />
  </CommerceEnvironment>;
}
export async function SalesPricingView({ actor, base }: { actor: WorkContext; base: string }) {
  return <section className={styles.section}><SalesPricingConsole base={base} data={await pricingWorkspace(actor)} /></section>;
}
export async function SalesReportsView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const defaults = defaultAnalyticsRange(), raw = { from: queryValue(query, "from", defaults.from), to: queryValue(query, "to", defaults.to), branchId: queryValue(query, "branchId") || undefined, propertyId: queryValue(query, "propertyId") || undefined, campaignId: queryValue(query, "campaignId") || undefined, ownerId: queryValue(query, "ownerId") || undefined };
  const [report, options] = await Promise.all([crmAnalytics(actor, raw), analyticsOptions(actor)]);
  return <section className={styles.section}><AnalyticsReport kind="crm" base={base} report={report} options={options} workspaceHref={canonicalUiHref(`${base}/sales?view=reports`)} /></section>;
}
