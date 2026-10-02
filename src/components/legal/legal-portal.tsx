"use client";
import { TimeText } from "@/components/time/time-preference-context";
import { useCallback, useEffect, useState } from "react";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import type { QuickAccessItem, QuickAccessPreference } from "@/lib/quick-access";
import rail from "@/components/quick-access/area-rail.module.css";
import s from "./legal.module.css";
type Shared = {
  shareId: string;
  kind: string;
  billing: {
    id: string;
    title: string;
    state: string;
    data: Record<string, string>;
  }[];
  doc: { name: string; body: string; version: number; checksum: string };
  comments: { body: string; created_at: number }[];
  envelopes: {
    id: string;
    state: string;
    consent: string;
    signer_state: string;
    expires_at: number;
  }[];
};
export function LegalPortal({
  slug,
  initial,
  initialShareId,
  quickAccessItems,
  quickAccessPreferences,
}: {
  slug: string;
  initial: Record<string, unknown>[];
  initialShareId?: string;
  quickAccessItems: QuickAccessItem[];
  quickAccessPreferences: QuickAccessPreference;
}) {
  const [selected, setSelected] = useState<Shared | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [body, setBody] = useState("");
  const api = `/api/legal-portal/${encodeURIComponent(slug)}`;
  const call = useCallback(async (path: string, input?: unknown) => {
    const r = await fetch(
      api + path,
      input
        ? {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": crypto.randomUUID(),
            },
            body: JSON.stringify(input),
          }
        : { cache: "no-store" },
    );
    const j = await r.json();
    if (!r.ok) throw new Error(j.error?.message ?? "Request failed.");
    return j.data;
  }, [api]);
  const open = useCallback(async (id: string) => {
    setError("");
    try {
      setSelected(await call(`/${encodeURIComponent(id)}`));
    } catch (e) {
      setError((e as Error).message);
      setSelected(null);
    }
  }, [call]);
  useEffect(() => {
    if (!initialShareId) return;
    let active = true;
    void call(`/${encodeURIComponent(initialShareId)}`)
      .then((data) => {
        if (active) {
          setSelected(data);
          setError("");
        }
      })
      .catch((error) => {
        if (active) setError(error instanceof Error ? error.message : "Request failed.");
      });
    return () => { active = false; };
  }, [initialShareId, call]);
  async function act(action: string, extra = {}) {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await call("", { action, shareId: selected.shareId, ...extra });
      await open(selected.shareId);
      setBody("");
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
    <a href="#legal-portal-content" className="skip-link">Skip to shared documents</a>
    <main className={s.workspace} style={{ padding: 32, maxWidth: 1360 }}>
      <header className={s.header}>
        <div>
          <span className={s.eyebrow}>ERPX · Secure collaboration</span>
          <h1>Legal portal</h1>
          <p>
            Only document versions explicitly shared with your account appear
            here.
          </p>
        </div>
      </header>
      <div className={rail.layout}>
      <aside className={rail.rail} aria-label="Quick access">
        <QuickAccessShelf
          title="Quick access"
          items={quickAccessItems}
          initialPreferences={quickAccessPreferences}
          defaultIds={quickAccessItems.slice(0, 6).map((item) => item.id)}
          endpoint={`${api}/quick-access`}
          requestMode="plain"
          embedded
          placement="rail"
          showAllMode={false}
        />
      </aside>
      <div id="legal-portal-content" tabIndex={-1} className={rail.content}>
      {error && (
        <div className={s.error} role="alert">
          {error}
        </div>
      )}
      {selected ? (
        <section className={s.panel}>
          <a className={s.button} href={`/legal-portal/${encodeURIComponent(slug)}`}>
            ← Shared documents
          </a>
          <h2>
            {selected.doc.name} · v{selected.doc.version}
          </h2>
          <div className={s.small}>SHA-256: {selected.doc.checksum}</div>
          <pre className={s.body}>{selected.doc.body}</pre>
          <a href={`${api}/${selected.shareId}/download`}>Download original</a>
          <div className={s.actions}>
            <button
              className={s.button}
              disabled={busy}
              onClick={() => act("accept")}
            >
              Accept invitation
            </button>
          </div>
          <label className={s.field}>
            Submit a revised PDF or DOCX (up to 10 MB)
            <input
              type="file"
              accept=".pdf,.docx"
              disabled={busy}
              onChange={async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                setBusy(true);
                setError("");
                try {
                  const r = await fetch(`${api}/${selected.shareId}/upload`, {
                    method: "POST",
                    headers: {
                      "Content-Type": file.type,
                      "X-File-Name": encodeURIComponent(file.name),
                      "Idempotency-Key": crypto.randomUUID(),
                    },
                    body: file,
                  });
                  const j = await r.json();
                  if (!r.ok)
                    throw new Error(j.error?.message ?? "Upload failed");
                  setBody("");
                  await open(selected.shareId);
                } catch (e) {
                  setError((e as Error).message);
                } finally {
                  setBusy(false);
                }
              }}
            />
          </label>
          <h3>Shared discussion</h3>
          {selected.comments.map((c, i) => (
            <p key={i} className={s.body}>
              {c.body}
            </p>
          ))}
          <label className={s.field}>
            Comment
            <textarea value={body} onChange={(e) => setBody(e.target.value)} />
          </label>
          <button
            className={s.button}
            disabled={busy || !body.trim()}
            onClick={() => act("comment", { body })}
          >
            Send comment
          </button>
          {selected.kind === "counsel" && (
            <details>
              <summary>Submit time or fees for review</summary>
              <form
                className={s.detail}
                onSubmit={(e) => {
                  e.preventDefault();
                  void act(
                    "billing.submit",
                    Object.fromEntries(new FormData(e.currentTarget)),
                  );
                }}
              >
                <label className={s.field}>
                  Type
                  <select name="entryType">
                    <option value="time">Time</option>
                    <option value="fee">Fee</option>
                    <option value="invoice">Invoice</option>
                  </select>
                </label>
                <label className={s.field}>
                  Work description
                  <textarea name="description" required minLength={3} />
                </label>
                <label className={s.field}>
                  Minutes
                  <input
                    name="minutes"
                    type="number"
                    min={0}
                    defaultValue={0}
                  />
                </label>
                <label className={s.field}>
                  Amount in engagement currency minor units
                  <input name="amount" type="number" min={0} defaultValue={0} />
                </label>
                <label className={s.field}>
                  Timesheet / invoice evidence
                  <textarea name="evidence" required minLength={3} />
                </label>
                <button className={s.button} disabled={busy}>
                  Submit for internal review
                </button>
              </form>
              {selected.billing.map((b) => (
                <p key={b.id}>
                  {b.title} · {b.state} · {b.data.minutes} minutes ·{" "}
                  {b.data.amount} minor units
                </p>
              ))}
            </details>
          )}
          {selected.envelopes.map((e) => (
            <div className={s.notice} key={e.id}>
              <h3>Signature · {e.state}</h3>
              <p className={s.body}>{e.consent}</p>
              <p>
                <a href="/my-access">Verify MFA</a> before signing.
              </p>
              {e.state === "sent" && e.signer_state === "pending" && (
                <div className={s.actions}>
                  <button
                    className={s.button}
                    disabled={busy}
                    onClick={() => act("view", { envelopeId: e.id })}
                  >
                    I reviewed this document
                  </button>
                  <button
                    className={s.button}
                    disabled={busy}
                    onClick={() =>
                      act("sign", { envelopeId: e.id, consent: e.consent })
                    }
                  >
                    Accept and sign
                  </button>
                  <button
                    className={s.button}
                    disabled={busy}
                    onClick={() => act("decline", { envelopeId: e.id })}
                  >
                    Decline
                  </button>
                </div>
              )}
            </div>
          ))}
        </section>
      ) : initialShareId && !error ? (
        <section className={s.panel} role="status" aria-busy="true">Opening shared document…</section>
      ) : (
        <section className={s.panel}>
          {initial.length ? (
            initial.map((x) => (
              <div className={s.event} key={String(x.id)}>
                <a className={s.link} href={`/legal-portal/${encodeURIComponent(slug)}?share=${encodeURIComponent(String(x.id))}`}>
                  {String(x.name)} · v{String(x.version)}
                </a>
                <p className={s.small}>
                  Access expires{" "}
                  <TimeText value={Number(x.expires_at)} />
                </p>
              </div>
            ))
          ) : (
            <div className={s.empty}>
              No documents have been shared with this verified account.
            </div>
          )}
        </section>
      )}
      </div>
      </div>
    </main>
    </>
  );
}
