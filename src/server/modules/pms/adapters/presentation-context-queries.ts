import "server-only";
import { bookingDatabase } from "@/server/modules/pms/public/pms-booking";
import { bookingCatalog } from "@/server/modules/pms/public/pms-booking";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function bookingPresentationQueries(holdingSlug: string) {
  const db = await bookingDatabase(holdingSlug);
  return {
    bookingCatalog: (...args: ReadArguments<typeof bookingCatalog>) => bookingCatalog(db, ...args),
  };
}
