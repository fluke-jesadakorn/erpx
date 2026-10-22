import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { PageContent, ActionLink } from "@erpx/ui/components/foundation";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { CustomerNotes } from "@erpx/feature-records/components/loaders/modules--customer-notes";
import { HubForm } from "@erpx/feature-records/components/loaders/sales-customers--hub-form";
import styles from "@erpx/feature-sales/components/customers/hub.module.css";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { customerWorkspaceProfile } from "@erpx/server-sales/public/sales-customers";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { queryValue, type HubQuery } from "@erpx/server-sales/public/rules/navigation";
export const dynamic = "force-dynamic";
export default async function Page({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<HubQuery> }) {
  const { holdingSlug, id } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
  const data = await customerWorkspaceProfile(actor, id, query).catch(error => pageError(error, holdingSlug)), customer = data.customer, history = queryValue(query, "section") === "history" || !!query.cursor;
  return <PageContent className={styles.profile}>
    <Link href={base + "/customers"}>Customers</Link>
    <header className={styles.profileHeader}><div><h1>{String(customer.name)}</h1><p className={styles.meta}>{String(customer.code)} · <StatusBadge appearance="standard" status={String(customer.status)} /></p></div>{hasPermission(actor, "commerce.sales") && <ActionLink appearance="standard" variant="secondary" href={base + "/sales?view=documents&type=quote&new=1&customerId=" + encodeURIComponent(id)}>New quotation</ActionLink>}{data.supportSource && <ActionLink appearance="standard" variant="secondary" href={base + "/customers?view=support&new=1&orderId=" + encodeURIComponent(data.supportSource.id)}>New service case</ActionLink>}</header>
    <div className={styles.profileContact}>{customer.phone ? <span>{String(customer.phone)}</span> : null}{customer.notification_email ? <span>{String(customer.notification_email)}</span> : null}{customer.tax_registration ? <span>Tax ID {String(customer.tax_registration)}</span> : null}</div>
    <nav className={styles.tabs} aria-label="Customer profile views"><Link href={base + "/customers/" + id} record={{ kind: "customer", id }} aria-current={!history ? "page" : undefined}>Summary</Link><Link href={base + "/customers/" + id + "?section=history"} record={{ kind: "customer", id, args: { section: "history" } }} aria-current={history ? "page" : undefined}>History</Link></nav>
    {!history ? <>
      <dl className={styles.summaryStrip}><div><dt>Next follow-up</dt><dd>{data.summary.nextFollowup?.dueDate ?? "No date"}</dd></div><div><dt>Open sales documents</dt><dd>{data.summary.openOrders}</dd></div><div><dt>Open opportunities</dt><dd>{data.summary.openOpportunities ?? "Restricted"}</dd></div><div><dt>Open service cases</dt><dd>{data.summary.openSupport ?? "Restricted"}</dd></div></dl>
      <section><div className={styles.inlineHeading}><h2>Pending work</h2>{data.notes?.canCreate && <HubForm title="Record customer contact" label="Record contact" primary={false}><CustomerNotes base={base} data={data.notes} mode="create" /></HubForm>}</div>
        <div className={styles.sourceList} data-slot="record-list">
          {data.followups.map(row => <Link key={String(row.id)} href={base + "/customer-followups/" + row.id}><div><strong>{String(row.next_action)}</strong><small>Follow-up · {String(row.order_number)}</small></div><span>{String(row.due_date)}</span></Link>)}
          {data.opportunities?.map(row => <Link key={String(row.id)} href={base + "/crm/" + row.id}><div><strong>{String(row.title)}</strong><small>Opportunity · {String(row.stage)}</small></div><span>{String(row.close_date ?? row.due_date)}</span></Link>)}
          {data.orders.map(row => <Link key={String(row.id)} href={base + "/commerce/orders/" + row.id + "?view=" + row.kind}><div><strong>{String(row.number)}</strong><small>{row.kind === "quote" ? "Quotation" : "Sales order"} · {String(row.state)}</small></div><MoneyAmount units={String(row.total_minor)} legacyScale={2} currency={String(row.currency)} /></Link>)}
          {data.cases?.map(row => <Link key={String(row.id)} href={base + "/support/" + row.id}><div><strong>{String(row.title)}</strong><small>Service · {String(row.status)}</small></div><span>{String(row.due_date ?? "Needs triage")}</span></Link>)}
        </div>
        {!data.followups.length && !data.orders.length && !data.opportunities?.length && !data.cases?.length && <p className={styles.meta}>No pending work visible in this scope.</p>}
      </section>
    </> : <>
      <section><h2>Customer history</h2><div className={styles.sourceList} data-slot="record-list">{data.timeline?.items.map(event => <Link key={event.id} href={base + event.href}><div><strong>{event.title}</strong><small>{event.status}</small></div><TimeText appearance="standard" value={Number(event.at)} /></Link>)}</div>{!data.timeline?.items.length && <p className={styles.meta}>No source history visible in this scope.</p>}{data.timeline?.nextCursor && <Link record={{ kind: "customer", id, args: { section: "history", cursor: data.timeline.nextCursor } }} href={base + "/customers/" + id + "?section=history&cursor=" + encodeURIComponent(data.timeline.nextCursor)}>Older activity</Link>}{query.cursor && <Link record={{ kind: "customer", id, args: { section: "history" } }} href={base + "/customers/" + id + "?section=history"}>Latest activity</Link>}</section>
      {data.notes && <CustomerNotes base={base} data={data.notes} mode="history" />}
    </>}
  </PageContent>;
}
