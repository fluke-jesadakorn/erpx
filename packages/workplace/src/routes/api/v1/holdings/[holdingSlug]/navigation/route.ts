import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { nativeNavigationSnapshot, nativeMarkHomeRead } from "@erpx/server-work_projection/adapters/native-navigation";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { z } from "zod";

type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const data = await nativeNavigationSnapshot(request.headers, actor, new URL(request.url).searchParams.get("include") === "attention");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    z.object({ action: z.literal("read-home") }).strict().parse(await betaBody(request));
    return Response.json(canonicalDestinationLinks({ data: { home: await nativeMarkHomeRead(request.headers, actor) } }), { headers });
  } catch (error) { return errorResponse(error); }
}
