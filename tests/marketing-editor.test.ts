import { describe, expect, it } from "vitest";
import { marketingAssetDraft, marketingAssetEditorSchema } from "../src/lib/marketing-editor";

describe("novice marketing asset editor", () => {
  it("creates a consented public form from readable fields", () => {
    expect(marketingAssetDraft({ kind: "form", name: "Service inquiry", headline: "Tell us what you need",
      summary: "We will contact you.", consentRequired: true })).toEqual({
      kind: "form", name: "Service inquiry", configuration: { consentRequired: true },
      preview: { headline: "Tell us what you need", summary: "We will contact you." },
    });
    expect(marketingAssetEditorSchema.safeParse({ kind: "form", name: "Service inquiry",
      headline: "Tell us what you need", summary: "", consentRequired: false }).success).toBe(false);
  });

  it("retains a review reference and human-readable preview for other assets", () => {
    for (const kind of ["content", "domain", "provider"] as const)
      expect(marketingAssetDraft({ kind, name: "Launch review", reference: "POLICY-2026-1",
        summary: "Ready for independent review" })).toEqual({
        kind, name: "Launch review", configuration: { reference: "POLICY-2026-1" },
        preview: { summary: "Ready for independent review" },
      });
    expect(marketingAssetEditorSchema.safeParse({ kind: "domain", name: "Launch review",
      reference: "", summary: "Ready for independent review" }).success).toBe(false);
  });
});
