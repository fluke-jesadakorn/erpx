import { currentSession } from "@/server/authorization";
import { accountQuickAccessItems, type AccountQuickAccessArea } from "@/server/account-quick-access";
import { workRoleOptions } from "@/server/work-role-options";
import { workspaceChoices } from "@/server/workspace-choices";
import { safeReturnPath } from "@/lib/access-problem";
import { parseHoldingWorkspace, workspaceBase } from "@/lib/workspace";
import { ApiError, errorResponse } from "@/server/http";
import { quickAccessPreferenceSchema, readQuickAccessPreferences, saveQuickAccessPreferences } from "@/server/quick-access-preferences";
import { serviceCommandBody } from "@/server/service-http";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ area: string }> };
const headers = { "Cache-Control": "private, no-store" };

function areaFrom(value: string): AccountQuickAccessArea {
  if (value === "my-access" || value === "security" || value === "work-role") return value;
  throw new ApiError(404, "NOT_FOUND", "Unknown account area.");
}

async function catalog(request: Request, area: AccountQuickAccessArea) {
  const { session } = await currentSession();
  const choices = await workspaceChoices();
  if (area !== "work-role") return {
    userId: session.user.id,
    surface: `account-${area}`,
    contextKey: "account",
    items: accountQuickAccessItems(area, { hasWorkspace: choices.length > 0 }),
  };
  const from = safeReturnPath(new URL(request.url).searchParams.get("from"));
  if (!from) throw new ApiError(422, "INVALID_RETURN_PATH", "Choose a valid workspace page.");
  const parts = new URL(from, "https://erpx.invalid").pathname.split("/").filter(Boolean);
  if (parts[0] !== "holdings" || !parts[1])
    throw new ApiError(422, "INVALID_RETURN_PATH", "Choose a holding workspace page.");
  const parsed = parseHoldingWorkspace(parts[1], parts.slice(2));
  if (!parsed) throw new ApiError(422, "INVALID_RETURN_PATH", "Choose a valid workspace page.");
  const { access, options } = await workRoleOptions(parsed.scope);
  return {
    userId: access.userId,
    surface: "account-work-role",
    contextKey: workspaceBase(parsed.scope),
    items: accountQuickAccessItems(area, { hasWorkspace: choices.length > 0, hasRoles: options.length > 0 }),
  };
}

export async function GET(request: Request, context: Context) {
  try {
    const area = areaFrom((await context.params).area);
    const scope = await catalog(request, area);
    return Response.json({ data: await readQuickAccessPreferences(scope.userId, scope.surface, scope.contextKey) }, { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    const area = areaFrom((await context.params).area);
    const { input, key } = await serviceCommandBody(request);
    const parsed = quickAccessPreferenceSchema.parse(input);
    const scope = await catalog(request, area);
    const saved = await saveQuickAccessPreferences({
      userId: scope.userId,
      surface: scope.surface,
      contextKey: scope.contextKey,
      pinnedIds: parsed.pinnedIds,
      expectedVersion: parsed.expectedVersion,
      availableIds: scope.items.map((item) => item.id),
      requestKey: key,
    });
    return Response.json({ data: saved }, { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
