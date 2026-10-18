export const browserTransactionLifetime = 5 * 60_000;
export const singleUseGrantLifetime = 60_000;
export const sessionLifetime = 12 * 60 * 60_000;

/** Other account auth endpoints must verify an audience-bound central session. */
export function anonymousAccountAuthPath(path: string) {
  return /^\/api\/auth\/(?:sign-in\/(?:email|social)|directory\/sign-in|callback\/directory-[a-f0-9-]{36}|forget-password|request-password-reset|reset-password(?:\/[A-Za-z0-9_-]+)?|send-verification-email|verify-email)\/?$/i.test(path);
}

export function safeProductReturnPath(value: string | null | undefined) {
  if (!value || value.length > 4096 || !value.startsWith("/") || value.startsWith("//") || /[\\\s\u0000-\u001f\u007f]/u.test(value)) return "/";
  let decoded: string;
  try { decoded = decodeURIComponent(value); } catch { return "/"; }
  if (decoded.startsWith("//") || /[\\\u0000-\u001f\u007f]/u.test(decoded)) return "/";
  const url = new URL(value, "https://product.invalid");
  if (url.origin !== "https://product.invalid" || /^\/(?:sso|api\/auth|login|product-entry)(?:\/|$)/.test(decodeURIComponent(url.pathname))) return "/";
  // Never carry callback credentials on to a clean application URL.
  for (const key of ["code", "state", "token", "session_token", "_rsc"]) url.searchParams.delete(key);
  return url.pathname + url.search;
}

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
