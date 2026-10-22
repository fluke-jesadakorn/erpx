import { lastAuditOwnerProjectionSources as filesLastAuditProjectionSources } from "@erpx/server-files/public/last-audit-owner-projection-sources";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { authorizePosProductImageTarget } from "@erpx/server-files/public/attachments/policy";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { createPosImageSource, type PosImageSourceUnit, type ImageSource } from "@erpx/server-pos/application/pos-image-source";
import { createHash, randomUUID } from "node:crypto";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
function unit(db: SqlDatabase): PosImageSourceUnit<SqlStatement> {
    return {
        readPosProductImages1: (facts: {
            imageId: string;
            companyId: string;
            fact2: string | null;
            fact3: string | null;
        }) => db.prepare(`SELECT image.*,set_row.state set_state,link.state link_state,
      set_row.expected_target_version,set_row.component_ref
    FROM pos_product_images image
    JOIN (${filesLastAuditProjectionSources.attachments}) source ON source.id=image.attachment_source_id AND source.company_id=image.company_id
      AND source.owner_type='product' AND source.owner_id=image.product_id AND source.source_state='available'
      AND source.checksum_sha256=image.sha256 AND source.object_key=image.object_key
    JOIN (${filesProjectionSources.attachment_links}) link ON link.id=image.attachment_link_id AND link.attachment_id=source.id
    JOIN (${filesProjectionSources.attachment_sets}) set_row ON set_row.id=image.attachment_set_id AND link.set_id=set_row.id
      AND set_row.company_id=image.company_id AND set_row.target_type='product' AND set_row.target_id=image.product_id
      AND set_row.purpose_key='product_image' AND set_row.component_kind='record'
    WHERE image.id=? AND image.company_id=? AND (?::text IS NULL OR image.product_id=?)
    FOR SHARE OF image,source,link,set_row`).bind(facts.imageId, facts.companyId, facts.fact2, facts.fact3).first<ImageSource>(),
        authorizePosProductImageTarget: (arg0: WorkContext, arg1: string, arg2: "read" | "bind") => authorizePosProductImageTarget(db, arg0, arg1, arg2),
        prepareAttachmentBinding: (arg0: WorkContext, arg1: {
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
        }) => prepareAttachmentBinding(db, arg0, arg1, arg2), batch: effects => db.batch(effects)
    };
}
const application = createPosImageSource<SqlStatement>({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), environment: key => process.env[key], require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write) });
export function requirePosImageSource(db: SqlDatabase, actor: WorkContext, imageId: string, productId?: string, bindKey?: string) { return application.requirePosImageSource(unit(db), actor, imageId, productId, bindKey); }
