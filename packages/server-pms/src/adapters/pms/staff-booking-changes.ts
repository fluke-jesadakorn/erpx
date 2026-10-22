import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { TransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { bookingEnv } from "@erpx/server-pms/public/pms-booking";
import { publicChangeQuote, confirmBookingChange } from "@erpx/server-pms/public/pms-booking-changes";
import { createStaffBookingChange } from "@erpx/server-pms/application/staff-booking-changes";
export async function staffBookingChange(db: TransactionalSqlDatabase, actor: WorkContext, slug: string, raw: unknown, key: string, now = Date.now()) {
    return createStaffBookingChange({ clock: { now: () => Date.now() }, environment: bookingEnv, quote: (env, authority, kind, items, now) => publicChangeQuote(db, "", kind, { items }, env, now, authority), confirm: (env, slug, authority, kind, token, key, now) => confirmBookingChange(db, slug, "", token, key, env, now, authority, kind) })(actor, slug, raw, key, now);
}
