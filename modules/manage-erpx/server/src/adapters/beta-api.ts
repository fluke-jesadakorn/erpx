import { applicationOrigin } from "@erpx/platform-runtime-ui/lib/application-origins";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {betaManagementPeopleSources} from '@erpx/people-payroll-server/public/beta-management-sources';
import {betaManagementCollaborationSources} from '@erpx/collaboration-server/public/beta-management-sources';
import {betaManagementFinanceSources} from '@erpx/finance-server/public/beta-management-sources';
import {betaManagementRequestSources} from '@erpx/records-server/public/beta-management-sources';
import { mentionsSchema, textSuggestions } from "@erpx/collaboration-server/public/social-text";
import { retryAgentReply } from "@erpx/collaboration-server/public/social-agent";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { runtimeEnv, holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { betaBody } from "@erpx/manage-erpx-server/adapters/beta-http";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import {homePosts,postAction,postDraftSchema,postView,postContentSchema,publishPost,savePostDraft,socialAudienceOptions} from '@erpx/collaboration-server/public/social-home';
import {sourceSnapshot,readablePost} from '@erpx/collaboration-server/public/social-source';
import { createAiDraft, aiRequestSchema } from "@erpx/collaboration-server/public/social-ai";
import { taskSourceSchema } from "@erpx/collaboration-server/public/social-task";
import {getTask} from '@erpx/operations-server/public/staff-task-read';
import {taskSchema,listTasks} from '@erpx/operations-server/public/staff-tasks';
import {saveTaskNative} from './native-beta-task';
import {
  staffPeople,
  requireCompanyAuthority,
  authorityGuard,
} from "@erpx/operations-server/public/staff-policy";
import { addBetaBranch, inviteMember } from "@erpx/manage-erpx-server/adapters/beta-team";
import { executeCommand } from "@erpx/workflow-server/public/commands";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { sendBetaEmail } from "@erpx/manage-erpx-server/adapters/beta-email";
import { nativeOwnerWrite, NATIVE_OWNER_WRITE_NOOP_SQL } from "./native-owner-write";
import { betaPolicyPointKind } from "./beta-policy-point-sql";
import { nativeManageOwnerReadFirst } from "./native-owner-read";
import { nativeBetaFeedback, nativeBetaInvitations } from "./native-lane-34/client";
import { usageHeadSql } from "./usage-head-sql";
import { billingLink } from "@erpx/billing-server/public/beta-billing";

import type {SqlDatabase,SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import type {RuntimeEnv} from '@erpx/platform-runtime-server/public/runtime/env';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createBetaApiApplication,type BetaApiRepository} from '@erpx/manage-erpx-server/application/api';
import {betaManagementFacts as owner0} from '@erpx/organization-server/public/beta-management-facts';
import {betaManagementFacts as owner1} from '@erpx/collaboration-server/public/beta-management-facts';
import {betaManagementFacts as owner2} from '@erpx/files-server/public/beta-management-facts';
import {betaManagementFacts as owner3} from '@erpx/billing-server/public/beta-management-facts';
import {betaManagementFacts as owner4} from '@erpx/identity-access-server/public/beta-management-facts';
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
function repository(db:SqlDatabase,env:RuntimeEnv):BetaApiRepository<SqlStatement>{const people=betaManagementPeopleSources(db),collaboration=betaManagementCollaborationSources(db),finance=betaManagementFinanceSources(db),requests=betaManagementRequestSources(db);return {async policy(companyId:string){return nativeManageOwnerReadFirst(betaPolicyPointKind['*'], [companyId], db);},
async usage(companyId:string,month:string,companyId2:string,month2:string){return db
              .prepare(
                `${usageHeadSql('count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens')}${collaboration.aiUsageSource} UNION ALL ${usageHeadSql('input_tokens,output_tokens')}${people.aiUsageSource} UNION ALL ${requests.aiUsageSource} UNION ALL ${finance.aiUsageSource}) WHERE company_id=? AND month=?)`,
              )
              .bind(
                companyId,
                month,
                companyId2,
                month2,
              )
              .first();},
// Native axum owner owns the company-scoped invitations query and the
// invitation_state projection. This adapter only transports the named Holding.
async invitations(companyId:string,holdingSlug:string){return nativeBetaInvitations(companyId,holdingSlug);},
// Native axum owner owns the beta_feedback SELECT, Company scope and projection.
// This adapter only transports the returned rows; no TypeScript query remains.
async feedback(companyId:string){return nativeBetaFeedback(companyId);},
// Native axum owner-write commands execute both writes in Rust. The returned
// statements are inert placeholders; no write SQL remains in TypeScript.
async prepareFeedback(key:string,companyId:string,userId:string,body:string,month:number){return nativeOwnerWrite('me_beta_feedback_insert',[key,companyId,userId,body,month],`me-feedback:${key}`,{companyId,now:month},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL));},
async prepareMembershipRemoval(user:string){return nativeOwnerWrite('me_beta_membership_delete',[user],`me-member-remove:${user}`,{},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL));},...owner0(db),...owner1(db),...owner2(db),...owner3(env.CONTROL_DB),...owner4(db),commit:(...args)=>executeCommand(db,...args),authorityGuard:(...args)=>authorityGuard(db,...args)};}
function application(db:SqlDatabase,env:RuntimeEnv,action:(name:string)=>void){return createBetaApiApplication({repository:repository(db,env),clock:{now:()=>Date.now()},schemas:{postDraftSchema,postContentSchema,mentionsSchema,taskSourceSchema,taskSchema,aiRequest:aiRequestSchema},services:{homePosts:(...args)=>inward(()=>homePosts(db,...args)),postAction:(...args)=>inward(()=>postAction(db,...args)),postView:(...args)=>inward(()=>postView(db,...args)),publishPost:(...args)=>inward(()=>publishPost(db,...args)),savePostDraft:(...args)=>inward(()=>savePostDraft(db,...args)),socialAudienceOptions:(...args)=>inward(()=>socialAudienceOptions(db,...args)),sourceSnapshot:(...args)=>inward(()=>sourceSnapshot(db,...args)),readablePost:(...args)=>inward(()=>readablePost(db,...args)),textSuggestions:(...args)=>inward(()=>textSuggestions(db,...args)),retryAgentReply:(...args)=>inward(()=>retryAgentReply(db,...args)),getTask:(...args)=>inward(()=>getTask(db,...args)),saveTask:(...args)=>inward(()=>saveTaskNative(...args)),listTasks:(...args)=>inward(()=>listTasks(db,...args)),staffPeople:(...args)=>inward(()=>staffPeople(db,...args)),addBetaBranch:(...args)=>inward(()=>addBetaBranch(db,...args)),inviteMember:(...args)=>inward(()=>inviteMember(db,...args))},workspaceBase:actor=>workspaceBase(publicScope(actor)),action,requireCompanyAuthority:(...args)=>inward(()=>requireCompanyAuthority(db,...args)),invitationToken:(actor,key)=>sha256(`${env.BETTER_AUTH_SECRET}:beta-invite:${actor.holdingId}:${key}`),invitationUrl:(slug,token)=>new URL(`/join?holding=${slug}&token=${token}`,applicationOrigin("account",env.BETTER_AUTH_URL)).toString(),email:(...args)=>sendBetaEmail(env,...args),billingLink:(...args)=>inward(()=>billingLink(env,...args)),createAiDraft:(actor,id,input,key)=>inward(()=>createAiDraft(db,actor,id,input,env,undefined,key))});}
type Context = {
  params: Promise<{ holdingSlug: string; path?: string[]; id?: string }>;
};
export function betaHandler(resource = "beta") {
  return async (request: Request, context: Context) => {
    const requestId = crypto.randomUUID();
    let actionName: string | undefined;
    try {
      const { holdingSlug, path = [], id } = await context.params,
        actor = await requireWorkContext(holdingSlug),
        { write: db } = await holdingResources(actor.holdingId),
        env = await runtimeEnv();
      const data=await application(db,env,name=>{actionName=name;}).handle(actor,resource,path,id,holdingSlug,request.method,new URL(request.url).searchParams,request.method==='GET'?{}:await betaBody(request),request.headers.get('Idempotency-Key')??'');
      return Response.json(
        canonicalDestinationLinks({ data }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    } catch (e) {
      if(e instanceof BusinessRuleError)e=new ApiError(e.status,e.code,e.message);
      const response = errorResponse(e);
      if (response.status >= 500) {
        const errorCode = e instanceof ApiError
          ? e.code
          : e && typeof e === "object" && "code" in e && typeof e.code === "string" && /^[A-Z0-9_]{1,80}$/i.test(e.code)
            ? e.code
            : undefined;
        console.error(JSON.stringify({
          event: "beta_request_failed",
          requestId,
          method: request.method,
          resource,
          action: actionName,
          errorName: e instanceof Error ? e.name : typeof e,
          errorCode,
        }));
        response.headers.set("X-Request-ID", requestId);
      }
      return response;
    }
  };
}
