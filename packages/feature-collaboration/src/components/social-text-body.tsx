"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button } from "@erpx/ui/components/foundation";

import { useState } from "react";
import Link from "@erpx/workplace/components/workspace-link";
import { hashtags, type Mention } from "@erpx/server-collaboration/public/rules/social-text";
import { safeRichUrl, type RichNode } from "@erpx/feature-shared/lib/home-rich-content";
import { createElement, type ReactNode } from "react";
import styles from "@erpx/feature-collaboration/components/social-text.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";
import { SocialRecordMention } from "@erpx/feature-collaboration/components/social-record-mention";
export function SocialTextBody({ appearance = "legacy",
  body,
  mentions = [],
  base,
  doc,
}: ({
  body: string;
  mentions?: Mention[];
  base: string;
  doc?: RichNode | null;
}) & { appearance?: UiAppearance }) {
  const [person, setPerson] = useState<Mention | null>(null);
  const [expanded, setExpanded] = useState(false);
  const render = (node: RichNode, key: number): ReactNode => {
    if (node.type === "text") {
      let result: ReactNode = node.text ?? "";
      for (const mark of node.marks ?? []) {
        const attrs = mark.attrs ?? {};
        if (mark.type === "bold") result = <strong data-ui-appearance={appearance}>{result}</strong>;
        else if (mark.type === "italic") result = <em data-ui-appearance={appearance}>{result}</em>;
        else if (mark.type === "underline") result = <u data-ui-appearance={appearance}>{result}</u>;
        else if (mark.type === "strike") result = <s data-ui-appearance={appearance}>{result}</s>;
        else if (mark.type === "code") result = <code data-ui-appearance={appearance}>{result}</code>;
        else if (mark.type === "link") {
          const href = safeRichUrl(attrs.href);
          if (href)
            result = (
              <a data-ui-appearance={appearance} href={href} target="_blank" rel="noopener noreferrer">
                {result}
              </a>
            );
        } else if (mark.type === "textStyle")
          result = (
            <span data-ui-appearance={appearance}
              style={{
                fontFamily: typeof attrs.fontFamily === "string" ? attrs.fontFamily : undefined,
                fontSize: typeof attrs.fontSize === "string" ? attrs.fontSize : undefined,
                color: typeof attrs.color === "string" ? attrs.color : undefined,
                backgroundColor:
                  typeof attrs.backgroundColor === "string" ? attrs.backgroundColor : undefined,
                lineHeight: typeof attrs.lineHeight === "string" ? attrs.lineHeight : undefined,
              }}
            >
              {result}
            </span>
          );
        else if (mark.type === "highlight")
          result = (
            <mark data-ui-appearance={appearance}
              style={{ backgroundColor: typeof attrs.color === "string" ? attrs.color : undefined }}
            >
              {result}
            </mark>
          );
        else if (mark.type === "mention")
          result = attrs.kind === "record" ? <SocialRecordMention key={`${base}:${String(attrs.targetId)}`} appearance={appearance} base={base} targetId={String(attrs.targetId)} label={String(attrs.label)} className={styles.token}>{result}</SocialRecordMention> : (
            <Button appearance={appearance}
              variant="plain"
              type="button"
              className={styles.token}
              onClick={() =>
                setPerson({
                  kind: attrs.kind as Mention["kind"],
                  targetId: String(attrs.targetId),
                  label: String(attrs.label),
                  start: 0,
                  end: 0,
                })
              }
            >
              {result}
            </Button>
          );
      }
      return <span data-ui-appearance={appearance} key={key}>{result}</span>;
    }
    if (node.type === "hardBreak") return <br data-ui-appearance={appearance} key={key} />;
    if (node.type === "horizontalRule") return <hr data-ui-appearance={appearance} key={key} />;
    const tags: Record<string, string> = {
      doc: "div",
      paragraph: "p",
      heading: `h${[1, 2, 3].includes(Number(node.attrs?.level)) ? node.attrs?.level : 2}`,
      blockquote: "blockquote",
      bulletList: "ul",
      orderedList: "ol",
      listItem: "li",
      table: "table",
      tableRow: "tr",
      tableCell: "td",
      tableHeader: "th",
    };
    const tag = tags[node.type] ?? "span";
    const children = (node.content ?? []).map(render);
    const align = ["left", "right", "center", "justify"].includes(String(node.attrs?.textAlign))
      ? (String(node.attrs?.textAlign) as "left" | "right" | "center" | "justify")
      : undefined;
    const cellAlign = ["left", "right", "center"].includes(String(node.attrs?.align))
      ? (String(node.attrs?.align) as "left" | "right" | "center")
      : undefined;
    const colwidth = Array.isArray(node.attrs?.colwidth) ? node.attrs.colwidth[0] : undefined;
    const cellWidth =
      typeof colwidth === "number" && colwidth >= 40 && colwidth <= 1200
        ? `${colwidth}px`
        : undefined;
    return createElement(
      tag,
      {
        key,
        style: {
          textAlign: align ?? cellAlign,
          width: ["td", "th"].includes(tag) ? cellWidth : undefined,
        },
        colSpan: ["td", "th"].includes(tag) ? Number(node.attrs?.colspan) || undefined : undefined,
        rowSpan: ["td", "th"].includes(tag) ? Number(node.attrs?.rowspan) || undefined : undefined,
      },
      children,
    );
  };
  const spans = [
    ...mentions.map((m) => ({ ...m, type: "mention" as const })),
    ...hashtags(body)
      .filter((t) => !mentions.some((m) => t.start < m.end && t.end > m.start))
      .map((t) => ({ ...t, type: "tag" as const })),
  ].sort((a, b) => a.start - b.start);
  const parts = [];
  let end = 0;
  for (const span of spans) {
    if (span.start < end || span.end > body.length) continue;
    parts.push(body.slice(end, span.start));
    const text = body.slice(span.start, span.end);
    if (span.type === "tag")
      parts.push(
        <Link
          key={span.start}
          className={styles.token}
          href={canonicalUiHref(`${base}/home?tag=${encodeURIComponent(span.tag)}`)}
        >
          {text}
        </Link>,
      );
    else if (span.kind === "record") {
      parts.push(
        <SocialRecordMention appearance={appearance} base={base} targetId={span.targetId} label={span.label}
          key={`${base}:${span.targetId}:${span.start}`}
          className={styles.token}
        >
          {text}
        </SocialRecordMention>,
      );
    } else
      parts.push(
        <Button appearance={appearance}
          variant="plain"
          type="button"
          key={span.start}
          className={styles.token}
          onClick={() => setPerson(span)}
        >
          {text}
        </Button>,
      );
    end = span.end;
  }
  parts.push(body.slice(end));
  return (
    <>
      <div data-ui-appearance={appearance}
        className={`${styles.body} ${doc ? styles.richBody : ""} ${!expanded && body.length > 700 ? styles.collapsed : ""}`}
      >
        {doc ? render(doc, 0) : <p data-ui-appearance={appearance}>{parts}</p>}
      </div>
      {body.length > 700 && (
        <Button appearance={appearance} variant="plain" type="button" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show less" : "อ่านเพิ่มเติม"}
        </Button>
      )}
      {person && (
        <div data-ui-appearance={appearance} className={styles.identity} role="dialog" aria-label="Mention details">
          <strong data-ui-appearance={appearance}>{person.label}</strong>
          <p data-ui-appearance={appearance}>
            {person.kind === "agent"
              ? "AI replies using the conversation and your company’s configured provider."
              : `Person · ${person.targetId}`}
          </p>
          <Button appearance={appearance} variant="plain" type="button" onClick={() => setPerson(null)}>
            Close
          </Button>
        </div>
      )}
    </>
  );
}
