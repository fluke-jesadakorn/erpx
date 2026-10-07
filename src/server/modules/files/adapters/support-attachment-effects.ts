import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Preserve the caller's staged Files mutation inside the same business command transaction. */
export function supportAttachmentEffects(db:SqlDatabase){return {revoke:(id:string,at:number)=>db.prepare("UPDATE attachments SET revoked_at=? WHERE id=?").bind(at,id)};}
