import "server-only";
import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { salesCustomerLegacyDestination, type HubQuery } from "@erpx/sales-server/domain/navigation";

export async function redirectSalesCustomerList(holdingSlug: string, route: string, query: HubQuery) {
  const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const destination = salesCustomerLegacyDestination(base, route, query);
  if (destination) redirect(destination);
}
