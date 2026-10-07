import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export function portalAttachmentCollaborationFacts(db:SqlDatabase){return {
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="social_post"?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=? AND state='published')",[targetId,companyId,version]):null,
versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>targetType==="social_post"?db.prepare("UPDATE social_posts SET version=version+1,updated_at=? WHERE id=? AND version=? AND state='published'").bind(now,targetId,expectedVersion):null
};}
