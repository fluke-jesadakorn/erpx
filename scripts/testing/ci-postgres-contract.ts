/** A telemetry boundary delegates only the existing explicit CI commands, byte-for-byte. */
export function ciVerificationArgv(arguments_: readonly string[]): string[] {
  // Bun1.4.3 consumes the script separator; Node retains it. Normalize only
  // that observed boundary, never a second separator or any child argument.
  const normalized=arguments_[0]==='--'?arguments_.slice(1):arguments_;
  const [command, run, task,...flags] = normalized;
  if (command !== 'bun' || run !== 'run' || !['test:ci', 'release:acceptance'].includes(task))
    throw new Error('EXPLICIT_CI_VERIFICATION_COMMAND_REQUIRED');
  const required=task==='test:ci'?['--base','--head']:['--candidate'];
  const names=flags.map(flag=>flag.slice(0,flag.indexOf('=')));
  if(flags.length!==required.length||required.some(name=>names.filter(value=>value===name).length!==1)
    ||flags.some(flag=>!flag.includes('=')||flag.endsWith('=')))
    throw new Error('EXPLICIT_CI_VERIFICATION_COMMAND_REQUIRED');
  return ['run', ...normalized.slice(2)];
}

export function ciPostgresInvocation(arguments_: readonly string[]):
  {kind:'startup'}|{kind:'verification';argv:string[]} {
  if(arguments_.length===1&&arguments_[0]==='--probe-only')return {kind:'startup'};
  return {kind:'verification',argv:ciVerificationArgv(arguments_)};
}

/** Preserve hosted checks while admitting an explicitly selected isolated Linux container. */
export function isolatedVerificationRuntime(ci: string | undefined, github: string | undefined,
  local: string | undefined, platform: string, container: boolean): boolean {
  return ci === 'true' && (github === 'true' ||
    (github !== 'true' && local === 'owned-linux-container' && platform === 'linux' && container));
}
