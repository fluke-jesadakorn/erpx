import type { JSONContent } from "@tiptap/core";
import { plainRichContent, type RichContent, type RichNode } from "@/lib/shared/home-rich-content";
import type { Mention } from "@/server/modules/collaboration/public/rules/social-text";
import type { PostContent } from "@/server/modules/collaboration/public/social-home";

export type PostTextBlock = {
  type: "text";
  id: string;
  text: string;
  mentions: Mention[];
  richDoc?: JSONContent;
};
export type PostImageBlock = {
  type: "image";
  slotId: string;
  attachmentId: string | null;
  name: string;
  alt: string;
  width?: number;
  file?: File;
  uploadUrl?: string;
  previewUrl?: string;
};
export type PostEditorBlock = PostTextBlock | PostImageBlock;
export type PostWireBlock =
  | { type: "text"; text: string }
  | {
      type: "image";
      slotId: string;
      attachmentId: string | null;
      name: string;
      alt: string;
      width?: number;
    };

export const newPostTextBlock = (text = "", mentions: Mention[] = []): PostTextBlock => ({
  type: "text",
  id: crypto.randomUUID(),
  text,
  mentions,
});

export const postContentBody = (blocks: PostEditorBlock[]) => {
  const texts = blocks.filter((block): block is PostTextBlock => block.type === "text");
  return texts.some((block) => block.text.trim())
    ? texts.map((block) => block.text).join("\n")
    : "";
};

export function postContentMentions(blocks: PostEditorBlock[]) {
  const mentions: Mention[] = [];
  let offset = 0;
  for (const block of blocks) {
    if (block.type !== "text") continue;
    for (const mention of block.mentions)
      mentions.push({ ...mention, start: mention.start + offset, end: mention.end + offset });
    offset += block.text.length + 1;
  }
  return mentions;
}

export const postWireContent = (blocks: PostEditorBlock[]): PostWireBlock[] =>
  blocks.map((block) =>
    block.type === "text"
      ? { type: "text", text: block.text }
      : {
          type: "image",
          slotId: block.slotId,
          attachmentId: block.attachmentId,
          name: block.name,
          alt: block.alt,
          width: block.width,
        },
  );

export const postWireRichContent = (blocks: PostEditorBlock[]): RichContent => ({
  version: 1,
  blocks: blocks.map((block) =>
    block.type === "text"
      ? {
          type: "text" as const,
          doc: (block.richDoc ??
            (plainRichContent(block.text, block.mentions).blocks[0] as { doc: RichNode })
              .doc) as RichNode,
        }
      : {
          type: "image" as const,
          slotId: block.slotId,
          attachmentId: block.attachmentId,
          name: block.name,
          alt: block.alt,
          width: block.width,
        },
  ),
});

export function postEditorBlocksFromStored(
  content: PostContent | null,
  richContent: RichContent | null,
  body: string,
  mentions: Mention[],
  legacyFiles: Array<{ id: string; original_name: string }> = [],
): PostEditorBlock[] {
  const blocks: PostEditorBlock[] = [];
  if (content?.length) {
    let textIndex = 0;
    content.forEach((block, index) => {
      if (block.type === "image") {
        blocks.push({
          type: "image",
          slotId: block.slotId,
          attachmentId: block.attachmentId,
          name: block.name,
          alt: block.alt ?? block.name,
          width: block.width,
        });
      } else {
        const rich = richContent?.blocks[index];
        blocks.push({
          type: "text",
          id: `stored-text-${textIndex++}`,
          text: block.text,
          mentions: [],
          richDoc: rich?.type === "text" ? (rich.doc as JSONContent) : undefined,
        });
      }
    });
  } else {
    const rich = richContent?.blocks.find((block) => block.type === "text");
    blocks.push({
      type: "text",
      id: "stored-text-0",
      text: body,
      mentions,
      richDoc: rich?.type === "text" ? (rich.doc as JSONContent) : undefined,
    });
    for (const file of legacyFiles)
      blocks.push({
        type: "image",
        slotId: `legacy-${file.id}`,
        attachmentId: file.id,
        name: file.original_name,
        alt: file.original_name,
      });
  }
  if (!blocks.some((block) => block.type === "text")) blocks.unshift(newPostTextBlock());
  if (blocks.at(-1)?.type === "image") blocks.push(newPostTextBlock());
  let offset = 0;
  for (const block of blocks) {
    if (block.type !== "text") continue;
    block.mentions = mentions
      .filter((mention) => mention.start >= offset && mention.end <= offset + block.text.length)
      .map((mention) => ({ ...mention, start: mention.start - offset, end: mention.end - offset }));
    offset += block.text.length + 1;
  }
  return blocks;
}
