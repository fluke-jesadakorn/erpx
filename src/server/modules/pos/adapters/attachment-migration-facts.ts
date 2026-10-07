import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import type {SqlDatabase} from '../../runtime/public/runtime/types';export function attachmentMigrationpos(database:SqlDatabase){return {readBackfillSpecializedAttachmentsImages(){return database.prepare(`SELECT image.*,product.version product_version FROM pos_product_images image
    JOIN (${commerceProjectionSources.commerce_products}) product ON product.id=image.product_id AND product.company_id=image.company_id
    WHERE image.attachment_source_id IS NULL ORDER BY image.created_at,image.id`).all<Record<string,unknown>>();},
stageBackfillSpecializedAttachmentsEffect13(setId:string,linkId:string,attachmentId:string,checksum:string,id:string){return database.prepare("UPDATE pos_product_images SET attachment_set_id=?,attachment_link_id=?,attachment_source_id=?,thumbnail_checksum_sha256=? WHERE id=? AND attachment_source_id IS NULL")
              .bind(setId,linkId,attachmentId,checksum,id);}};}
