import 'server-only';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const exec = promisify(execFile);

export class NativeLegalWorkerError extends Error {
  readonly code: string;
  constructor(code: string) {
    super('The native Legal document worker could not complete its cycle.');
    this.name = 'NativeLegalWorkerError';
    this.code = code;
  }
}

/**
 * Runs one bounded native Legal OCR/PDF cycle for one exact Holding. The native
 * worker owns the queue claim, provider probes, original read and persisted
 * inspection; this transport only supplies the reviewed private configuration.
 * A missing configuration is a hard failure, never a TypeScript fallback.
 */
export async function runNativeLegalWorker(holdingId: string): Promise<void> {
  const configuration = process.env.ERPX_LEGAL_RUNTIME_WORKER_CONFIG;
  const binary = process.env.ERPX_NATIVE_SERVER_BIN;
  if (!configuration || !binary) {
    throw new NativeLegalWorkerError('NATIVE_LEGAL_WORKER_CONFIGURATION_REQUIRED');
  }
  let stdout: string;
  try {
    ({ stdout } = await exec(
      binary,
      [
        'legal-worker',
        '--once',
        '--runtime-config',
        configuration,
        '--holding',
        holdingId,
      ],
      {
        timeout: 120_000,
        maxBuffer: 2 * 1024 * 1024,
        env: process.env,
        windowsHide: true,
      },
    ));
  } catch {
    throw new NativeLegalWorkerError('NATIVE_LEGAL_WORKER_FAILED');
  }
  let outcome: unknown;
  try {
    outcome = JSON.parse(stdout);
  } catch {
    throw new NativeLegalWorkerError('NATIVE_LEGAL_WORKER_RESPONSE_INVALID');
  }
  const holdings =
    outcome && typeof outcome === 'object' && !Array.isArray(outcome)
      ? (outcome as { holdings?: unknown }).holdings
      : undefined;
  if (
    !Array.isArray(holdings) ||
    holdings.length !== 1 ||
    !holdings[0] ||
    typeof holdings[0] !== 'object' ||
    (holdings[0] as { holdingId?: unknown }).holdingId !== holdingId
  ) {
    throw new NativeLegalWorkerError('NATIVE_LEGAL_WORKER_TARGET_MISMATCH');
  }
}
