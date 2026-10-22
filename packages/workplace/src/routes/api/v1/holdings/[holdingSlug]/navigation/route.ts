import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { navigationSnapshot } from "@erpx/server-work_projection/public/navigation-snapshot";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { markHomeRead, unreadHomeCount } from "@erpx/server-work_projection/public/navigation-counts";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { z } from "zod";

type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const data = await navigationSnapshot(actor, new URL(request.url).searchParams.get("include") === "attention");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  const readThrough = Date.now();
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    z.object({ action: z.literal("read-home") }).strict().parse(await betaBody(request));
    const { write } = await holdingResources(actor.holdingId);
    await markHomeRead(write, actor, readThrough);
    return Response.json(canonicalDestinationLinks({ data: { home: await unreadHomeCount(write, actor) } }), { headers });
  } catch (error) { return errorResponse(error); }
}
