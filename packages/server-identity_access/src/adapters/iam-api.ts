import { nativeIdentityWrite } from '@erpx/server-identity_access/public/native-identity-write';
import { iamMfaProbeSql, iamReviewProbeSql } from './read-tail-family-r28';
import { samePersonQuerySql, sessionExistsSql } from "./iam-probe-sql";
import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {identitySnapshotSources as manageerpxSnapshotSources} from '@erpx/server-manage_erpx/public/identity-snapshot-sources';
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const manageerpxSources=manageerpxSnapshotSources();
const organizationSources=organizationSnapshotSources();
import { applyProvisioningReview } from "@erpx/server-identity_access/public/technology";
import { applyLifecycleReview } from "@erpx/server-people_payroll/public/people-lifecycle";
import { inspectIamAccess, inspectorInput } from "@erpx/server-identity_access/public/iam-inspector";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { nativeUserManagementAvailable, nativeUserManagementDecideReview, nativeUserManagementPreview, nativeUserManagementSave } from "@erpx/server-identity_access/public/native-user-management";
import { nativeEmergencyAccess } from "@erpx/server-identity_access/adapters/native-recovery";
import "server-only";
import { peopleRelationshipsSnapshot, peopleAccessSnapshot } from "@erpx/server-identity_access/public/people-access";
import {
  createNativePeopleRelationshipsClient,
  nativePeopleRelationshipsAvailable,
  nativePeopleRelationshipsConfiguration,
  nativePeopleRelationshipsFailure,
  type NativePeopleRelationshipsContext,
} from "../public/native-people-relationships";
import {
  createNativePeopleAccessClient,
  nativePeopleAccessAvailable,
  nativePeopleAccessFailure,
} from "../public/native-people-grants";
import { z } from "zod";
import { currentSession,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {bindIamReviewEvidence,bindIamMigrationEvidence} from "@erpx/server-platform_administration/public/system-evidence";
import { iamContext } from "@erpx/server-identity_access/public/iam-context";
import {grantIamAccess,grantInput,proposeIamReview,revokeIamAssignment,decideIamReview,reviewInput} from "@erpx/server-identity_access/public/iam-service";
import {iamOverview,iamAuditHistory} from '@erpx/server-identity_access/public/iam-service-reads';
import {validateIamGrant} from '@erpx/server-identity_access/public/iam-grants';
import {proposeMapping,mappingInput} from "@erpx/server-identity_access/public/directory-mapping";
import {
  prepareMigration,
  migrationInput,
  applyMigration,
  proposeEmergency,
  applyEmergency,
} from "@erpx/server-identity_access/public/iam-migrations";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import {nativeAccountHttp} from "@erpx/server-identity_access/public/native-account-http";
import { nativeIamSessionProbe } from './native-iam-session-probe';
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import { runtimeEnv, activeHoldingBindings } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamPermission } from "@erpx/server-identity_access/public/iam-context";
import {
  createNativeIamAssignmentsClient,
  nativeIamAssignmentsFailure,
  nativeIamAssignmentsConfiguration,
  NativeIamAssignmentsError,
  type NativeIamPageContext,
} from "../public/native-iam-assignments";

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

async function nativeIamAssignmentRevoke(
  actor: {
    userId: string;
    holdingId: string | null;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    assignmentId: string;
    role: string;
    scopeKind: string;
    platform: boolean;
    authority?: { revision: number; assignmentVersion: number };
    iam?: { permissions: readonly string[] };
  },
  holdingSlug: string | null,
  request: Request,
  id: string,
  version: number,
  reason: string,
  key: string,
) {
  if (process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG === undefined || actor.platform)
    return undefined;
  if (!/^[A-Za-z0-9:_.-]{8,180}$/.test(key))
    throw new ApiError(
      422,
      "COMMAND_KEY_REQUIRED",
      "A valid Idempotency-Key is required.",
    );
  if (!holdingSlug || !actor.holdingId || !actor.authority || !actor.iam)
    throw new NativeIamAssignmentsError("NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED");
  const page: NativeIamPageContext = {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.scopeKind,
    authority: actor.authority,
    iam: { permissions: actor.iam.permissions },
  };
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  try {
    const client = createNativeIamAssignmentsClient(
      nativeIamAssignmentsConfiguration(raw),
    );
    return await client.revoke(
      request.headers,
      page,
      id,
      JSON.stringify({ expectedVersion: version, reason }),
      key,
      {
        holdingSlug,
        expected: {
          companyId: page.companyId,
          assignmentId: page.assignmentId,
          revision: page.authority.revision,
          assignmentVersion: page.authority.assignmentVersion,
        },
      },
    );
  } catch (error) {
    const failure = nativeIamAssignmentsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
export async function iamAccountApi(request: Request, path: string[]) {
  if(path.join('/')==='mfa/enroll'||path.join('/')==='mfa/verify'){
    // Static logical path from this owned entry, independent of the external
    // Holding namespace. Rust owns authentication, rate/counter and writes.
    const url=new URL(request.url);url.pathname='/api/v1/account/iam/'+path.join('/');
    return nativeAccountHttp(new Request(url,request));
  }
  try {
    sameOrigin(request);
    const { session } = await currentSession(),
      actor = { userId: session.user.id, sessionId: session.session.id };
    if (request.method === "GET") {
      if (path.join("/") === "emergency/active")
        return Response.json(
          canonicalDestinationLinks({ data: await nativeEmergencyAccess.active(request) }),
          { headers: { "Cache-Control": "no-store" } },
        );
      const env = await runtimeEnv();
      return Response.json(
        canonicalDestinationLinks({
          data: {
            mfaEnabled: !!(await env.CONTROL_DB.prepare(
              iamMfaProbeSql("WHERE user_id=? AND enabled"),
            )
              .bind(actor.userId)
              .first()),
            sessions: await nativeIamSessionProbe(
              { userId: actor.userId },
              request.headers,
            ),
            currentSessionId: actor.sessionId,
          },
        }),
        { headers: { "Cache-Control": "no-store" } },
      );
    }
    const input = await request.json();
    if (path.join("/") === "emergency/activate")
      return Response.json(canonicalDestinationLinks({
        data: await nativeEmergencyAccess.activate(
          request,
          request.headers.get("Idempotency-Key") ?? "",
          input,
        ),
      }));
    if (path.join("/") === "emergency/revoke")
      return Response.json(canonicalDestinationLinks({
        data: await nativeEmergencyAccess.revoke(
          request,
          request.headers.get("Idempotency-Key") ?? "",
          input,
        ),
      }));
    if (path.join("/") === "sessions/revoke") {
      await requireFreshMfa(actor);
      const id = z.object({ id: z.string().min(1) }).parse(input).id;
      const env = await runtimeEnv();
      if (
        await env.CONTROL_DB.prepare(
          sessionExistsSql(''),
        )
          .bind(id, actor.userId)
          .first()
      ) {
        for (const _holding of await activeHoldingBindings())
          await nativePrepare("w_52777ef4470d")
            .bind(id, Date.now(), actor.userId)
            .run();
        await nativePrepare("w_1a1bf71b659f")
          .bind(id, actor.userId, Date.now())
          .run();
      }
      await nativePrepare("w_0668ea9cd0dd")
        .bind(id, actor.userId)
        .run();
      return Response.json(canonicalDestinationLinks({ data: { revoked: true } }));
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
function nativePeopleRelationshipsContext(
  actor: {
    userId: string;
    holdingId: string | null;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    assignmentId: string;
    role: string;
    scopeKind: string;
    platform: boolean;
    authority?: { revision: number; assignmentVersion: number };
    iam?: { permissions: readonly string[] };
  },
  holdingSlug: string,
): NativePeopleRelationshipsContext {
  if (!actor.holdingId || !actor.authority || !actor.iam)
    throw new ApiError(
      409,
      "WORK_CONTEXT_CHANGED",
      "Your work role changed. Reload and review before retrying.",
    );
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    scopeKind: actor.scopeKind,
    authority: actor.authority,
    iam: { permissions: actor.iam.permissions },
  };
}
async function nativePeopleRelationshipsCommand(
  actor: Parameters<typeof nativePeopleRelationshipsContext>[0],
  holdingSlug: string | null,
  request: Request,
  command: { kind: "preview"; change: unknown } | { kind: "apply"; token: string; key: string },
) {
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  if (!nativePeopleRelationshipsAvailable() || !raw || !holdingSlug)
    throw new ApiError(
      503,
      "NATIVE_PEOPLE_RELATIONSHIPS_REQUIRED",
      "This People relationship command requires the native owner service.",
    );
  if (actor.platform || !actor.authority || !actor.iam || !actor.holdingId)
    throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Select a Company workspace for People access.");
  try {
    const client = createNativePeopleRelationshipsClient(
      nativePeopleRelationshipsConfiguration(raw),
    );
    const context = nativePeopleRelationshipsContext(actor, holdingSlug);
    return command.kind === "preview"
      ? await client.preview(request.headers, context, command.change)
      : await client.apply(request.headers, context, command.token, command.key);
  } catch (error) {
    const failure = nativePeopleRelationshipsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
async function nativePeopleAccessCommand(
  actor: Parameters<typeof nativePeopleRelationshipsContext>[0],
  holdingSlug: string | null,
  request: Request,
  command: { kind: "preview"; change: unknown } | { kind: "apply"; token: string; key: string },
) {
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  if (!nativePeopleAccessAvailable() || !raw || !holdingSlug)
    throw new ApiError(
      503,
      "NATIVE_PEOPLE_ACCESS_REQUIRED",
      "This People access command requires the native owner service.",
    );
  if (actor.platform || !actor.authority || !actor.iam || !actor.holdingId)
    throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Select a Company workspace for People access.");
  if (command.kind === "apply" && !/^[A-Za-z0-9:_.-]{8,180}$/.test(command.key))
    throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
  try {
    const client = createNativePeopleAccessClient(
      nativePeopleRelationshipsConfiguration(raw),
    );
    const context = nativePeopleRelationshipsContext(actor, holdingSlug);
    return command.kind === "preview"
      ? await client.preview(request.headers, context, command.change)
      : await client.apply(request.headers, context, command.token, command.key);
  } catch (error) {
    const failure = nativePeopleAccessFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
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
      if (route === "people-relationships") return Response.json({ data: await peopleRelationshipsSnapshot(db, actor) }, { headers: { "Cache-Control": "private, no-store" } });
      if (route === "people-access") return Response.json(canonicalDestinationLinks({ data: await peopleAccessSnapshot(db, actor) }), { headers: { "Cache-Control": "no-store" } });
      if (route === "audit") {
        const params = new URL(request.url).searchParams;
        return Response.json(
          canonicalDestinationLinks({
            data: await iamAuditHistory(
              db,
              actor,
              params.get("search") ?? "",
              params.get("cursor") ?? undefined,
            ),
          }),
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
        await nativePrepare("w_e130d1b143d2")
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
          canonicalDestinationLinks({
            data: await inspectIamAccess(
              raw,
              actor,
              inspectorInput.parse(params),
            ),
          }),
          { headers: { "Cache-Control": "no-store" } },
        );
      }
      return Response.json(
        canonicalDestinationLinks({ data }),
        { headers: { "Cache-Control": "no-store" } },
      );
    }
    sameOrigin(request);
    const input = await request.json();
    let result: unknown;
    if (route === "people-relationships/preview") result = await nativePeopleRelationshipsCommand(actor, holdingSlug, request, { kind: "preview", change: input.change });
    else if (route === "people-relationships/apply") result = await nativePeopleRelationshipsCommand(actor, holdingSlug, request, { kind: "apply", token: z.string().uuid().parse(input.previewToken), key });
    else if (route === "people-access/preview") result = await nativePeopleAccessCommand(actor, holdingSlug, request, { kind: "preview", change: input.change });
    else if (route === "people-access/apply") result = await nativePeopleAccessCommand(actor, holdingSlug, request, { kind: "apply", token: z.string().uuid().parse(input.previewToken), key });
    else if(holdingSlug&&path.length===3&&path[2]==='evidence'&&path[0]==='reviews')
      result=await bindIamReviewEvidence(db,await requireWorkContext(holdingSlug),path[1],input,key);
    else if(holdingSlug&&path.length===3&&path[2]==='evidence'&&path[0]==='migrations')
      result=await bindIamMigrationEvidence(db,await requireWorkContext(holdingSlug),path[1],input,key);
    else if (holdingSlug && path[0] === "user-management" && ["preview", "save"].includes(path[1])) {
      if (!nativeUserManagementAvailable())
        throw new ApiError(503, "NATIVE_USER_MANAGEMENT_REQUIRED", "The native user-management service is unavailable.");
      result =
        path[1] === "preview"
          ? await nativeUserManagementPreview(request.headers, holdingSlug, input.change)
          : await nativeUserManagementSave(
              request.headers,
              holdingSlug,
              input.change,
              z.string().parse(input.previewToken),
              key,
            );
    }
    else if (route === "assignments")
      result = await grantIamAccess(db, actor, grantInput.parse(input), key);
    else if (path[0] === "assignments" && path[2] === "revoke") {
      const v = versionInput.parse(input);
      result =
        (await nativeIamAssignmentRevoke(
          actor,
          holdingSlug,
          request,
          path[1],
          v.expectedVersion,
          v.reason,
          key,
        )) ??
        (await revokeIamAssignment(
          db,
          actor,
          path[1],
          v.expectedVersion,
          v.reason,
          key,
        ));
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
          const changed = await nativeIdentityWrite("w_477f64b8b491", [
              path[1],
              actor.companyId,
              v.expectedVersion,
              actor.scopeNodeId,
              ["holding", "platform"].includes(actor.scopeKind),
            ]);
          if (!changed.rows)
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
    else if (path[0] === "reviews" && path[1] && holdingSlug && nativeUserManagementAvailable() && await db.prepare(iamReviewProbeSql("id=? AND company_id=? AND kind='policy.grant'")).bind(path[1], actor.companyId).first())
      result = await nativeUserManagementDecideReview(request.headers, holdingSlug, path[1], reviewInput.parse(input), key);
    else if (path[0] === "reviews" && path[1])
      {
      let directoryOperation: Record<string, unknown> | undefined;
      result = await decideIamReview(
        db,
        actor,
        path[1],
        reviewInput.parse(input),
        key,
        async (c, kind, p) => {
          if (!actor.platform && kind === "people.provision.grant") return applyProvisioningReview(c,actor,p);
          if (!actor.platform && kind === "directory.operation") { directoryOperation = p as Record<string, unknown>; return; }
          if (!actor.platform && kind.startsWith("people.lifecycle.")) return applyLifecycleReview(c, actor, kind, p);
          if (kind === "identity.restore") {
            if (
              p.userId === actor.userId ||
              (
                await c.query(samePersonQuerySql, [
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
            const changed = await nativeIdentityWrite("w_d0707ba38dc2", [actor.companyId, p.userId, p.version]);
            if (!changed.rows)
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
                `SELECT 1 ${manageerpxSources.iam_api_beta_team_invitations_i_from} ${organizationSources.iam_api_organization_nodes_b_join} ON b.id=i.branch_id WHERE i.id=$1 AND b.company_id=$2 AND i.revoked_at IS NULL AND i.accepted_at IS NULL AND i.expires_at>$3`,
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
          throw new ApiError(409, "REVIEW_UNSUPPORTED", "The review kind is not supported here.");
        },
        async (c, kind, p) => {
          if (!actor.platform && kind === "people.provision.grant") return applyProvisioningReview(c,actor,p,"reject");
          if (!actor.platform && kind === "directory.operation") { directoryOperation = p as Record<string, unknown>; return; }
          if (!actor.platform && kind.startsWith("people.lifecycle."))
            await applyLifecycleReview(c, actor, kind, p, "reject");
        },
      );
      if (directoryOperation && holdingSlug) {
        // The review decision committed above. Queue the reviewed Directory
        // operation natively; a transport failure leaves it recoverable by the
        // native worker, which sweeps approved pending reviews.
        const { nativeDirectoryOperationsAvailable, nativeDirectoryOperationsReview } = await import("@erpx/server-identity_access/adapters/native-directory-operations");
        const parsed = reviewInput.parse(input);
        if (nativeDirectoryOperationsAvailable())
          try {
            await nativeDirectoryOperationsReview(
              request.headers,
              holdingSlug,
              { ...directoryOperation, decision: parsed.decision },
              key + ":directory",
            );
          } catch {}
      }
      }
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
            await nativeIdentityWrite("w_d162e4b42ba4", [session.id, actor.companyId, userId, Date.now()]);
          if (!actor.platform)
            await nativeIdentityWrite("w_8774a0e28e6e", [actor.companyId, userId]);
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
          await nativeIdentityWrite("w_f78e88caed87", [actor.companyId, userId, `local:${userId}`]);
          if (!actor.platform)
            await nativeIdentityWrite("w_8774a0e28e6e", [actor.companyId, userId]);
          return { suspended: true };
        },
      );
    } else throw new ApiError(404, "NOT_FOUND", "IAM action not found.");
    return Response.json(canonicalDestinationLinks({ data: result }));
  } catch (error) {
    return errorResponse(error);
  }
}
