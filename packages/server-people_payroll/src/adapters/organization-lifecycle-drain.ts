import 'server-only';
import { nativeOrganizationConfigured } from './native-organization-config';

/** Exact lifecycle position-move drain selector.
 *
 * The retained People lifecycle job keeps draining its own cases in every
 * deployment. Scheduled position moves are drained by the TypeScript
 * `drainPositionMoves` only while `ERPX_NATIVE_ORGANIZATION_CONFIG` is unset.
 * Once that configuration is present, the native Organization owner is active
 * and the native `position-move-worker --development-config` (or the
 * production worker supervisor) owns the drain, so the retained drain must be
 * skipped instead of double-applying the same moves. */
export function lifecyclePositionMoveDrain<Database>(
  database: Database,
  retained: (database: Database, now: number) => Promise<number>,
  environment: Readonly<Record<string, string | undefined>> = process.env,
): (now: number) => Promise<number> {
  if (nativeOrganizationConfigured(environment)) return async () => 0;
  return (now) => retained(database, now);
}
