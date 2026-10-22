import {z} from "zod";
export function formRequiresConsent(configuration: unknown): boolean {
  let value = configuration;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return false;
    }
  }
  return (
    typeof value === "object" &&
    value !== null &&
    (value as Record<string, unknown>).consentRequired === true
  );
}

export const assetSchema = z.object({
  kind: z.enum(["form", "content", "domain", "provider"]),
  name: z.string().trim().min(2).max(160),
  configuration: z.record(z.string(), z.unknown()),
  preview: z.record(z.string(), z.unknown()),
});
export type MarketingAssetInput=z.infer<typeof assetSchema>;
