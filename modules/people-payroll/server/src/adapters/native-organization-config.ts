/** Exact opt-in selector shared by the People Payroll Organization consumers.
 *
 * Presence, not validity, is the signal: every process that serves the native
 * Organization owner carries `ERPX_NATIVE_ORGANIZATION_CONFIG`, and the same
 * deployment runs the native position-move worker. A malformed configured
 * value fails closed in the access read; it must never re-enable the retained
 * TypeScript owner. An unset value keeps the retained readers/drain exactly as
 * before. */
export function nativeOrganizationConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment.ERPX_NATIVE_ORGANIZATION_CONFIG !== undefined;
}
