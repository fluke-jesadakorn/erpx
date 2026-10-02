import { z } from "zod";
import { sessionFromRequest } from "@/server/auth";
import { runtimeEnv } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { defaultTimePreference, validTimeZone } from "@/lib/ui-format";

const inputSchema = z.object({
  mode: z.enum(["system", "manual"]),
  timeZone: z.string().max(80).nullable(),
  hourCycle: z.enum(["h12", "h23"]),
  version: z.number().int().min(0),
}).strict().refine(value => value.mode === "system" ? value.timeZone === null : !!value.timeZone && validTimeZone(value.timeZone), {
  message: "Choose a valid IANA timezone for Manual mode.",
});

async function userId(request: Request) {
  const session = await sessionFromRequest(request);
  if (!session) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in to edit your time settings.");
  return session.user.id;
}

export async function GET(request: Request) {
  try {
    const id = await userId(request);
    const db = (await runtimeEnv()).CONTROL_DB;
    const row = await db.prepare("SELECT mode,time_zone AS \"timeZone\",hour_cycle AS \"hourCycle\",version FROM account_time_preferences WHERE user_id=?")
      .bind(id).first();
    return Response.json({ data: row ?? defaultTimePreference }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}

export async function PATCH(request: Request) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const id = await userId(request);
    const parsed = inputSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "INVALID_TIME_PREFERENCE", parsed.error.issues[0]?.message ?? "Invalid time preference.");
    const { mode, timeZone, hourCycle, version } = parsed.data;
    const db = (await runtimeEnv()).CONTROL_DB;
    const row = version === 0
      ? await db.prepare(`INSERT INTO account_time_preferences(user_id,mode,time_zone,hour_cycle,version,updated_at)
          VALUES(?,?,?,?,1,?) ON CONFLICT(user_id) DO NOTHING
          RETURNING mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version`)
          .bind(id,mode,timeZone,hourCycle,Date.now()).first()
      : await db.prepare(`UPDATE account_time_preferences
          SET mode=?,time_zone=?,hour_cycle=?,version=version+1,updated_at=?
          WHERE user_id=? AND version=?
          RETURNING mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version`)
          .bind(mode,timeZone,hourCycle,Date.now(),id,version).first();
    if (!row) throw new ApiError(409, "TIME_PREFERENCE_CHANGED", "Reload your time settings and try again.");
    return Response.json({ data: row }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
