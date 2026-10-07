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
import { EditorContent, useEditor } from "@tiptap/react";
import { Mark, type Editor as TiptapEditor, type JSONContent } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { TextStyleKit } from "@tiptap/extension-text-style";
import { TextAlign } from "@tiptap/extension-text-align";
import { TableKit } from "@tiptap/extension-table";
import { betaFetch } from "@/lib/beta-client";
import { plainRichContent, validateRichContent } from "@/lib/home-rich-content";
import type { Mention, Suggestion } from "@/server/modules/collaboration/public/rules/social-text";
import { SocialSuggestionMenu, type SuggestionAnchor } from "./social-suggestion-menu";
import styles from "./social-text.module.css";
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
} from "lucide-react";

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
export function SocialAdvancedEditor({
  api,
  value,
  mentions,
  doc,
  onChange,
  onImages,
  maxLength,
  disabled,
  label,
}: {
  api: string;
  value: string;
  mentions: Mention[];
  doc: JSONContent | null;
  onChange: (body: string, mentions: Mention[], doc: JSONContent) => void;
  onImages?: (files: File[]) => void;
  maxLength: number;
  disabled: boolean;
  label: string;
}) {
  const listId = useId();
  const [token, setToken] = useState<{ start: number; end: number; query: string } | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [suggestionStatus, setSuggestionStatus] = useState("");
  const [composing, setComposing] = useState(false);
  const [error, setError] = useState("");
  const composingRef = useRef(false);
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
  const actionsRef = useRef<{
    close: () => void;
    move: (delta: number) => void;
    choose: (suggestion: Suggestion) => void;
  }>({ close: () => {}, move: () => {}, choose: () => {} });
  function syncMentionToken(activeEditor: TiptapEditor) {
    if (composingRef.current || !activeEditor.isFocused || !activeEditor.state.selection.empty) {
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
  }
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false } }),
      TextStyleKit,
      TextAlign.configure({ types: ["heading", "paragraph"] }),
      TableKit.configure({ table: { resizable: true } }),
      MentionMark,
    ],
    content: doc ?? (plainRichContent(value, mentions).blocks[0] as { doc: JSONContent }).doc,
    immediatelyRender: false,
    editable: !disabled,
    editorProps: {
      handlePaste(_view, event) {
        const files = Array.from(event.clipboardData?.files ?? []).filter((file) =>
          file.type.startsWith("image/"),
        );
        if (files.length && onImages) {
          event.preventDefault();
          onImages(files);
          return true;
        }
        return false;
      },
      handleDrop(_view, event) {
        const files = Array.from(event.dataTransfer?.files ?? []).filter((file) =>
          file.type.startsWith("image/"),
        );
        if (files.length && onImages) {
          event.preventDefault();
          onImages(files);
          return true;
        }
        return false;
      },
      handleKeyDown(_view, event) {
        if (composingRef.current || event.isComposing) return false;
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
    onUpdate({ editor }) {
      const next = editor.getJSON();
      try {
        const parsed = validateRichContent(
          { version: 1, blocks: [{ type: "text", doc: next }] },
          maxLength,
        );
        setError("");
        onChange(parsed.body, parsed.mentions, next);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Document is invalid.");
        onChange(editor.getText({ blockSeparator: "\n" }), [], next);
      }
    },
    onSelectionUpdate({ editor }) {
      syncMentionToken(editor);
    },
  });
  useLayoutEffect(() => {
    editorRef.current = editor;
  });
  useEffect(() => {
    editor?.setEditable(!disabled);
  }, [editor, disabled]);
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
      if (!editor || !active) return;
      const currentText = editor.getText({ blockSeparator: "\n" });
      if (currentText.length - (active.end - active.start) + item.label.length + 2 > maxLength) {
        setSuggestionStatus("This mention would exceed the message limit.");
        return;
      }
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
    [editor, maxLength, dismissSuggestions],
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
  useEffect(() => {
    if (!editor) return;
    const node = editor.view.dom;
    node.setAttribute("role", "combobox");
    node.setAttribute("aria-label", label);
    node.setAttribute("aria-autocomplete", "list");
    node.setAttribute("aria-haspopup", "listbox");
    node.setAttribute("aria-controls", listId);
    node.setAttribute("aria-expanded", String(!!token && !composing));
    const active = suggestions[selectedIndex];
    if (token && !composing && active)
      node.setAttribute("aria-activedescendant", `${listId}-${selectedIndex}`);
    else node.removeAttribute("aria-activedescendant");
  }, [editor, label, listId, token, composing, suggestions, selectedIndex]);
  if (!editor) return <p>Opening editor…</p>;
  const button = (name: string, icon: ReactNode, action: () => void, active = false) => (
    <Button
      variant="plain"
      key={name}
      type="button"
      aria-label={name}
      title={name}
      aria-pressed={active}
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
  return (
    <div className={styles.advanced} aria-label={label}>
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
            onChange={(e) => chain().setFontFamily(e.target.value).run()}
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
              onChange={(e) => chain().setFontSize(e.target.value).run()}
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
              onChange={(e) => chain().setColor(e.target.value).run()}
            />
          </Field>
          <Field title="Highlight">
            <Highlighter size={16} />
            <TextInput
              aria-label="Highlight"
              type="color"
              onChange={(e) => chain().setBackgroundColor(e.target.value).run()}
            />
          </Field>
          <Field className={styles.valueControl}>
            <span aria-hidden="true">Spacing</span>
            <Select
              className={`${styles.numericSelect} ${styles.spacingSelect}`}
              aria-label="Line spacing"
              defaultValue="1.15"
              onChange={(e) => chain().setLineHeight(e.target.value).run()}
            >
              {["1", "1.15", "1.5", "2"].map((v) => (
                <option key={v}>{v}</option>
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
              } catch {}
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
      <EditorContent editor={editor} className={styles.richInput} />
      <SocialSuggestionMenu
        open={!!token && !composing}
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
          <strong>{value.length.toLocaleString()}</strong>
          <span aria-hidden="true">/</span>
          <span>{maxLength.toLocaleString()}</span>
          <span className={styles.countUnit}>characters</span>
        </small>
      )}
    </div>
  );
}
