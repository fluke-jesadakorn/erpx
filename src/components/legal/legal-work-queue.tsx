"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useEffect, useState } from "react";
import { labels, type LegalKind } from "@/lib/legal/model";
import s from "./legal.module.css";

type QueueItem = {
  id: string;
  kind: LegalKind;
  title: string;
  state: string;
  due_date: string | null;
  owner_id: string;
};
export function LegalWorkQueue({
  view,
  people,
  call,
  open,
}: {
  view: "mine" | "deadlines";
  people: { id: string; name: string }[];
  call: (path: string) => Promise<unknown>;
  open: (id: string) => Promise<void>;
}) {
  const [data, setData] = useState<{ deadlines: QueueItem[]; assigned: QueueItem[] } | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let current = true;
    call("/workspace")
      .then((value) => {
        if (current) setData(value as typeof data);
      })
      .catch((e) => {
        if (current) setError(e.message);
      });
    return () => {
      current = false;
    };
  }, [call]);
  const items = data?.[view === "mine" ? "assigned" : "deadlines"] ?? [];
  return (
    <section
      className={s.detail}
      aria-label={view === "mine" ? "My legal work" : "Legal deadlines"}
    >
      <h2>{view === "mine" ? "My legal work" : "Deadlines"}</h2>
      <p className={s.small}>
        Work visible in your current Company, branch, department and matter membership.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!data && !error && <InlineNotice tone="info" role="status">Loading legal work…</InlineNotice>}
      {data && !items.length && <p>No open work in this view.</p>}
      {items.map((item) => (
        <article className={s.event} key={item.id}>
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={s.link}
            onClick={() => void open(item.id)}
          >
            {item.title}
          </Button>
          <p>
            {labels[item.kind]} · {item.state.replaceAll("_", " ")} ·{" "}
            {people.find((person) => person.id === item.owner_id)?.name ?? "Assigned owner"}
          </p>
          <p className={s.small}>
            {item.due_date ? `Due ${item.due_date} · Asia/Bangkok` : "No due date recorded"}
          </p>
        </article>
      ))}
    </section>
  );
}
