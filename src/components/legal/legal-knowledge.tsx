"use client";
import { useState } from "react";
import s from "./legal.module.css";
export function LegalKnowledge({
  call,
  open,
}: {
  call: (path: string) => Promise<unknown>;
  open: (id: string) => Promise<void>;
}) {
  const [data, setData] = useState<{
      notice: string;
      sources: {
        item_id: string;
        document_id: string;
        title: string;
        name: string;
        version: number;
        checksum: string;
        excerpt: string;
      }[];
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
                `/knowledge?q=${encodeURIComponent(String(q))}`,
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
        <button className={s.button}>Search all registers</button>
      </form>
      {error && <p role="alert">{error}</p>}
      {data && <p>{data.notice}</p>}
      {data?.sources.map((d) => (
        <article className={s.event} key={d.document_id}>
          <button className={s.button} onClick={() => void open(d.item_id)}>
            {d.title} · {d.name} v{d.version}
          </button>
          <p>{d.excerpt}</p>
          <small>
            {d.document_id} · SHA256 {d.checksum}
          </small>
        </article>
      ))}
    </div>
  );
}
