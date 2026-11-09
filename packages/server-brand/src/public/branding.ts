/** Retained Branding reads and pure image normalization. Mutations are owned by
 * the native Branding command/HTTP path. */
export { documentBrandModes, resolveWorkspaceBrand, resolveDocumentBrand, recordBranchId, listBrandProfiles, readStorefrontAppBrand, requireBrandManagement, normalizeBrandImage, brandProfileEvidenceRow } from "@erpx/server-brand/adapters/branding";
export type { DocumentBrandMode, BrandVariant, WorkspaceBrand, StorefrontAppBrand, DocumentBrand } from "@erpx/server-brand/domain/branding";
