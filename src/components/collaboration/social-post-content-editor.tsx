"use client";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  EditorContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  useEditor,
  type NodeViewProps,
} from "@tiptap/react";
import { Mark, Node, type Editor as TiptapEditor, type JSONContent } from "@tiptap/core";
import { Fragment } from "@tiptap/pm/model";
import { NodeSelection } from "@tiptap/pm/state";
import StarterKit from "@tiptap/starter-kit";
import { TextStyleKit } from "@tiptap/extension-text-style";
import { TextAlign } from "@tiptap/extension-text-align";
import { TableKit } from "@tiptap/extension-table";
import { betaFetch } from "@/lib/shared/beta-client";
import { plainRichContent, validateRichContent, type RichNode } from "@/lib/shared/home-rich-content";
import type { Mention, Suggestion } from "@/server/modules/collaboration/public/rules/social-text";
import { SocialSuggestionMenu, type SuggestionAnchor } from "@/components/collaboration/social-suggestion-menu";
import styles from "@/components/collaboration/social-text.module.css";
import {
  Undo2,
  Redo2,
  Bold,
  Italic,
  Underline,
  Strikethrough,
  Heading1,
  Heading2,
  List,
  ListOrdered,
  TextAlignStart,
  TextAlignCenter,
  TextAlignEnd,
  TextAlignJustify,
  Link2,
  Table2,
  Rows3,
  Columns3,
  Combine,
  Split,
  Plus,
  Minus,
  Paintbrush,
  Highlighter,
  ImagePlus,
  X,
  ArrowUp,
  ArrowDown,
} from "lucide-react";

import {
  postContentBody,
  postWireRichContent,
  type PostEditorBlock,
  type PostTextBlock,
  type PostImageBlock,
} from "@/lib/collaboration/social-post-editor-state";

function contentToDocument(blocks: PostEditorBlock[]): JSONContent {
  const content: JSONContent[] = [];
  for (const block of blocks) {
    if (block.type === "image") {
      content.push({
        type: "homeImage",
        attrs: {
          slotId: block.slotId,
          attachmentId: block.attachmentId,
          name: block.name,
          alt: block.alt,
          width: block.width ?? 100,
          previewUrl: block.previewUrl ?? null,
        },
      });
      continue;
    }
    const fallback = plainRichContent(block.text, block.mentions).blocks[0] as { doc: JSONContent };
    const doc = block.richDoc?.type === "doc" ? block.richDoc : fallback.doc;
    content.push(...(doc.content?.length ? doc.content : [{ type: "paragraph" }]));
  }
  if (!content.length) content.push({ type: "paragraph" });
  if (content.at(-1)?.type === "homeImage") content.push({ type: "paragraph" });
  return { type: "doc", content };
}

function documentToBlocks(
  doc: JSONContent,
  files: Map<string, { file: File; previewUrl: string }>,
  prior: PostEditorBlock[],
): PostEditorBlock[] {
  const output: PostEditorBlock[] = [];
  let textNodes: JSONContent[] = [];
  let textIndex = 0;
  const flushText = () => {
    const richDoc: JSONContent = {
      type: "doc",
      content: textNodes.length ? textNodes : [{ type: "paragraph" }],
    };
    const rich = {
      version: 1 as const,
      blocks: [{ type: "text" as const, doc: richDoc as RichNode }],
    };
    let text = "";
    let mentions: Mention[] = [];
    try {
      const parsed = validateRichContent(rich, 30000);
      text = parsed.body;
      mentions = parsed.mentions;
    } catch {
      /* The editor keeps the current document so the user can fix invalid content. */
    }
    const old = prior.filter((block): block is PostTextBlock => block.type === "text")[textIndex];
    output.push({
      type: "text",
      id: old?.id ?? `post-text-${textIndex}-${crypto.randomUUID()}`,
      text,
      mentions,
      richDoc,
    });
    textIndex++;
    textNodes = [];
  };
  for (const node of doc.content ?? []) {
    if (node.type === "homeImage") {
      flushText();
      const attrs = node.attrs ?? {};
      const slotId = String(attrs.slotId ?? crypto.randomUUID());
      const fileData = files.get(slotId);
      const file = fileData?.file;
      output.push({
        type: "image",
        slotId,
        attachmentId: typeof attrs.attachmentId === "string" ? attrs.attachmentId : null,
        name: String(attrs.name || file?.name || "Image"),
        alt: String(attrs.alt ?? ""),
        width: Number.isInteger(attrs.width) ? Number(attrs.width) : 100,
        file,
        previewUrl: typeof attrs.previewUrl === "string" ? attrs.previewUrl : undefined,
      });
    } else textNodes.push(node);
  }
  if (textNodes.length || !output.length || doc.content?.at(-1)?.type !== "homeImage") flushText();
  return output;
}

function HomeImageView({
  node,
  editor,
  getPos,
  updateAttributes,
  deleteNode,
  api,
}: NodeViewProps & { api: string }) {
  const attrs = node.attrs as {
    slotId: string;
    attachmentId: string | null;
    name: string;
    alt: string;
    width: number;
    previewUrl: string | null;
  };
  const position = typeof getPos === "function" ? getPos() : undefined;
  let canMoveEarlier = false,
    canMoveLater = false;
  if (typeof position === "number") {
    const resolved = editor.state.doc.resolve(position);
    canMoveEarlier = !!resolved.nodeBefore;
    canMoveLater = !!editor.state.doc.nodeAt(position + node.nodeSize);
  }
  const move = (direction: -1 | 1) => {
    if (typeof position !== "number") return;
    const current = editor.state.doc.nodeAt(position);
    if (!current) return;
    const tr = editor.state.tr;
    if (direction < 0) {
      const previous = tr.doc.resolve(position).nodeBefore;
      if (!previous) return;
      const start = position - previous.nodeSize;
      tr.replaceWith(start, position + current.nodeSize, Fragment.fromArray([current, previous]));
      tr.setSelection(NodeSelection.create(tr.doc, start));
    } else {
      const next = tr.doc.nodeAt(position + current.nodeSize);
      if (!next) return;
      tr.replaceWith(
        position,
        position + current.nodeSize + next.nodeSize,
        Fragment.fromArray([next, current]),
      );
      tr.setSelection(NodeSelection.create(tr.doc, position + next.nodeSize));
    }
    editor.view.dispatch(tr.scrollIntoView());
    editor.view.focus();
  };
  const src =
    attrs.previewUrl ||
    (attrs.attachmentId ? `${api}/attachments/${attrs.attachmentId}/content?variant=preview` : "");
  return (
    <NodeViewWrapper
      as="figure"
      className="beta-inline-image beta-editor-image"
      style={{ maxWidth: `${attrs.width || 100}%` }}
      contentEditable={false}
    >
      {src ? (
        <>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={src} alt={attrs.alt || attrs.name} />
        </>
      ) : (
        <div className="beta-image-missing" role="status">
          Image ready to upload.
        </div>
      )}
      <div className="beta-inline-image-controls">
        <TextInput
          aria-label={`Image description for ${attrs.name}`}
          value={attrs.alt ?? ""}
          maxLength={160}
          placeholder="Image description"
          onChange={(event) => updateAttributes({ alt: event.target.value })}
        />
        <Field>
          Width{" "}
          <Select
            aria-label={`Width for ${attrs.name}`}
            value={attrs.width || 100}
            onChange={(event) => updateAttributes({ width: Number(event.target.value) })}
          >
            {[25, 50, 75, 100].map((width) => (
              <option key={width} value={width}>
                {width}%
              </option>
            ))}
          </Select>
        </Field>
        <Button
          variant="plain"
          type="button"
          aria-label={`Move ${attrs.name} earlier in the post`}
          title="Move image earlier"
          disabled={!canMoveEarlier}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => move(-1)}
        >
          <ArrowUp size={15} />
        </Button>
        <Button
          variant="plain"
          type="button"
          aria-label={`Move ${attrs.name} later in the post`}
          title="Move image later"
          disabled={!canMoveLater}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => move(1)}
        >
          <ArrowDown size={15} />
        </Button>
        <Button
          variant="plain"
          type="button"
          aria-label={`Remove ${attrs.name}`}
          onMouseDown={(event) => event.preventDefault()}
          onClick={deleteNode}
        >
          <X size={15} />
        </Button>
      </div>
    </NodeViewWrapper>
  );
}

const HomeImage = Node.create<{ api: string }>({
  name: "homeImage",
  group: "block",
  atom: true,
  selectable: true,
  draggable: true,
  addOptions() {
    return { api: "" };
  },
  addAttributes() {
    return {
      slotId: { default: null },
      attachmentId: { default: null },
      name: { default: "Image" },
      alt: { default: "" },
      width: { default: 100 },
      previewUrl: { default: null },
    };
  },
  parseHTML() {
    return [{ tag: "figure[data-home-image]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const attrs = { ...HTMLAttributes };
    delete attrs.previewUrl;
    return ["figure", { ...attrs, "data-home-image": "" }];
  },
  addNodeView() {
    const api = this.options.api;
    return ReactNodeViewRenderer((props) => <HomeImageView {...props} api={api} />);
  },
});

const MentionMark = Mark.create({
  name: "mention",
  inclusive: false,
  addAttributes() {
    return { kind: { default: null }, targetId: { default: null }, label: { default: null } };
  },
  parseHTML() {
    return [{ tag: "span[data-home-mention]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["span", { "data-home-mention": "", class: "home-mention", ...HTMLAttributes }, 0];
  },
});

const fonts = [
  "Arial",
  "Georgia",
  "Times New Roman",
  "Noto Sans Thai",
  "Noto Serif Thai",
  "Courier New",
];

export function SocialPostContentEditor({
  api,
  blocks,
  onChange,
  resetKey,
  focusSignal,
  maxLength,
  label,
  placeholder,
  disabled = false,
  showHint = true,
}: {
  api: string;
  blocks: PostEditorBlock[];
  onChange: (blocks: PostEditorBlock[]) => void;
  resetKey: number;
  focusSignal?: number;
  maxLength: number;
  label: string;
  placeholder: string;
  disabled?: boolean;
  showHint?: boolean;
}) {
  const listId = useId();
  const [advanced, setAdvanced] = useState(false);
  const [token, setToken] = useState<{ start: number; end: number; query: string } | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [suggestionStatus, setSuggestionStatus] = useState("");
  const [composing, setComposing] = useState(false);
  const composingRef = useRef(false);
  const [error, setError] = useState("");
  const files = useRef(new Map<string, { file: File; previewUrl: string }>());
  const editorRef = useRef<TiptapEditor | null>(null);
  const tokenRef = useRef(token);
  useLayoutEffect(() => {
    tokenRef.current = token;
  });
  const suggestionsRef = useRef(suggestions);
  useLayoutEffect(() => {
    suggestionsRef.current = suggestions;
  });
  const selectedIndexRef = useRef(selectedIndex);
  useLayoutEffect(() => {
    selectedIndexRef.current = selectedIndex;
  });
  const imageInsertRef = useRef<(files: File[], range?: { from: number; to: number }) => void>(
    () => {},
  );
  const actionsRef = useRef<{
    close: () => void;
    move: (delta: number) => void;
    choose: (suggestion: Suggestion) => void;
  }>({ close: () => {}, move: () => {}, choose: () => {} });

  const syncMentionToken = useCallback((activeEditor: TiptapEditor) => {
    if (!activeEditor.isFocused || !activeEditor.state.selection.empty) {
      setToken(null);
      setSuggestions([]);
      return;
    }
    const { from, $from } = activeEditor.state.selection;
    const mentionMark = activeEditor.schema.marks.mention;
    if (
      mentionMark &&
      from > 0 &&
      activeEditor.state.doc.rangeHasMark(from - 1, from, mentionMark)
    ) {
      setToken(null);
      setSuggestions([]);
      return;
    }
    const before = $from.parent.textBetween(0, $from.parentOffset, "", "");
    const match = /(?:^|[\s(])@([^\s@#]*)$/u.exec(before);
    if (!match) {
      setToken(null);
      setSuggestions([]);
      return;
    }
    const next = { start: from - match[1].length - 1, end: from, query: match[1] };
    const prior = tokenRef.current;
    if (
      !prior ||
      prior.start !== next.start ||
      prior.end !== next.end ||
      prior.query !== next.query
    ) {
      setSuggestions([]);
      setSelectedIndex(0);
      setSuggestionStatus("Searching…");
      setToken(next);
    }
  }, []);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false } }),
      TextStyleKit,
      TextAlign.configure({ types: ["heading", "paragraph"] }),
      TableKit.configure({ table: { resizable: true } }),
      MentionMark,
      HomeImage.configure({ api }),
    ],
    content: contentToDocument(blocks),
    immediatelyRender: false,
    editable: !disabled,
    editorProps: {
      handlePaste(view, event) {
        const images = Array.from(event.clipboardData?.files ?? []).filter((file) =>
          file.type.startsWith("image/"),
        );
        if (!images.length) return false;
        event.preventDefault();
        imageInsertRef.current(images, {
          from: view.state.selection.from,
          to: view.state.selection.to,
        });
        return true;
      },
      handleDrop(view, event) {
        const images = Array.from(event.dataTransfer?.files ?? []).filter((file) =>
          file.type.startsWith("image/"),
        );
        if (!images.length) return false;
        event.preventDefault();
        const point =
          view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos ??
          view.state.selection.from;
        imageInsertRef.current(images, { from: point, to: point });
        return true;
      },
      handleKeyDown(_view, event) {
        if (event.isComposing || composingRef.current) return false;
        if (!tokenRef.current) return false;
        if (event.key === "Escape") {
          actionsRef.current.close();
          return true;
        }
        if (suggestionsRef.current.length && ["ArrowDown", "ArrowUp"].includes(event.key)) {
          actionsRef.current.move(event.key === "ArrowDown" ? 1 : -1);
          return true;
        }
        const selected = suggestionsRef.current[selectedIndexRef.current];
        if (selected && ["Enter", "Tab"].includes(event.key)) {
          actionsRef.current.choose(selected);
          return true;
        }
        return false;
      },
      handleDOMEvents: {
        compositionstart() {
          composingRef.current = true;
          setComposing(true);
          return false;
        },
        compositionend() {
          composingRef.current = false;
          setComposing(false);
          window.setTimeout(() => {
            if (editorRef.current) syncMentionToken(editorRef.current);
          }, 0);
          return false;
        },
      },
    },
    onUpdate({ editor: active }) {
      const next = documentToBlocks(active.getJSON(), files.current, blocks);
      const serialized = postWireRichContent(next);
      try {
        validateRichContent(serialized, maxLength);
        setError("");
      } catch (e) {
        setError(e instanceof Error ? e.message : "Document is invalid.");
      }
      const validSlots = new Set(
        next
          .filter((block): block is PostImageBlock => block.type === "image")
          .map((block) => block.slotId),
      );
      for (const [slotId, file] of files.current) {
        if (!validSlots.has(slotId)) {
          files.current.delete(slotId);
          URL.revokeObjectURL(file.previewUrl);
        }
      }
      onChange(next);
    },
    onSelectionUpdate({ editor: active }) {
      syncMentionToken(active);
    },
  });
  useLayoutEffect(() => {
    editorRef.current = editor;
  });

  useEffect(() => {
    editor?.setEditable(!disabled);
  }, [editor, disabled]);
  const previousReset = useRef(resetKey);
  useEffect(() => {
    if (!editor || previousReset.current === resetKey) return;
    previousReset.current = resetKey;
    for (const [slotId, file] of files.current) {
      const image = blocks.find(
        (block): block is PostImageBlock => block.type === "image" && block.slotId === slotId,
      );
      if (!image?.file) {
        files.current.delete(slotId);
        URL.revokeObjectURL(file.previewUrl);
      }
    }
    editor.commands.setContent(contentToDocument(blocks));
  }, [editor, resetKey, blocks]);

  useEffect(
    () => () => {
      for (const file of files.current.values()) URL.revokeObjectURL(file.previewUrl);
      files.current.clear();
    },
    [],
  );

  useEffect(() => {
    if (!editor || !focusSignal) return;
    editor.commands.focus("end");
  }, [editor, focusSignal]);

  useEffect(() => {
    if (!editor) return;
    const wanted = new Map(
      blocks
        .filter((block): block is PostImageBlock => block.type === "image")
        .map((block) => [block.slotId, block]),
    );
    for (const [slotId, file] of files.current) {
      if (!wanted.get(slotId)?.file) {
        files.current.delete(slotId);
        URL.revokeObjectURL(file.previewUrl);
      }
    }
    const transaction = editor.state.tr;
    let changed = false;
    editor.state.doc.descendants((node, position) => {
      if (node.type.name !== "homeImage") return;
      const current = node.attrs as {
        slotId: string;
        attachmentId: string | null;
        name: string;
        alt: string;
        width: number;
        previewUrl: string | null;
      };
      const value = wanted.get(current.slotId);
      if (!value) return;
      const previewUrl = value.file ? current.previewUrl : null;
      if (
        current.attachmentId !== value.attachmentId ||
        current.name !== value.name ||
        current.alt !== value.alt ||
        current.width !== (value.width ?? 100) ||
        current.previewUrl !== previewUrl
      ) {
        if (current.previewUrl && !value.file) URL.revokeObjectURL(current.previewUrl);
        transaction.setNodeMarkup(position, undefined, {
          ...current,
          attachmentId: value.attachmentId,
          name: value.name,
          alt: value.alt,
          width: value.width ?? 100,
          previewUrl,
        });
        changed = true;
      }
    });
    if (changed) editor.view.dispatch(transaction);
  }, [editor, blocks]);

  useEffect(() => {
    editor?.setOptions({
      editorProps: {
        ...editor.options.editorProps,
        attributes: {
          ...editor.options.editorProps?.attributes,
          role: "combobox",
          "aria-label": label,
          "aria-autocomplete": "list",
          "aria-haspopup": "listbox",
          "aria-controls": listId,
          "aria-expanded": String(!!token && !composing),
          ...(token && !composing && suggestions[selectedIndex]
            ? { "aria-activedescendant": `${listId}-${selectedIndex}` }
            : {}),
        },
      },
    });
  }, [editor, label, listId, token, composing, suggestions, selectedIndex]);

  useEffect(() => {
    if (!token || composing) return;
    let live = true;
    const timer = setTimeout(() => {
      void betaFetch<Suggestion[]>(
        `${api}/beta/suggestions?${new URLSearchParams({ trigger: "@", q: token.query })}`,
      )
        .then((rows) => {
          if (live) {
            setSuggestions(rows);
            setSelectedIndex(0);
            setSuggestionStatus(rows.length ? "" : "No matching people, AI agents, or records.");
          }
        })
        .catch(() => {
          if (live) {
            setSuggestions([]);
            setSuggestionStatus("Suggestions unavailable. Keep writing or type @ again to retry.");
          }
        });
    }, 150);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [api, token, composing]);

  const dismissSuggestions = useCallback(() => {
    setToken(null);
    setSuggestions([]);
    setSuggestionStatus("");
  }, []);
  const chooseSuggestion = useCallback(
    (item: Suggestion) => {
      const active = tokenRef.current;
      if (disabled || !editor || !active) return;
      editor
        .chain()
        .focus()
        .deleteRange({ from: active.start, to: active.end })
        .insertContent([
          {
            type: "text",
            text: `@${item.label}`,
            marks: [
              {
                type: "mention",
                attrs: { kind: item.kind, targetId: item.targetId, label: item.label },
              },
            ],
          },
          { type: "text", text: " " },
        ])
        .run();
      dismissSuggestions();
    },
    [disabled, editor, dismissSuggestions],
  );
  const moveSelection = useCallback((delta: number) => {
    if (!suggestionsRef.current.length) return;
    setSelectedIndex(
      (index) => (index + delta + suggestionsRef.current.length) % suggestionsRef.current.length,
    );
  }, []);
  useLayoutEffect(() => {
    actionsRef.current = {
      close: dismissSuggestions,
      move: moveSelection,
      choose: chooseSuggestion,
    };
  });
  const getAnchor = useCallback((): SuggestionAnchor | null => {
    if (!editor || !token) return null;
    try {
      const caret = editor.view.coordsAtPos(token.end),
        bounds = editor.view.dom.getBoundingClientRect();
      return {
        caret: { left: caret.left, top: caret.top, bottom: caret.bottom },
        bounds: { left: bounds.left, right: bounds.right, width: bounds.width },
      };
    } catch {
      return null;
    }
  }, [editor, token]);

  const addImages = useCallback((incoming: File[], range?: { from: number; to: number }) => {
    const active = editorRef.current;
    if (!active || !incoming.length) return;
    const supported = incoming.filter(
      (file) =>
        ["image/jpeg", "image/png", "image/webp"].includes(file.type) &&
        file.size > 0 &&
        file.size <= 10 * 1024 * 1024,
    );
    let count = 0;
    active.state.doc.descendants((node) => {
      if (node.type.name === "homeImage") count++;
    });
    const available = Math.max(0, 10 - count);
    const added = supported.slice(0, available);
    if (added.length) {
      const from = range?.from ?? active.state.selection.from,
        to = range?.to ?? active.state.selection.to;
      const nodes = added.map((file) => {
        const slotId = crypto.randomUUID();
        const previewUrl = URL.createObjectURL(file);
        files.current.set(slotId, { file, previewUrl });
        return {
          type: "homeImage",
          attrs: {
            slotId,
            attachmentId: null,
            name: file.name || "Pasted image",
            alt: file.name || "Pasted image",
            width: 100,
            previewUrl,
          },
        };
      });
      active.chain().focus().insertContentAt({ from, to }, nodes).run();
    }
    if (incoming.some((file) => !["image/jpeg", "image/png", "image/webp"].includes(file.type)))
      setError("Posts accept JPEG, PNG, and WebP images.");
    else if (incoming.some((file) => file.size < 1 || file.size > 10 * 1024 * 1024))
      setError("Each image must be between 1 byte and 10 MB.");
    else if (incoming.length > available) setError("You can attach up to 10 images to an update.");
  }, []);
  useLayoutEffect(() => {
    imageInsertRef.current = addImages;
  });

  if (!editor) return <p>Opening editor…</p>;
  const button = (name: string, icon: ReactNode, action: () => void, active = false) => (
    <Button
      variant="plain"
      key={name}
      type="button"
      aria-label={name}
      title={name}
      aria-pressed={active}
      onMouseDown={(event) => event.preventDefault()}
      onClick={action}
    >
      {icon}
    </Button>
  );
  const paired = (Icon: typeof Rows3, Modifier: typeof Plus) => (
    <span className={styles.pairedIcon}>
      <Icon size={17} />
      <Modifier size={10} />
    </span>
  );
  const chain = () => editor.chain().focus();
  const toolbar = (
    <div className={styles.toolbar} role="toolbar" aria-label="Formatting">
      <div className={styles.toolGroup}>
        {button("Undo", <Undo2 size={17} />, () => chain().undo().run())}
        {button("Redo", <Redo2 size={17} />, () => chain().redo().run())}
      </div>
      <div className={styles.toolGroup}>
        {button(
          "Bold",
          <Bold size={17} />,
          () => chain().toggleBold().run(),
          editor.isActive("bold"),
        )}
        {button(
          "Italic",
          <Italic size={17} />,
          () => chain().toggleItalic().run(),
          editor.isActive("italic"),
        )}
        {button(
          "Underline",
          <Underline size={17} />,
          () => chain().toggleUnderline().run(),
          editor.isActive("underline"),
        )}
        {button(
          "Strike",
          <Strikethrough size={17} />,
          () => chain().toggleStrike().run(),
          editor.isActive("strike"),
        )}
      </div>
      <div className={styles.toolGroup}>
        {button("Heading 1", <Heading1 size={17} />, () =>
          chain().toggleHeading({ level: 1 }).run(),
        )}
        {button("Heading 2", <Heading2 size={17} />, () =>
          chain().toggleHeading({ level: 2 }).run(),
        )}
        {button("Bullets", <List size={17} />, () => chain().toggleBulletList().run())}
        {button("Numbers", <ListOrdered size={17} />, () => chain().toggleOrderedList().run())}
      </div>
      <div className={styles.toolGroup}>
        {button("Align left", <TextAlignStart size={17} />, () =>
          chain().setTextAlign("left").run(),
        )}
        {button("Align center", <TextAlignCenter size={17} />, () =>
          chain().setTextAlign("center").run(),
        )}
        {button("Align right", <TextAlignEnd size={17} />, () =>
          chain().setTextAlign("right").run(),
        )}
        {button("Justify", <TextAlignJustify size={17} />, () =>
          chain().setTextAlign("justify").run(),
        )}
      </div>
      <div className={styles.toolGroup}>
        <Select
          aria-label="Font"
          defaultValue="Arial"
          onChange={(event) => chain().setFontFamily(event.target.value).run()}
        >
          {fonts.map((font) => (
            <option key={font}>{font}</option>
          ))}
        </Select>
        <Field className={styles.valueControl}>
          <span aria-hidden="true">Size</span>
          <Select
            className={styles.numericSelect}
            aria-label="Font size"
            defaultValue="16px"
            onChange={(event) => chain().setFontSize(event.target.value).run()}
          >
            {["12px", "14px", "16px", "18px", "24px", "30px", "36px"].map((size) => (
              <option key={size}>{size}</option>
            ))}
          </Select>
        </Field>
        <Field title="Text color">
          <Paintbrush size={16} />
          <TextInput
            aria-label="Text color"
            type="color"
            onChange={(event) => chain().setColor(event.target.value).run()}
          />
        </Field>
        <Field title="Highlight">
          <Highlighter size={16} />
          <TextInput
            aria-label="Highlight"
            type="color"
            onChange={(event) => chain().setBackgroundColor(event.target.value).run()}
          />
        </Field>
        <Field className={styles.valueControl}>
          <span aria-hidden="true">Spacing</span>
          <Select
            className={`${styles.numericSelect} ${styles.spacingSelect}`}
            aria-label="Line spacing"
            defaultValue="1.15"
            onChange={(event) => chain().setLineHeight(event.target.value).run()}
          >
            {["1", "1.15", "1.5", "2"].map((value) => (
              <option key={value}>{value}</option>
            ))}
          </Select>
        </Field>
        {button("Link", <Link2 size={17} />, () => {
          const href = prompt("Link URL");
          if (href) {
            try {
              const url = new URL(href);
              if (["http:", "https:"].includes(url.protocol))
                chain().setLink({ href: url.href }).run();
            } catch {
              /* Invalid links stay plain text. */
            }
          }
        })}
      </div>
      <div className={styles.toolGroup}>
        {button("Insert table", <Table2 size={17} />, () =>
          chain().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
        )}
        {button("Add row", paired(Rows3, Plus), () => chain().addRowAfter().run())}
        {button("Delete row", paired(Rows3, Minus), () => chain().deleteRow().run())}
        {button("Add column", paired(Columns3, Plus), () => chain().addColumnAfter().run())}
        {button("Delete column", paired(Columns3, Minus), () => chain().deleteColumn().run())}
        {button("Merge cells", <Combine size={17} />, () => chain().mergeCells().run())}
        {button("Split cell", <Split size={17} />, () => chain().splitCell().run())}
      </div>
    </div>
  );
  const body = postContentBody(blocks);
  return (
    <div className={styles.editor}>
      <Button
        variant="plain"
        type="button"
        className={styles.advancedToggle}
        onClick={() => {
          setToken(null);
          setAdvanced((open) => !open);
        }}
      >
        {advanced ? "Done" : "Advanced"}
      </Button>
      <div className={advanced ? styles.advancedShell : styles.basicShell}>
        <div className={styles.advanced}>
          {advanced && toolbar}
          <div className={styles.postRichInput}>
            {editor.isEmpty && (
              <span className={styles.postPlaceholder} aria-hidden="true">
                {placeholder}
              </span>
            )}
            <EditorContent editor={editor} />
            <Field className={styles.postImageButton} title="Add images">
              <ImagePlus size={16} aria-hidden="true" /> Add images
              <TextInput
                type="file"
                aria-label="Add images to post"
                accept="image/png,image/jpeg,image/webp"
                multiple
                disabled={disabled}
                onChange={(event) => {
                  const selected = Array.from(event.target.files ?? []);
                  if (selected.length) addImages(selected);
                  event.target.value = "";
                }}
              />
            </Field>
          </div>
          <SocialSuggestionMenu
            open={!disabled && !!token && !composing}
            listId={listId}
            trigger="@"
            results={suggestions}
            selectedIndex={selectedIndex}
            status={suggestionStatus}
            getAnchor={getAnchor}
            onChoose={chooseSuggestion}
            onHoverIndex={setSelectedIndex}
          />
          {error ? (
            <small className={styles.editorError} role="status">
              {error}
            </small>
          ) : (
            <small className={styles.characterCount} role="status">
              <strong>{body.length.toLocaleString()}</strong>
              <span aria-hidden="true">/</span>
              <span>{maxLength.toLocaleString()}</span>
              <span className={styles.countUnit}>characters</span>
            </small>
          )}
        </div>
      </div>
      {showHint && (
        <small className={styles.hint}>
          Use @ for people, AI agents, and records · Paste or drag images where they belong
        </small>
      )}
      {blocks.some(
        (block) =>
          block.type === "text" && block.mentions.some((mention) => mention.kind === "agent"),
      ) && <small className={styles.hint}>AI will reply after you publish.</small>}
    </div>
  );
}
