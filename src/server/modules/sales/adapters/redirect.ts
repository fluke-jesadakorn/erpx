import "server-only";
import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { salesCustomerLegacyDestination, type HubQuery } from "../domain/navigation";

export async function redirectSalesCustomerList(holdingSlug: string, route: string, query: HubQuery) {
  const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const destination = salesCustomerLegacyDestination(base, route, query);
  if (destination) redirect(destination);
}

