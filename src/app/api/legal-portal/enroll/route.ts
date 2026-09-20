import { z } from "zod";
import { boundedBody } from "@/server/legal/body";
import { enrollmentAccess } from "@/server/legal/enrollment";
import { createAuth } from "@/server/auth";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
export async function POST(request: Request) {
  try {
    const origin = requestPublicOrigin(request);
    if (request.headers.get("origin") !== origin)
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const raw = JSON.parse(
        new TextDecoder().decode(await boundedBody(request, 16000)),
      ),
      x = z
        .object({
          token: z.string().max(8000),
          name: z.string().trim().min(1).max(100),
          password: z.string().min(12).max(200),
        })
        .parse(raw),
      invite = await enrollmentAccess(x.token);
    if (!invite.env.RESEND_API_KEY || !invite.env.RESEND_FROM)
      throw new ApiError(
        503,
        "EMAIL_NOT_CONFIGURED",
        "Invitation account verification is not configured.",
      );
    const rate = await invite.write
      .prepare(
        "INSERT INTO legal_enrollment_attempts(share_id,window_start,attempts) VALUES(?,?,1) ON CONFLICT(share_id) DO UPDATE SET window_start=CASE WHEN legal_enrollment_attempts.window_start<? THEN excluded.window_start ELSE legal_enrollment_attempts.window_start END,attempts=CASE WHEN legal_enrollment_attempts.window_start<? THEN 1 ELSE legal_enrollment_attempts.attempts+1 END RETURNING attempts",
      )
      .bind(invite.id, Date.now(), Date.now() - 3600000, Date.now() - 3600000)
      .first<{ attempts: number }>();
    if (!rate || rate.attempts > 3)
      throw new ApiError(
        429,
        "INVITATION_RATE_LIMIT",
        "Try again later or use the verification email already sent.",
      );
    const auth = await createAuth(origin, undefined, invite.email);
    const headers = new Headers(request.headers);
    headers.delete("content-length");
    headers.set("content-type", "application/json");
    return auth.handler(
      new Request(`${origin}/api/auth/sign-up/email`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          name: x.name,
          email: invite.email,
          password: x.password,
          callbackURL: `${origin}/legal-portal/${encodeURIComponent(invite.slug)}`,
        }),
      }),
    );
  } catch (e) {
    return errorResponse(e);
  }
}
