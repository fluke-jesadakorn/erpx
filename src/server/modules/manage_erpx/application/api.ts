import {z} from 'zod';
import type {WorkContext} from '../../identity_access/public/authorization';
import {BusinessRuleError,type Clock} from '../../shared';
import {invitationSchema} from '../domain/team';
import type {mentionsSchema,textSuggestions} from "@/server/modules/collaboration/public/social-text";
import type {retryAgentReply} from "@/server/modules/collaboration/public/social-agent";
import type {homePosts,postAction,postDraftSchema,postView,postContentSchema,publishPost,savePostDraft,socialAudienceOptions,createCommentUploadDraft,cancelCommentUploadDraft,createPostEditUploadDraft,cancelPostEditUploadDraft} from "@/server/modules/collaboration/public/social-home";
import type {sourceSnapshot,readablePost} from "@/server/modules/collaboration/public/social-source";
import type {aiRequestSchema} from "@/server/modules/collaboration/public/social-ai";
import type {taskSourceSchema} from "@/server/modules/collaboration/public/social-task";
import type {getTask,saveTask,taskSchema,listTasks} from "@/server/modules/operations/public/staff-tasks";
import type {staffPeople} from "@/server/modules/operations/public/staff-policy";
import type {addBetaBranch,inviteMember} from "../public/beta-team";
type WithoutDatabase<F> = F extends (...args:infer Args)=>infer Result ? Args extends [unknown,...infer Rest] ? (...args:Rest)=>Result : never : never;
export interface BetaApiRepository<Effect>{branches(companyId:string,branchId:string | null,branchId2:string | null):Promise<({ id: string; display_name: string })[]>;
postDrafts(userId:string,companyId:string):Promise<(Record<string,unknown>)[]>;
policy(companyId:string):Promise<Record<string,unknown>|null>;
usage(companyId:string,month:string,companyId2:string,month2:string):Promise<Record<string,unknown>|null>;
files(companyId:string,companyId2:string,companyId3:string,companyId4:string,companyId5:string,companyId6:string):Promise<Record<string,unknown>|null>;
invitations(companyId:string):Promise<(Record<string,unknown>)[]>;
feedback(companyId:string):Promise<(Record<string,unknown>)[]>;
reports(companyId:string):Promise<(Record<string,unknown>)[]>;
billing(holdingId:string):Promise<Record<string,unknown>|null>;
postTask(id:string | undefined):Promise<{ task_id: string }|null>;
recoverPostTask(id:string | undefined):Promise<{ task_id: string }|null>;
prepareFeedback(key:string,companyId:string,userId:string,body:string,month:number):Effect;
prepareRevocations(month:number,user:string,companyId:string):Effect;
prepareMembershipRemoval(user:string):Effect;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;authorityGuard(key:string,actor:WorkContext,roles:string[]):Effect;}
export interface BetaApiPorts<Effect>{repository:BetaApiRepository<Effect>;clock:Clock;schemas:{postDraftSchema:typeof postDraftSchema;postContentSchema:typeof postContentSchema;mentionsSchema:typeof mentionsSchema;taskSourceSchema:typeof taskSourceSchema;taskSchema:typeof taskSchema;aiRequest:typeof aiRequestSchema};services:{homePosts:WithoutDatabase<typeof homePosts>;
postAction:WithoutDatabase<typeof postAction>;
postView:WithoutDatabase<typeof postView>;
publishPost:WithoutDatabase<typeof publishPost>;
savePostDraft:WithoutDatabase<typeof savePostDraft>;
socialAudienceOptions:WithoutDatabase<typeof socialAudienceOptions>;
createCommentUploadDraft:WithoutDatabase<typeof createCommentUploadDraft>;
cancelCommentUploadDraft:WithoutDatabase<typeof cancelCommentUploadDraft>;
createPostEditUploadDraft:WithoutDatabase<typeof createPostEditUploadDraft>;
cancelPostEditUploadDraft:WithoutDatabase<typeof cancelPostEditUploadDraft>;
sourceSnapshot:WithoutDatabase<typeof sourceSnapshot>;
readablePost:WithoutDatabase<typeof readablePost>;
textSuggestions:WithoutDatabase<typeof textSuggestions>;
retryAgentReply:WithoutDatabase<typeof retryAgentReply>;
getTask:WithoutDatabase<typeof getTask>;
saveTask:WithoutDatabase<typeof saveTask>;
listTasks:WithoutDatabase<typeof listTasks>;
staffPeople:WithoutDatabase<typeof staffPeople>;
addBetaBranch:WithoutDatabase<typeof addBetaBranch>;
inviteMember:WithoutDatabase<typeof inviteMember>;};workspaceBase(actor:WorkContext):string;action(name:string):void;requireCompanyAuthority(actor:WorkContext,roles:string[]):Promise<void>;invitationToken(actor:WorkContext,key:string):Promise<string>;invitationUrl(holdingSlug:string,token:string):string;email(to:string,subject:string,url:string,key:string):Promise<string|null>;billingLink(holdingId:string,slug:string,email:string,action:'checkout'|'portal'):Promise<unknown>;createAiDraft(actor:WorkContext,id:string,input:z.infer<typeof aiRequestSchema>,key:string):Promise<unknown>;}
export function createBetaApiApplication<Effect>(ports:BetaApiPorts<Effect>){const r=ports.repository;return {async handle(actor:WorkContext,resource:string,path:string[],id:string|undefined,holdingSlug:string,method:string,query:URLSearchParams,input:Record<string,unknown>,key:string){let data: unknown;
      
      if (method === "GET") {
        if (resource === "post") data = await ports.services.postView(actor, id!);
        else if (resource === "task-source")
          data = await ports.services.sourceSnapshot(actor,
            id!,
            query.getAll("commentId"),
          );
        else if (path[0] === "suggestions") data = await ports.services.textSuggestions(actor, z.enum(["@", "#"]).parse(query.get("trigger")), z.string().max(160).parse(query.get("q") ?? ""));
        else if (path[0] === "home")
          data = query.get("post")
            ? [await ports.services.postView(actor, query.get("post")!)]
            : await ports.services.homePosts(actor, {
                tag: query.get("tag")?.normalize("NFC").toLowerCase(),
                following: query.get("filter") === "following",
                branchId: query.get("branch") ?? undefined,
                teamId: query.has("team") ? z.string().min(1).max(160).parse(query.get("team")) : undefined,
                stream: query.has("stream") ? z.enum(["conversations", "milestones"]).parse(query.get("stream")) : undefined,
                beforeId: query.has("beforeId")
                  ? z.string().max(160).parse(query.get("beforeId"))
                  : undefined,
                before: query.has("before")
                  ? z.coerce
                      .number()
                      .int()
                      .positive()
                      .parse(query.get("before"))
                  : undefined,
                limit: query.has("limit")
                  ? z.coerce.number().int().min(1).max(20).parse(query.get("limit"))
                  : undefined,
              });
        else if (path[0] === "meta") {
          const branches = (await r.branches(actor.companyId,actor.branchId,actor.branchId));
          const options = await ports.services.socialAudienceOptions(actor);
          data = {
            handoffContext: { companyId: actor.companyId, userId: actor.userId,
              assignmentId: actor.assignmentId, base: ports.workspaceBase(actor) },
            people: (await ports.services.staffPeople(actor.companyId)).map((p) => ({
              id: p.user_id,
              name: p.legal_name,
              branchId: p.branch_id,
            })),
            branches,
            audiences: options.audiences,
            projects: options.projects,
            drafts: (await r.postDrafts(actor.userId,actor.companyId)),
            tasks: await ports.services.listTasks(actor),
          };
        }
        else if (path[0] === "settings") {
          await ports.requireCompanyAuthority(actor, ["holding_owner"]);
          data = {
            policy: await r.policy(actor.companyId),
            usage: await r.usage(actor.companyId,new Date(ports.clock.now()).toISOString().slice(0, 7),actor.companyId,new Date(ports.clock.now()).toISOString().slice(0, 7)),
            files: await r.files(actor.companyId,actor.companyId,actor.companyId,actor.companyId,actor.companyId,actor.companyId),
            invites: (await r.invitations(actor.companyId)),
            feedback: (await r.feedback(actor.companyId)),
            reports: (await r.reports(actor.companyId)),
            billing: await r.billing(actor.holdingId),
          };
        } else throw new BusinessRuleError(404, "NOT_FOUND", "Resource unavailable.");
      } else {
        
        if (!/^[a-zA-Z0-9:_.-]{8,120}$/.test(key))
          throw new BusinessRuleError(
            422,
            "COMMAND_KEY_REQUIRED",
            "Provide an Idempotency-Key.",
          );
        if (resource === "ai-drafts")
          data = await ports.createAiDraft(actor,id!,ports.schemas.aiRequest.parse(input),key);
        else if (resource === "task") {
          await ports.services.readablePost(actor, id!);
          const existing = await r.postTask(id);
          if (existing) {
            const task = await ports.services.getTask(actor, existing.task_id);
            data = { id: task.id, state: task.state, existing: true };
          } else {
            const source = ports.schemas.taskSourceSchema.parse({
                ...(input.source as Record<string,unknown>),
                postId: id,
              }),
              taskInput = ports.schemas.taskSchema.parse(input.task);
            try {
              data = await ports.services.saveTask(actor,
                null,
                taskInput,
                key,
                false,
                source,
              );
            } catch (e) {
              const other = await r.recoverPostTask(id);
              if (!other) throw e;
              const task = await ports.services.getTask(actor, other.task_id);
              data = { id: task.id, state: task.state, existing: true };
            }
          }
        } else if (resource === "post") {
          const command = z
              .object({
                action: z.enum([
                  "comment",
                  "delete_comment",
                  "edit_comment",
                  "appreciate",
                  "follow",
                  "report",
                  "hide",
                  "edit",
                  "retract",
                ]),
                body: z.string().max(30000).optional(),
                content: ports.schemas.postContentSchema.optional(),
                richContent: z.unknown().optional(),
                expectedVersion: z.number().int().positive().optional(),
                active: z.boolean().optional(),
                mentions: ports.schemas.mentionsSchema.optional(),
                mentionUserIds: z.array(z.string().max(160)).max(10).optional(),
                parentCommentId: z.string().min(1).max(160).optional(),
                commentId: z.string().min(1).max(160).optional(),
                commentDraftId: z.string().min(1).max(160).optional(),
                editDraftId: z.string().min(1).max(160).optional(),
              })
              .parse(input);
          ports.action(command.action);
          data = await ports.services.postAction(actor, id!, command, key);
        }
        else if (path[0] === "agent-replies") data = await ports.services.retryAgentReply(actor, path[1], key);
        else if (path[0] === "comment-uploads") data = path[1]
          ? await ports.services.cancelCommentUploadDraft(actor,path[1],key)
          : await ports.services.createCommentUploadDraft(actor,z.string().min(1).max(160).parse(input.postId),key);
        else if (path[0] === "post-edit-uploads") data = path[1]
          ? await ports.services.cancelPostEditUploadDraft(actor,path[1],key)
          : await ports.services.createPostEditUploadDraft(actor,z.string().min(1).max(160).parse(input.postId),
              z.number().int().positive().parse(input.expectedVersion),key);
        else if (path[0] === "drafts")
          data =
            path[2] === "publish"
              ? await ports.services.publishPost(actor,
                  path[1],
                  z.number().int().positive().parse(input.expectedVersion),
                  key,
                )
              : await ports.services.savePostDraft(actor,
                  path[1] ?? null,
                  ports.schemas.postDraftSchema.parse(input),
                  key,
                );
        else if (path[0] === "feedback") {
          const body = z.string().trim().min(1).max(3000).parse(input.body);
          data = await r.commit(actor,
            key,
            "beta.feedback",
            { body },
            key,
            1,
            { id: key },
            [
              r.prepareFeedback(key,actor.companyId,actor.userId,body,ports.clock.now()),
            ],
          );
        } else if (path[0] === "branches")
          data = await ports.services.addBetaBranch(actor, input.name as string, key);
        else if (path[0] === "invitations") {
          const parsed = invitationSchema.parse(input);
          const token=await ports.invitationToken(actor,key);
          data = await ports.services.inviteMember(actor, parsed, key, token);
          await ports.email(parsed.email,`Join ${actor.holdingName} on ERPX`,ports.invitationUrl(holdingSlug,token),`beta-invite-${actor.holdingId}-${key}`);
        } else if (path[0] === "revoke") {
          await ports.requireCompanyAuthority(actor, ["holding_owner"]);
          const user = z.string().min(1).max(160).parse(input.userId);
          if (user === actor.userId)
            throw new BusinessRuleError(
              422,
              "SELF_REVOKE",
              "Another owner must manage your own access.",
            );
          data = await r.commit(actor,
            key,
            "beta.member.revoke",
            { user },
            user,
            1,
            { id: user },
            [
              r.authorityGuard(key, actor, ["holding_owner"]),
              r.prepareRevocations(ports.clock.now(),user,actor.companyId),
              r.prepareMembershipRemoval(user),
            ],
          );
        } else if (path[0] === "billing") {
          await ports.requireCompanyAuthority(actor, ["holding_owner"]);
          data = await ports.billingLink(
            actor.holdingId,
            holdingSlug,
            actor.email,
            z.enum(["checkout", "portal"]).parse(input.action),
          );
        } else throw new BusinessRuleError(404, "NOT_FOUND", "Action unavailable.");
      }
      
return data;}};}
