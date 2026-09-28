export type FrontHostApp = "pms" | "pos";

function configuredOrigin(value: string | undefined): URL | null {
  if (!value) return null;
  const url = new URL(value);
  if (url.pathname !== "/" || url.search || url.hash || url.username || url.password || !["https:", "http:"].includes(url.protocol))
    throw new Error("INVALID_FRONT_ORIGIN");
  if (process.env.NODE_ENV === "production" && url.protocol !== "https:") throw new Error("INVALID_FRONT_ORIGIN");
  return url;
}

export function frontOrigin(app: FrontHostApp): URL | null {
  return configuredOrigin(process.env[app === "pms" ? "ERPX_FRONT_PMS_ORIGIN" : "ERPX_FRONT_POS_ORIGIN"]);
}

export function frontAppForHost(host: string | null): FrontHostApp | null {
  if (!host) return null;
  for (const app of ["pms", "pos"] as const) if (frontOrigin(app)?.host === host) return app;
  return null;
}

export function frontVisiblePath(holding: string, company: string, locationAndView: string[] = []) {
  return "/" + [holding, company, ...locationAndView].map(encodeURIComponent).join("/");
}

export function frontVisibleLocation(holding: string, company: string, location: string) {
  return frontVisiblePath(holding, company, location.split("/").filter(Boolean)) + "/";
}

/** A same-origin destination for generic Front push alerts; detail remains in the ERPX Inbox. */
export function frontPushLandingPath(
  scope: { kind: "holding" | "branch" | "department"; holdingSlug: string; branchSlug?: string; departmentSlug?: string },
  companySlug: string,
) {
  const location = scope.kind === "holding" ? ["holding"]
    : scope.kind === "branch" ? ["branches", scope.branchSlug!]
    : scope.branchSlug ? ["branches", scope.branchSlug, "departments", scope.departmentSlug!]
      : ["departments", scope.departmentSlug!];
  return frontVisiblePath(scope.holdingSlug, companySlug, [...location, "notifications"]);
}

export function safeFrontReturnPath(value: string | undefined) {
  if (!value || !value.startsWith("/") || value.startsWith("//") || value.includes("\\") || /[\r\n]/.test(value)) return "/";
  const url = new URL(value, "https://front.invalid");
  if (url.origin !== "https://front.invalid") return "/";
  if (["/", "/login", "/forgot-password", "/verify-email", "/reset-password"].includes(url.pathname)) return url.pathname + url.search;
  const parts = url.pathname.split("/").filter(Boolean);
  if (parts.length < 2 || ["holdings", "platform", "api", "front", "front-access"].includes(parts[0]) || parts.some((part) => !/^[A-Za-z0-9_-]+$/.test(part))) return "/";
  return url.pathname + url.search;
}
