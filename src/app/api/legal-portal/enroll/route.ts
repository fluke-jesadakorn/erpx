import {appLegalEnrollmentAttempt} from "@/server/modules/legal/public/app-enrollment-attempt-effects";
import { z } from "zod";
import { boundedBody } from "@/server/modules/legal/public/legal/body";
import { enrollmentAccess } from "@/server/modules/legal/public/legal/enrollment";
import { createAuth } from "@/server/modules/identity_access/public/auth";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {configuredProductOrigin} from '@/lib/runtime/product-hosts';
import {legalEnrollmentReturnPath} from '@/server/modules/identity_access/public/rules/public-surface-entry';
export async function POST(request: Request) {
  try {
    const origin = requestPublicOrigin(request);
    const accountOrigin=configuredProductOrigin('account');
    if(accountOrigin&&(origin!==accountOrigin||new URL(request.url).origin!==accountOrigin||request.headers.get('host')!==new URL(accountOrigin).host))throw new ApiError(403,'LEGAL_ENROLLMENT_ACCOUNT_REQUIRED','Continue invitation enrollment on ERPX Account.');
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
    const rate = await appLegalEnrollmentAttempt(invite.write,invite.id,Date.now(),Date.now()-3600000,Date.now()-3600000);
    if (!rate || rate.attempts > 3)
      throw new ApiError(
        429,
        "INVITATION_RATE_LIMIT",
        "Try again later or use the verification email already sent.",
      );
    const continuation=legalEnrollmentReturnPath('/legal-portal/continue?'+new URLSearchParams({holding:invite.slug,share:invite.id}));
    if(accountOrigin&&!continuation)throw new ApiError(404,'LEGAL_INVITATION_UNAVAILABLE','The original invitation return is unavailable.');
    const callbackURL=accountOrigin&&continuation?new URL(continuation,origin).href:`${origin}/legal-portal/${encodeURIComponent(invite.slug)}`;
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
          callbackURL,
        }),
      }),
    );
  } catch (e) {
    return errorResponse(e);
  }
}
