import "server-only";
import { createHash } from "node:crypto";
import {
  currentSession,
  publicScope,
  requirePlatformContext,
  requireWorkContext,
} from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { navigationContext } from "../public/navigation-context";
import { companyAuthority } from "@/server/modules/operations/public/staff-policy";
import { workContextGuards } from "@/server/modules/workflow/public/commands";
import { iamMutationGuards } from "@/server/modules/identity_access/public/iam-store";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { serviceCommandBody } from "@/server/modules/support/public/service-http";

import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {workLayoutAuthorityFacts} from "../../identity_access/public/work-ui-layout-authority";
import {workLayoutClearAssertions} from "../../workflow/public/work-ui-layout-effects";
import {createPageLayoutService,type PageLayoutUnit} from "../application/page-layouts";
function unit(db:SqlDatabase):PageLayoutUnit<SqlStatement>{return {companyAuthority:actor=>companyAuthority(db,actor),layouts:async(company,page,user,base)=>(await db.prepare("SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=? AND page_key=? AND ((owner_id=? AND workspace_base=?) OR (owner_id=? AND workspace_base=?))").bind(company,page,user,base,"organization","*").all<{owner_id:string;layout_json:string|null;version:number}>()).results,lock:scope=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(scope).run(),workGuards:(actor,key)=>workContextGuards(db,actor,key),iamGuards:(actor,key)=>iamMutationGuards(db,actor,key),clearAssertions:key=>workLayoutClearAssertions(db,key),batch:effects=>db.batch(effects),...workLayoutAuthorityFacts(db),receipt:key=>db.prepare("SELECT request_hash,result_json FROM page_layout_receipts WHERE id=?").bind(key).first<{request_hash:string;result_json:string}>(),save:(company,owner,base,page,value,version,now)=>db.prepare("INSERT INTO page_layouts(company_id,owner_id,workspace_base,page_key,layout_json,version,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,owner_id,workspace_base,page_key) DO UPDATE SET layout_json=excluded.layout_json,version=excluded.version,updated_at=excluded.updated_at").bind(company,owner,base,page,value,version,now).run(),saveReceipt:(key,hash,result,user,company,base,page,action,now)=>db.prepare("INSERT INTO page_layout_receipts(id,request_hash,result_json,actor_id,company_id,workspace_base,page_key,action,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(key,hash,result,user,company,base,page,action,now).run()};}
export async function pageLayoutRequest(request:Request,pageKey:string,holdingSlug?:string,account=false){let db:PostgresDatabase;const service=createPageLayoutService({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,hash:value=>createHash("sha256").update(value).digest("hex"),currentSession,requireWorkContext,requirePlatformContext,prepare:async work=>{db=(work?(await holdingResources(work.holdingId)).write:(await runtimeEnv()).CONTROL_DB) as PostgresDatabase;},unit:()=>unit(db),navigation:navigationContext,base:actor=>workspaceBase(publicScope(actor)),transaction:action=>db.transaction(tx=>action(unit(tx)))});try{return Response.json({data:await service(request.method,()=>serviceCommandBody(request),pageKey,holdingSlug,account)},{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
