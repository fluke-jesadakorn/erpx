import {appLegalEnrollmentAttempt} from "@erpx/server-legal/public/app-enrollment-attempt-effects";
import { z } from "zod";
import { boundedBody } from "@erpx/server-legal/public/legal/body";
import { enrollmentAccess } from "@erpx/server-legal/public/legal/enrollment";
import { nativeAccountHttp } from "@erpx/server-identity_access/public/native-account-http";
import { sendBetaEmail } from "@erpx/server-manage_erpx/public/beta-email";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {legalEnrollmentReturnPath} from '@erpx/server-identity_access/public/rules/public-surface-entry';
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
    const native = await nativeAccountHttp(
      new Request(`${origin}/api/v1/account/legal-enrollment`, {
        method: "POST",
        headers: {
          host: new URL(origin).host,
          origin,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          token: x.token,
          name: x.name,
          password: x.password,
          continuation: continuation ?? `/legal-portal/${encodeURIComponent(invite.slug)}`,
        }),
      }),
    );
    if (native.status !== 200) {
      return new Response(await native.text(), {
        status: native.status,
        headers: { "content-type": "application/json", "cache-control": "private, no-store" },
      });
    }
    const enrolled = (await native.json()) as {
      data?: { verificationToken?: unknown; callbackURL?: unknown };
    };
    const verificationToken = enrolled.data?.verificationToken;
    const verificationCallback = enrolled.data?.callbackURL;
    if (typeof verificationToken !== "string" || typeof verificationCallback !== "string")
      throw new ApiError(503, "LEGAL_ENROLLMENT_UNAVAILABLE", "The Legal invitation is unavailable.");
    const verificationUrl = `${accountOrigin}/api/auth/verify-email?token=${encodeURIComponent(verificationToken)}&callbackURL=${encodeURIComponent(verificationCallback)}`;
    await sendBetaEmail(
      invite.env,
      invite.email,
      "Verify your ERPX email",
      `Verify your ERPX email\n\n${verificationUrl}\n\nIf you did not request this, you can ignore this email.`,
      `legal-enrollment-${invite.id}-${verificationToken.slice(0, 16)}`,
      { sensitive: true },
    );
    return Response.json(
      { data: { enrolled: true } },
      { headers: { "cache-control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
