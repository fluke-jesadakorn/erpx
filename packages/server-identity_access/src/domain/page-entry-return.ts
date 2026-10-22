import { accountWorkspaceTarget } from '@erpx/server-identity_access/domain/account-workspace';
import { safeProductReturnPath } from '@erpx/server-identity_access/domain/product-sso';

export type EntryPageSearchParams = Record<string, string | string[] | undefined>;
type AccountSelfPath = '/my-access' | '/security' | '/my-profile';

function pageTarget(path: string, query: EntryPageSearchParams) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query))
    for (const item of Array.isArray(value) ? value : [value])
      if (item !== undefined) search.append(key, item);
  return path + (search.size ? '?' + search : '');
}

/** Page/query presentation only. The closed Account bridge and normal session
 * policy still authorize every destination; query fields never select a role. */
export function accountSelfPageReturn(path: AccountSelfPath, query: EntryPageSearchParams = {}) {
  if (accountWorkspaceTarget(path) !== path) throw new Error('ACCOUNT_SELF_PATH_REQUIRED');
  return accountWorkspaceTarget(pageTarget(path, query)) ?? path;
}

export function accountSelfLoginHref(path: AccountSelfPath, query: EntryPageSearchParams = {}) {
  const target = accountSelfPageReturn(path, query);
  // Preserve the existing no-query redirects, including their legacy encoding.
  const fallback = path === '/my-profile' ? encodeURIComponent(path) : path;
  return '/login?next=' + (target === path ? fallback : encodeURIComponent(target));
}

/** A normal product-root SSO return, sanitized by the existing native store
 * policy. It preserves query data without granting Company/role authority. */
export function productEntrySsoHref(query: EntryPageSearchParams = {}) {
  const target = safeProductReturnPath(pageTarget('/', query));
  return target === '/' ? '/sso/start' : '/sso/start?' + new URLSearchParams({ next: target });
}
