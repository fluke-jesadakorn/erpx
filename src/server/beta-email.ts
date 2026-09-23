import type { SqlDatabase } from "./runtime/types";
import type { BetaEnvironment } from "./beta-config";

type EmailEnvironment = BetaEnvironment & {
  ERPX_EMAIL_ADAPTER?: string;
  ERPX_ENVIRONMENT?: string;
  CONTROL_DB?: SqlDatabase;
};

export async function sendBetaEmail(
  env: EmailEnvironment,
  to: string,
  subject: string,
  url: string,
  key?: string,
) {
  if (env.ERPX_EMAIL_ADAPTER === "sandbox") {
    if (env.ERPX_ENVIRONMENT === "production")
      throw new Error("EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION");
    if (!env.CONTROL_DB) throw new Error("EMAIL_SANDBOX_NOT_CONFIGURED");
    const deliveryKey = key ?? `sandbox-email-${crypto.randomUUID()}`;
    const id = `sandbox-email-${crypto.randomUUID()}`;
    const providerId = `sandbox-${deliveryKey}`;
    const body = `${subject}\n\n${url}\n\nIf you did not request this, you can ignore this email.`;
    await env.CONTROL_DB
      .prepare(
        "INSERT INTO sandbox_email_deliveries(id,delivery_key,recipient,subject,body_text,provider_id,created_at,metadata_json) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(delivery_key) DO NOTHING",
      )
      .bind(
        id,
        deliveryKey,
        to,
        subject,
        body,
        providerId,
        Date.now(),
        JSON.stringify({ adapter: "sandbox" }),
      )
      .run();
    const stored = await env.CONTROL_DB
      .prepare(
        "SELECT recipient,subject,body_text,provider_id FROM sandbox_email_deliveries WHERE delivery_key=?",
      )
      .bind(deliveryKey)
      .first<{
        recipient: string;
        subject: string;
        body_text: string;
        provider_id: string;
      }>();
    if (!stored) throw new Error("EMAIL_SANDBOX_RECEIPT_MISSING");
    if (
      stored.recipient !== to ||
      stored.subject !== subject ||
      stored.body_text !== body
    )
      throw new Error("EMAIL_IDEMPOTENCY_CONFLICT");
    return stored.provider_id || providerId;
  }
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
