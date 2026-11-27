import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {redirect} from "next/navigation";
import {routeContextHref} from "@erpx/feature-shared/lib/route-context";
import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { queryValue, type HubQuery, type SalesView } from "@erpx/server-sales/public/rules/navigation";
import { salesCustomersAccess } from "@erpx/server-sales/public/sales-customers";
import { HubShell } from "@erpx/feature-sales/components/customers/hub-shell";
import { SalesDocumentsView, SalesPipelineView, SalesReportsView, SalesTodayView } from "@erpx/feature-sales/components/customers/sales-views";
import { oasisDemoStagingEnabled } from "@erpx/contracts/holding-hosts";
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
