function origin(value: string | undefined): URL | null {
  if (!value) return null;
  const url = new URL(value);
  if (url.pathname !== "/" || url.search || url.hash || url.username || url.password ||
      (process.env.NODE_ENV === "production" && url.protocol !== "https:"))
    throw new Error("INVALID_ERPX_ORIGIN");
  return url;
}

export function consoleOrigin() {
  return origin(process.env.ERPX_PUBLIC_ORIGIN);
}

export function marketingOrigin() {
  return origin(process.env.ERPX_MARKETING_ORIGIN);
}

export function isMarketingHost(host: string | null) {
  return Boolean(host && marketingOrigin()?.host === host);
}

export function marketingIsIndexable(host: string | null) {
  return isMarketingHost(host) && host === "erpx.jesadakorn.com";
}

export function marketingConsoleUrl(pathname = "/", search = "") {
  const base = consoleOrigin();
  if (!base) throw new Error("CONSOLE_ORIGIN_REQUIRED");
  base.pathname = pathname;
  base.search = search;
  return base;
}
