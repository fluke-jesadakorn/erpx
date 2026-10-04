import "server-only";

import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { authorizePosProductImageTarget } from "./attachments/policy";
import { prepareAttachmentBinding } from "./attachments/service";

type ImageSource = {
  id: string; product_id: string; object_key: string; thumbnail_key: string; mime_type: string;
  sha256: string; thumbnail_checksum_sha256: string; attachment_source_id: string; attachment_link_id: string;
  attachment_set_id: string; set_state: string; link_state: string; expected_target_version: number;
  component_ref: string | null;
};

export async function requirePosImageSource(db: SqlDatabase, actor: WorkContext, imageId: string, productId?: string, bindKey?: string) {
  const row = await db.prepare(`SELECT image.*,set_row.state set_state,link.state link_state,
      set_row.expected_target_version,set_row.component_ref
    FROM pos_product_images image
    JOIN attachments source ON source.id=image.attachment_source_id AND source.company_id=image.company_id
      AND source.owner_type='product' AND source.owner_id=image.product_id AND source.source_state='available'
      AND source.checksum_sha256=image.sha256 AND source.object_key=image.object_key
    JOIN attachment_links link ON link.id=image.attachment_link_id AND link.attachment_id=source.id
    JOIN attachment_sets set_row ON set_row.id=image.attachment_set_id AND link.set_id=set_row.id
      AND set_row.company_id=image.company_id AND set_row.target_type='product' AND set_row.target_id=image.product_id
      AND set_row.purpose_key='product_image' AND set_row.component_kind='record'
    WHERE image.id=? AND image.company_id=? AND (?::text IS NULL OR image.product_id=?)
    FOR SHARE OF image,source,link,set_row`)
    .bind(imageId,actor.companyId,productId??null,productId??null).first<ImageSource>();
  if (!row) throw new ApiError(404,"POS_IMAGE_UNAVAILABLE","Product image is unavailable until its File Source passes scanning.");
  await authorizePosProductImageTarget(db,actor,row.product_id,bindKey?"bind":"read");
  if (row.set_state==="draft" && row.link_state==="draft" && bindKey) {
    const binding = await prepareAttachmentBinding(db,actor,{
      attachmentSetIds:[row.attachment_set_id],
      bindingContract:{targetType:"product",bindingType:"record_document",contexts:[{purposeKey:"product_image",componentKind:"record",componentRef:row.component_ref}]},
      targetId:row.product_id,expectedTargetVersion:Number(row.expected_target_version),commandId:bindKey,bindingId:row.product_id,
    },{compatibilityAdapter:true});
    await db.batch([...binding.guards,...binding.steps]);
  } else if (row.set_state!=="bound" || row.link_state!=="bound") {
    throw new ApiError(404,"POS_IMAGE_UNAVAILABLE","Product image has no current bound File Source.");
  }
  return row;
}
