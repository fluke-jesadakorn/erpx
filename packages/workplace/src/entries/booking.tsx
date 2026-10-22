import { headers } from "next/headers";
import { notFound } from "next/navigation";
import { productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { PublicBookingDirectory } from "@erpx/feature-pms/components/public-booking-directory";

export default async function BookingEntry() {
  if (productHostForHost((await headers()).get("host")) !== "booking") notFound();
  return <PublicBookingDirectory />;
}
