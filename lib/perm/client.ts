// perm/client.ts — browser-side hooks.
//
// Server-rendered sessions already pass the permission list to the client
// via props. This hook reads from that prop, no HTTP roundtrip needed.
//
// Usage:
//   const can = useHasPerm(sessionPerms, 'finance:expense:approve::allow');

import { useMemo, useEffect, useState } from 'react';
import { matchPerm, parseDeptFromPerms, parseLevelFromRoles, parseRoleId } from './grammar';

export function useHasPerm(sessionPerms: string[] | null | undefined, requested: string): boolean {
  return useMemo(() => {
    if (!sessionPerms || sessionPerms.length === 0) return false;
    return matchPerm(sessionPerms, requested);
  }, [sessionPerms, requested]);
}

export function useActorDept(sessionPerms: string[] | null | undefined): string | null {
  return useMemo(() => {
    if (!sessionPerms) return null;
    return parseDeptFromPerms(sessionPerms);
  }, [sessionPerms]);
}

export function useActorLevel(roleIds: string[] | null | undefined): number {
  return useMemo(() => {
    if (!roleIds || roleIds.length === 0) return 10;
    return parseLevelFromRoles(roleIds);
  }, [roleIds]);
}

export function useActorRoleName(roleId: string | null | undefined): string | null {
  return useMemo(() => {
    if (!roleId) return null;
    return parseRoleId(roleId)?.name ?? null;
  }, [roleId]);
}

// Batch hook: resolve many perms at once against the same session.
export function useHasPerms(
  sessionPerms: string[] | null | undefined,
  requested: string[],
): Record<string, boolean> {
  return useMemo(() => {
    const out: Record<string, boolean> = {};
    if (!sessionPerms || sessionPerms.length === 0) {
      for (const r of requested) out[r] = false;
      return out;
    }
    for (const r of requested) out[r] = matchPerm(sessionPerms, r);
    return out;
  }, [sessionPerms, requested]);
}

// Async loader helper for components that fetch the session in effect.
export function useResolvedSession(loader: () => Promise<string[]>): {
  perms: string[] | null;
  loading: boolean;
} {
  const [perms, setPerms] = useState<string[] | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let mounted = true;
    loader()
      .then((p) => {
        if (mounted) {
          setPerms(p);
          setLoading(false);
        }
      })
      .catch(() => {
        if (mounted) {
          setPerms(null);
          setLoading(false);
        }
      });
    return () => {
      mounted = false;
    };
  }, [loader]);
  return { perms, loading };
}
