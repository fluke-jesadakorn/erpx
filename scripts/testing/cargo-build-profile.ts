/** Build artifacts only: Cargo debug information is independent of assertions,
 * overflow checks and optimization. Local invocations retain their prior env. */
export function qualifiedCargoBuildProfile(environment: Readonly<Record<string, string | undefined>>): Record<string, string> {
  const marker = environment.ERPX_CI_CARGO_PROFILE;
  if (marker === undefined) return {};
  if (marker !== 'compact-v1') throw Error('CI_CARGO_PROFILE_UNKNOWN');
  const profile = { ERPX_CI_CARGO_PROFILE: marker, CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_PROFILE_TEST_DEBUG: '0' };
  for (const [name, value] of Object.entries(profile)) {
    if (environment[name] !== value) throw Error(`CI_CARGO_PROFILE_MISMATCH:${name}`);
  }
  return profile;
}
