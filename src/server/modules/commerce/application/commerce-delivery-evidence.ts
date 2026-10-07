import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import type { AttachmentManifestItem } from "../../files/public/attachments/types";
import type { FunctionalFxEvidence } from "../../finance/public/commerce-journal-contracts";
export type Row = Record<string, string | number | null>;
export type CommerceRows<Row> = {
    results: Row[];
};
export type CommerceAttachmentBinding<Effect> = {
    manifest: AttachmentManifestItem[];
    guards: Effect[];
    steps: Effect[];
};
export type CommerceJournal<Effect> = {
    steps: Effect[];
    guards: Effect[];
    preparedLines: Array<{
        debit: number;
        credit: number;
        baseDebit: number;
        baseCredit: number;
        accountId: string;
        description?: string;
        dimensions?: Record<string, string>;
    }>;
    preparedFxEvidence: FunctionalFxEvidence | null;
    result: {
        id: string;
        number: string;
        status: "draft" | "posted";
        version: number;
        debit: string;
        credit: string;
        fxRateMicros: number;
        baseCurrency: string;
    };
};
export type CommerceDeliveryUnit<Effect> = {
    readCommerceOrders1(facts: {
        orderId: string;
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        departmentId: string | null;
        departmentId2: string | null;
    }): Promise<Row | null>;
    guardRule1(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
    }): Effect;
};
export function createCommerceDeliveryApplication<Effect, Context = unknown>(ports: {
    clock: Clock;
    require(actor: WorkContext, permission: string): WorkContext;
    error(status: number, code: string, message: string): Error;
    hash(value: string): Promise<string>;
    forActor(actor: WorkContext): Promise<CommerceDeliveryUnit<Effect>>;
    forContext(context: Context): Promise<CommerceDeliveryUnit<Effect>>;
    attachmentReadable(): boolean;
    returnCompletionValues(input: {
        totalMinor: number;
        baseCreditMinor: number;
        outstandingMinor: number;
        baseOutstandingMinor: number;
    }): {
        creditedMinor: number;
        refundMinor: number;
        baseCreditedMinor: number;
        baseRefundMinor: number;
        realizedFxMinor: number;
    };
}) {
    async function deliveryEvidenceGuard(db: CommerceDeliveryUnit<Effect>, actor: WorkContext, orderId: string, expectedVersion: number, key: string) {
        ports.require(actor, 'inventory.prepare');
        const row = await db.readCommerceOrders1({ orderId: orderId, companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, departmentId: actor.departmentId, departmentId2: actor.departmentId });
        if (!row)
            throw ports.error(404, 'DELIVERY_ORDER_NOT_FOUND', 'The sales order is unavailable in this work scope or is not ready for delivery.');
        if (Number(row.version) !== expectedVersion)
            throw ports.error(409, 'VERSION_CONFLICT', 'The sales order changed. Refresh before attaching delivery evidence.');
        return { row, guards: [db.guardRule1({ key: key, guardId: 'delivery-evidence-order', values: [orderId, actor.companyId, expectedVersion, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] })] };
    }
    return { deliveryEvidenceGuard };
}
