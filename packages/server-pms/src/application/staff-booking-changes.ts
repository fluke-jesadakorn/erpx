import { z } from "zod";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export type StaffBookingAuthority = {
    actor: WorkContext;
    reservationId: string;
    reason: string;
    override: boolean;
};
export function createStaffBookingChange<Environment, Quote, Confirmation>(ports: {
    clock: Clock;
    environment(): Promise<Environment>;
    quote(environment: Environment, authority: StaffBookingAuthority, kind: "cancel" | "change", items: unknown, now: number): Promise<Quote>;
    confirm(environment: Environment, slug: string, authority: StaffBookingAuthority, kind: "cancel" | "change", token: string, key: string, now: number): Promise<Confirmation>;
}) {
    return async (actor: WorkContext, slug: string, raw: unknown, key: string, now = ports.clock.now()) => {
        const value = z.object({ action: z.enum(["change-quote", "cancel-quote", "change-confirm", "cancel-confirm"]), reservationId: z.string().min(1).max(128), reason: z.string().trim().min(3).max(2000), override: z.boolean().default(false), items: z.unknown().optional(), proposalToken: z.string().max(100000).optional() }).strict().parse(raw);
        const authority = { actor, reservationId: value.reservationId, reason: value.reason, override: value.override }, kind = value.action.startsWith("cancel") ? "cancel" : "change", environment = await ports.environment();
        if (value.action.endsWith("quote"))
            return ports.quote(environment, authority, kind, value.items, now);
        return ports.confirm(environment, slug, authority, kind, z.string().min(1).parse(value.proposalToken), key, now);
    };
}
