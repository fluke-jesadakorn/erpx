"use client";
import { useState } from "react";
import s from "./legal.module.css";
import type { LegalEvidenceSource } from "@/server/legal/knowledge";
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
      <form className={s.toolbar}
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
        <input
          aria-label="Knowledge search"
          placeholder="Search clauses, documents or a legal topic…"
          name="query"
          required
          minLength={2}
          maxLength={200}
        />
        <label className={s.field}>Applicable date (Gregorian)<input name="asOf" type="date" required defaultValue={asOf} /></label>
        <button className={s.button}>Search documents and laws</button>
      </form>
      {error && <p role="alert">{error}</p>}
      {data && <p>{data.notice}</p>}
      {data?.sources.map((d) => (
        <article className={s.event} key={d.sourceId}>
          <button className={s.button} onClick={() => void open(d.item_id)}>
            {d.title} · {d.name ?? d.revision_label ?? "Official source"} · v{d.version}
          </button>
          <p>{d.excerpt}</p>
          <p className={s.small}>{d.kind === "law" ? `Law revision ${d.revision_label} · Effective ${d.effective_from} · As of ${data!.asOf}` : "Reviewed document version"}{d.page ? ` · Page ${d.page}` : ""}{d.section ? ` · ${d.section}` : ""}</p>
          <a className={s.button} href={`${base}/legal/citations/${encodeURIComponent(d.kind === "law" ? `law:${d.id}` : d.sourceId)}?asOf=${encodeURIComponent(data!.asOf)}`}>Open citation and original</a>
          <small>
            {d.document_id} · SHA256 {d.checksum}
          </small>
        </article>
      ))}
    </div>
  );
}
