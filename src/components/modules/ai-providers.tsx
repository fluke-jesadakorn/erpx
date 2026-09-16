"use client";

import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { AiConnectionForm } from "./ai-connection-form";
import { Sparkles, Plus, ShieldCheck, MessageSquare, ScanLine, Activity, ArrowUpRight, SlidersHorizontal, Plug } from "lucide-react";
import styles from "./ai-providers.module.css";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { useHydrated } from "@/components/ui/use-hydrated";
import type {
  AiConnectionView,
  AiProviderOverview,
} from "@/server/ai-providers";

export function AiProviders({
  base,
  data,
  canBilling = false,
}: {
  base: string;
  data: AiProviderOverview;
  canBilling?: boolean;
}) {
  const router = useRouter(),
    ready = useHydrated(),
    attempt = useRef<{ body: string; path: string; key: string } | null>(null);
  const [edit, setEdit] = useState<AiConnectionView | "new" | null>(null),
    [remove, setRemove] = useState<{
      row: AiConnectionView;
      action: "delete" | "disable";
    } | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  async function command(path: string, input: unknown) {
    const body = JSON.stringify(input);
    if (attempt.current?.body !== body || attempt.current.path !== path)
      attempt.current = { body, path, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    setMessage("");
    try {
      const response = await workFetch(`/api/v1${base}/ai-providers${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body,
      });
      const result = (await response.json()) as {
        data: {
          error?: string;
          textVerified?: boolean;
          imageVerified?: boolean;
        };
        error?: { message: string };
      };
      if (!response.ok)
        throw Error(result.error?.message ?? "Unable to save AI settings.");
      attempt.current = null;
      setMessage(
        result.data.error ??
          (path.endsWith("/test")
            ? `Text test: ${result.data.textVerified ? "passed" : "failed"}. Image test: ${result.data.imageVerified ? "passed" : "not verified"}.`
            : "Settings saved."),
      );
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save AI settings.");
      return false;
    } finally {
      setPending(false);
    }
  }
  const active = data.connections.filter(
    (c) => c.active_revision && !c.disabled,
  );
  return (
    <div className={styles.page}>
      <header className={styles.hero}>
        <div className={styles.heroTitle}><span className={styles.heroIcon}><Sparkles size={25} /></span><div><span className={styles.eyebrow}>WORKSPACE INTELLIGENCE</span><h2>AI, connected to your work.</h2><p>Manage the models that help your team move work forward.</p></div></div>
        <span className={styles.security}><ShieldCheck size={16} /> {data.keyStorageReady ? "Encrypted connections" : "Credential storage needs setup"}</span>
      </header>
      <div className={styles.layout}>
      <section className={styles.connections}>
        <div className="section-head">
          <div>
            <h2>Connections <span className={styles.count}>{data.connections.length}</span></h2>
            <p>
              Connect a provider, test your model, then activate it.
            </p>
          </div>
          <button
            className="button"
            disabled={!ready || pending}
            onClick={() => {
              setEdit("new");
              setError("");
            }}
          >
            <Plus size={16} /> Add connection
          </button>
        </div>
        {!data.keyStorageReady && (
          <p role="status" className={styles.notice}>
            Encrypted credential storage needs deployment configuration before
            API keys can be saved.
          </p>
        )}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {message && <p role="status" className={styles.notice}><Activity size={16} />{message}</p>}
        {!data.connections.length && (
          <div className={styles.empty}><Plug size={30} /><h3>Your first connection starts here</h3><p>Add an AI provider to help your team with conversations and documents.</p></div>
        )}
        {data.connections.map((c) => (
          <article className={styles.connection} key={c.id}>
            <div className="section-head">
              <div className={styles.providerTitle}><span className={styles.providerIcon}><Plug size={21} /></span><div><h3>{c.name}</h3><p className={styles.endpoint}>{c.base_url}</p></div></div>
              <span className={`${styles.status} ${c.disabled ? styles.disabled : c.active_revision ? styles.active : styles.draft}`}>
                {c.disabled
                  ? "Disabled"
                  : c.active_revision
                    ? "Active"
                    : "Draft"}
              </span>
            </div>
            {c.active && c.active_revision !== c.draft_revision && (
              <div className="notice">
                <strong>Currently in use</strong>
                <p>{c.active.base_url}</p>
                <p>
                  Text: {c.active.text_model} · Images:{" "}
                  {c.active.image_model || "Not configured"}
                </p>
                <p>The edits below are a draft until you activate them.</p>
              </div>
            )}
            <div className={styles.models}>
              <div><span className={styles.modelLabel}><MessageSquare size={15} /> Text model</span><strong>{c.text_model}</strong><span className={c.text_verified ? styles.verified : styles.caption}>{c.text_verified ? "✓ Connection verified" : "Awaiting connection test"}</span></div>
              <div><span className={styles.modelLabel}><ScanLine size={15} /> Document reading</span><strong>{c.image_model || "Not configured"}</strong><span className={c.image_verified ? styles.verified : styles.caption}>{c.image_model ? c.image_verified ? "✓ Image reading verified" : "Awaiting image test" : "Add an image model to read documents"}</span></div>
            </div>
            {!c.image_model && <button type="button" className="button secondary" disabled={pending || !ready} onClick={() => { setEdit({ ...c, image_model: c.text_model, image_protocol: c.text_protocol }); setError(""); }}><ScanLine size={14} /> Use main model to read receipts</button>}
            <div className={styles.metadata}><span><ShieldCheck size={14} />{c.hasKey ? "API key secured" : "No API key"}</span><span>Draft v{c.draft_revision}{c.active_revision ? ` · Active v${c.active_revision}` : ""}</span></div>
            {c.test_error && <p role="note">{c.test_error}</p>}
            <div className={styles.actions}>
              <button
                className="button secondary"
                disabled={pending}
                onClick={() => {
                  setEdit(c);
                  setError("");
                }}
              >
                <SlidersHorizontal size={14} /> Edit connection
              </button>
              <button
                className="button secondary"
                disabled={pending}
                onClick={() =>
                  void command(`/${c.id}/test`, { expectedVersion: c.version })
                }
              >
                Test connection
              </button>
              <button
                className="button"
                disabled={
                  pending ||
                  !c.text_verified ||
                  (!!c.image_model && !c.image_verified)
                }
                onClick={() =>
                  void command(`/${c.id}/activate`, {
                    expectedVersion: c.version,
                  })
                }
              >
                Activate
              </button>
              <details className={styles.more}><summary aria-label={`More actions for ${c.name}`}>More <span aria-hidden="true">⌄</span></summary><div><button type="button" aria-label="Disable connection" disabled={pending} onClick={() => setRemove({ row: c, action: "disable" })}>Disable connection</button><button type="button" aria-label="Delete connection" disabled={pending} onClick={() => setRemove({ row: c, action: "delete" })}>Delete connection</button></div></details>
            </div>
          </article>
        ))}
      </section>
      <aside className={styles.sidebar}>
      <section className={styles.defaults}>
        <div className={styles.panelHeading}><SlidersHorizontal size={18} /><h2>Company defaults</h2></div>
        <p>
          Choose how AI is used across your company.
        </p>
        <form
          key={data.selection.version}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void command("/selection", {
              expectedVersion: data.selection.version,
              mode: f.get("mode"),
              textConnectionId: f.get("textConnectionId") || null,
              imageMode: f.get("imageMode"),
              imageConnectionId: f.get("imageConnectionId") || null,
            });
          }}
        >
          <div className={styles.defaultFields}>
            <label>
              AI mode
              <select name="mode" defaultValue={data.selection.mode}>
                <option value="deployment">
                  Deployment default
                  {data.deploymentAvailable ? "" : " · not configured"}
                </option>
                <option value="custom">Company connections</option>
                <option value="disabled">Disabled</option>
              </select>
            </label>
            <label>
              Default text connection
              <select
                name="textConnectionId"
                defaultValue={data.selection.text_connection_id ?? ""}
              >
                <option value="">Choose connection</option>
                {active.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Document images
              <select name="imageMode" defaultValue={data.selection.image_mode}>
                <option value="default">
                  Use default connection
                </option>
                <option value="custom">Use separate connection</option>
                <option value="disabled">Manual entry only</option>
              </select>
            </label>
            <label>
              Document image connection
              <select
                name="imageConnectionId"
                defaultValue={data.selection.image_connection_id ?? ""}
              >
                <option value="">Choose connection</option>
                {active.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <button className="button" disabled={pending || !ready}>
            Save defaults
          </button>
        </form>
      </section>
      <section className={styles.usage}>
        <div className={styles.panelHeading}><Activity size={18} /><h2>Usage</h2><span className={styles.caption}>Last 30 days</span></div>
        <div className={styles.usageStats}><div><strong>{data.usage.reduce((total, u) => total + u.requests, 0).toLocaleString()}</strong><span>Preparations</span></div><div><strong>{data.usage.reduce((total, u) => total + u.input_tokens + u.output_tokens, 0).toLocaleString()}</strong><span>Tokens used</span></div></div>
        {!data.usage.length ? (
          <p className={styles.caption}>Usage appears here after your team starts using AI.</p>
        ) : (
          data.usage.map((u, i) => (
            <div className="row" key={i}>
              <strong>
                {data.connections.find((c) => c.id === u.connection_id)?.name ??
                  "Deployment default"}{" "}
                · {u.model}
              </strong>
              <span>
                {u.requests} preparations · {u.input_tokens + u.output_tokens}{" "}
                tokens · {u.failures} failed
              </span>
            </div>
          ))
        )}
        {canBilling && (
          <Link className={styles.billingLink} href={`${base}/settings?section=usage`}>Usage & billing <ArrowUpRight size={15} /></Link>
        )}
      </section>
      <p className={styles.reviewNote}><ShieldCheck size={16} /> Your team stays in control. Every AI suggestion is reviewed before it changes your work.</p>
      </aside>
      </div>
      <Dialog
        title={edit === "new" ? "Add AI connection" : "Edit AI connection"}
        open={edit !== null}
        onClose={() => {
          if (!pending) setEdit(null);
        }}
      >
        {edit && <AiConnectionForm key={edit === "new" ? "new" : edit.id} edit={edit} base={base} pending={pending} error={error} save={async input => {
          if (await command(edit === "new" ? "" : `/${edit.id}`, input)) setEdit(null);
        }} />}
      </Dialog>
      <Dialog
        title={
          remove?.action === "delete"
            ? "Delete connection"
            : "Disable connection"
        }
        open={!!remove}
        onClose={() => {
          if (!pending) setRemove(null);
        }}
      >
        {remove && (
          <form
            onSubmit={async (e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              if (
                await command(`/${remove.row.id}/${remove.action}`, {
                  expectedVersion: remove.row.version,
                  disableUses: f.get("disableUses") === "on",
                })
              )
                setRemove(null);
            }}
          >
            <p>{remove.row.name}</p>
            <label>
              <input type="checkbox" name="disableUses" /> Disable AI
              capabilities currently using this connection
            </label>
            <p>You can select a replacement in Company defaults first.</p>
            {error && <p role="alert">{error}</p>}
            <button className="button" disabled={pending}>
              {remove.action === "delete"
                ? "Delete connection"
                : "Disable connection"}
            </button>
          </form>
        )}
      </Dialog>
    </div>
  );
}
