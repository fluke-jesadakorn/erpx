import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { nativeOwnerWrite } from "./native-owner-write";
import { noopSql, receiptByIdSql } from "./read-tail-sql";
import "server-only";
import { createHash } from "node:crypto";
import {
  currentSession,
  publicScope,
  requirePlatformContext,
  requireWorkContext,
} from "@erpx/server-identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { navigationContext } from "@erpx/server-work_projection/public/navigation-context";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";

import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {workLayoutAuthorityFacts} from "@erpx/server-identity_access/public/work-ui-layout-authority";
import {workLayoutClearAssertions} from "@erpx/server-workflow/public/work-ui-layout-effects";
import {createPageLayoutService,type PageLayoutUnit} from "@erpx/server-work_projection/application/page-layouts";
const keyOf=(...parts:(string|number)[])=>parts.join(":");
function unit(db:SqlDatabase):PageLayoutUnit<SqlStatement>{const noop=()=>db.prepare(noopSql);return {companyAuthority:actor=>companyAuthority(db,actor),layouts:async(company,page,user,base)=>(await db.prepare("SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=? AND page_key=? AND ((owner_id=? AND workspace_base=?) OR (owner_id=? AND workspace_base=?))").bind(company,page,user,base,"organization","*").all<{owner_id:string;layout_json:string|null;version:number}>()).results,lock:scope=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(scope).run(),workGuards:(actor,key)=>workContextGuards(db,actor,key),iamGuards:(actor,key)=>iamMutationGuards(db,actor,key),clearAssertions:(key:string,companyId:string)=>workLayoutClearAssertions(db,key,companyId),batch:effects=>db.batch(effects),...workLayoutAuthorityFacts(db),receipt:key=>db.prepare(receiptByIdSql('request_hash,result_json','page_layout_receipts')).bind(key).first<{request_hash:string;result_json:string}>(),save:async(company,owner,base,page,value,version,now)=>nativeOwnerWrite('wp_page_layout_save',[company,owner,base,page,value,version,now],`wp-layout:${keyOf(company,page,owner,base,version)}`,{companyId:company,now},noop),saveReceipt:async(key,hash,result,user,company,base,page,action,now)=>nativeOwnerWrite('wp_page_layout_receipt_save',[key,hash,result,user,company,base,page,action,now],`wp-layout-receipt:${key}`,{companyId:company,now},noop)};}
export async function pageLayoutRequest(request:Request,pageKey:string,holdingSlug?:string,account=false){let db:PostgresDatabase;const service=createPageLayoutService({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,hash:value=>createHash("sha256").update(value).digest("hex"),currentSession,requireWorkContext,requirePlatformContext,prepare:async work=>{db=(work?(await holdingResources(work.holdingId)).write:(await runtimeEnv()).CONTROL_DB) as PostgresDatabase;},unit:()=>unit(db),navigation:navigationContext,base:actor=>workspaceBase(publicScope(actor)),transaction:action=>db.transaction(tx=>action(unit(tx)))});try{return Response.json(canonicalDestinationLinks({data:await service(request.method,()=>serviceCommandBody(request),pageKey,holdingSlug,account)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
