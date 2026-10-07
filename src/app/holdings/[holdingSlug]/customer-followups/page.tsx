import { redirectSalesCustomerList } from "@/server/modules/sales/public/sales-customer-redirect";
import type { HubQuery } from "@/server/modules/sales/public/rules/navigation";
export const dynamic = "force-dynamic";
export default async function Page({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<HubQuery> }) {
  const { holdingSlug } = await params;
  await redirectSalesCustomerList(holdingSlug, "customer-followups", await searchParams);
}
