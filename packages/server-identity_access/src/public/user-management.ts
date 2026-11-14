import 'server-only';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';

const exec = promisify(execFile);

/**
 * Runs one bounded native user-management reconciliation cycle for every
 * Company that owns a policy attachment in this Holding database. The native
 * worker owns the advisory lock, eligibility invalidation, candidate proposal
 * and audit; this transport only supplies the reviewed manifest path.
 */
export async function managementWork(db: PostgresDatabase): Promise<void> {
  const binary = process.env.ERPX_NATIVE_SERVER_BIN;
  const manifest = process.env.ERPX_DATABASE_MANIFEST;
  if (!binary || !manifest)
    throw Object.assign(
      new Error('The native user-management worker is not configured.'),
      { code: 'NATIVE_USER_MANAGEMENT_WORKER_REQUIRED' },
    );
  const companies = await db
    .prepare(
      'SELECT DISTINCT company_id FROM um_attachments ORDER BY company_id',
    )
    .all<{ company_id: string }>();
  for (const row of companies.results ?? []) {
    if (!row.company_id) continue;
    let stdout: string;
    try {
      ({ stdout } = await exec(
        binary,
        [
          'user-management-worker',
          '--once',
          '--database-manifest',
          manifest,
          '--company',
          row.company_id,
        ],
        { timeout: 120_000, maxBuffer: 2 * 1024 * 1024, env: process.env, windowsHide: true },
      ));
    } catch {
      throw Object.assign(
        new Error('The native user-management worker did not complete.'),
        { code: 'NATIVE_USER_MANAGEMENT_WORKER_FAILED' },
      );
    }
    try {
      JSON.parse(stdout);
    } catch {
      throw Object.assign(
        new Error('The native user-management worker response is invalid.'),
        { code: 'NATIVE_USER_MANAGEMENT_WORKER_RESPONSE_INVALID' },
      );
    }
  }
}

/** Native management snapshot for the current Holding request. */
export async function readManagement(
  _db: unknown,
  actor: { holdingId?: string | null },
): Promise<unknown> {
  if (!actor.holdingId) {
    throw Object.assign(
      new Error('The native user-management service requires a Holding context.'),
      { code: 'NATIVE_USER_MANAGEMENT_REQUIRED', status: 503 },
    );
  }
  const { headers } = await import('next/headers');
  const { nativeUserManagementReadForHolding } = await import('../adapters/native-user-management');
  return nativeUserManagementReadForHolding(await headers(), actor.holdingId);
}
