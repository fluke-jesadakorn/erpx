import {betaManagementPeopleSources} from '../../people_payroll/public/beta-management-sources';
import {betaManagementOrganizationSources} from '../../organization/public/beta-management-sources';
import {betaManagementCollaborationSources} from '../../collaboration/public/beta-management-sources';
import {betaManagementFinanceSources} from '../../finance/public/beta-management-sources';
import {betaManagementRequestSources} from '../../records/public/beta-management-sources';
import { mentionsSchema, textSuggestions } from "@/server/modules/collaboration/public/social-text";
import { retryAgentReply } from "@/server/modules/collaboration/public/social-agent";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { runtimeEnv, holdingResources } from "@/server/modules/runtime/public/runtime";
import { betaBody } from "@/server/beta-http";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
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
} from "@/server/modules/collaboration/public/social-home";
import { sourceSnapshot, readablePost } from "@/server/modules/collaboration/public/social-source";
import { createAiDraft, aiRequestSchema } from "@/server/modules/collaboration/public/social-ai";
import { taskSourceSchema } from "@/server/modules/collaboration/public/social-task";
import { getTask, saveTask, taskSchema, listTasks } from "@/server/modules/operations/public/staff-tasks";
import {
  staffPeople,
  requireCompanyAuthority,
  authorityGuard,
} from "@/server/modules/operations/public/staff-policy";
import { addBetaBranch, inviteMember } from "@/server/beta-team";
import { executeCommand } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { sendBetaEmail } from "@/server/beta-email";
import { billingLink } from "@/server/modules/billing/public/beta-billing";

import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import type {RuntimeEnv} from '../../runtime/public/runtime/env';
import {BusinessRuleError} from '../../shared';
import {createBetaApiApplication,type BetaApiRepository} from '../application/api';
import {betaManagementFacts as owner0} from '../../organization/public/beta-management-facts';
import {betaManagementFacts as owner1} from '../../collaboration/public/beta-management-facts';
import {betaManagementFacts as owner2} from '../../files/public/beta-management-facts';
import {betaManagementFacts as owner3} from '../../billing/public/beta-management-facts';
import {betaManagementFacts as owner4} from '../../identity_access/public/beta-management-facts';
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
function application(db:SqlDatabase,env:RuntimeEnv,action:(name:string)=>void){return createBetaApiApplication({repository:repository(db,env),clock:{now:()=>Date.now()},schemas:{postDraftSchema,postContentSchema,mentionsSchema,taskSourceSchema,taskSchema,aiRequest:aiRequestSchema},services:{homePosts:(...args)=>inward(()=>homePosts(db,...args)),postAction:(...args)=>inward(()=>postAction(db,...args)),postView:(...args)=>inward(()=>postView(db,...args)),publishPost:(...args)=>inward(()=>publishPost(db,...args)),savePostDraft:(...args)=>inward(()=>savePostDraft(db,...args)),socialAudienceOptions:(...args)=>inward(()=>socialAudienceOptions(db,...args)),createCommentUploadDraft:(...args)=>inward(()=>createCommentUploadDraft(db,...args)),cancelCommentUploadDraft:(...args)=>inward(()=>cancelCommentUploadDraft(db,...args)),createPostEditUploadDraft:(...args)=>inward(()=>createPostEditUploadDraft(db,...args)),cancelPostEditUploadDraft:(...args)=>inward(()=>cancelPostEditUploadDraft(db,...args)),sourceSnapshot:(...args)=>inward(()=>sourceSnapshot(db,...args)),readablePost:(...args)=>inward(()=>readablePost(db,...args)),textSuggestions:(...args)=>inward(()=>textSuggestions(db,...args)),retryAgentReply:(...args)=>inward(()=>retryAgentReply(db,...args)),getTask:(...args)=>inward(()=>getTask(db,...args)),saveTask:(...args)=>inward(()=>saveTask(db,...args)),listTasks:(...args)=>inward(()=>listTasks(db,...args)),staffPeople:(...args)=>inward(()=>staffPeople(db,...args)),addBetaBranch:(...args)=>inward(()=>addBetaBranch(db,...args)),inviteMember:(...args)=>inward(()=>inviteMember(db,...args))},workspaceBase:actor=>workspaceBase(publicScope(actor)),action,requireCompanyAuthority:(...args)=>inward(()=>requireCompanyAuthority(db,...args)),invitationToken:(actor,key)=>sha256(`${env.BETTER_AUTH_SECRET}:beta-invite:${actor.holdingId}:${key}`),invitationUrl:(slug,token)=>new URL(`/join?holding=${slug}&token=${token}`,env.BETTER_AUTH_URL).toString(),email:(...args)=>sendBetaEmail(env,...args),billingLink:(...args)=>inward(()=>billingLink(env,...args)),createAiDraft:(actor,id,input,key)=>inward(()=>createAiDraft(db,actor,id,input,env,undefined,key))});}
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
        { data },
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

