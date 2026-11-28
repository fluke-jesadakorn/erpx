import { headers } from "next/headers";
import { notFound } from "next/navigation";
import { productHostForHost } from "@erpx/platform-runtime-ui/lib/product-hosts";
import { PublicBookingDirectory } from "@erpx/pms-ui/components/public-booking-directory";

export default async function BookingEntry() {
  if (productHostForHost((await headers()).get("host")) !== "booking") notFound();
  return <PublicBookingDirectory />;
}
