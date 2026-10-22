import "server-only";

/** A single release/rollback switch; independent of financial command authority. */
export function financeWorkspaceV2Enabled() {
  return process.env.ERPX_FINANCE_WORKSPACE_V2 === "1";
}
