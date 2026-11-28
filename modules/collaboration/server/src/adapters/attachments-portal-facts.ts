import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {nativeOwnerWriteSystemUnscoped} from "@erpx/collaboration-server/adapters/native-owner-write";
import {attachmentVersionGuard} from './read-tail-sql';
export function portalAttachmentCollaborationFacts(db:SqlDatabase){return {
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="social_post"?assertion(db,commandId,`attachment-owner-${index}`,attachmentVersionGuard('social_posts', " AND state='published'"),[targetId,companyId,version]):null,
versionStep:async(targetType:string,targetId:string,expectedVersion:number,now:number)=>targetType==="social_post"?nativeOwnerWriteSystemUnscoped(db,'social_post_version_step',[now,targetId,expectedVersion],`attachment-version:${targetId}`,now):null
};}
