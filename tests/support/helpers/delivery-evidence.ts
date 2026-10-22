import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/adapters/runtime/types";

export async function deliveryEvidence(db:SqlDatabase,actor:WorkContext,orderId:string,note='Signed customer delivery confirmation') {
  const token=crypto.randomUUID(),id=`attachment-${token}`,uploadId=`upload-${token}`,now=Date.now(),objectKey=`test-delivery/${token}`;
  await db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version) VALUES(?,'commerce_order_delivery',?,'proof_of_delivery','proof-of-delivery.pdf','application/pdf',8,8,?,?,?,'ready',?,?,?,?,1)")
    .bind(uploadId,orderId,token,`test-staging/${token}`,objectKey,actor.userId,now+60000,now,now).run();
  await db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,'commerce_order_delivery',?,'proof_of_delivery',?,1,'proof-of-delivery.pdf','application/pdf',8,?,?,?)")
    .bind(id,uploadId,orderId,objectKey,token,actor.userId,now).run();
  return {evidenceIds:[id],evidenceNote:note};
}
