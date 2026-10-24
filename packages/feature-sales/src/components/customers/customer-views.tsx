import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { ActionLink, Button, Field, Select } from "@erpx/ui/components/foundation";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { MasterEditor } from "@erpx/feature-commerce/components/lazy-master-editor";
import { CRMIdentityConsole } from "@erpx/feature-crm/components/crm-identity-console";
import { SupportConsole } from "@erpx/feature-support/components/support-console";
import { CustomerIdentityConsole } from "@erpx/feature-customer_360/components/customer-identity-console";
import { CustomerDuplicateConsole } from "@erpx/feature-customer_360/components/customer-duplicate-console";
import { CustomerAccessConsole } from "@erpx/feature-customer_360/components/customer-access-console";
import { customerReviews, hubHref, queryValue, type HubQuery } from "@erpx/server-sales/public/rules/navigation";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {commerceOverview} from '@erpx/server-commerce/public/commerce';
import { ApiError } from "@erpx/server-runtime/public/http";
import { editedCustomerParty } from "@erpx/server-finance/public/document-posting-preview";
import { customerWorkspaceDirectory } from "@erpx/server-sales/public/sales-customers";
import { crmIdentities } from "@erpx/server-crm/public/crm-identities";
import {supportWorkspace,supportRecipients} from '@erpx/server-support/public/support';
import { readSourceRecord } from "@erpx/server-records/public/source-record-policy";
import { customerIdentityWorkspace } from "@erpx/server-customer_authority/public/customer-identity-changes";
import { customerDuplicateWorkspace } from "@erpx/server-customer_authority/public/customer-duplicates";
import { customerAccessWorkspace } from "@erpx/server-customer_authority/public/customer-access";
import { HubEmpty, HubPagination, HubSegments } from "@erpx/feature-sales/components/customers/hub-shell";
import { HubCreate, HubCursor, HubFilters, HubTable, hubPage } from "@erpx/feature-sales/components/customers/hub-list";
import styles from "@erpx/feature-sales/components/customers/hub.module.css";

export async function CustomerDirectoryView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const data = await customerWorkspaceDirectory(actor, query), canCreate = hasPermission(actor, "commerce.sales"), editing = queryValue(query, "edit");
  const options = canCreate && (query.new === "1" || editing) ? await commerceOverview(actor) : null;
  const row = editing && options ? await editedCustomerParty(actor, editing) : undefined;
  if (editing && !row) throw new ApiError(404, "CUSTOMER_NOT_FOUND", "Customer is not available in this workspace.");
  return <>
    <div className={styles.filterRow}><HubFilters query={query} placeholder="Search customer name or code" filters={[{ name: "status", label: "Customer status", values: [["", "All statuses"], ["active", "Active"], ["blocked", "Blocked"]] }]} />{canCreate && <HubCreate base={base} hub="customers" query={query} label={editing ? "Edit customer" : "New customer"}>{options && <MasterEditor base={base} kind="parties" partyKind="customer" options={options} row={row ?? undefined} initialOpen />}</HubCreate>}</div>
    {data.items.length ? <HubTable label="Customer directory" columns={["Customer", "Status", "Next follow-up", "Open sales", "Open service"]}>{data.items.map(customer => <tr key={customer.id}><td><Link className={styles.recordName} href={canonicalUiHref(`${base}/customers/${customer.id}`)}>{String(customer.name)}</Link><span className={styles.meta}>{String(customer.code)}</span></td><td data-label="Status"><StatusBadge appearance="standard" status={String(customer.status)} /></td><td data-label="Next follow-up">{customer.summary.nextFollowup ? <Link href={canonicalUiHref(`${base}/customer-followups/${customer.summary.nextFollowup.id}`)}><span>{customer.summary.nextFollowup.dueDate}</span><span className={styles.meta}>{customer.summary.nextFollowup.action}</span></Link> : "No follow-up"}</td><td data-label="Open sales" className={styles.number}>{customer.summary.openOpportunities === null ? `${customer.summary.openOrders} documents` : customer.summary.openOrders + customer.summary.openOpportunities}</td><td data-label="Open service" className={styles.number}>{customer.summary.openSupport ?? "Restricted"}</td></tr>)}</HubTable> : <HubEmpty title="No customers match">Customers can be registered before their first sales document.</HubEmpty>}
    <HubCursor base={base} hub="customers" query={query} page={data} />
  </>;
}
export async function CustomerContactsView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const data = await crmIdentities(actor);
  return <section className={styles.section}><CRMIdentityConsole base={base} data={data} query={query} /></section>;
}
export async function CustomerSupportView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const data = await supportWorkspace(actor), status = queryValue(query, "status"), search = queryValue(query, "q").toLowerCase(), source = data.sources.find(row => row.id === query.orderId);
  const creating = query.new === "1", recipients = creating && source ? await supportRecipients(actor, String(source.id)) : [];
  const sourceKind = queryValue(query, "sourceKind"), sourceId = queryValue(query, "sourceId");
  const typed = creating && sourceId && ["pos_sale", "pms_record", "asset"].includes(sourceKind) ? await readSourceRecord(actor, { kind: sourceKind as "pos_sale" | "pms_record" | "asset", id: sourceId, version: Number(queryValue(query, "sourceVersion")) }) : undefined;
  const items = data.items.filter(row => (!status || row.status === status) && (!search || `${row.title} ${row.status}`.toLowerCase().includes(search))), rows = hubPage(items, query);
  return <>
    <div className={styles.filterRow}><HubFilters query={query} placeholder="Search service cases" filters={[{ name: "status", label: "Service status", values: [["", "All statuses"], ["open", "Open"], ["triage", "Triage"], ["in_progress", "In progress"], ["resolved", "Resolved"], ["closed", "Closed"], ["cancelled", "Cancelled"]] }]} />{(data.canCreate || data.canCreateOrder) && <HubCreate base={base} hub="customers" query={query} label="New service case">{creating && <>
      {data.canCreateOrder && <form className={styles.section} data-work-dirty="false"><input type="hidden" name="view" value="support" /><input type="hidden" name="new" value="1" /><Field appearance="standard">Link a customer document (optional)<Select appearance="standard" name="orderId" defaultValue={queryValue(query, "orderId")}><option value="">Direct report</option>{data.sources.map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.customer_name)} · {String(row.number)}</option>)}</Select></Field><Button appearance="standard" type="submit" variant="secondary">Choose source</Button></form>}
      <SupportConsole base={base} data={{ ...data, canCreate: data.canCreate && !source }} source={source ? { id: String(source.id), version: Number(source.version) } : undefined} recipients={recipients} typedSource={typed ? { ...typed, kind: typed.kind as "pos_sale" | "pms_record" | "asset" } : undefined} />
    </>}</HubCreate>}</div>
    {rows.length ? <HubTable label={data.ownReportsOnly ? "My service reports" : "Service cases"} columns={["Case", "Status", "Due"]}>{rows.map(row => <tr key={String(row.id)}><td><Link className={styles.recordName} href={canonicalUiHref(`${base}/support/${row.id}`)}>{String(row.title)}</Link></td><td data-label="Status"><StatusBadge appearance="standard" status={String(row.status)} /></td><td data-label="Due">{String(row.due_date ?? "Needs triage")}</td></tr>)}</HubTable> : <HubEmpty title="No service cases match">Adjust the filters or report a new issue.</HubEmpty>}
    <HubPagination base={base} hub="customers" query={query} count={rows.length} total={items.length} />
    {!data.ownReportsOnly && <div className={styles.actions}><ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/support/sla`)}>Resolution-time watch</ActionLink><ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/support/incidents`)}>Shared incidents</ActionLink></div>}
  </>;
}
export async function CustomerReviewsView({ actor, base, query }: { actor: WorkContext; base: string; query: HubQuery }) {
  const review = queryValue(query, "review", "identity"), creating = query.new === "1";
  const data = review === "duplicates" ? await customerDuplicateWorkspace(actor) : review === "access" ? await customerAccessWorkspace(actor) : await customerIdentityWorkspace(actor);
  const canCreate = "canCreate" in data ? data.canCreate : true, status = queryValue(query, "status", "pending"), search = queryValue(query, "q").toLowerCase();
  const items = data.rows.filter(row => (!status || row.status === status) && (!search || `${"customer_name" in row ? row.customer_name : ""} ${"left_name" in row ? row.left_name : ""} ${"right_name" in row ? row.right_name : ""} ${"number" in row ? row.number : ""} ${"email" in row ? row.email : ""}`.toLowerCase().includes(search))), rows = hubPage(items, query);
  const route = review === "access" ? "customer-access" : review === "duplicates" ? "customer-duplicates" : "customer-identity";
  return <>
    <div className={styles.filterRow}><HubSegments base={base} hub="customers" query={query} name="review" label="Customer review type" selected={review} values={customerReviews} />{canCreate && <HubCreate base={base} hub="customers" query={query} label="New review request">{creating && (review === "duplicates" && "canCreate" in data && "customers" in data ? <CustomerDuplicateConsole base={base} data={data as Awaited<ReturnType<typeof customerDuplicateWorkspace>>} /> : review === "access" && "sources" in data ? <CustomerAccessConsole base={base} data={data} selectedOrderId={queryValue(query, "orderId")} requestFormInline /> : <CustomerIdentityConsole base={base} data={data as Awaited<ReturnType<typeof customerIdentityWorkspace>>} />)}</HubCreate>}</div>
    <HubFilters query={{ ...query, status }} placeholder="Search review requests" filters={[{ name: "status", label: "Review status", values: [["pending", "Pending"], ["", "All statuses"], ["applied", "Applied"], ["approved", "Approved"], ["rejected", "Rejected"], ["duplicate", "Duplicate"], ["distinct", "Distinct"], ["merged", "Merged"], ["revoked", "Revoked"]] }]} />
    {rows.length ? <HubTable label="Customer data reviews" columns={["Request", "Status"]}>{rows.map(row => <tr key={String(row.id)}><td><Link className={styles.recordName} href={canonicalUiHref(`${base}/${route}/${row.id}`)}>{String("customer_name" in row ? row.customer_name : "left_name" in row ? `${row.left_name} / ${row.right_name}` : `${row.number} · ${row.email}`)}</Link></td><td data-label="Status"><StatusBadge appearance="standard" status={String(row.status)} /></td></tr>)}</HubTable> : <HubEmpty title="No review requests match">All statuses includes completed requests.</HubEmpty>}
    <HubPagination base={base} hub="customers" query={query} count={rows.length} total={items.length} />
    <Link href={hubHref(base, "customers", { view: "directory" })}>Back to customers</Link>
  </>;
}
