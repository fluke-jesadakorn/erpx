export const browserTransactionLifetime = 5 * 60_000;
export const singleUseGrantLifetime = 60_000;
export const sessionLifetime = 12 * 60 * 60_000;

/** Other account auth endpoints must verify an audience-bound central session. */
export function anonymousAccountAuthPath(path: string) {
  return /^\/api\/auth\/(?:sign-in\/(?:email|social)|directory\/sign-in|callback\/directory-[a-f0-9-]{36}|forget-password|request-password-reset|reset-password(?:\/[A-Za-z0-9_-]+)?|send-verification-email|verify-email)\/?$/i.test(path);
}

export { safeProductReturnPath } from '@erpx/contracts/account-navigation';

export type SsoSessionBinding = {
  sessionId: string;
  audience: string;
  environment: string;
  parentSessionId: string | null;
  authenticatedAt: number;
  absoluteExpiresAt: number;
};
export function bindingIsCurrent(binding: SsoSessionBinding, audience: string, environment: string, now: number) {
  return binding.audience === audience && binding.environment === environment &&
    binding.authenticatedAt <= now && binding.absoluteExpiresAt > now &&
    binding.absoluteExpiresAt <= binding.authenticatedAt + sessionLifetime;
}
