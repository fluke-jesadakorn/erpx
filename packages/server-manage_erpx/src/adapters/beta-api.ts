import { applicationOrigin } from "@erpx/feature-runtime/lib/application-origins";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {betaManagementPeopleSources} from '@erpx/server-people_payroll/public/beta-management-sources';
import {betaManagementOrganizationSources} from '@erpx/server-organization/public/beta-management-sources';
import {betaManagementCollaborationSources} from '@erpx/server-collaboration/public/beta-management-sources';
import {betaManagementFinanceSources} from '@erpx/server-finance/public/beta-management-sources';
import {betaManagementRequestSources} from '@erpx/server-records/public/beta-management-sources';
import { mentionsSchema, textSuggestions } from "@erpx/server-collaboration/public/social-text";
import { retryAgentReply } from "@erpx/server-collaboration/public/social-agent";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { runtimeEnv, holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { betaBody } from "@erpx/server-manage_erpx/adapters/beta-http";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  homePosts,
  postAction,
  postDraftSchema,
  postView,
  postContentSchema,
  publishPost,
  savePostDraft,
  socialAudienceOptions,
  createCommentUploadDraft,
  cancelCommentUploadDraft,
  createPostEditUploadDraft,
  cancelPostEditUploadDraft,
} from "@erpx/server-collaboration/public/social-home";
import { sourceSnapshot, readablePost } from "@erpx/server-collaboration/public/social-source";
import { createAiDraft, aiRequestSchema } from "@erpx/server-collaboration/public/social-ai";
import { taskSourceSchema } from "@erpx/server-collaboration/public/social-task";
import { getTask, saveTask, taskSchema, listTasks } from "@erpx/server-operations/public/staff-tasks";
import {
  staffPeople,
  requireCompanyAuthority,
  authorityGuard,
} from "@erpx/server-operations/public/staff-policy";
import { addBetaBranch, inviteMember } from "@erpx/server-manage_erpx/adapters/beta-team";
import { executeCommand } from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { sendBetaEmail } from "@erpx/server-manage_erpx/adapters/beta-email";
import { billingLink } from "@erpx/server-billing/public/beta-billing";

import type {SqlDatabase,SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import type {RuntimeEnv} from '@erpx/server-runtime/public/runtime/env';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createBetaApiApplication,type BetaApiRepository} from '@erpx/server-manage_erpx/application/api';
import {betaManagementFacts as owner0} from '@erpx/server-organization/public/beta-management-facts';
import {betaManagementFacts as owner1} from '@erpx/server-collaboration/public/beta-management-facts';
import {betaManagementFacts as owner2} from '@erpx/server-files/public/beta-management-facts';
import {betaManagementFacts as owner3} from '@erpx/server-billing/public/beta-management-facts';
import {betaManagementFacts as owner4} from '@erpx/server-identity_access/public/beta-management-facts';
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
function repository(db:SqlDatabase,env:RuntimeEnv):BetaApiRepository<SqlStatement>{const people=betaManagementPeopleSources(db),organization=betaManagementOrganizationSources(db),collaboration=betaManagementCollaborationSources(db),finance=betaManagementFinanceSources(db),requests=betaManagementRequestSources(db);return {async policy(companyId:string){return db
              .prepare("SELECT * FROM beta_policy WHERE company_id=?")
              .bind(companyId)
              .first();},
async usage(companyId:string,month:string,companyId2:string,month2:string){return db
              .prepare(
                `SELECT count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens FROM (${collaboration.aiUsageSource} UNION ALL SELECT input_tokens,output_tokens FROM (${people.aiUsageSource} UNION ALL ${requests.aiUsageSource} UNION ALL ${finance.aiUsageSource}) WHERE company_id=? AND month=?)`,
              )
              .bind(
                companyId,
                month,
                companyId2,
                month2,
              )
              .first();},
async invitations(companyId:string){return (await db
                .prepare(
                  `SELECT i.id,i.email,i.branch_id,i.expires_at,i.accepted_at,i.revoked_at,CASE WHEN i.revoked_at IS NOT NULL THEN 'Revoked' WHEN i.accepted_at IS NOT NULL THEN 'Joined' WHEN i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'Expired' ELSE 'Invited' END invitation_state FROM beta_team_invitations i ${organization.invitationCompanySource} ORDER BY i.created_at DESC LIMIT 100`,
                )
                .bind(companyId)
                .all()).results;},
async feedback(companyId:string){return (await db
                .prepare(
                  "SELECT id,body,created_at FROM beta_feedback WHERE company_id=? ORDER BY created_at DESC LIMIT 100",
                )
                .bind(companyId)
                .all()).results;},
prepareFeedback(key:string,companyId:string,userId:string,body:string,month:number){return db
                .prepare(
                  "INSERT INTO beta_feedback(id,company_id,actor_id,body,created_at) VALUES(?,?,?,?,?)",
                )
                .bind(key, companyId, userId, body, month);},
prepareMembershipRemoval(user:string){return db.prepare("DELETE FROM beta_members WHERE user_id=?").bind(user);},...owner0(db),...owner1(db),...owner2(db),...owner3(env.CONTROL_DB),...owner4(db),commit:(...args)=>executeCommand(db,...args),authorityGuard:(...args)=>authorityGuard(db,...args)};}
function application(db:SqlDatabase,env:RuntimeEnv,action:(name:string)=>void){return createBetaApiApplication({repository:repository(db,env),clock:{now:()=>Date.now()},schemas:{postDraftSchema,postContentSchema,mentionsSchema,taskSourceSchema,taskSchema,aiRequest:aiRequestSchema},services:{homePosts:(...args)=>inward(()=>homePosts(db,...args)),postAction:(...args)=>inward(()=>postAction(db,...args)),postView:(...args)=>inward(()=>postView(db,...args)),publishPost:(...args)=>inward(()=>publishPost(db,...args)),savePostDraft:(...args)=>inward(()=>savePostDraft(db,...args)),socialAudienceOptions:(...args)=>inward(()=>socialAudienceOptions(db,...args)),createCommentUploadDraft:(...args)=>inward(()=>createCommentUploadDraft(db,...args)),cancelCommentUploadDraft:(...args)=>inward(()=>cancelCommentUploadDraft(db,...args)),createPostEditUploadDraft:(...args)=>inward(()=>createPostEditUploadDraft(db,...args)),cancelPostEditUploadDraft:(...args)=>inward(()=>cancelPostEditUploadDraft(db,...args)),sourceSnapshot:(...args)=>inward(()=>sourceSnapshot(db,...args)),readablePost:(...args)=>inward(()=>readablePost(db,...args)),textSuggestions:(...args)=>inward(()=>textSuggestions(db,...args)),retryAgentReply:(...args)=>inward(()=>retryAgentReply(db,...args)),getTask:(...args)=>inward(()=>getTask(db,...args)),saveTask:(...args)=>inward(()=>saveTask(db,...args)),listTasks:(...args)=>inward(()=>listTasks(db,...args)),staffPeople:(...args)=>inward(()=>staffPeople(db,...args)),addBetaBranch:(...args)=>inward(()=>addBetaBranch(db,...args)),inviteMember:(...args)=>inward(()=>inviteMember(db,...args))},workspaceBase:actor=>workspaceBase(publicScope(actor)),action,requireCompanyAuthority:(...args)=>inward(()=>requireCompanyAuthority(db,...args)),invitationToken:(actor,key)=>sha256(`${env.BETTER_AUTH_SECRET}:beta-invite:${actor.holdingId}:${key}`),invitationUrl:(slug,token)=>new URL(`/join?holding=${slug}&token=${token}`,applicationOrigin("account",env.BETTER_AUTH_URL)).toString(),email:(...args)=>sendBetaEmail(env,...args),billingLink:(...args)=>inward(()=>billingLink(env,...args)),createAiDraft:(actor,id,input,key)=>inward(()=>createAiDraft(db,actor,id,input,env,undefined,key))});}
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

