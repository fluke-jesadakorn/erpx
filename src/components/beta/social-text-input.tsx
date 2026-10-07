"use client";
import { Button, TextArea } from "@/components/ui/foundation";

import { useCallback, useEffect, useId, useRef, useState } from "react";
import dynamic from "next/dynamic";
import type { JSONContent } from "@tiptap/core";
import { activeToken, editMentions, type Mention, type Suggestion } from "@/server/modules/collaboration/public/rules/social-text";
import { betaFetch } from "@/lib/beta-client";
import { SocialSuggestionMenu, textareaSuggestionAnchor } from "./social-suggestion-menu";
import styles from "./social-text.module.css";
import { type UiAppearance } from "@/components/ui/foundation";
const AdvancedEditor = dynamic(
  () => import("./social-advanced-editor").then((m) => m.SocialAdvancedEditor),
  { ssr: false },
);
export function SocialTextInput({ appearance = "legacy",
  api,
  value,
  mentions,
  onChange,
  onRichChange,
  richDoc,
  onPasteImages,
  onSelectionChange,
  focusSignal,
  focusCaret = 0,
  showHint = true,
  maxLength,
  label,
  placeholder,
  disabled = false,
}: ({
  api: string;
  value: string;
  mentions: Mention[];
  onChange: (body: string, mentions: Mention[]) => void;
  onRichChange?: (body: string, mentions: Mention[], doc: JSONContent) => void;
  richDoc?: JSONContent | null;
  onPasteImages?: (files: File[], selection: { start: number; end: number }) => void;
  onSelectionChange?: (selection: { start: number; end: number }) => void;
  focusSignal?: number;
  focusCaret?: number;
  showHint?: boolean;
  maxLength: number;
  label: string;
  placeholder: string;
  disabled?: boolean;
}) & { appearance?: UiAppearance }) {
  const input = useRef<HTMLTextAreaElement>(null),
    listId = useId();
  const [token, setToken] = useState<ReturnType<typeof activeToken>>(null),
    [results, setResults] = useState<Suggestion[]>([]),
    [index, setIndex] = useState(0),
    [status, setStatus] = useState("");
  const [composing, setComposing] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const trigger = token?.trigger,
    query = token?.query;
  const getAnchor = useCallback(
    () => (input.current && token ? textareaSuggestionAnchor(input.current, token.end) : null),
    [token],
  );
  useEffect(() => {
    if (!focusSignal) return;
    const node = input.current;
    if (!node) return;
    node.focus();
    node.setSelectionRange(focusCaret, focusCaret);
  }, [focusSignal, focusCaret]);
  useEffect(() => {
    if (!trigger || composing) return;
    let current = true;
    const timer = setTimeout(() => {
      betaFetch<Suggestion[]>(
        `${api}/beta/suggestions?${new URLSearchParams({ trigger, q: query ?? "" })}`,
      )
        .then((rows) => {
          if (current) {
            setResults(rows);
            setIndex(0);
            setStatus(
              rows.length
                ? ""
                : trigger === "#"
                  ? "Keep typing to create a hashtag."
                  : "No matching people, agents, or records.",
            );
          }
        })
        .catch(() => {
          if (current) {
            setResults([]);
            setStatus("Suggestions unavailable. Keep writing or type @ again to retry.");
          }
        });
    }, 150);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [api, trigger, query, composing]);
  function locate(body: string, caret: number) {
    const next = activeToken(body, caret);
    // Never search from inside an already resolved mention.
    const currentMentions = body === value ? mentions : editMentions(value, body, mentions);
    const resolved = currentMentions.some((m) => caret > m.start && caret <= m.end);
    if (next?.query !== token?.query || next?.trigger !== token?.trigger) {
      setResults([]);
      setStatus("Searching…");
      setIndex(0);
    }
    setToken(resolved ? null : next);
  }
  function choose(result: Suggestion) {
    if (!token) return;
    const text = `${result.kind === "tag" ? "#" : "@"}${result.label}`,
      insert = `${text} `;
    const body = value.slice(0, token.start) + insert + value.slice(token.end);
    if (body.length > maxLength) {
      setStatus("This mention would exceed the message limit.");
      return;
    }
    const next = editMentions(value, body, mentions);
    if (result.kind !== "tag")
      next.push({
        kind: result.kind,
        targetId: result.targetId,
        label: result.label,
        start: token.start,
        end: token.start + text.length,
      });
    onChange(
      body,
      next.sort((a, b) => a.start - b.start),
    );
    setToken(null);
    const caret = token.start + insert.length;
    requestAnimationFrame(() => {
      input.current?.focus();
      input.current?.setSelectionRange(caret, caret);
      setToken(null);
    });
  }
  return (
    <div data-ui-appearance={appearance} className={styles.editor}>
      <div data-ui-appearance={appearance} className={advanced ? styles.advancedShell : undefined}>
        {onRichChange && (
          <Button appearance={appearance}
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
        )}
        {advanced ? (
          <AdvancedEditor
            api={api}
            value={value}
            mentions={mentions}
            doc={richDoc ?? null}
            onChange={(body, mentions, doc) => onRichChange?.(body, mentions, doc)}
            onImages={
              onPasteImages
                ? (files) => onPasteImages(files, { start: value.length, end: value.length })
                : undefined
            }
            maxLength={maxLength}
            disabled={disabled}
            label={label}
          />
        ) : (
          <TextArea appearance={appearance}
            ref={input}
            value={value}
            disabled={disabled}
            aria-label={label}
            placeholder={placeholder}
            maxLength={maxLength}
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={!!token && !composing}
            aria-controls={listId}
            aria-activedescendant={token && results[index] ? `${listId}-${index}` : undefined}
            onCompositionStart={() => setComposing(true)}
            onCompositionEnd={(e) => {
              setComposing(false);
              locate(e.currentTarget.value, e.currentTarget.selectionStart);
            }}
            onChange={(e) => {
              const body = e.target.value;
              onChange(body, editMentions(value, body, mentions));
              locate(body, e.target.selectionStart);
            }}
            onPaste={(e) => {
              const images = Array.from(e.clipboardData.items)
                .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
                .map((item) => item.getAsFile())
                .filter((file): file is File => !!file);
              if (images.length && onPasteImages) {
                e.preventDefault();
                onPasteImages(images, {
                  start: e.currentTarget.selectionStart,
                  end: e.currentTarget.selectionEnd,
                });
              }
            }}
            onSelect={(e) => {
              locate(e.currentTarget.value, e.currentTarget.selectionStart);
              onSelectionChange?.({
                start: e.currentTarget.selectionStart,
                end: e.currentTarget.selectionEnd,
              });
            }}
            onKeyDown={(e) => {
              if (!token || composing || e.nativeEvent.isComposing) return;
              if (e.key === "Escape") {
                e.preventDefault();
                setToken(null);
              } else if (results.length && ["ArrowDown", "ArrowUp"].includes(e.key)) {
                e.preventDefault();
                setIndex(
                  (i) => (i + (e.key === "ArrowDown" ? 1 : -1) + results.length) % results.length,
                );
              } else if (results[index] && ["Enter", "Tab"].includes(e.key)) {
                e.preventDefault();
                choose(results[index]);
              }
            }}
            onBlur={(e) => {
              const target = e.relatedTarget;
              if (!(target instanceof Node && e.currentTarget.parentElement?.contains(target)))
                setToken(null);
            }}
          />
        )}
        <SocialSuggestionMenu appearance={appearance}
          open={!advanced && !!token && !composing}
          listId={listId}
          trigger={trigger === "#" ? "#" : "@"}
          results={results}
          selectedIndex={index}
          status={status}
          getAnchor={getAnchor}
          onChoose={choose}
          onHoverIndex={setIndex}
        />
        {showHint && (
          <small data-ui-appearance={appearance} className={styles.hint}>
            Use @ for people, AI agents, and records · # for hashtags
            {onPasteImages ? " · Paste images to attach them" : ""}
          </small>
        )}
        {mentions.some((m) => m.kind === "agent") && (
          <small data-ui-appearance={appearance} className={styles.hint}>AI will reply after you publish.</small>
        )}
      </div>
    </div>
  );
}
