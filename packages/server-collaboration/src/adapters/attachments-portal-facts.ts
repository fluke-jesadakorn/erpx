import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {nativeOwnerWriteSystemUnscoped} from "@erpx/server-collaboration/adapters/native-owner-write";
export function portalAttachmentCollaborationFacts(db:SqlDatabase){return {
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="social_post"?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=? AND state='published')",[targetId,companyId,version]):null,
versionStep:async(targetType:string,targetId:string,expectedVersion:number,now:number)=>targetType==="social_post"?nativeOwnerWriteSystemUnscoped(db,'social_post_version_step',[now,targetId,expectedVersion],`attachment-version:${targetId}`,now):null
};}
