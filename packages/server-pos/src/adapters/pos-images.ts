import { nativePosOne, nativePosRun } from './native-owner-write';
import { nativePosRegisterCatalogStatement, nativePosCatalogStatement, nativePosProductImagesStatement } from './native-pos-core-statement';
import {matchesUploadSignature} from "@erpx/server-files/public/rules/upload-validation";
import 'server-only';
import sharp from 'sharp';
import { createHash } from 'node:crypto';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import {posCommand} from '@erpx/server-pos/public/pos-core';
import { readBoundedUploadBody } from "@erpx/server-files/public/attachments/upload";
import { createAttachmentSet, reserveAttachmentFile, completeAttachmentUpload, scanAttachmentNow } from "@erpx/server-files/public/attachments/service";
import { requirePosImageSource } from "@erpx/server-pos/public/pos-image-source";
import { authorizePosProductImageTarget } from "@erpx/server-files/public/attachments/policy";
import type { SqlDatabase, FileStore } from "@erpx/server-runtime/public/runtime/types";
import { createPosImages, type PosImagesUnit } from "@erpx/server-pos/application/pos-images";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePosPurchased } from "@erpx/server-pos/adapters/purchase-boundary";
import { randomUUID } from "node:crypto";
import { posImagesCommerceFacts } from "@erpx/server-commerce/public/pos-images-facts";
import { posImageSourceFacts } from "@erpx/server-files/public/pos-images-facts";
import {withPosProductImageRead} from '@erpx/server-pos/adapters/product-image-read';
import { writeAttachmentEvents1 as nativeWriteAttachmentEvents1 } from '@erpx/server-collaboration/adapters/native-attachment-access-event';
function unit(db: SqlDatabase, files: FileStore): PosImagesUnit {
    return {
        readPosCatalog1: (facts: {
            productId: string;
            companyId: string;
        }) => nativePosCatalogStatement(db, facts.productId, facts.companyId),
        readPosRegisterCatalog1: (facts: {
            productId: string;
            branchId: string;
            companyId: string;
        }) => nativePosRegisterCatalogStatement(db, facts.productId, facts.branchId, facts.companyId),
        readPosProductImages1: (facts: {
            imageId: string;
            companyId: string;
            productId: string;
        }) => nativePosProductImagesStatement(db, facts.imageId, facts.companyId, facts.productId),
        readCommerceProducts1: facts => posImagesCommerceFacts(db).readCommerceProducts1(facts),
        readAttachments1: facts => posImageSourceFacts(db).readAttachments1(facts),
        writePosProductImages1: (facts: {
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
        }) => nativePosRun(db, facts.companyId, 'images_writeposproductimages1_60', [facts.imageId, facts.companyId, facts.productId, facts.object_key, facts.preview_object_key, facts.digest, facts.width, facts.height, facts.mime, facts.source, facts.fact10, facts.userId, facts.fact12, facts.sourceId, facts.attachmentLinkId, facts.id, facts.preview_checksum_sha256]),
        writeAttachmentEvents1: async facts => { const stmt = await nativeWriteAttachmentEvents1(db, facts); await stmt.run(); return { results: [] }; },
        authorizePosProductImageTarget: (arg0: WorkContext, arg1: string, arg2: "write") => authorizePosProductImageTarget(db, arg0, arg1, arg2),
        createAttachmentSet: (arg0: WorkContext, arg1: {
            targetType: string;
            targetId: string;
            componentKind: "record";
            componentRef: string;
            purposeKey: string;
            expectedTargetVersion: number;
        }, arg2: string, arg3: {
            compatibilityAdapter: boolean;
        }) => createAttachmentSet(db, arg0, arg1, arg2, arg3),
        reserveAttachmentFile: (arg0: WorkContext, arg1: string, arg2: {
            filename: string;
            mediaType: string;
            bytes: number;
        }, arg3: string, arg4: {
            compatibilityAdapter: boolean;
        }) => reserveAttachmentFile(db, arg0, arg1, arg2, arg3, arg4),
        completeAttachmentUpload: (arg0: WorkContext, arg1: string, arg2: ArrayBuffer, arg3: {
            compatibilityAdapter: boolean;
        }) => completeAttachmentUpload(db, files, arg0, arg1, arg2, arg3),
        scanAttachmentNow: (arg0: string) => scanAttachmentNow(db, files, arg0),
        requirePosImageSource: (arg0: WorkContext, arg1: string) => requirePosImageSource(db, arg0, arg1),
        posCommand: (actor, command, payload, key) => posCommand(db as PostgresDatabase, actor, command, payload, key), file: key => files.get(key)
    };
}
function imageApplication(forActor:(actor:WorkContext)=>Promise<PosImagesUnit>){return createPosImages({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor, imageMetadata: bytes => sharp(bytes, { limitInputPixels: 25000000, animated: false }).metadata(), matchesSignature:matchesUploadSignature,readBody: readBoundedUploadBody });}
const application = imageApplication(async actor=>{const {write,files}=await holdingResources(actor.holdingId);return unit(write,files);});
export async function savePosProductImage(actor: WorkContext, productId: string, bytes: Uint8Array, mime: string, source: 'generated' | 'upload', key: string, expectedVersion: number, metadata: Record<string, unknown> = {}) { requirePermission(actor, "pos.configure"); await requirePosPurchased((await holdingResources(actor.holdingId)).write, actor, "pos.sales", "new-work"); return application.savePosProductImage(actor, productId, bytes, mime, source, key, expectedVersion, metadata); }
export async function uploadPosProductImage(request: Request, actor: WorkContext) { requirePermission(actor, "pos.configure"); await requirePosPurchased((await holdingResources(actor.holdingId)).write, actor, "pos.sales", "new-work"); return application.uploadPosProductImage(request, actor); }
export async function posProductImage(actor: WorkContext, imageId: string, original = false) { requirePermission(actor, "pos.read"); const {write,files}=await holdingResources(actor.holdingId);await requirePosPurchased(write, actor); const view = await withPosProductImageRead(write,actor,imageId,async database=>imageApplication(async()=>unit(database,files)).posProductImage(actor,imageId,original)); return new Response(view.buffer, { headers: { "Content-Type": view.mediaType, "Content-Length": String(view.size), "Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff", "Content-Security-Policy": "default-src 'none'" } }); }
