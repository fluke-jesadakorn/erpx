import { pmsCommandPurchases } from "@erpx/server-pms/public/rules/package-commands";

export function pmsPackageCommandAllowed(purchases: Record<string, { newWork: boolean; settleExisting: boolean; history: boolean }> | undefined, operation: string, input: unknown) {
  return !purchases || pmsCommandPurchases(operation, input).every(scope => {
    const access = purchases[scope.capability];
    return access && (scope.purpose === "new-work" ? access.newWork : scope.purpose === "settle-existing" ? access.settleExisting : access.history);
  });
}
