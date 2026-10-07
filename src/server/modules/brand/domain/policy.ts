import type { BrandCategoryLock, BrandIdentity } from "./identity";

export class BrandRuleError extends Error {
  constructor(readonly status: number, readonly code: string, message: string) { super(message); }
}

export function assertBrandCategoryLocks(locks: BrandCategoryLock[], identity: BrandIdentity): void {
  for (const lock of locks) {
    if (lock.locked && JSON.stringify(identity[lock.category]) !== JSON.stringify(JSON.parse(lock.approved_snapshot_json))) {
      throw new BrandRuleError(409, "BRAND_CATEGORY_LOCKED", `The approved ${lock.category} category is locked. An owner must review and unlock it before a change.`);
    }
  }
}

export function assertBrandIndependentPublisher(isSamePerson: (userId: string) => boolean, createdBy: string, updatedBy: string): void {
  if (isSamePerson(createdBy) || isSamePerson(updatedBy)) throw new BrandRuleError(403, "BRAND_INDEPENDENT_PUBLICATION", "Another authorized person must review and publish this draft.");
}
