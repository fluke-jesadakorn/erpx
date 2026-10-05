import { redirectSalesCustomerList } from "@/server/sales-customer-redirect";
import type { HubQuery } from "@/lib/sales-customers";
export const dynamic = "force-dynamic";
export default async function Page({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<HubQuery> }) {
  const { holdingSlug } = await params;
  await redirectSalesCustomerList(holdingSlug, "customer-duplicates", await searchParams);
}
