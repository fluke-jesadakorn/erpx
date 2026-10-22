import type { AttachmentTargetScope } from "@erpx/server-files/public/attachments/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
export type ImageSource = {
    id: string;
    product_id: string;
    object_key: string;
    thumbnail_key: string;
    mime_type: string;
    sha256: string;
    thumbnail_checksum_sha256: string;
    attachment_source_id: string;
    attachment_link_id: string;
    attachment_set_id: string;
    set_state: string;
    link_state: string;
    expected_target_version: number;
    component_ref: string | null;
};
export type PosRows<Row> = {
    results: Row[];
};
export type PosReadEffect = {
    run(): Promise<PosRows<Record<string, unknown>>>;
};
export type PosReadAttachmentBinding<Effect> = {
    guards: Effect[];
    steps: Effect[];
};
export type PosImageSourceUnit<Effect extends PosReadEffect> = {
    readPosProductImages1(facts: {
        imageId: string;
        companyId: string;
        fact2: string | null;
        fact3: string | null;
    }): Promise<ImageSource | null>;
    authorizePosProductImageTarget(arg0: WorkContext, arg1: string, arg2: "read" | "bind"): Promise<AttachmentTargetScope>;
    prepareAttachmentBinding(arg0: WorkContext, arg1: {
        attachmentSetIds: string[];
        bindingContract: {
            targetType: string;
            bindingType: string;
            contexts: {
                purposeKey: string;
                componentKind: "record";
                componentRef: string | null;
            }[];
        };
        targetId: string;
        expectedTargetVersion: number;
        commandId: string;
        bindingId: string;
    }, arg2: {
        compatibilityAdapter: true;
    }): Promise<PosReadAttachmentBinding<Effect>>;
    batch(effects: Effect[]): Promise<unknown>;
};
export function createPosImageSource<Effect extends PosReadEffect>(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    environment(key: "NODE_ENV" | "ERPX_POS_ROOM_CHARGE_PILOT_ENABLED" | "POS_STRIPE_LIVE_ENABLED"): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PosImageSourceUnit<Effect>>;
}) {
    async function requirePosImageSource(db: PosImageSourceUnit<Effect>, actor: WorkContext, imageId: string, productId?: string, bindKey?: string) {
        const row = await db.readPosProductImages1({ imageId: imageId, companyId: actor.companyId, fact2: productId ?? null, fact3: productId ?? null });
        if (!row)
            throw ports.error(404, "POS_IMAGE_UNAVAILABLE", "Product image is unavailable until its File Source passes scanning.");
        await db.authorizePosProductImageTarget(actor, row.product_id, bindKey ? "bind" : "read");
        if (row.set_state === "draft" && row.link_state === "draft" && bindKey) {
            const binding = await db.prepareAttachmentBinding(actor, {
                attachmentSetIds: [row.attachment_set_id],
                bindingContract: { targetType: "product", bindingType: "record_document", contexts: [{ purposeKey: "product_image", componentKind: "record", componentRef: row.component_ref }] },
                targetId: row.product_id, expectedTargetVersion: Number(row.expected_target_version), commandId: bindKey, bindingId: row.product_id,
            }, { compatibilityAdapter: true });
            await db.batch([...binding.guards, ...binding.steps]);
        }
        else if (row.set_state !== "bound" || row.link_state !== "bound") {
            throw ports.error(404, "POS_IMAGE_UNAVAILABLE", "Product image has no current bound File Source.");
        }
        return row;
    }
    return { requirePosImageSource };
}
