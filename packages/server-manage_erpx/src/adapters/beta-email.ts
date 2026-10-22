import { createCipheriv, createDecipheriv, hkdfSync, randomBytes } from 'node:crypto';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { BetaEnvironment } from '@erpx/server-manage_erpx/domain/config';
import { createBetaEmailApplication } from '@erpx/server-manage_erpx/application/email';
import type { BetaEmailReceipt } from '@erpx/server-manage_erpx/domain/email';

type EmailEnvironment = BetaEnvironment & {
  BETTER_AUTH_SECRET?: string;
  ERPX_EMAIL_ADAPTER?: string;
  ERPX_ENVIRONMENT?: string;
  CONTROL_DB?: SqlDatabase;
};

export type BetaEmailOptions = { sensitive?: boolean };

const redactedSensitiveBody = '[redacted sensitive email body]';
const encryptionSalt = Buffer.from('erpx:sandbox-email-body:hkdf-salt:v1', 'utf8');
const encryptionContext = Buffer.from('erpx:manage_erpx:sandbox-email-body:aes-256-gcm:v1', 'utf8');

type CipherContext = { deliveryKey: string; to: string; subject: string; providerId: string };
type StoredSandboxEmailReceipt = {
  delivery_key: string;
  recipient: string;
  subject: string;
  body_text: string;
  body_ciphertext: string | null;
  provider_id: string;
};

function encryptionKey(secret: string | undefined): Buffer {
  if (!secret) throw new Error('EMAIL_SENSITIVE_ENCRYPTION_NOT_CONFIGURED');
  return Buffer.from(hkdfSync('sha256', Buffer.from(secret, 'utf8'), encryptionSalt, encryptionContext, 32));
}

function cipherAad(context: CipherContext): Buffer {
  return Buffer.from(JSON.stringify([
    'erpx:sandbox-email-body:v1',
    context.deliveryKey,
    context.to,
    context.subject,
    context.providerId,
  ]), 'utf8');
}

function encryptBody(body: string, secret: string | undefined, context: CipherContext): string {
  const nonce = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', encryptionKey(secret), nonce);
  cipher.setAAD(cipherAad(context));
  const encrypted = Buffer.concat([cipher.update(body, 'utf8'), cipher.final()]);
  const tag = cipher.getAuthTag();
  return `v1.${nonce.toString('base64url')}.${tag.toString('base64url')}.${encrypted.toString('base64url')}`;
}

function decodeBase64Url(value: string): Buffer {
  const decoded = Buffer.from(value, 'base64url');
  if (!value || decoded.toString('base64url') !== value) throw new Error('invalid envelope');
  return decoded;
}

function decryptBody(envelope: string, secret: string | undefined, context: CipherContext): string {
  const key = encryptionKey(secret);
  try {
    const [version, nonceText, tagText, ciphertextText, extra] = envelope.split(".");
    if (version !== 'v1' || !nonceText || !tagText || ciphertextText === undefined || extra !== undefined) {
      throw new Error('invalid envelope');
    }
    const nonce = decodeBase64Url(nonceText);
    const tag = decodeBase64Url(tagText);
    const ciphertext = ciphertextText ? decodeBase64Url(ciphertextText) : Buffer.alloc(0);
    if (nonce.length !== 12 || tag.length !== 16) throw new Error('invalid envelope');
    const decipher = createDecipheriv('aes-256-gcm', key, nonce);
    decipher.setAAD(cipherAad(context));
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString('utf8');
  } catch {
    throw new Error('EMAIL_SENSITIVE_RECEIPT_INVALID');
  }
}

export async function sendBetaEmail(
  env: EmailEnvironment,
  to: string,
  subject: string,
  url: string,
  key?: string,
  options: BetaEmailOptions = {},
) {
  return createBetaEmailApplication({
    configuration: {
      sandbox: env.ERPX_EMAIL_ADAPTER === 'sandbox',
      production: env.ERPX_ENVIRONMENT === 'production',
      repositoryAvailable: !!env.CONTROL_DB,
      providerConfigured: !!env.RESEND_API_KEY && !!env.RESEND_FROM,
    },
    clock: { now: () => Date.now() },
    identifiers: { next: () => crypto.randomUUID() },
    repository: {
      async asyncReserve(input) {
        const context = { deliveryKey: input.deliveryKey, to: input.to, subject: input.subject, providerId: input.providerId };
        const bodyCiphertext = input.sensitive ? encryptBody(input.body, env.BETTER_AUTH_SECRET, context) : null;
        const bodyText = input.sensitive ? redactedSensitiveBody : input.body;
        await env.CONTROL_DB!.prepare(
          'INSERT INTO sandbox_email_deliveries(id,delivery_key,recipient,subject,body_text,body_ciphertext,provider_id,created_at,metadata_json) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(delivery_key) DO NOTHING',
        ).bind(
          input.id,
          input.deliveryKey,
          input.to,
          input.subject,
          bodyText,
          bodyCiphertext,
          input.providerId,
          input.now,
          JSON.stringify({ adapter: 'sandbox' }),
        ).run();
      },
      async receipt(deliveryKey) {
        const stored = await env.CONTROL_DB!.prepare(
          'SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id FROM sandbox_email_deliveries WHERE delivery_key=?',
        ).bind(deliveryKey).first<StoredSandboxEmailReceipt>();
        if (!stored) return null;
        if (stored.body_ciphertext === null) {
          return {
            recipient: stored.recipient,
            subject: stored.subject,
            body_text: stored.body_text,
            provider_id: stored.provider_id,
          };
        }
        const context = { deliveryKey: stored.delivery_key, to: stored.recipient, subject: stored.subject, providerId: stored.provider_id };
        return {
          recipient: stored.recipient,
          subject: stored.subject,
          body_text: decryptBody(stored.body_ciphertext, env.BETTER_AUTH_SECRET, context),
          provider_id: stored.provider_id,
          sensitive: true,
        } satisfies BetaEmailReceipt;
      },
    },
    provider: {
      async send(input) {
        const response = await fetch('https://api.resend.com/emails', {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${env.RESEND_API_KEY}`,
            'Content-Type': 'application/json',
            ...(input.key ? { 'Idempotency-Key': input.key } : {}),
          },
          body: JSON.stringify({ from: env.RESEND_FROM, to: [input.to], subject: input.subject, text: input.body }),
          signal: AbortSignal.timeout(10000),
        });
        if (!response.ok) throw new Error('EMAIL_DELIVERY_FAILED');
        const receipt = await response.json().catch(() => null) as { id?: unknown } | null;
        return typeof receipt?.id === 'string' ? receipt.id : null;
      },
    },
  }).send({ to, subject, url, key, sensitive: options.sensitive === true });
}
