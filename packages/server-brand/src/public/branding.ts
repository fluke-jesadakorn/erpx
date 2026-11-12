/** Retained Branding reads and pure image normalization. Mutations are owned by
 * the native Branding command/HTTP path; the workspace/document brand, profile
 * list and executive evidence reads are bound to the native Brand owner. */
export { documentBrandModes, resolveWorkspaceBrand, resolveDocumentBrand, recordBranchId, listBrandProfiles, readStorefrontAppBrand, requireBrandManagement, normalizeBrandImage, readBrandProfileEvidence } from "@erpx/server-brand/adapters/branding";
export { createNativeBrandPresentationClient, nativeBrandPresentationConfigured, nativeBrandProfileEvidence, nativeListBrandProfiles, nativeResolveDocumentBrand, nativeResolveWorkspaceBrand } from "../adapters/native-brand-presentation";
export type { NativeBrandDocumentScope, NativeBrandPresentationTransport, NativeBrandProfileEvidence, NativeBrandProfiles } from "../adapters/native-brand-presentation";
export type { DocumentBrandMode, BrandVariant, WorkspaceBrand, StorefrontAppBrand, DocumentBrand } from "@erpx/server-brand/domain/branding";
