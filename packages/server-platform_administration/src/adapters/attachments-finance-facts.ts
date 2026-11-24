import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-files/public/contracts/attachments-finance";
import { nativeOwnerWrite, NATIVE_OWNER_WRITE_NOOP_SQL } from "./native-owner-write";
import { releaseAcceptanceExistsSql, releaseAcceptanceRowSql } from "@erpx/server-platform_administration/adapters/release-predicate";
const target = "company_release_acceptance";
export function FinanceAttachmentPlatformAdministrationFacts(db:SqlDatabase){return {
 readOwner10:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>db.prepare(releaseAcceptanceRowSql('company_id,NULL::text branch_id,NULL::text department_id,version','id=? AND company_id=?')).bind(facts.targetId,facts.companyId).first<Row>(),
 versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number):SqlStatement|null=>targetType===target?assertion(db,commandId,`attachment-owner-${index}`,releaseAcceptanceExistsSql("id=? AND company_id=? AND version=?"),[targetId,companyId,version]):null,
 // Native axum owner-write command owns this version step; the returned
 // statement is an inert placeholder. No write SQL remains in TypeScript.
 versionStep:async(targetType:string,targetId:string,expectedVersion:number,now:number):Promise<SqlStatement|null>=>targetType===target
   ? nativeOwnerWrite('pl_attachment_version_step',[now,targetId,expectedVersion],`pl-attach:${targetId}:${expectedVersion}`,{now},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL))
   : null,
};}
