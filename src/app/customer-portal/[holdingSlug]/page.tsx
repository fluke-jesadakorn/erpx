import { customerPortalPresentationQueries } from "@/server/modules/customer_360/public/presentation-context-queries";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { CustomerPortalActions } from "@/components/customer_authority/customer-portal-actions";
import { EmptyState, StatusBadge } from "@/components/ui/primitives";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import rail from "@/components/work_projection/quick-access/area-rail.module.css";

import { customerPortalQuickAccess } from "@/server/modules/work_projection/public/portal-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { PageContent, Card } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const { data, items, preferences } = await (async () => {
    const binding = await holdingBindingBySlug(holdingSlug);
    const { queries, actor } = await customerPortalPresentationQueries(holdingSlug);
    const data = await queries.customerPortalProjection(actor);
    const items = customerPortalQuickAccess(holdingSlug, data.items);
    const preferences = await readQuickAccessPreferences(actor.userId, "customer_portal", binding.id);
    return { data, items, preferences };
  })().catch((error) => pageError(error));

  return (
    <>
      <a href="#customer-portal-content" className="skip-link">Skip to documents</a>
      <PageContent as="main" className="ui-page-content">
        <header>
          <h1>Customer documents</h1>
          <p>Only documents explicitly approved for your verified account are shown. Each view is recorded in the access audit.</p>
        </header>
        <div className={rail.layout}>
        <aside className={rail.rail} aria-label="Quick access">
          <QuickAccessShelf appearance="standard"
            title="Quick access"
            items={items}
            initialPreferences={preferences}
            defaultIds={items.slice(0, 6).map((item) => item.id)}
            endpoint={`/api/customer-portal/${encodeURIComponent(holdingSlug)}/quick-access`}
            requestMode="plain"
            embedded
            placement="rail"
            showAllMode={false}
          />
        </aside>
        <div id="customer-portal-content" tabIndex={-1} className={rail.content}>
          {data.items.map((record) => (
            <Card as="section" appearance="standard" className="ui-panel" id={`customer-order-${encodeURIComponent(String(record.binding_id))}`} key={String(record.binding_id)}>
              <h2>{String(record.number)}</h2>
              <p className="customer-record-meta"><StatusBadge appearance="standard" status={String(record.state)} /><span>{String(record.kind)} · Due {String(record.due_date)} · {String(record.currency)}</span></p>
              <h3>Deliveries</h3>
              {(Array.isArray(record.fulfillments) ? record.fulfillments : []).map((fulfillment: Record<string, unknown>) => (
                <p key={String(fulfillment.number)}>{String(fulfillment.number)} · recorded {<TimeText appearance="standard" value={Number(fulfillment.createdAt)} />}</p>
              ))}
              <CustomerPortalActions
                holdingSlug={holdingSlug}
                bindingId={String(record.binding_id)}
                bindingVersion={Number(record.binding_version)}
                actions={Array.isArray(record.actions) ? record.actions as Record<string, unknown>[] : []}
                sharedFiles={Array.isArray(record.sharedFiles) ? record.sharedFiles as Array<{linkId:string;label:string;previewAvailable:true}> : []}
              />
            </Card>
          ))}
          {!data.items.length && <EmptyState title="No approved documents yet" description="Documents approved for your verified account will appear here." />}
        </div>
        </div>
      </PageContent>
    </>
  );
}
