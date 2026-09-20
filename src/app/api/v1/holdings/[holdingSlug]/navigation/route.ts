import { isMyAction } from "@/lib/work-item";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { workSnapshot } from "@/server/work-items";
import { markHomeRead, unreadHomeCount } from "@/server/navigation-counts";
import { betaBody } from "@/server/beta-http";
import { errorResponse } from "@/server/http";
import { z } from "zod";

type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    const [home, work] = await Promise.all([unreadHomeCount(write, actor), workSnapshot(actor)]);
    return Response.json({ data: {
      home,
      work: work.unavailableSources.length ? null : work.items.filter(isMyAction).length,
    } }, { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  const readThrough = Date.now();
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    z.object({ action: z.literal("read-home") }).strict().parse(await betaBody(request));
    const { write } = await holdingResources(actor.holdingId);
    await markHomeRead(write, actor, readThrough);
    return Response.json({ data: { home: await unreadHomeCount(write, actor) } }, { headers });
  } catch (error) { return errorResponse(error); }
}
