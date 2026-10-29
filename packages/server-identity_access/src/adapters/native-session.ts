import 'server-only';
import { createNativeSessionClient, NativeSessionReadError, type NativeSessionClientConfiguration, type NativeSessionRead } from '@erpx/native-sdk/session';
import type { IdentitySession } from '@erpx/server-identity_access/domain/authorization';

function configuration(raw: string): NativeSessionClientConfiguration {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID');
  const record = value as Record<string, unknown>;
  if (typeof record.privateBaseUrl !== 'string' || typeof record.accountOrigin !== 'string' || typeof record.timeoutMs !== 'number' ||
      !Array.isArray(record.holdingOrigins) || !record.holdingOrigins.every((origin: unknown) => typeof origin === 'string')) {
    throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID');
  }
  return record as NativeSessionClientConfiguration;
}

type NativeIdentitySession = IdentitySession & {
  session: { userId: string; createdAt: Date; updatedAt: Date };
  user: { emailVerified: boolean; image: string | null };
};

/** The native endpoint supplies only session identity. Grants and selected work
 * remain owned by the normal database-backed Identity application. */
export function createIdentitySessionAuthenticator<S extends IdentitySession>(
  legacy: (origin: string | undefined, headers: Headers) => Promise<S | null>,
  client: (configuration: NativeSessionClientConfiguration) => NativeSessionRead = createNativeSessionClient,
) {
  return async (origin: string | undefined, headers: Headers): Promise<S | NativeIdentitySession | null> => {
    const raw = process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
    if (raw === undefined) return legacy(origin, headers);
    const result = await client(configuration(raw))(headers);
    if (result === null) return null;
    return {
      session: { id: result.session.id, userId: result.session.userId, expiresAt: new Date(result.session.expiresAt), createdAt: new Date(result.session.createdAt), updatedAt: new Date(result.session.updatedAt) },
      user: { id: result.user.id, name: result.user.name, email: result.user.email, emailVerified: result.user.emailVerified, image: result.user.image },
    };
  };
}
