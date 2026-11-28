import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { queryValue, type HubQuery, type CustomersView } from "@erpx/sales-server/public/rules/navigation";
import { customerReviewCounts, salesCustomersAccess } from "@erpx/sales-server/public/sales-customers";
import { HubShell } from "@erpx/sales-ui/components/customers/hub-shell";
import { CustomerContactsView, CustomerDirectoryView, CustomerReviewsView, CustomerSupportView } from "@erpx/sales-ui/components/customers/customer-views";
export const dynamic = "force-dynamic";
export default async function Page({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<HubQuery> }) {
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), access = await salesCustomersAccess(actor), query = await searchParams;
  const view = (queryValue(query, "view") || (["directory", "contacts", "support", "reviews"] as const).find(view => access[view])) as CustomersView;
  if (!["directory", "contacts", "support", "reviews"].includes(view) || !access[view]) forbidden();
  if (view === "reviews" && !["identity", "duplicates", "access"].includes(queryValue(query, "review", "identity"))) forbidden();
  const props = { actor, base, query: { ...query, view } };
  const content = await (view === "directory" ? CustomerDirectoryView(props) : view === "contacts" ? CustomerContactsView(props) : view === "support" ? CustomerSupportView(props) : CustomerReviewsView(props)).catch(error => pageError(error, holdingSlug));
  const counts = await customerReviewCounts(actor).catch(error => pageError(error, holdingSlug));
  return <HubShell base={base} hub="customers" view={view} access={access} reviewCount={counts ? counts.identity + counts.duplicates + counts.access : null}>{content}</HubShell>;
}
