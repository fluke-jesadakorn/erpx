import { applyProvisioningReview } from "./technology";
import { applyLifecycleReview } from "./people-lifecycle";
import { applyDirectoryOperationReview } from "./directory-operations";
import { inspectIamAccess, inspectorInput } from "./iam-inspector";
import { requestPublicOrigin } from "./public-origin";
import { applyManagementReview, managementCommand } from "./user-management";
import { activateEmergency, recoveryInput } from "./iam-recovery";
import "server-only";
import { z } from "zod";
import { currentSession } from "./authorization";
import { iamContext } from "./iam-context";
import {
  iamOverview,
  iamAuditHistory,
  saveIamRole,
  roleInput,
  grantIamAccess,
  grantInput,
  validateIamGrant,
  proposeIamReview,
  revokeIamAssignment,
  decideIamReview,
  reviewInput,
  accessReviewCampaignInput,
  accessReviewDecisionInput,
  startAccessReviewCampaign,
  decideAccessReviewItem,
  cancelAccessReviewCampaign,
} from "./iam-service";
import {
  saveConnection,
  connectionInput,
  testConnection,
  proposeConnectionActivation,
  disableConnection,
  connectionById,
  synchronizeConnection,
  applyDirectoryReview,
  proposeMapping,
  mappingInput,
} from "./iam-directories";
import {
  prepareMigration,
  migrationInput,
  applyMigration,
  proposeEmergency,
  applyEmergency,
} from "./iam-migrations";
import { beginMfa, verifyMfa, requireFreshMfa } from "./iam-mfa";
import { ApiError, errorResponse } from "./http";
import { iamCommand } from "./iam-transactions";
import { iamDb } from "./iam-db";
import { runtimeEnv, activeHoldingBindings, holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { iamPermission } from "./iam-context";

const versionInput = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(1000).default("Administrator action"),
});
function sameOrigin(request: Request) {
  const origin = request.headers.get("origin");
  if (origin && origin !== requestPublicOrigin(request))
    throw new ApiError(
      403,
      "ORIGIN_FORBIDDEN",
      "Use the ERPX application to submit this request.",
    );
}
export async function iamAccountApi(request: Request, path: string[]) {
  try {
    sameOrigin(request);
    const { session } = await currentSession(),
      actor = { userId: session.user.id, sessionId: session.session.id };
    if (request.method === "GET") {
      const env = await runtimeEnv();
      return Response.json(
        {
          data: {
            mfaEnabled: !!(await env.CONTROL_DB.prepare(
              "SELECT 1 FROM iam_mfa WHERE user_id=? AND enabled",
            )
              .bind(actor.userId)
              .first()),
            sessions: (
              await env.CONTROL_DB.prepare(
                'SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=?',
              )
                .bind(actor.userId)
                .all()
            ).results,
            currentSessionId: actor.sessionId,
          },
        },
        { headers: { "Cache-Control": "no-store" } },
      );
    }
    const input = await request.json();
    if (path.join("/") === "emergency/activate")
      return Response.json({
        data: await activateEmergency(actor, recoveryInput.parse(input)),
      });
    if (path.join("/") === "mfa/enroll")
      return Response.json({ data: await beginMfa(actor) });
    if (path.join("/") === "mfa/verify")
      return Response.json({
        data: await verifyMfa(
          actor,
          z.object({ code: z.string() }).parse(input).code,
        ),
      });
    if (path.join("/") === "sessions/revoke") {
      await requireFreshMfa(actor);
      const id = z.object({ id: z.string().min(1) }).parse(input).id;
      const env = await runtimeEnv();
      if (
        await env.CONTROL_DB.prepare(
          'SELECT 1 FROM session WHERE id=? AND "userId"=?',
        )
          .bind(id, actor.userId)
          .first()
      ) {
        for (const holding of await activeHoldingBindings())
          await (
            await holdingResources(holding.id)
          ).write
            .prepare(
              "INSERT INTO iam_revoked_sessions SELECT DISTINCT ?,company_id,user_id,? FROM assignments WHERE user_id=? ON CONFLICT DO NOTHING",
            )
            .bind(id, Date.now(), actor.userId)
            .run();
        await env.CONTROL_DB.prepare(
          "INSERT INTO iam_revoked_sessions VALUES(?,'platform',?,?) ON CONFLICT DO NOTHING",
        )
          .bind(id, actor.userId, Date.now())
          .run();
      }
      await env.CONTROL_DB.prepare(
        'DELETE FROM session WHERE id=? AND "userId"=?',
      )
        .bind(id, actor.userId)
        .run();
      return Response.json({ data: { revoked: true } });
    }
    throw new ApiError(404, "NOT_FOUND", "Account action not found.");
  } catch (error) {
    return errorResponse(error);
  }
}
function csvCell(value: unknown) {
  const s = String(value ?? "");
  return (
    '"' + (/^[=+@\-\t\r]/.test(s) ? "'" : "") + s.replaceAll('"', '""') + '"'
  );
}
export async function iamApi(
  request: Request,
  holdingSlug: string | null,
  path: string[],
) {
  try {
    const { db: raw, actor } = await iamContext(holdingSlug),
      db = iamDb(raw),
      route = path.join("/"),
      key = request.headers.get("Idempotency-Key") ?? "";
    if (request.method === "GET") {
      if (route === "audit") {
        const params = new URL(request.url).searchParams;
        return Response.json(
          {
            data: await iamAuditHistory(
              db,
              actor,
              params.get("search") ?? "",
              params.get("cursor") ?? undefined,
            ),
          },
          { headers: { "Cache-Control": "no-store" } },
        );
      }
      const data = await iamOverview(db, actor, route === "my-access");
      if (route === "matrix/export") {
        requirePermission(actor, iamPermission(actor, "iam.export"));
        const ids = new URL(request.url).searchParams.getAll("assignment");
        if (!ids.length || ids.length > 12)
          throw new ApiError(
            422,
            "EXPORT_SELECTION_REQUIRED",
            "Select one to twelve assignments.",
          );
        const lines = [
          [
            "User",
            "Assignment",
            "Permission",
            "Decision",
            "Role",
            "Scope",
            "Expires",
            "Policy revision",
            "Reason",
            "Conditions",
          ],
        ];
        for (const entry of data.decisions.filter((d) =>
          ids.includes(d.assignmentId),
        ))
          for (const d of entry.decisions)
            lines.push([
              entry.userId,
              entry.assignmentId,
              d.permission,
              d.result,
              d.role,
              d.scope ?? "",
              String(d.expiresAt ?? ""),
              String(d.revision),
              d.reasons.join("; "),
              d.conditions.join("; "),
            ]);
        await db
          .prepare("INSERT INTO iam_audit VALUES(?,?,?,?,?,?,?,?)")
          .bind(
            crypto.randomUUID(),
            actor.companyId,
            actor.userId,
            actor.assignmentId,
            "matrix.export",
            actor.companyId,
            JSON.stringify({ assignments: ids, rows: lines.length - 1 }),
            Date.now(),
          )
          .run();
        return new Response(
          lines.map((row) => row.map(csvCell).join(",")).join("\r\n"),
          {
            headers: {
              "Content-Type": "text/csv; charset=utf-8",
              "Content-Disposition": 'attachment; filename="erpx-access.csv"',
              "Cache-Control": "no-store",
            },
          },
        );
      }
      if (route === "inspector") {
        const params = Object.fromEntries(new URL(request.url).searchParams);
        for (const k of Object.keys(params))
          if (params[k] === "") delete params[k];
        return Response.json(
          {
            data: await inspectIamAccess(
              raw,
              actor,
              inspectorInput.parse(params),
            ),
          },
          { headers: { "Cache-Control": "no-store" } },
        );
      }
      return Response.json(
        { data },
        { headers: { "Cache-Control": "no-store" } },
      );
    }
    sameOrigin(request);
    const input = await request.json();
    let result: unknown;
    if (path[0] === "user-management" && ["preview", "save"].includes(path[1]))
      result = await managementCommand(db, actor, input.change, key, path[1] === "preview", input.previewToken);
    else if (route === "roles")
      result = await saveIamRole(db, actor, roleInput.parse(input), key);
    else if (route === "assignments")
      result = await grantIamAccess(db, actor, grantInput.parse(input), key);
    else if(route==="access-reviews")result=await startAccessReviewCampaign(db,actor,accessReviewCampaignInput.parse(input),key);
    else if(path[0]==="access-review-items"&&path[2]==="decide")result=await decideAccessReviewItem(db,actor,path[1],accessReviewDecisionInput.parse(input),key);
    else if(path[0]==="access-reviews"&&path[2]==="cancel"){const v=versionInput.parse(input);result=await cancelAccessReviewCampaign(db,actor,path[1],v.expectedVersion,v.reason,key);}
    else if (path[0] === "assignments" && path[2] === "revoke") {
      const v = versionInput.parse(input);
      result = await revokeIamAssignment(
        db,
        actor,
        path[1],
        v.expectedVersion,
        v.reason,
        key,
      );
    } else if (route === "connections")
      result = await saveConnection(
        db,
        actor,
        connectionInput.parse(input),
        key,
      );
    else if (path[0] === "connections") {
      const v = versionInput.parse(input),
        id = path[1];
      if (path[2] === "test")
        result = await testConnection(db, actor, id, v.expectedVersion, key);
      else if (path[2] === "activate")
        result = await proposeConnectionActivation(
          db,
          actor,
          id,
          v.expectedVersion,
          key,
        );
      else if (path[2] === "disable")
        result = await disableConnection(db, actor, id, v.expectedVersion, key);
      else if (path[2] === "sync") {
        const connection = await connectionById(db, actor.companyId, id);
        await iamCommand(
          db,
          actor,
          key,
          "connection.sync.request",
          { id, ...v },
          "iam.directory.manage",
          async () => {
            if (
              connection.version !== v.expectedVersion ||
              connection.status !== "active"
            )
              throw new ApiError(
                409,
                "CONNECTION_CHANGED",
                "Choose an active connection.",
              );
            return { id };
          },
        );
        await synchronizeConnection(db, connection);
        result = { id, status: "synchronized" };
      } else
        throw new ApiError(404, "NOT_FOUND", "Connection action not found.");
    } else if (route === "mappings")
      result = await proposeMapping(db, actor, mappingInput.parse(input), key);
    else if (path[0] === "mappings" && path[2] === "revoke") {
      const v = versionInput.parse(input);
      result = await iamCommand(
        db,
        actor,
        key,
        "mapping.revoke",
        { id: path[1], ...v },
        "iam.manage",
        async (c) => {
          const changed = await c.query(
            "UPDATE iam_mappings SET status='revoked',version=version+1 WHERE id=$1 AND company_id=$2 AND version=$3 AND (scope_node_id=$4 OR $5) RETURNING id",
            [
              path[1],
              actor.companyId,
              v.expectedVersion,
              actor.scopeNodeId,
              ["holding", "platform"].includes(actor.scopeKind),
            ],
          );
          if (!changed.rowCount)
            throw new ApiError(
              409,
              "MAPPING_CHANGED",
              "The mapping changed or is outside your scope.",
            );
          await c.query(
            `UPDATE ${actor.platform ? "platform_assignments" : "assignments"} SET revoked_at=$1,version=version+1 WHERE source='directory' AND source_id=$2 AND revoked_at IS NULL`,
            [Date.now(), path[1]],
          );
          return { revoked: true };
        },
      );
    } else if (route === "migrations")
      result = await prepareMigration(
        db,
        actor,
        migrationInput.parse(input),
        key,
      );
    else if (route === "emergency")
      result = await proposeEmergency(
        db,
        actor,
        z.object({ userId: z.string() }).parse(input).userId,
        key,
      );
    else if (path[0] === "reviews" && path[1])
      result = await decideIamReview(
        db,
        actor,
        path[1],
        reviewInput.parse(input),
        key,
        async (c, kind, p) => {
          if (!actor.platform && kind === "people.provision.grant") return applyProvisioningReview(c,actor,p);
          if (!actor.platform && kind === "directory.operation") return applyDirectoryOperationReview(c, actor, p);
          if (!actor.platform && kind.startsWith("people.lifecycle.")) return applyLifecycleReview(c, actor, kind, p);
          if (kind === "policy.grant") return applyManagementReview(c, actor, p);
          if (kind === "identity.restore") {
            if (
              p.userId === actor.userId ||
              (
                await c.query("SELECT iam_same_person($1,$2,$3) same", [
                  p.userId,
                  actor.userId,
                  actor.companyId,
                ])
              ).rows[0].same
            )
              throw new ApiError(
                403,
                "SELF_ACCESS_CHANGE",
                "Another person must approve your restoration.",
              );
            const changed = await c.query(
              "UPDATE iam_members SET status='active',version=version+1 WHERE company_id=$1 AND user_id=$2 AND status='suspended' AND version=$3 RETURNING user_id",
              [actor.companyId, p.userId, p.version],
            );
            if (!changed.rowCount)
              throw new ApiError(
                409,
                "IDENTITY_CHANGED",
                "The suspended identity changed.",
              );
            return;
          }
          if (kind === "beta.invitation") {
            const invitation = (
              await c.query(
                "SELECT 1 FROM beta_team_invitations i JOIN organization_nodes b ON b.id=i.branch_id WHERE i.id=$1 AND b.company_id=$2 AND i.revoked_at IS NULL AND i.accepted_at IS NULL AND i.expires_at>$3",
                [p.invitationId, actor.companyId, Date.now()],
              )
            ).rowCount;
            if (!invitation)
              throw new ApiError(
                409,
                "INVITATION_UNAVAILABLE",
                "The invitation expired or was revoked.",
              );
            for (const grant of z.array(grantInput).parse(p.grants))
              await validateIamGrant(c, actor, grant, String(p.parentId));
            return;
          }
          if (kind === "invitation.authorize") {
            const invitation = await (
              await runtimeEnv()
            ).CONTROL_DB.prepare(
              "SELECT 1 FROM account_invitations WHERE id=? AND company_id=? AND status='pending' AND expires_at>?",
            )
              .bind(p.invitationId, actor.companyId, Date.now())
              .first();
            if (!invitation)
              throw new ApiError(
                409,
                "INVITATION_UNAVAILABLE",
                "This invitation expired or was revoked.",
              );
            await validateIamGrant(
              c,
              actor,
              grantInput.parse(p),
              String(p.parentId),
            );
            return;
          }
          if (kind === "migration.activate") return applyMigration(c, actor, p);
          if (kind === "emergency.designate")
            return applyEmergency(c, actor, p);
          return applyDirectoryReview(c, actor, kind, p);
        },
        async (c, kind, p) => {
          if (!actor.platform && kind === "people.provision.grant") return applyProvisioningReview(c,actor,p,"reject");
          if (!actor.platform && kind === "directory.operation") return applyDirectoryOperationReview(c, actor, p, "reject");
          if (!actor.platform && kind.startsWith("people.lifecycle."))
            await applyLifecycleReview(c, actor, kind, p, "reject");
        },
      );
    else if (path[0] === "users" && path[2] === "sessions") {
      const userId = path[1];
      result = await iamCommand(
        db,
        actor,
        key,
        "user.sessions.revoke",
        { userId },
        "iam.manage",
        async (c) => {
          if (!["holding", "platform"].includes(actor.scopeKind))
            throw new ApiError(
              403,
              "COMPANY_SCOPE_REQUIRED",
              "Session revocation requires company scope.",
            );
          const target = (
            await c.query(
              `SELECT 1 FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE user_id=$1 ${actor.platform ? "" : "AND company_id=$2"}`,
              actor.platform ? [userId] : [userId, actor.companyId],
            )
          ).rowCount;
          if (!target)
            throw new ApiError(
              404,
              "USER_NOT_FOUND",
              "User not found in this company.",
            );
          const sessions = (
            await (
              await runtimeEnv()
            ).CONTROL_DB.prepare('SELECT id FROM session WHERE "userId"=?')
              .bind(userId)
              .all<{ id: string }>()
          ).results;
          for (const session of sessions)
            await c.query(
              "INSERT INTO iam_revoked_sessions VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
              [session.id, actor.companyId, userId, Date.now()],
            );
          if (!actor.platform)
            await c.query(
              "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id=$2",
              [actor.companyId, userId],
            );
          return {
            revoked: sessions.length,
            message: "Company sessions revoked. The user must sign in again.",
          };
        },
      );
    } else if (path[0] === "users" && path[2] === "restore") {
      await requireFreshMfa(actor);
      const v = versionInput.parse(input),
        userId = path[1];
      result = await iamCommand(
        db,
        actor,
        key,
        "identity.restore.propose",
        { userId, ...v },
        "iam.manage",
        async (c) => {
          if (!["holding", "platform"].includes(actor.scopeKind))
            throw new ApiError(
              403,
              "COMPANY_SCOPE_REQUIRED",
              "Restore access in company scope.",
            );
          const m = (
            await c.query(
              "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 AND status='suspended' AND version=$3",
              [actor.companyId, userId, v.expectedVersion],
            )
          ).rows[0];
          if (!m)
            throw new ApiError(
              409,
              "IDENTITY_CHANGED",
              "Only suspended identities can be restored.",
            );
          return proposeIamReview(
            c,
            actor,
            "identity.restore",
            { userId, version: v.expectedVersion },
            v.reason,
          );
        },
      );
    } else if (path[0] === "users" && path[2] === "suspend") {
      const v = versionInput.parse(input),
        userId = path[1];
      result = await iamCommand(
        db,
        actor,
        key,
        "identity.suspend",
        { userId, ...v },
        "iam.manage",
        async (c) => {
          if (userId === actor.userId)
            throw new ApiError(
              403,
              "SELF_ACCESS_CHANGE",
              "Another administrator must suspend your access.",
            );
          const table = actor.platform ? "platform_assignments" : "assignments";
          const grants = (
            await c.query(
              `SELECT * FROM ${table} WHERE user_id=$1 ${actor.platform ? "" : "AND company_id=$2"} FOR UPDATE`,
              actor.platform ? [userId] : [userId, actor.companyId],
            )
          ).rows;
          if (
            !grants.length ||
            !["holding", "platform"].includes(actor.scopeKind)
          )
            throw new ApiError(
              403,
              "COMPANY_SCOPE_REQUIRED",
              "Account suspension requires company administration scope.",
            );
          if (
            grants.some((g) =>
              ["holding_owner", "platform_admin"].includes(g.role),
            )
          )
            throw new ApiError(
              409,
              "PROTECTED_ADMINISTRATOR",
              "Revoke the administrator role through the protected assignment workflow first.",
            );
          const member = (
            await c.query(
              "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR UPDATE",
              [actor.companyId, userId],
            )
          ).rows[0];
          if (Number(member?.version ?? 1) !== v.expectedVersion)
            throw new ApiError(
              409,
              "IDENTITY_CHANGED",
              "The identity changed.",
            );
          if (member?.login_policy === "emergency")
            throw new ApiError(
              409,
              "PROTECTED_IDENTITY",
              "Emergency access requires an independent review.",
            );
          await c.query(
            "INSERT INTO iam_members(company_id,user_id,principal_id,status) VALUES($1,$2,$3,'suspended') ON CONFLICT(company_id,user_id) DO UPDATE SET status='suspended',version=iam_members.version+1",
            [actor.companyId, userId, `local:${userId}`],
          );
          if (!actor.platform)
            await c.query(
              "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id=$2",
              [actor.companyId, userId],
            );
          return { suspended: true };
        },
      );
    } else throw new ApiError(404, "NOT_FOUND", "IAM action not found.");
    return Response.json({ data: result });
  } catch (error) {
    return errorResponse(error);
  }
}
