import type { DocumentBrand } from "@erpx/server-brand/domain/branding";
import type { BrandRecordKind } from "@erpx/server-brand/application/branding";

export interface BusinessDocumentBrandPorts {
  prepare(): Promise<void>;
  recordBranch(kind: BrandRecordKind, id: string): Promise<string | null>;
  document(scope: { branchId: string | null; requestedMode?: string | null }): Promise<DocumentBrand>;
}
export function createBusinessDocumentBrandQuery(ports: BusinessDocumentBrandPorts) {
  return async (input: { kind?: BrandRecordKind; recordId?: string; branchId?: string | null; requestedMode?: string | null }) => {
    await ports.prepare();
    const branchId = input.kind && input.recordId ? await ports.recordBranch(input.kind, input.recordId) : input.branchId ?? null;
    return ports.document({ branchId, requestedMode: input.requestedMode });
  };
}
