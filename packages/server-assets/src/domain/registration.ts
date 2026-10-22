export type AssetRegistrationInput = {
  branchId: string; departmentId?: string | null; code: string; name: string; category: string; currency: string;
  costMinor?: number; cost?: string; acquiredDate: string; warrantyEnd?: string | null;
  financeAssetId?: string; financeAssetVersion?: number;
};
export function acquisitionCostMinor(units: bigint): number | null {
  return units > BigInt(Number.MAX_SAFE_INTEGER) ? null : Number(units);
}
export function assetRegistrationScope(scope: { branchId: string | null; departmentId: string | null }, input: AssetRegistrationInput) {
  return { branch: scope.branchId ?? input.branchId, department: scope.departmentId ?? input.departmentId ?? null };
}
