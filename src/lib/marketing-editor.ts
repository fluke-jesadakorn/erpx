import { z } from "zod";

const name = z.string().trim().min(2).max(160);
export const marketingAssetEditorSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("form"), name,
    headline: z.string().trim().min(2).max(160),
    summary: z.string().trim().max(500),
    consentRequired: z.literal(true),
    contentAssetId:z.string().min(1).max(180).optional(),
    contentVersion:z.number().int().positive().optional(),
    mediaPublicationId:z.string().min(1).max(180).optional(),
  }),
  z.object({
    kind: z.enum(["content", "domain", "provider"]), name,
    reference: z.string().trim().min(2).max(160),
    summary: z.string().trim().min(3).max(500),
  }),
]);

export function marketingAssetDraft(raw: unknown) {
  const input = marketingAssetEditorSchema.parse(raw);
  return input.kind === "form"
    ? { kind: input.kind, name: input.name,
        configuration: { consentRequired: true,...(input.contentAssetId?{contentAssetId:input.contentAssetId,contentVersion:input.contentVersion}:{}),...(input.mediaPublicationId?{mediaPublicationId:input.mediaPublicationId}:{}) },
        preview: { headline: input.headline, summary: input.summary } }
    : { kind: input.kind, name: input.name,
        configuration: { reference: input.reference },
        preview: { summary: input.summary } };
}
