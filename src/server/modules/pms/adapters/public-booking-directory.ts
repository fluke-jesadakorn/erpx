import "server-only";
import { resourceSlots } from "@/lib/runtime/holding-registry";
import { activeHoldingBindings, holdingResources, runtimeEnv } from "../../runtime/public/runtime";
import { bookingCatalog } from "./pms-booking";

/** Published guest content only. Internal/beta workspaces never join this index. */
export async function publicBookingDirectory() {
  if (process.env.PMS_STRIPE_BOOKING_ENABLED === "false") return [];
  const [env, bindings] = await Promise.all([runtimeEnv(), activeHoldingBindings()]);
  const publicHoldings = new Set(resourceSlots(env).filter(slot => !slot.internal && !slot.beta).map(slot => slot.id));
  const groups = await Promise.all(bindings.filter(binding => publicHoldings.has(binding.id)).map(async binding => {
    const hotels = await bookingCatalog((await holdingResources(binding.id)).write);
    return hotels.map(hotel => ({
      slug: binding.slug, id: hotel.id, name: hotel.name,
      description: hotel.description, address: hotel.address, images: hotel.images,
    }));
  }));
  return groups.flat().sort((left, right) => left.name.localeCompare(right.name) || left.slug.localeCompare(right.slug) || left.id.localeCompare(right.id));
}
