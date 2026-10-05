import "server-only";
import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { salesCustomerLegacyDestination, type HubQuery } from "@/lib/sales-customers";

export async function redirectSalesCustomerList(holdingSlug: string, route: string, query: HubQuery) {
  const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const destination = salesCustomerLegacyDestination(base, route, query);
  if (destination) redirect(destination);
}
