export type DirectProductEntryChoice = {
  productId: string;
  href: string;
  scope: { holdingSlug: string };
};
export type DirectProductEntryHolding = { holdingSlug: string; origin: string };

/** Shared hosts own a chooser, feed or administration surface rather than one
 * product workspace. A bare product root may follow its only current choice. */
const sharedEntryHosts = new Set(["apps", "admin", "account", "booking", "portal", "legal-portal", "forms"]);
const holdingSlugPattern = /^[a-z0-9][a-z0-9_-]*$/;
const unsafeHrefPattern = /[\s\\\u0000-\u001f\u007f]/u;

/**
 * A product root opens its sole authorized workspace directly. Every second
 * choice, shared host, cross-Holding choice or malformed owner-issued href
 * keeps the explicit chooser. This follows a fresh server authorization result
 * and never derives Company, role, package or scope authority from the href
 * itself.
 */
export function directProductEntryHref(host: string, choices: readonly DirectProductEntryChoice[], holding?: DirectProductEntryHolding | null): string | null {
  if (!host || sharedEntryHosts.has(host)) return null;
  if (choices.length !== 1) return null;
  const [choice] = choices;
  if (choice.productId !== host) return null;
  if (!choice.scope || !holdingSlugPattern.test(choice.scope.holdingSlug)) return null;
  let expectedOrigin: string | null = null;
  if (holding) {
    if (!holdingSlugPattern.test(holding.holdingSlug) || choice.scope.holdingSlug !== holding.holdingSlug) return null;
    try {
      const origin = new URL(holding.origin);
      if (origin.protocol !== "https:" || origin.pathname !== "/" || origin.search || origin.hash || origin.username || origin.password) return null;
      expectedOrigin = origin.origin;
    } catch {
      return null;
    }
  }
  const href = choice.href;
  if (typeof href !== "string" || href.length === 0 || href.length > 4096 || href.startsWith("//") || unsafeHrefPattern.test(href)) return null;
  let target: URL;
  try {
    target = new URL(href, "https://product-entry.invalid");
  } catch {
    return null;
  }
  if (target.username || target.password) return null;
  if (href.startsWith("/")) {
    if (target.origin !== "https://product-entry.invalid" || target.pathname === "/") return null;
  } else {
    if (target.protocol !== "https:") return null;
    if (expectedOrigin && target.origin !== expectedOrigin) return null;
  }
  return href;
}
