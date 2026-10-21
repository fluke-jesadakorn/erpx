import { createDecipheriv, hkdfSync } from 'node:crypto';

export type EncryptedSandboxEmail = {
  delivery_key: string;
  recipient: string;
  subject: string;
  body_text: string;
  body_ciphertext: string | null;
  provider_id: string;
};

/** Decrypts an isolated sensitive sandbox email in memory for owner-flow tests. */
export function decryptSensitiveSandboxEmailBody(secret: string, delivery: EncryptedSandboxEmail): string {
  if (!secret || delivery.body_text !== '[redacted sensitive email body]' || !delivery.body_ciphertext) {
    throw new Error('SENSITIVE_SANDBOX_EMAIL_REQUIRED');
  }
  try {
    const [version, nonceText, tagText, ciphertextText, extra] = delivery.body_ciphertext.split('.');
    if (version !== 'v1' || !nonceText || !tagText || ciphertextText === undefined || extra !== undefined) throw new Error();
    const decode = (value: string) => {
      const decoded = Buffer.from(value, 'base64url');
      if (!value || decoded.toString('base64url') !== value) throw new Error();
      return decoded;
    };
    const nonce = decode(nonceText), tag = decode(tagText), ciphertext = ciphertextText ? decode(ciphertextText) : Buffer.alloc(0);
    if (nonce.length !== 12 || tag.length !== 16) throw new Error();
    const key = Buffer.from(hkdfSync(
      'sha256',
      Buffer.from(secret, 'utf8'),
      Buffer.from('erpx:sandbox-email-body:hkdf-salt:v1', 'utf8'),
      Buffer.from('erpx:manage_erpx:sandbox-email-body:aes-256-gcm:v1', 'utf8'),
      32,
    ));
    const decipher = createDecipheriv('aes-256-gcm', key, nonce);
    decipher.setAAD(Buffer.from(JSON.stringify([
      'erpx:sandbox-email-body:v1',
      delivery.delivery_key,
      delivery.recipient,
      delivery.subject,
      delivery.provider_id,
    ]), 'utf8'));
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString('utf8');
  } catch {
    throw new Error('SENSITIVE_SANDBOX_EMAIL_INVALID');
  }
}
