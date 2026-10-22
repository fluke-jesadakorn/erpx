export const manufacturingPostingSetupLanding = "production/manufacturing/settings/posting";
/** Exact included Finance entry only. The native owner independently rechecks
 * Company purchase, release, current assignment and command authority. */
export function manufacturingPostingSetupEntry(destination: string): boolean {
  return destination === manufacturingPostingSetupLanding;
}
