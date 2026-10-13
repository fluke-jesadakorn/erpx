import { parseGuestAccountJson } from "@/lib/identity_access/guest-account-compat";
import { z } from "zod";
import type { Clock } from "../../shared/application/ports";
import type { Entry, GuestAccount, Guest, Snapshot } from "../domain/pms-model";
export type GuestDocumentUnit = {
    authorized(token: string, now: number): Promise<{
        s: Snapshot;
        id: string;
    }>;
    document(id: string, propertyId: string, reservationId: string): Promise<{
        kind: string;
        snapshot_json: string;
        created_at: number;
    } | null>;
};
export function createGuestBookingDocument(ports: {
    repository: {
        transaction<Result>(work: (unit: GuestDocumentUnit) => Promise<Result>): Promise<Result>;
    };
    clock: Clock;
    error(status: number, code: string, message: string): Error;
}) {
    return async (token: string, raw: unknown, now = ports.clock.now()) => {
        const { id: documentId } = z.object({ id: z.string().min(1).max(128) }).strict().parse(raw);
        return ports.repository.transaction(async (unit) => {
            const { s, id } = await unit.authorized(token, now), row = await unit.document(documentId, s.property.id, id);
            if (!row)
                throw ports.error(404, "PMS_DOCUMENT_NOT_FOUND", "Document not found for this booking");
            const snapshot = parseGuestAccountJson(row.snapshot_json) as {
                property: {
                    name: string;
                    currency: string;
                };
                guest: Guest;
                guestAccount: GuestAccount;
                entries: Array<{
                    data: Entry;
                }>;
                amount: number;
                businessDate: string;
            };
            return { number: `PMS-${documentId.slice(-12)}`, kind: row.kind, issuedAt: Number(row.created_at), date: snapshot.businessDate, hotel: snapshot.property.name, currency: snapshot.property.currency, guest: snapshot.guest.name, guestAccount: snapshot.guestAccount.name, amount: snapshot.amount, dueAtIssue: snapshot.guestAccount.due, entries: snapshot.entries.map(({ data: e }) => ({ date: e.date, description: e.description, net: e.net, service: e.service, tax: e.tax, total: e.total })) };
        });
    };
}
