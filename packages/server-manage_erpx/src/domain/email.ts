export type BetaEmail = { to: string; subject: string; url: string; key?: string; sensitive?: boolean };
export type BetaEmailReceipt = { recipient: string; subject: string; body_text: string; provider_id: string; sensitive?: boolean };

export const betaEmailBody = (subject: string, url: string) =>
  `${subject}\n\n${url}\n\nIf you did not request this, you can ignore this email.`;

export function verifyBetaEmailReceipt(
  stored: BetaEmailReceipt | null,
  email: BetaEmail,
  body: string,
  providerId: string,
) {
  if (!stored) throw new Error('EMAIL_SANDBOX_RECEIPT_MISSING');
  if (
    stored.recipient !== email.to ||
    stored.subject !== email.subject ||
    stored.body_text !== body ||
    (stored.sensitive ?? false) !== (email.sensitive === true)
  ) {
    throw new Error('EMAIL_IDEMPOTENCY_CONFLICT');
  }
  return stored.provider_id || providerId;
}
