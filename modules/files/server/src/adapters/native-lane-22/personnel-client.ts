import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  parseWorkEvidenceConfig,
  selectWorkEvidenceTarget,
  workEvidenceHttpTransport,
  type WorkEvidenceActor,
  type WorkEvidenceTransport,
} from './client';

export type { WorkEvidenceActor } from './client';

/**
 * Strict native transport for the Files-owned personnel-evidence family.
 *
 * The axum owner executes the three statements with sqlx and returns the full
 * projection. This bridge emits no SQL, accepts no SQL from a caller and fails
 * closed when the native service is unavailable or the Holding is unregistered.
 * It shares the lane 22 registered-holding config and target selector.
 */
export type PersonnelEvidenceKind =
  | 'evaluation-files'
  | 'evaluation-link'
  | 'employee-request-attachments';
export type PersonnelEvidenceArgs = Readonly<{
  assessmentId?: string;
  sourceId?: string | null;
  recordId?: string;
}>;

const ENV = 'ERPX_NATIVE_FILES_CONFIG';
const unavailable = (
  code = 'NATIVE_PERSONNEL_EVIDENCE_UNAVAILABLE',
  message = 'The native Files personnel-evidence service is unavailable.',
): never => {
  throw new ApiError(503, code, message);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && !/[\u0000-\u001f/?#\\]/.test(value);

function validateRow(value: unknown, kind: PersonnelEvidenceKind): value is Record<string, unknown> {
  if (!object(value)) return false;
  if (!identifier(value.id, 200)) return false;
  if (kind === 'evaluation-files') return text(value.filename, 8192);
  if (kind === 'evaluation-link') return Object.keys(value).length === 1;
  return (
    text(value.original_name, 8192) &&
    text(value.media_type, 512) &&
    typeof value.bytes === 'number' && Number.isSafeInteger(value.bytes) &&
    text(value.checksum_sha256, 512) &&
    text(value.purpose, 200) &&
    typeof value.revision === 'number' && Number.isSafeInteger(value.revision)
  );
}

/** Create a client with an injectable transport for contract tests. */
export function createPersonnelEvidenceClient(
  config: unknown,
  transport: WorkEvidenceTransport,
) {
  const settings = parseWorkEvidenceConfig(config);
  return {
    async read(
      actor: WorkEvidenceActor,
      _database: SqlDatabase,
      kind: PersonnelEvidenceKind,
      args: PersonnelEvidenceArgs,
    ): Promise<SqlResult<Record<string, unknown>>> {
      const invalid = () =>
        unavailable('NATIVE_PERSONNEL_EVIDENCE_REQUEST_INVALID', 'Use one bounded Files personnel-evidence request.');
      if (
        kind !== 'evaluation-files' &&
        kind !== 'evaluation-link' &&
        kind !== 'employee-request-attachments'
      )
        return invalid();
      if (!identifier(actor.companyId)) return invalid();
      if (kind === 'evaluation-files' || kind === 'evaluation-link') {
        if (!identifier(args.assessmentId)) return invalid();
      }
      if (kind === 'evaluation-link' && !identifier(args.sourceId)) return invalid();
      if (kind === 'employee-request-attachments' && !identifier(args.recordId)) return invalid();
      const target = selectWorkEvidenceTarget(settings, actor);

      let response: { status: number; value: unknown };
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/files/personnel-evidence?kind=${kind}`,
        settings.base,
      );
      const headers: Record<string, string> = {
        host: target.host,
        origin: target.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        'x-erpx-context-company': actor.companyId,
      };
      if (args.assessmentId) headers['x-erpx-context-assessment'] = args.assessmentId;
      if (args.sourceId) headers['x-erpx-context-source'] = args.sourceId;
      if (args.recordId) headers['x-erpx-context-record'] = args.recordId;
      try {
        response = await transport(url, headers, settings.timeoutMs);
      } catch {
        return unavailable();
      }
      const value: unknown = response.value;
      if (response.status !== 200) {
        const error = object(value) && object(value.error) ? value.error : {};
        if (response.status >= 400 && response.status < 500)
          throw new ApiError(
            response.status,
            text(error.code, 100) ? String(error.code) : 'NATIVE_PERSONNEL_EVIDENCE_READ_FAILED',
            text(error.message, 300) ? String(error.message) : 'The native Files personnel-evidence read was rejected.',
          );
        return unavailable();
      }
      if (
        !object(value) ||
        value.kind !== kind ||
        !Array.isArray(value.rows) ||
        !value.rows.every(row => validateRow(row, kind))
      )
        return unavailable('NATIVE_PERSONNEL_EVIDENCE_RESPONSE_INVALID', 'The native Files personnel-evidence reply was invalid.');
      return {
        results: value.rows as Record<string, unknown>[],
        success: true,
        meta: { changes: 0, duration: 0 },
      };
    },
  };
}

/** Env-bound entry point used by the owner adapters. */
export async function readNativePersonnelEvidence(
  actor: WorkEvidenceActor,
  database: SqlDatabase,
  kind: PersonnelEvidenceKind,
  args: PersonnelEvidenceArgs,
): Promise<SqlResult<Record<string, unknown>>> {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_PERSONNEL_EVIDENCE_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_PERSONNEL_EVIDENCE_CONFIGURATION_INVALID');
  }
  return createPersonnelEvidenceClient(value, workEvidenceHttpTransport).read(actor, database, kind, args);
}
