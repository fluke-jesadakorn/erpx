import type { AttachmentTargetScope, AttachmentSetDto } from "../../files/public/attachments/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import type { PosRow } from "../domain/pos-model";
export type PosRows<Row> = {
    results: Row[];
};
export type PosImagesUnit = {
    readPosCatalog1(facts: {
        productId: string;
        companyId: string;
    }): Promise<PosRow | null>;
    readPosRegisterCatalog1(facts: {
        productId: string;
        branchId: string;
        companyId: string;
    }): Promise<PosRow | null>;
    readPosProductImages1(facts: {
        imageId: string;
        companyId: string;
        productId: string;
    }): Promise<PosRow | null>;
    readCommerceProducts1(facts: {
        productId: string;
        companyId: string;
    }): Promise<PosRow | null>;
    readAttachments1(facts: {
        sourceId: string;
        companyId: string;
    }): Promise<PosRow | null>;
    writePosProductImages1(facts: {
        imageId: string;
        companyId: string;
        productId: string;
        object_key: string | number | boolean | null;
        preview_object_key: string | number | true;
        digest: string;
        width: number;
        height: number;
        mime: string;
        source: "generated" | "upload";
        fact10: string;
        userId: string;
        fact12: number;
        sourceId: string;
        attachmentLinkId: string;
        id: string;
        preview_checksum_sha256: string | number | boolean | null;
    }): Promise<PosRows<Record<string, unknown>>>;
    writeAttachmentEvents1(facts: {
        fact0: string;
        companyId: string;
        attachment_source_id: string;
        attachment_link_id: string;
        attachment_set_id: string;
        userId: string;
        fact6: string;
        fact7: number;
    }): Promise<PosRows<Record<string, unknown>>>;
    authorizePosProductImageTarget(arg0: WorkContext, arg1: string, arg2: "write"): Promise<AttachmentTargetScope>;
    createAttachmentSet(arg0: WorkContext, arg1: {
        targetType: string;
        targetId: string;
        componentKind: "record";
        componentRef: string;
        purposeKey: string;
        expectedTargetVersion: number;
    }, arg2: string, arg3: {
        compatibilityAdapter: boolean;
    }): Promise<AttachmentSetDto>;
    reserveAttachmentFile(arg0: WorkContext, arg1: string, arg2: {
        filename: string;
        mediaType: string;
        bytes: number;
    }, arg3: string, arg4: {
        compatibilityAdapter: boolean;
    }): Promise<{
        uploadIntentId: string;
        attachmentLinkId: string;
        uploadUrl: string;
        expiresAt: number;
    }>;
    completeAttachmentUpload(arg0: WorkContext, arg1: string, arg2: ArrayBuffer, arg3: {
        compatibilityAdapter: boolean;
    }): Promise<{
        uploadIntentId: string;
        attachmentLinkId: string;
        sourceId: string;
        state: "legacy_unscanned" | "quarantined" | "scanning" | "available" | "scan_error" | "malware_rejected" | "destroyed";
        status: "scan_pending";
    }>;
    scanAttachmentNow(arg0: string): Promise<"legacy_unscanned" | "quarantined" | "scanning" | "available" | "scan_error" | "malware_rejected" | "destroyed">;
    requirePosImageSource(arg0: WorkContext, arg1: string): Promise<{
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
    }>;
    posCommand(actor: WorkContext, command: string, payload: unknown, key: string): Promise<Record<string, unknown>>;
    file(key: string): Promise<{
        arrayBuffer(): Promise<ArrayBuffer>;
        size: number;
    } | null>;
};
export function createPosImages(ports: {
    clock: Clock;
    uuid(): string;
    hash(value: string | Uint8Array): string;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PosImagesUnit>;
    imageMetadata(bytes: Uint8Array): Promise<{
        width?: number;
        height?: number;
        pages?: number;
    }>;
    readBody(request: Request, length: number): Promise<ArrayBuffer>;
    matchesSignature(type:string,bytes:Uint8Array):boolean;
}) {
    function fail(code: string, message: string, status = 409): never { throw ports.error(status, code, message); }
    async function savePosProductImage(actor: WorkContext, productId: string, bytes: Uint8Array, mime: string, source: 'generated' | 'upload', key: string, expectedVersion: number, metadata: Record<string, unknown> = {}) {
        ports.require(actor, 'pos.configure');
        if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key) || !Number.isSafeInteger(expectedVersion) || expectedVersion < 1)
            fail('POS_IMAGE_INPUT', 'Use a command key and current catalog version', 422);
        if (!['image/png', 'image/jpeg', 'image/webp'].includes(mime) || !ports.matchesSignature(mime, bytes) || bytes.byteLength > 10 * 1024 * 1024)
            fail('POS_IMAGE_FORMAT', 'Use a PNG, JPEG or WebP image up to 10 MB', 422);
        const write = await ports.forActor(actor);
        const catalog = await write.readPosCatalog1({ productId: productId, companyId: actor.companyId });
        if (!catalog)
            fail('POS_PRODUCT_INVALID', 'Save the product catalog first', 422);
        if (actor.departmentId || actor.branchId && !await write.readPosRegisterCatalog1({ productId: productId, branchId: actor.branchId, companyId: actor.companyId }))
            fail('POS_PRODUCT_INVALID', 'Catalog is outside this Branch', 404);
        const digest = ports.hash(bytes), imageId = `pos-image-${ports.hash(`${actor.companyId}:${productId}:${digest}`)}`;
        const prior = await write.readPosProductImages1({ imageId: imageId, companyId: actor.companyId, productId: productId });
        if (prior?.attachment_source_id)
            return write.posCommand(actor, 'catalog.image', { productId, imageId, expectedVersion }, key);
        if (Number(catalog.version) !== expectedVersion)
            fail('POS_VERSION_CONFLICT', 'Product data changed');
        await write.authorizePosProductImageTarget(actor, productId, 'write');
        const info = await ports.imageMetadata(bytes);
        if (!info.width || !info.height || info.pages && info.pages > 1)
            fail('POS_IMAGE_FORMAT', 'Use a single static image', 422);
        const product = await write.readCommerceProducts1({ productId: productId, companyId: actor.companyId });
        if (!product)
            fail('POS_PRODUCT_INVALID', 'Product not found', 404);
        const options = { compatibilityAdapter: true };
        const set = await write.createAttachmentSet(actor, { targetType: 'product', targetId: productId, componentKind: 'record', componentRef: imageId,
            purposeKey: 'product_image', expectedTargetVersion: Number(product.version) }, `${key}:image-set`, options);
        const reserved = await write.reserveAttachmentFile(actor, set.id, { filename: `${imageId}.${mime === 'image/png' ? 'png' : mime === 'image/webp' ? 'webp' : 'jpg'}`,
            mediaType: mime, bytes: bytes.byteLength }, `${key}:image-file`, options);
        const uploaded = await write.completeAttachmentUpload(actor, reserved.uploadIntentId, bytes.slice().buffer, options);
        const state = await write.scanAttachmentNow(uploaded.sourceId);
        if (state !== 'available')
            fail(state === 'malware_rejected' ? 'POS_IMAGE_REJECTED' : 'POS_IMAGE_SCAN_PENDING', 'The image must pass file scanning before it can be used', 409);
        const available = await write.readAttachments1({ sourceId: uploaded.sourceId, companyId: actor.companyId });
        if (!available?.preview_object_key)
            fail('POS_IMAGE_PREVIEW_UNAVAILABLE', 'A safe image preview could not be generated', 409);
        await write.writePosProductImages1({ imageId: imageId, companyId: actor.companyId, productId: productId, object_key: available.object_key, preview_object_key: available.preview_object_key, digest: digest, width: info.width, height: info.height, mime: mime, source: source, fact10: JSON.stringify(metadata), userId: actor.userId, fact12: ports.clock.now(), sourceId: uploaded.sourceId, attachmentLinkId: reserved.attachmentLinkId, id: set.id, preview_checksum_sha256: available.preview_checksum_sha256 });
        return write.posCommand(actor, 'catalog.image', { productId, imageId, expectedVersion }, key);
    }
    async function uploadPosProductImage(request: Request, actor: WorkContext) {
        ports.require(actor, 'pos.configure');
        const length = Number(request.headers.get('content-length'));
        const bytes = new Uint8Array(await ports.readBody(request, length));
        return savePosProductImage(actor, request.headers.get('x-pos-product') ?? '', bytes, request.headers.get('content-type') ?? '', 'upload', request.headers.get('Idempotency-Key') ?? '', Number(request.headers.get('x-pos-catalog-version')));
    }
    async function posProductImage(actor: WorkContext, imageId: string, original = false) {
        ports.require(actor, 'pos.read');
        const write = await ports.forActor(actor);
        const row = await write.requirePosImageSource(actor, imageId);
        const file = await write.file(original ? row.object_key : row.thumbnail_key);
        if (!file)
            fail('POS_IMAGE_MISSING', 'Image bytes are unavailable', 404);
        const buffer = await file.arrayBuffer(), checksum = ports.hash(new Uint8Array(buffer));
        if (checksum !== (original ? row.sha256 : row.thumbnail_checksum_sha256))
            fail('POS_IMAGE_CHECKSUM_MISMATCH', 'Product image integrity check failed', 409);
        await write.writeAttachmentEvents1({ fact0: ports.uuid(), companyId: actor.companyId, attachment_source_id: row.attachment_source_id, attachment_link_id: row.attachment_link_id, attachment_set_id: row.attachment_set_id, userId: actor.userId, fact6: JSON.stringify({ owner: 'pos_product_image', variant: original ? 'original' : 'preview' }), fact7: ports.clock.now() });
        return { buffer, mediaType: original ? row.mime_type : 'image/webp', size: file.size };
    }
    return { savePosProductImage, uploadPosProductImage, posProductImage };
}
