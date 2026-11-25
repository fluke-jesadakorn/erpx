import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  nativeBetaPostDrafts,
  nativeBetaReports,
  nativeBetaPostTask,
  nativeBetaRecoverPostTask,
} from './native-lane-15/client';

/**
 * Collaboration-owned beta-management facts. The four `social_*` reads are
 * now owned end to end by the native lane 15 axum router; this module is a
 * thin transport client with no SQL and no TypeScript business fallback.
 * `db` is retained for the repository call signature only.
 */
export function betaManagementFacts(_db: SqlDatabase) {
  return {
    async postDrafts(userId: string, companyId: string) {
      return nativeBetaPostDrafts(userId, companyId);
    },
    async reports(companyId: string) {
      return nativeBetaReports(companyId);
    },
    async postTask(id: string | undefined) {
      return nativeBetaPostTask(id);
    },
    async recoverPostTask(id: string | undefined) {
      return nativeBetaRecoverPostTask(id);
    },
  };
}
