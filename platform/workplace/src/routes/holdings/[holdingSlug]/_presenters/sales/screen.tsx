import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import {redirect} from "next/navigation";
import {routeContextHref} from "@erpx/platform-shared-ui/lib/route-context";
import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { queryValue, type HubQuery, type SalesView } from "@erpx/sales-server/public/rules/navigation";
import { salesCustomersAccess } from "@erpx/sales-server/public/sales-customers";
import { HubShell } from "@erpx/sales-ui/components/customers/hub-shell";
import { SalesDocumentsView, SalesPipelineView, SalesReportsView, SalesTodayView } from "@erpx/sales-ui/components/customers/sales-views";
import { oasisDemoStagingEnabled } from "@erpx/platform-contracts/holding-hosts";
import OrderPage from "../commerce/orders/[id]/screen";
import LeadPage from "../crm/[id]/screen";
export const dynamic = "force-dynamic";
export default async function Page({ params, searchParams }: { params: Promise<{ holdingSlug: string; id?: string }>; searchParams: Promise<HubQuery> }) {
  const { holdingSlug, id } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), access = await salesCustomersAccess(actor), query = await searchParams;
  const requested = queryValue(query, "view"), defaultView = (["today", "pipeline", "documents"] as const).find(view => access[view]);
  const view = (requested || defaultView) as SalesView;
  if (!["today", "pipeline", "documents", "pricing", "reports"].includes(view) || !access[view]) forbidden();
  if (id && oasisDemoStagingEnabled(process.env)) {
    const record = Promise.resolve({ holdingSlug, id });
    if (view === "documents") {
      return <OrderPage params={record} searchParams={Promise.resolve({ print: queryValue(query, "print") })} />;
    }
    if (view === "pipeline") return <LeadPage params={record} />;
  }
  if(view==='pricing')redirect(routeContextHref(`${base}/commerce`,query,{view:'pricing'}));
  const props = { actor, base, query: { ...query, view } };
  const content = await (view === "today" ? SalesTodayView(props) : view === "pipeline" ? SalesPipelineView(props) : view === "documents" ? SalesDocumentsView(props) : SalesReportsView(props)).catch(error => pageError(error, holdingSlug));
  return <HubShell base={base} hub="sales" view={view} access={access}>{content}</HubShell>;
}
