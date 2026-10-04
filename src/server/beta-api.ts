import { mentionsSchema, textSuggestions } from "./social-text";
import { retryAgentReply } from "./social-agent";
import { z } from "zod";
import { publicScope, requireWorkContext } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { runtimeEnv, holdingResources } from "./runtime";
import { betaBody } from "./beta-http";
import { ApiError, errorResponse } from "./http";
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
} from "./social-home";
import { sourceSnapshot, readablePost } from "./social-source";
import { createAiDraft, aiRequestSchema } from "./social-ai";
import { taskSourceSchema } from "./social-task";
import { getTask, saveTask, taskSchema, listTasks } from "./staff-tasks";
import {
  staffPeople,
  requireCompanyAuthority,
  authorityGuard,
} from "./staff-policy";
import { addBetaBranch, inviteMember, invitationSchema } from "./beta-team";
import { executeCommand } from "./commands";
import { sha256 } from "./crypto";
import { sendBetaEmail } from "./beta-email";
import { billingLink } from "./beta-billing";
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
      let data: unknown;
      const query = new URL(request.url).searchParams;
      if (request.method === "GET") {
        if (resource === "post") data = await postView(db, actor, id!);
        else if (resource === "task-source")
          data = await sourceSnapshot(
            db,
            actor,
            id!,
            query.getAll("commentId"),
          );
        else if (path[0] === "suggestions") data = await textSuggestions(db, actor, z.enum(["@", "#"]).parse(query.get("trigger")), z.string().max(160).parse(query.get("q") ?? ""));
        else if (path[0] === "home")
          data = query.get("post")
            ? [await postView(db, actor, query.get("post")!)]
            : await homePosts(db, actor, {
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
          const branches = (
            await db.prepare(
              "SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)",
            ).bind(actor.companyId, actor.branchId, actor.branchId)
              .all<{ id: string; display_name: string }>()
          ).results;
          const options = await socialAudienceOptions(db, actor);
          data = {
            handoffContext: { companyId: actor.companyId, userId: actor.userId,
              assignmentId: actor.assignmentId, base: workspaceBase(publicScope(actor)) },
            people: (await staffPeople(db, actor.companyId)).map((p) => ({
              id: p.user_id,
              name: p.legal_name,
              branchId: p.branch_id,
            })),
            branches,
            audiences: options.audiences,
            projects: options.projects,
            drafts: (
              await db
                .prepare(
                  "SELECT id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,version,mentions_json,content_json,rich_content_json FROM social_post_drafts WHERE author_id=? AND company_id=? AND published_post_id IS NULL AND comment_parent_post_id IS NULL AND edit_post_id IS NULL ORDER BY updated_at DESC LIMIT 20",
                )
                .bind(actor.userId, actor.companyId)
                .all()
            ).results,
            tasks: await listTasks(db, actor),
          };
        }
        else if (path[0] === "settings") {
          await requireCompanyAuthority(db, actor, ["holding_owner"]);
          data = {
            policy: await db
              .prepare("SELECT * FROM beta_policy WHERE company_id=?")
              .bind(actor.companyId)
              .first(),
            usage: await db
              .prepare(
                "SELECT count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens FROM (SELECT input_tokens,output_tokens FROM social_ai_usage WHERE company_id=? AND month=? UNION ALL SELECT input_tokens,output_tokens FROM (SELECT company_id,month,input_tokens,output_tokens FROM expense_ai_usage UNION ALL SELECT company_id,month,input_tokens,output_tokens FROM request_ai_usage UNION ALL SELECT company_id,month,input_tokens,output_tokens FROM finance_ai_usage) WHERE company_id=? AND month=?)",
              )
              .bind(
                actor.companyId,
                new Date().toISOString().slice(0, 7),
                actor.companyId,
                new Date().toISOString().slice(0, 7),
              )
              .first(),
            files: await db
              .prepare(
                `SELECT COALESCE(SUM(a.bytes),0)+COALESCE((SELECT SUM(bytes) FROM attachment_derivatives WHERE company_id=?),0) bytes FROM attachments a WHERE a.revoked_at IS NULL AND (
              (a.owner_type='social_draft' AND EXISTS(SELECT 1 FROM social_post_drafts p WHERE p.id=a.owner_id AND p.company_id=?)) OR
              (a.owner_type='staff_task' AND EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)) OR
              (a.owner_type='expense_claim' AND EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=a.owner_id AND e.company_id=?)) OR
              (a.owner_type='expense_payment' AND EXISTS(SELECT 1 FROM expense_payment_drafts e WHERE e.id=a.owner_id AND e.company_id=?)) OR
              (a.owner_type='leave_request' AND EXISTS(SELECT 1 FROM leave_requests l WHERE l.id=a.owner_id AND l.company_id=?)))`,
              )
              .bind(
                actor.companyId,
                actor.companyId,
                actor.companyId,
                actor.companyId,
                actor.companyId,
                actor.companyId,
              )
              .first(),
            invites: (
              await db
                .prepare(
                  "SELECT i.id,i.email,i.branch_id,i.expires_at,i.accepted_at,i.revoked_at,CASE WHEN i.revoked_at IS NOT NULL THEN 'Revoked' WHEN i.accepted_at IS NOT NULL THEN 'Joined' WHEN i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'Expired' ELSE 'Invited' END invitation_state FROM beta_team_invitations i JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=? ORDER BY i.created_at DESC LIMIT 100",
                )
                .bind(actor.companyId)
                .all()
            ).results,
            feedback: (
              await db
                .prepare(
                  "SELECT id,body,created_at FROM beta_feedback WHERE company_id=? ORDER BY created_at DESC LIMIT 100",
                )
                .bind(actor.companyId)
                .all()
            ).results,
            reports: (
              await db
                .prepare(
                  "SELECT r.id,r.reason,r.post_id,p.state FROM social_reports r JOIN social_posts p ON p.id=r.post_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100",
                )
                .bind(actor.companyId)
                .all()
            ).results,
            billing: await env.CONTROL_DB.prepare(
              "SELECT status FROM beta_billing WHERE holding_id=?",
            )
              .bind(actor.holdingId)
              .first(),
          };
        } else throw new ApiError(404, "NOT_FOUND", "Resource unavailable.");
      } else {
        const input = await betaBody(request),
          key = request.headers.get("Idempotency-Key") ?? "";
        if (!/^[a-zA-Z0-9:_.-]{8,120}$/.test(key))
          throw new ApiError(
            422,
            "COMMAND_KEY_REQUIRED",
            "Provide an Idempotency-Key.",
          );
        if (resource === "ai-drafts")
          data = await createAiDraft(
            db,
            actor,
            id!,
            aiRequestSchema.parse(input),
            env,
            undefined,
            key,
          );
        else if (resource === "task") {
          await readablePost(db, actor, id!);
          const existing = await db
            .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
            .bind(id)
            .first<{ task_id: string }>();
          if (existing) {
            const task = await getTask(db, actor, existing.task_id);
            data = { id: task.id, state: task.state, existing: true };
          } else {
            const source = taskSourceSchema.parse({
                ...input.source,
                postId: id,
              }),
              taskInput = taskSchema.parse(input.task);
            try {
              data = await saveTask(
                db,
                actor,
                null,
                taskInput,
                key,
                false,
                source,
              );
            } catch (e) {
              const other = await db
                .prepare(
                  "SELECT task_id FROM social_task_origins WHERE post_id=?",
                )
                .bind(id)
                .first<{ task_id: string }>();
              if (!other) throw e;
              const task = await getTask(db, actor, other.task_id);
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
                content: postContentSchema.optional(),
                richContent: z.unknown().optional(),
                expectedVersion: z.number().int().positive().optional(),
                active: z.boolean().optional(),
                mentions: mentionsSchema.optional(),
                mentionUserIds: z.array(z.string().max(160)).max(10).optional(),
                parentCommentId: z.string().min(1).max(160).optional(),
                commentId: z.string().min(1).max(160).optional(),
                commentDraftId: z.string().min(1).max(160).optional(),
                editDraftId: z.string().min(1).max(160).optional(),
              })
              .parse(input);
          actionName = command.action;
          data = await postAction(db, actor, id!, command, key);
        }
        else if (path[0] === "agent-replies") data = await retryAgentReply(db, actor, path[1], key);
        else if (path[0] === "comment-uploads") data = path[1]
          ? await cancelCommentUploadDraft(db,actor,path[1],key)
          : await createCommentUploadDraft(db,actor,z.string().min(1).max(160).parse(input.postId),key);
        else if (path[0] === "post-edit-uploads") data = path[1]
          ? await cancelPostEditUploadDraft(db,actor,path[1],key)
          : await createPostEditUploadDraft(db,actor,z.string().min(1).max(160).parse(input.postId),
              z.number().int().positive().parse(input.expectedVersion),key);
        else if (path[0] === "drafts")
          data =
            path[2] === "publish"
              ? await publishPost(
                  db,
                  actor,
                  path[1],
                  z.number().int().positive().parse(input.expectedVersion),
                  key,
                )
              : await savePostDraft(
                  db,
                  actor,
                  path[1] ?? null,
                  postDraftSchema.parse(input),
                  key,
                );
        else if (path[0] === "feedback") {
          const body = z.string().trim().min(1).max(3000).parse(input.body);
          data = await executeCommand(
            db,
            actor,
            key,
            "beta.feedback",
            { body },
            key,
            1,
            { id: key },
            [
              db
                .prepare(
                  "INSERT INTO beta_feedback(id,company_id,actor_id,body,created_at) VALUES(?,?,?,?,?)",
                )
                .bind(key, actor.companyId, actor.userId, body, Date.now()),
            ],
          );
        } else if (path[0] === "branches")
          data = await addBetaBranch(db, actor, input.name, key);
        else if (path[0] === "invitations") {
          const parsed = invitationSchema.parse(input);
          const token = await sha256(
            `${env.BETTER_AUTH_SECRET}:beta-invite:${actor.holdingId}:${key}`,
          );
          data = await inviteMember(db, actor, parsed, key, token);
          await sendBetaEmail(
            env,
            parsed.email,
            `Join ${actor.holdingName} on ERPX`,
            new URL(
              `/join?holding=${holdingSlug}&token=${token}`,
              env.BETTER_AUTH_URL,
            ).toString(),
            `beta-invite-${actor.holdingId}-${key}`,
          );
        } else if (path[0] === "revoke") {
          await requireCompanyAuthority(db, actor, ["holding_owner"]);
          const user = z.string().min(1).max(160).parse(input.userId);
          if (user === actor.userId)
            throw new ApiError(
              422,
              "SELF_REVOKE",
              "Another owner must manage your own access.",
            );
          data = await executeCommand(
            db,
            actor,
            key,
            "beta.member.revoke",
            { user },
            user,
            1,
            { id: user },
            [
              authorityGuard(db, key, actor, ["holding_owner"]),
              db
                .prepare(
                  "UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL",
                )
                .bind(Date.now(), user, actor.companyId),
              db.prepare("DELETE FROM beta_members WHERE user_id=?").bind(user),
            ],
          );
        } else if (path[0] === "billing") {
          await requireCompanyAuthority(db, actor, ["holding_owner"]);
          data = await billingLink(
            env,
            actor.holdingId,
            holdingSlug,
            actor.email,
            z.enum(["checkout", "portal"]).parse(input.action),
          );
        } else throw new ApiError(404, "NOT_FOUND", "Action unavailable.");
      }
      return Response.json(
        { data },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    } catch (e) {
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
