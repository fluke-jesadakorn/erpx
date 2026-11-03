/** Navigation input only; native SSO independently validates every return. */
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

