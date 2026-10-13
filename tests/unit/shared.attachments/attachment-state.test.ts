import { describe, expect, it } from "vitest";
import {
  attachmentStateLabel,
  canMutateAttachmentDraft,
  canReorderAttachmentPublications,
  canRevokeAttachmentPublication,
} from "../../../src/lib/files/attachment-state";

describe("attachment display state", () => {
  it.each([
    ["withdrawn", "available", "Withdrawn"],
    ["withdrawn", "scanning", "Withdrawn"],
    ["expired", "available", "Upload expired"],
    ["expired", "scan_error", "Upload expired"],
  ] as const)(
    "prioritizes a %s link over its %s source",
    (linkState, sourceState, expected) => {
      expect(attachmentStateLabel({ linkState, sourceState })).toBe(expected);
    },
  );

  it("keeps active available and immutable bound labels distinct", () => {
    expect(
      attachmentStateLabel({ linkState: "draft", sourceState: "available" }),
    ).toBe("Ready to attach");
    expect(
      attachmentStateLabel({ linkState: "bound", sourceState: "available" }),
    ).toBe("Attached and locked");
  });

  it("shows publication ordering only to an authorized creator or reviewer", () => {
    const base = {
      sourceComponentKind: "record",
      itemState: "approved",
      activeCount: 2,
    };
    expect(
      canReorderAttachmentPublications({
        ...base,
        canCreate: false,
        canReview: false,
      }),
    ).toBe(false);
    expect(
      canReorderAttachmentPublications({
        ...base,
        canCreate: true,
        canReview: false,
      }),
    ).toBe(true);
    expect(
      canReorderAttachmentPublications({
        ...base,
        canCreate: false,
        canReview: true,
      }),
    ).toBe(true);
    expect(
      canReorderAttachmentPublications({
        ...base,
        canCreate: true,
        canReview: true,
        itemState: "revoked",
      }),
    ).toBe(false);
  });

  it("blocks every draft mutation during read-only, disabled, frozen, or bound states", () => {
    expect(
      canMutateAttachmentDraft({
        disabled: false,
        readOnly: false,
        frozen: false,
        locked: false,
      }),
    ).toBe(true);
    for (const blocked of [
      { disabled: true, readOnly: false, frozen: false, locked: false },
      { disabled: false, readOnly: true, frozen: false, locked: false },
      { disabled: false, readOnly: false, frozen: true, locked: false },
      { disabled: false, readOnly: false, frozen: false, locked: true },
    ]) {
      expect(canMutateAttachmentDraft(blocked)).toBe(false);
    }
  });

  it("shows publication revocation only to a reviewer while the item remains approved", () => {
    expect(
      canRevokeAttachmentPublication({
        canReview: true,
        itemState: "approved",
      }),
    ).toBe(true);
    expect(
      canRevokeAttachmentPublication({
        canReview: false,
        itemState: "approved",
      }),
    ).toBe(false);
    expect(
      canRevokeAttachmentPublication({ canReview: true, itemState: "draft" }),
    ).toBe(false);
    expect(
      canRevokeAttachmentPublication({ canReview: true, itemState: "revoked" }),
    ).toBe(false);
  });
});
