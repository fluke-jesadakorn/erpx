import "server-only";
import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { salesCustomerLegacyDestination, type HubQuery } from "@erpx/server-sales/domain/navigation";

export async function redirectSalesCustomerList(holdingSlug: string, route: string, query: HubQuery) {
  const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const destination = salesCustomerLegacyDestination(base, route, query);
  if (destination) redirect(destination);
}

