import type { BetaEnvironment } from "./beta-config";
export async function sendBetaEmail(
  env: BetaEnvironment,
  to: string,
  subject: string,
  url: string,
  key?: string,
) {
  if (!env.RESEND_API_KEY || !env.RESEND_FROM)
    throw new Error("EMAIL_NOT_CONFIGURED");
  const response = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${env.RESEND_API_KEY}`,
      "Content-Type": "application/json",
      ...(key ? { "Idempotency-Key": key } : {}),
    },
    body: JSON.stringify({
      from: env.RESEND_FROM,
      to: [to],
      subject,
      text: `${subject}\n\n${url}\n\nIf you did not request this, you can ignore this email.`,
    }),
    signal: AbortSignal.timeout(10000),
  });
  if (!response.ok) throw new Error("EMAIL_DELIVERY_FAILED");
  const receipt = (await response.json().catch(() => null)) as
    | { id?: unknown }
    | null;
  return typeof receipt?.id === "string" ? receipt.id : null;
}
