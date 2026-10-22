import { parseGuestAccountJson } from "@erpx/feature-identity_access/lib/guest-account-compat";
import { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { PmsRecord, Snapshot } from "@erpx/server-pms/domain/pms-model";
import type { PmsHotelOverview } from "@erpx/server-pms/domain/home";
export type PmsFinancialRows<Row> = {
    results: Row[];
};
export type PmsRecordDetailsRepository = {
    refund(facts: {
        id: string;
        companyId: string;
        kind: "booking" | "counter";
    }): Promise<Record<string, string | number | null> | null>;
    document(facts: {
        documentId: string;
    }): Promise<{
        property_id: string;
        snapshot_json: string;
    } | null>;
    controlRows(facts: {
        propertyId: string;
        account: string;
        offset: number;
    }): Promise<PmsFinancialRows<Record<string, unknown>>>;
    controlBalance(facts: {
        propertyId: string;
        account: string;
    }): Promise<{
        balance: number;
    } | null>;
    hotel(actor: WorkContext, id: string): Promise<Snapshot>;
    hotels(actor: WorkContext, id?: string): Promise<PmsHotelOverview[]>;
};
export function createPmsRecordDetails(ports: {
    repository(actor: WorkContext): Promise<PmsRecordDetailsRepository>;
    clock: Clock;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
}) {
    /** Console finance queues keep their original company-level visibility. */
    async function pmsRefundRecord(actor: WorkContext, kind: 'booking' | 'counter', id: string) {
        ports.require(actor, hasPermission(actor,'pms.read') ? 'pms.read' : 'finance.read');
        if (!hasPermission(actor, 'finance.approve') || actor.branchId || actor.departmentId)
            throw ports.error(403, 'PMS_REFUND_REVIEW_FORBIDDEN', 'Finance refund review access is required.');
        const write = await ports.repository(actor);
        const row = await write.refund({ id: id, companyId: actor.companyId, kind: kind });
        if (!row)
            throw ports.error(404, 'REFUND_NOT_FOUND', 'Refund not found in this company.');
        await write.hotel(actor, String(row.property_id));
        return { row: { ...row, name: kind === 'booking' ? 'Online booking refund' : 'Counter payment refund', currency: 'THB' }, refundKind: kind };
    }
    async function pmsSnapshotEntry(actor: WorkContext, id: string, documentId: string) {
        ports.require(actor, hasPermission(actor,'pms.read') ? 'pms.read' : 'finance.read');
        if (!hasPermission(actor, 'pms.cashier') && !hasPermission(actor, 'pms.reports'))
            throw ports.error(403, 'PMS_DOCUMENT_FORBIDDEN', 'Document access is required.');
        const write = await ports.repository(actor);
        const doc = await write.document({ documentId: documentId });
        if (!doc)
            throw ports.error(404, 'DOCUMENT_NOT_FOUND', 'Property document not found.');
        await write.hotel(actor, doc.property_id);
        const snap = parseGuestAccountJson(doc.snapshot_json) as {
            entries: PmsRecord<'entry'>[];
            property: {
                currency: string;
            };
        };
        const entry = snap.entries.find(row => row.id === id);
        if (!entry)
            throw ports.error(404, 'ENTRY_NOT_FOUND', 'Entry not found in this retained document.');
        const { date, kind, description, reference, net, service, tax, total, method, originalId } = entry.data;
        return { row: { id, name: description, description, version: entry.version, currency: snap.property.currency, date, kind, reference, net, service, tax, total, method, originalId }, related: [{ kind: 'pms_document' as const, id: documentId }] };
    }
    async function pmsControlSources(actor: WorkContext, propertyId: string, key: string, page: number) {
        ports.require(actor, hasPermission(actor,'pms.read') ? 'pms.read' : 'finance.read');
        ports.require(actor, 'pms.reports');
        const control = z.enum(['receivable', 'advance', 'deposit']).parse(key), write = await ports.repository(actor), snapshot = await write.hotel(actor, propertyId), account = snapshot.property.config.accounts[control];
        const rows = (await write.controlRows({ propertyId: propertyId, account: account, offset: (page - 1) * 30 })).results;
        const amount = await write.controlBalance({ propertyId: propertyId, account: account });
        return { row: { name: `${snapshot.property.name} · ${control}`, currency: snapshot.property.config.currency, amount_minor: Number(amount?.balance ?? 0) * (control === 'receivable' ? 1 : -1) }, rows, page, hasNext: rows.length > 30 };
    }
    return { pmsRefundRecord, pmsSnapshotEntry, pmsControlSources };
}
