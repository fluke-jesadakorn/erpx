/** Compatibility exports; Brand owns the application and persistence adapters. */
export { brandIdentitySchema, publishedBrandIdentity, brandIdentityWorkspace, saveBrandIdentityDraft, publishBrandIdentity, rollbackBrandIdentity, changeBrandCategoryLock, cancelBrandDraft } from "./modules/brand";
export type { BrandIdentity } from "./modules/brand";
