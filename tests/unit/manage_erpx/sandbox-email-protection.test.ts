import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SqlDatabase, SqlResult, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { sendBetaEmail } from '@erpx/server-manage_erpx/adapters/beta-email';

type EmailRow = {
  id: string;
  delivery_key: string;
  recipient: string;
  subject: string;
  body_text: string;
  body_ciphertext: string | null;
  provider_id: string;
  created_at: number;
  metadata_json: string;
};

class MemoryEmailDatabase {
  readonly rows = new Map<string, EmailRow>();

  prepare(sql: string): SqlStatement {
    const rows = this.rows;
    let values: unknown[] = [];
    const statement: SqlStatement = {
      bind(...nextValues) {
        values = nextValues;
        return statement;
      },
      async first<T>() {
        if (!sql.startsWith('SELECT')) throw new Error('Unexpected query');
        const row = rows.get(String(values[0]));
        return row ? structuredClone(row) as T : null;
      },
      async all<T = Record<string, unknown>>(): Promise<SqlResult<T>> {
        throw new Error('Unexpected query');
      },
      async run<T = Record<string, unknown>>(): Promise<SqlResult<T>> {
        if (!sql.startsWith('INSERT')) throw new Error('Unexpected query');
        const [id, deliveryKey, recipient, subject, bodyText, bodyCiphertext, providerId, createdAt, metadataJson] = values as [string, string, string, string, string, string | null, string, number, string];
        if (!rows.has(deliveryKey)) {
          rows.set(deliveryKey, {
            id,
            delivery_key: deliveryKey,
            recipient,
            subject,
            body_text: bodyText,
            body_ciphertext: bodyCiphertext,
            provider_id: providerId,
            created_at: createdAt,
            metadata_json: metadataJson,
          });
        }
        return { results: [], success: true, meta: { changes: 1, duration: 0 } };
      },
      async raw<T = unknown[]>() {
        throw new Error('Unexpected query');
      },
    };
    return statement;
  }

  async batch<T = Record<string, unknown>>(): Promise<SqlResult<T>[]> {
    throw new Error('Unexpected query');
  }

  async exec(): Promise<{ count: number; duration: number }> {
    throw new Error('Unexpected query');
  }
}

const secret = 'a long local test secret that is not used outside this test';
const invitationUrl = 'https://portal.example.test/invite?token=one-time-secret';

function sandboxEnvironment(database: MemoryEmailDatabase, authSecret: string | undefined = secret) {
  return {
    ERPX_EMAIL_ADAPTER: 'sandbox',
    ERPX_ENVIRONMENT: 'development',
    BETTER_AUTH_SECRET: authSecret,
    RESEND_API_KEY: 'provider-key',
    RESEND_FROM: 'noreply@example.test',
    CONTROL_DB: database as unknown as SqlDatabase,
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('Manage ERPX sandbox sensitive email storage', () => {
  it('stores a redacted body and authenticated ciphertext, then verifies idempotent receipts by decrypting', async () => {
    const database = new MemoryEmailDatabase();
    const env = sandboxEnvironment(database);
    const args = [env, 'customer@example.test', 'Your portal invitation', invitationUrl, 'portal-invitation:key'] as const;

    await expect(sendBetaEmail(...args, { sensitive: true })).resolves.toBe('sandbox-portal-invitation:key');
    const row = database.rows.get('portal-invitation:key');
    expect(row).toBeDefined();
    expect(row?.body_text).toBe('[redacted sensitive email body]');
    expect(row?.body_ciphertext).toMatch(/^v1\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/);
    expect(row?.body_ciphertext).not.toContain('one-time-secret');
    expect(row?.body_ciphertext).not.toContain(invitationUrl);

    await expect(sendBetaEmail(...args, { sensitive: true })).resolves.toBe('sandbox-portal-invitation:key');
    await expect(sendBetaEmail(env, 'customer@example.test', 'Your portal invitation', 'https://portal.example.test/invite?token=changed', 'portal-invitation:key', { sensitive: true }))
      .rejects.toThrow('EMAIL_IDEMPOTENCY_CONFLICT');
  });

  it('uses a fresh GCM nonce for each newly stored sensitive message', async () => {
    const database = new MemoryEmailDatabase();
    const env = sandboxEnvironment(database);
    await sendBetaEmail(env, 'customer@example.test', 'Invite', invitationUrl, 'invite:first', { sensitive: true });
    await sendBetaEmail(env, 'customer@example.test', 'Invite', invitationUrl, 'invite:second', { sensitive: true });
    const nonce = (key: string) => database.rows.get(key)?.body_ciphertext?.split('.')[1];
    expect(nonce('invite:first')).toBeDefined();
    expect(nonce('invite:first')).not.toBe(nonce('invite:second'));
  });

  it('fails closed when encryption is unconfigured or an authenticated receipt is altered', async () => {
    const missingKeyDb = new MemoryEmailDatabase();
    await expect(sendBetaEmail(sandboxEnvironment(missingKeyDb, ''), 'customer@example.test', 'Invite', invitationUrl, 'invite:missing-key', { sensitive: true }))
      .rejects.toThrow('EMAIL_SENSITIVE_ENCRYPTION_NOT_CONFIGURED');
    expect(missingKeyDb.rows.size).toBe(0);

    const database = new MemoryEmailDatabase();
    const env = sandboxEnvironment(database);
    await sendBetaEmail(env, 'customer@example.test', 'Invite', invitationUrl, 'invite:tamper', { sensitive: true });
    const rotatedSecretEnv = sandboxEnvironment(database, 'a different secret');
    await expect(sendBetaEmail(rotatedSecretEnv, 'customer@example.test', 'Invite', invitationUrl, 'invite:tamper', { sensitive: true }))
      .rejects.toThrow('EMAIL_SENSITIVE_RECEIPT_INVALID');
    const row = database.rows.get('invite:tamper');
    if (!row?.body_ciphertext) throw new Error('Expected an encrypted receipt');
    const parts = row.body_ciphertext.split('.');
    const ciphertext = parts[3]!;
    parts[3] = `${ciphertext[0] === 'A' ? 'B' : 'A'}${ciphertext.slice(1)}`;
    row.body_ciphertext = parts.join('.');
    await expect(sendBetaEmail(env, 'customer@example.test', 'Invite', invitationUrl, 'invite:tamper', { sensitive: true }))
      .rejects.toThrow('EMAIL_SENSITIVE_RECEIPT_INVALID');
  });

  it('leaves ordinary sandbox receipts unchanged and sends sensitive bodies through the provider path normally', async () => {
    const database = new MemoryEmailDatabase();
    const env = sandboxEnvironment(database);
    await sendBetaEmail(env, 'customer@example.test', 'Ordinary notice', invitationUrl, 'ordinary:key');
    expect(database.rows.get('ordinary:key')).toMatchObject({
      body_text: `Ordinary notice\n\n${invitationUrl}\n\nIf you did not request this, you can ignore this email.`,
      body_ciphertext: null,
    });

    const providerFetch = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      new Response(JSON.stringify({ id: 'provider-receipt' }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', providerFetch);
    await expect(sendBetaEmail({ RESEND_API_KEY: 'provider-key', RESEND_FROM: 'noreply@example.test' }, 'customer@example.test', 'Invite', invitationUrl, 'provider:key', { sensitive: true }))
      .resolves.toBe('provider-receipt');
    expect(String(providerFetch.mock.calls[0]?.[1]?.body)).toContain(invitationUrl);
  });
});
