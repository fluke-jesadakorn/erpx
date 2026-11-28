import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { TransactionalSqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { bookingEnv } from "@erpx/pms-server/public/pms-booking";
import { publicChangeQuote, confirmBookingChange } from "@erpx/pms-server/public/pms-booking-changes";
import { createStaffBookingChange } from "@erpx/pms-server/application/staff-booking-changes";
export async function staffBookingChange(db: TransactionalSqlDatabase, actor: WorkContext, slug: string, raw: unknown, key: string, now = Date.now()) {
    return createStaffBookingChange({ clock: { now: () => Date.now() }, environment: bookingEnv, quote: (env, authority, kind, items, now) => publicChangeQuote(db, "", kind, { items }, env, now, authority), confirm: (env, slug, authority, kind, token, key, now) => confirmBookingChange(db, slug, "", token, key, env, now, authority, kind) })(actor, slug, raw, key, now);
}
