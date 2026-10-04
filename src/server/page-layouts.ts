import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  currentSession,
  publicScope,
  requirePlatformContext,
  requireWorkContext,
} from "./authorization";
import { holdingResources, runtimeEnv } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";
import { navigationContext } from "./navigation-context";
import { companyAuthority } from "./staff-policy";
import { workContextGuards } from "./commands";
import { iamMutationGuards } from "./iam-store";
import { workspaceBase } from "@/lib/workspace";
import {
  allowedWidgets,
  pageLayoutSchema,
  isConfigurablePage,
  type LayoutSnapshot,
} from "@/lib/page-layouts";
import type { NavigationContext } from "@/lib/workplace-navigation";
import { ApiError, errorResponse } from "./http";
import { serviceCommandBody } from "./service-http";

const inputSchema = z
  .object({
    action: z.enum(["save", "reset", "publish"]),
    expectedVersion: z.number().int().nonnegative(),
    layout: pageLayoutSchema.nullable(),
  })
  .strict();
export async function pageLayoutRequest(
  request: Request,
  pageKey: string,
  holdingSlug?: string,
  account = false,
) {
  try {
    if (!isConfigurablePage(pageKey))
      throw new ApiError(
        404,
        "PAGE_LAYOUT_UNKNOWN",
        "This page has no configurable widgets.",
      );
    const { session } = await currentSession();
    if (!session)
      throw new ApiError(
        401,
        "SIGN_IN_REQUIRED",
        "Sign in to arrange your workspace.",
      );
    const work = holdingSlug ? await requireWorkContext(holdingSlug) : null;
    const platform = !work && !account ? await requirePlatformContext() : null;
    const db = (
      work
        ? (await holdingResources(work.holdingId)).write
        : (await runtimeEnv()).CONTROL_DB
    ) as PostgresDatabase;
    const context: NavigationContext = work
      ? await navigationContext(work)
      : {
          workspaceBase: account ? "/account" : "/platform",
          scopeKind: "platform",
          role: platform?.role ?? "employee",
        };
    const company = work?.companyId ?? (account ? "account" : "platform"),
      base = work ? workspaceBase(publicScope(work)) : context.workspaceBase;
    if (
      work &&
      (pageKey.startsWith("platform-") || pageKey.startsWith("account-"))
    )
      throw new ApiError(
        403,
        "LAYOUT_SCOPE",
        "This layout belongs to another workspace.",
      );
    if (
      (account && !pageKey.startsWith("account-")) ||
      (!account && !work && !pageKey.startsWith("platform-"))
    )
      throw new ApiError(
        403,
        "LAYOUT_SCOPE",
        "This layout belongs to another workspace.",
      );
    const canPublish = work
      ? work.scope.kind === "holding" && (await companyAuthority(db, work))
      : !account && platform?.role === "platform_admin";
    const definitions = allowedWidgets(pageKey, context),
      ids = new Set(definitions.map((w) => w.id));
    if (!ids.size)
      throw new ApiError(
        403,
        "PAGE_LAYOUT_FORBIDDEN",
        "This page is unavailable for your current role.",
      );
    const user = session.user.id;
    const read = async (
      tx: Parameters<Parameters<PostgresDatabase["transaction"]>[0]>[0],
    ): Promise<LayoutSnapshot> => {
      const rows = (
        await tx
          .prepare(
            "SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=? AND page_key=? AND ((owner_id=? AND workspace_base=?) OR (owner_id=? AND workspace_base=?))",
          )
          .bind(company, pageKey, user, base, "organization", "*")
          .all<{
            owner_id: string;
            layout_json: string | null;
            version: number;
          }>()
      ).results;
      const personal = rows.find((r) => r.owner_id === user),
        organization = rows.find((r) => r.owner_id === "organization");
      const decode = (value: string | null | undefined) =>
        value
          ? {
              ...pageLayoutSchema.parse(JSON.parse(value)),
              widgets: pageLayoutSchema
                .parse(JSON.parse(value))
                .widgets.filter((w) => ids.has(w.id)),
            }
          : null;
      return {
        personal: decode(personal?.layout_json),
        organization: decode(organization?.layout_json),
        version: personal?.version ?? 0,
        organizationVersion: organization?.version ?? 0,
        canPublish: !!canPublish,
      };
    };
    if (request.method === "GET")
      return Response.json(
        { data: await read(db) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    const { input, key } = await serviceCommandBody(request),
      parsed = inputSchema.parse(input);
    if (parsed.action === "publish" && !canPublish)
      throw new ApiError(
        403,
        "LAYOUT_PUBLISH_FORBIDDEN",
        "Company organization management is required to publish a layout.",
      );
    if (parsed.action !== "reset" && !parsed.layout)
      throw new ApiError(422, "LAYOUT_REQUIRED", "Provide a layout.");
    if (parsed.layout?.widgets.some((w) => !ids.has(w.id)))
      throw new ApiError(
        403,
        "WIDGET_UNAVAILABLE",
        "A widget is unavailable for your current role.",
      );
    const hash = createHash("sha256")
      .update(
        JSON.stringify({
          user,
          company,
          base,
          pageKey,
          assignment: work?.assignmentId ?? platform?.assignmentId,
          input: parsed,
        }),
      )
      .digest("hex");
    const result = await db.transaction(async (tx) => {
      await tx
        .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
        .bind(`${company}:${base}:${pageKey}`)
        .run();
      if (work) {
        await tx.batch([
          ...workContextGuards(tx, work, key),
          ...iamMutationGuards(tx, work, key),
          tx
            .prepare("DELETE FROM command_assertions WHERE command_id=?")
            .bind(key),
        ]);
        if (parsed.action === "publish" && !(await companyAuthority(tx, work)))
          throw new ApiError(
            403,
            "LAYOUT_PUBLISH_FORBIDDEN",
            "Your organization authority changed.",
          );
      } else {
        const current = await tx
          .prepare(
            'SELECT id FROM session WHERE id=? AND "userId"=? AND "expiresAt"::timestamptz>now() FOR SHARE',
          )
          .bind(session.session.id, user)
          .first();
        if (!current)
          throw new ApiError(401, "SESSION_EXPIRED", "Sign in again.");
        if (
          platform &&
          !(await tx
            .prepare(
              "SELECT id FROM platform_assignments WHERE id=? AND user_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? FOR SHARE",
            )
            .bind(
              platform.assignmentId,
              user,
              platform.role,
              Date.now(),
              Date.now(),
            )
            .first())
        )
          throw new ApiError(
            403,
            "ASSIGNMENT_UNAVAILABLE",
            "Your platform role changed.",
          );
      }
      const prior = await tx
        .prepare(
          "SELECT request_hash,result_json FROM page_layout_receipts WHERE id=?",
        )
        .bind(key)
        .first<{ request_hash: string; result_json: string }>();
      if (prior) {
        if (prior.request_hash !== hash)
          throw new ApiError(
            409,
            "IDEMPOTENCY_CONFLICT",
            "Use the original command or a new key.",
          );
        const replay = JSON.parse(prior.result_json) as LayoutSnapshot;
        return {...replay,canPublish:!!canPublish,personal:replay.personal?{...replay.personal,widgets:replay.personal.widgets.filter(w=>ids.has(w.id))}:null,organization:replay.organization?{...replay.organization,widgets:replay.organization.widgets.filter(w=>ids.has(w.id))}:null};
      }
      const before = await read(tx),
        publish = parsed.action === "publish",
        current = publish ? before.organizationVersion : before.version;
      if (current !== parsed.expectedVersion)
        throw new ApiError(
          409,
          "VERSION_CONFLICT",
          "The saved layout changed. Reload the latest layout before saving.",
        );
      const value =
        parsed.action === "reset" ? null : JSON.stringify(parsed.layout);
      await tx
        .prepare(
          "INSERT INTO page_layouts(company_id,owner_id,workspace_base,page_key,layout_json,version,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,owner_id,workspace_base,page_key) DO UPDATE SET layout_json=excluded.layout_json,version=excluded.version,updated_at=excluded.updated_at",
        )
        .bind(
          company,
          publish ? "organization" : user,
          publish ? "*" : base,
          pageKey,
          value,
          current + 1,
          Date.now(),
        )
        .run();
      const after = await read(tx);
      await tx
        .prepare(
          "INSERT INTO page_layout_receipts(id,request_hash,result_json,actor_id,company_id,workspace_base,page_key,action,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          key,
          hash,
          JSON.stringify(after),
          user,
          company,
          base,
          pageKey,
          parsed.action,
          Date.now(),
        )
        .run();
      return after;
    });
    return Response.json(
      { data: result },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
