/** This additional fixture entry accepts only its exact disposable, unstarted browser cohort. */
export function assertAcmeBrowserSeedTarget(manifest: { databases: Record<string, string> }, state: { run: string; prefix: string; profile: string; ready?: boolean; pid?: number; jobsPid?: number }, run: string | undefined): void {
  const bindings = ["CONTROL_DB", "CONTROL_READ_DB", "H001_DB", "H001_READ_DB", "H002_DB", "H002_READ_DB", "H003_DB", "H003_READ_DB"];
  if (!run || !/^[a-f0-9]{12}$/.test(run) || state.run !== run || state.prefix !== `erpx_browser_${run}` || !["local-seed", "staff-work"].includes(state.profile) || state.ready || state.pid || state.jobsPid || JSON.stringify(Object.keys(manifest.databases).sort()) !== JSON.stringify(bindings.sort())) throw new Error("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  for (const [binding, value] of Object.entries(manifest.databases)) {
    const url = new URL(value);
    if (!["postgres:", "postgresql:"].includes(url.protocol) || !["127.0.0.1", "localhost"].includes(url.hostname) || url.port !== "5432" || url.username !== "erpx_test_admin" || url.pathname !== `/erpx_browser_${run}_${binding.toLowerCase()}`) throw new Error("EXACT_ACME_BROWSER_SEED_DATABASE_REQUIRED");
  }
}
