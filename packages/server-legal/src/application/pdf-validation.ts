export type LegalTrustValidationMaterial = { id: string; kind: string; content: string; checksum: string };
export function createSignedPdfValidation(ports: { materials(): Promise<LegalTrustValidationMaterial[]>; prepare(materials: LegalTrustValidationMaterial[]): Promise<void>; validate(): Promise<Record<string, unknown>> }) {
  return async () => {
    const materials = await ports.materials(); await ports.prepare(materials);
    const trustMaterials = () => materials.map(({ id, kind, checksum }) => ({ id, kind, checksum }));
    try {
      const result = await ports.validate(); if (!["valid", "invalid", "indeterminate"].includes(result.status as string)) throw new Error("INVALID_VALIDATOR_RESPONSE");
      return { ...result, trustMaterials: trustMaterials() } as Record<string, unknown>;
    } catch { return { status: "indeterminate", reasons: ["VALIDATOR_UNAVAILABLE_OR_TIMED_OUT"], signatures: [], trustMaterials: trustMaterials() }; }
  };
}
