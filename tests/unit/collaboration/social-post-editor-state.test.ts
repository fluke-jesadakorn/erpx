import { describe, expect, it } from "vitest";
import { postContentBody, postContentMentions, postEditorBlocksFromStored, postWireContent, postWireRichContent } from "../../../src/lib/collaboration/social-post-editor-state";
import { validateRichContent } from "../../../src/lib/shared/home-rich-content";

describe("deferred post editor state", () => {
  it("preserves Thai text, mention offsets and image order while the editor is unloaded", () => {
    const content = [{ type: "text" as const, text: "ถึง Mali" },
      { type: "image" as const, slotId: "image-slot", attachmentId: "attachment-id", name: "ภาพ.png", alt: "ข้อมูล", width: 75 },
      { type: "text" as const, text: "ขอบคุณ" }];
    const mentions = [{ kind: "person" as const, targetId: "mali", label: "Mali", start: 4, end: 8 }];
    const blocks = postEditorBlocksFromStored(content, null, "ถึง Mali\nขอบคุณ", mentions);
    expect(postContentBody(blocks)).toBe("ถึง Mali\nขอบคุณ");
    expect(postContentMentions(blocks)).toEqual(mentions);
    expect(postWireContent(blocks)).toEqual(content);
    const restored = validateRichContent(postWireRichContent(blocks), 30000);
    expect(restored.body).toBe("ถึง Mali\nขอบคุณ");
    expect(restored.mentions).toEqual(mentions);
    expect(restored.content.blocks[1]).toMatchObject({ type: "image", attachmentId: "attachment-id", width: 75 });
  });
  it("restores an image-only legacy draft without losing its attachment", () => {
    const blocks = postEditorBlocksFromStored(null, null, "", [], [{ id: "legacy", original_name: "ภาพ.png" }]);
    expect(postContentBody(blocks)).toBe("");
    expect(postWireContent(blocks)).toContainEqual({ type: "image", slotId: "legacy-legacy", attachmentId: "legacy", name: "ภาพ.png", alt: "ภาพ.png", width: undefined });
  });
});
