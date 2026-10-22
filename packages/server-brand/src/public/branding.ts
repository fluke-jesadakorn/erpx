/** Public native entry; application decisions and platform adapters are separate. */
export { documentBrandModes, resolveWorkspaceBrand, resolveDocumentBrand, recordBranchId, listBrandProfiles, readStorefrontAppBrand, requireBrandManagement, normalizeBrandImage, completeBrandUpload, completeStorefrontBrandUpload, updateStorefrontAppBrand, updateBrandProfile, brandProfileEvidenceRow } from "@erpx/server-brand/adapters/branding";
export type { DocumentBrandMode, BrandVariant, WorkspaceBrand, StorefrontAppBrand, DocumentBrand } from "@erpx/server-brand/domain/branding";
