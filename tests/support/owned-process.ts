import { spawn } from 'node:child_process';
import { basename } from 'node:path';

// A trapping group leader stays alive until escalation, reserving the PGID while
// Python and its pg_restore/age descendants are terminated and the leader reaped.
// All caller strings enter "$@" as argv; none are evaluated as shell source.
const supervisor = `stopping=0
trap 'stopping=1' TERM INT
"$@" &
owned=$!
wait "$owned"
result=$?
if [ "$stopping" = 1 ]; then while :; do sleep 1; done; fi
exit "$result"`;

export function executeOwnedProcess(binary: string, args: readonly string[], options: { timeout?: number; maxBuffer?: number } = {}) {
  if (process.platform === 'win32') throw new Error('POSIX_OWNED_PROCESS_REQUIRED');
  const timeout = options.timeout ?? 120_000, maximum = options.maxBuffer ?? 1024 * 1024;
  if (!Number.isSafeInteger(timeout) || timeout < 25 || timeout > 120_000 || !Number.isSafeInteger(maximum) || maximum < 1 || maximum > 1024 * 1024)
    throw new Error('BOUNDED_OWNED_PROCESS_REQUIRED');
  return new Promise<{ stdout: string; stderr: string }>((accept, reject) => {
    const child = spawn('/bin/sh', ['-c', supervisor, 'erpx-owned-process', binary, ...args], { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
    const stdout: Buffer[] = [], stderr: Buffer[] = [];
    let outputBytes = 0, errorBytes = 0, failure: 'OWNED_PROCESS_TIMEOUT' | 'OWNED_PROCESS_OUTPUT_LIMIT' | 'OWNED_PROCESS_INTERRUPTED' | undefined;
    let escalation: ReturnType<typeof setTimeout> | undefined;
    let resumeDefaultSignal: NodeJS.Signals | undefined;
    const signal = (value: NodeJS.Signals) => {
      if (!child.pid) return;
      try { process.kill(-child.pid, value); }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
    };
    const stop = (reason: typeof failure) => {
      if (failure) return;
      failure = reason; signal('SIGTERM');
      escalation = setTimeout(() => signal('SIGKILL'), 1_000);
    };
    const emergencyStop = () => {
      if (child.exitCode === null && child.signalCode === null) signal('SIGKILL');
    };
    const previousInterrupt = process.listenerCount('SIGINT'), previousTerminate = process.listenerCount('SIGTERM');
    const interrupt = (value: 'SIGINT' | 'SIGTERM', previous: number) => {
      failure ??= 'OWNED_PROCESS_INTERRUPTED'; emergencyStop();
      // A fixture helper must not turn a process's ordinary termination into a
      // swallowed signal. Re-raise it only after the owned child has been reaped.
      if (!previous) resumeDefaultSignal = value;
    };
    const onInterrupt = () => interrupt('SIGINT', previousInterrupt), onTerminate = () => interrupt('SIGTERM', previousTerminate);
    process.once('exit', emergencyStop); process.on('SIGINT', onInterrupt); process.on('SIGTERM', onTerminate);
    const deadline = setTimeout(() => stop('OWNED_PROCESS_TIMEOUT'), timeout);
    child.stdout.on('data', (bytes: Buffer) => {
      if (outputBytes + bytes.length > maximum) stop('OWNED_PROCESS_OUTPUT_LIMIT');
      else { outputBytes += bytes.length; stdout.push(bytes); }
    });
    child.stderr.on('data', (bytes: Buffer) => {
      if (errorBytes + bytes.length > maximum) stop('OWNED_PROCESS_OUTPUT_LIMIT');
      else { errorBytes += bytes.length; stderr.push(bytes); }
    });
    child.once('error', reject);
    child.once('close', (code, receivedSignal) => {
      clearTimeout(deadline); if (escalation) clearTimeout(escalation);
      process.off('exit', emergencyStop); process.off('SIGINT', onInterrupt); process.off('SIGTERM', onTerminate);
      if (resumeDefaultSignal) { process.kill(process.pid, resumeDefaultSignal); return; }
      const output = { stdout: Buffer.concat(stdout).toString('utf8'), stderr: Buffer.concat(stderr).toString('utf8') };
      if (!failure && code === 0 && !receivedSignal) { accept(output); return; }
      // Keep raw stderr off test/user error output; retain only existing bounded
      // operator refusal tokens required by these actual recovery assertions.
      const refusal = ['Schema differs', 'OFFSITE_CIPHERTEXT_MISMATCH', 'BACKUP_CHECKSUM_MISMATCH'].find(value => output.stderr.includes(value));
      const error = Object.assign(new Error(`${failure ?? `${basename(binary)} failed`}${refusal ? `: ${refusal}` : ''}`),
        { code: failure ?? code, killed: Boolean(failure), signal: receivedSignal });
      Object.defineProperties(error, { stdout: { value: output.stdout }, stderr: { value: output.stderr } });
      reject(error);
    });
  });
}
