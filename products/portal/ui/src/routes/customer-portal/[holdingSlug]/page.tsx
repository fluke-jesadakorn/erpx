import {publicHoldingPath} from '@erpx/platform-runtime-ui/lib/holding-paths';
import {createHash} from 'node:crypto';
import { customerPortalPresentationQueries } from "@erpx/customer-360-server/public/presentation-context-queries";
import { TimeText } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import {PortalSupportAcknowledgement} from "@erpx/customer-authority-ui/components/portal-support-acknowledgement";
import { CustomerPortalActions } from "@erpx/customer-authority-ui/components/customer-portal-actions";
import { EmptyState, StatusBadge } from "@erpx/platform-ui/components/primitives";
import { QuickAccessShelf } from "@erpx/work-projection-ui/components/quick-access/shelf";
import rail from "@erpx/work-projection-ui/components/quick-access/area-rail.module.css";

import { customerPortalQuickAccess } from "@erpx/work-projection-server/public/portal-quick-access";
import { readQuickAccessPreferences } from "@erpx/work-projection-server/public/quick-access-preferences";
import { holdingBindingBySlug } from "../../../../../../../platform/runtime/server/src/public/runtime";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
import {redirect} from 'next/navigation';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {portalActionsDisplay,portalPreferenceDisplay,portalPurchaseDisplay,portalSharedFilesDisplay,portalSupportThreadDisplay} from '@erpx/customer-authority-ui/lib/portal-display';

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const { data, items, preferences, accountKey } = await (async () => {
    const binding = await holdingBindingBySlug(holdingSlug);
    const { queries, actor } = await customerPortalPresentationQueries(holdingSlug).catch(error=>{
      if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/login?'+new URLSearchParams({next:`${new URL(publicHoldingPath(holdingSlug,'customer-portal'),'https://path.invalid').pathname}`}));
      throw error;
    });
    const data = await queries.customerPortalProjection(actor);
    const items = customerPortalQuickAccess(holdingSlug, data.items);
    const preferences = await readQuickAccessPreferences(actor.userId, "customer_portal", binding.id);
    const accountKey = createHash('sha256').update(JSON.stringify(['customer-portal',binding.id,actor.userId,actor.email.toLowerCase()])).digest('hex');
    return { data, items, preferences: portalPreferenceDisplay(preferences), accountKey };
  })().catch((error) => pageError(error));

  return (
    <>
      <a href="#customer-portal-content" className="skip-link">Skip to documents</a>
      <PageContent as="main" className="ui-page-content">
        <header>
          <h1>Customer documents</h1>
          <a href={`${publicHoldingPath(holdingSlug,'customer-portal')}/tickets`}>Support tickets</a>
          <p>Only documents explicitly approved for your verified account are shown. Each view is recorded in the access audit.</p>
        </header>
        <div className={rail.layout}>
        <aside className={rail.rail} aria-label="Quick access">
          <QuickAccessShelf key={accountKey} appearance="standard"
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
            <Card as="section" appearance="standard" className="ui-panel" id={`customer-order-${encodeURIComponent(String(record.binding_id))}`} key={`${accountKey}:${record.binding_id}`}>
              <h2>{String(record.number)}</h2>
              <p className="customer-record-meta"><StatusBadge appearance="standard" status={String(record.state)} /><span>{String(record.kind)} · Due {String(record.due_date)} · {String(record.currency)}</span></p>
              <h3>Deliveries</h3>
              {(Array.isArray(record.fulfillments) ? record.fulfillments : []).map((fulfillment: Record<string, unknown>) => (
                <p key={String(fulfillment.number)}>{String(fulfillment.number)} · recorded {<TimeText appearance="standard" value={Number(fulfillment.createdAt)} />}</p>
              ))}
              <CustomerPortalActions
                key={`${accountKey}:${record.binding_id}`}
                holdingSlug={holdingSlug}
                bindingId={String(record.binding_id)}
                bindingVersion={Number(record.binding_version)}
                purchase={portalPurchaseDisplay(record.purchase)}
                actions={portalActionsDisplay(record.actions)}
                sharedFiles={portalSharedFilesDisplay(record.sharedFiles)}
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
