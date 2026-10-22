import { execFileSync } from 'node:child_process';
/** Inspect every interface, including IPv6-only listeners, before binding. */
export function hasPortListener(port: number) {
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('INVALID_LISTENER_PORT');
  try {
    return Boolean(execFileSync('lsof', ['-nP', `-tiTCP:${port}`, '-sTCP:LISTEN'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim());
  } catch (error) {
    if ((error as { status?: number }).status === 1) return false;
    throw new Error('PORT_LISTENER_INSPECTION_FAILED:' + port, { cause: error });
  }
}
