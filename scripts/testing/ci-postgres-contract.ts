/** A telemetry boundary delegates only the existing explicit CI commands, byte-for-byte. */
export function ciVerificationArgv(arguments_: readonly string[]): string[] {
  const [separator, command, run, task] = arguments_;
  if (separator !== '--' || command !== 'bun' || run !== 'run' || !['test:ci', 'release:acceptance'].includes(task))
    throw new Error('EXPLICIT_CI_VERIFICATION_COMMAND_REQUIRED');
  return ['run', ...arguments_.slice(3)];
}

export function ciPostgresInvocation(arguments_: readonly string[]):
  {kind:'startup'}|{kind:'verification';argv:string[]} {
  if(arguments_.length===1&&arguments_[0]==='--probe-only')return {kind:'startup'};
  return {kind:'verification',argv:ciVerificationArgv(arguments_)};
}
