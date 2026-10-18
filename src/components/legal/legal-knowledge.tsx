"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import s from "./legal.module.css";
import type { LegalEvidenceSource } from "@/server/modules/legal/public/legal/knowledge";
export function LegalKnowledge({
  call,
  open,
  base,
}: {
  call: (path: string) => Promise<unknown>;
  open: (id: string) => Promise<void>;
  base: string;
}) {
  const [asOf] = useState(() => new Date(Date.now() + 7 * 3600000).toISOString().slice(0, 10));
  const [data, setData] = useState<{
      notice: string;
      asOf: string;
      sources: LegalEvidenceSource[];
    } | null>(null),
    [error, setError] = useState("");
  return (
    <div className={s.toolContent}>
      <form
        className={s.toolbar}
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            const q = new FormData(e.currentTarget).get("query");
            setData(
              (await call(
                `/knowledge?q=${encodeURIComponent(String(q))}&asOf=${new FormData(e.currentTarget).get("asOf")}`,
              )) as typeof data,
            );
            setError("");
          } catch (e) {
            setError((e as Error).message);
          }
        }}
      >
        <TextInput appearance="standard"
          aria-label="Knowledge search"
          placeholder="Search clauses, documents or a legal topic…"
          name="query"
          required
          minLength={2}
          maxLength={200}
        />
        <Field appearance="standard" className={s.field}>
          Applicable date (Gregorian)
          <TextInput appearance="standard" name="asOf" type="date" required defaultValue={asOf} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button}>
          Search documents and laws
        </Button>
      </form>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && <p>{data.notice}</p>}
      {data?.sources.map((d) => (
        <article className={s.event} key={d.sourceId}>
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={s.button}
            onClick={() => void open(d.item_id)}
          >
            {d.title} · {d.name ?? d.revision_label ?? "Official source"} · v{d.version}
          </Button>
          <p>{d.excerpt}</p>
          <p className={s.small}>
            {d.kind === "law"
              ? `Law revision ${d.revision_label} · Effective ${d.effective_from} · As of ${data!.asOf}`
              : "Reviewed document version"}
            {d.page ? ` · Page ${d.page}` : ""}
            {d.section ? ` · ${d.section}` : ""}
          </p>
          <a
            data-slot="button" className={['ui-standard-control', (s.button)].filter(Boolean).join(' ')}
            href={canonicalUiHref(`${base}/legal/citations/${encodeURIComponent(d.kind === "law" ? `law:${d.id}` : d.sourceId)}?asOf=${encodeURIComponent(data!.asOf)}`)}
          >
            Open citation and original
          </a>
          <small>
            {d.document_id} · SHA256 {d.checksum}
          </small>
        </article>
      ))}
    </div>
  );
}
