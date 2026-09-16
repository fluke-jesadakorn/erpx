"use client";
import { useState } from "react";
export function AccountSecurity() {
  const [message, setMessage] = useState(""),
    [secret, setSecret] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(path: string, body: unknown) {
    setBusy(true);
    setError("");
    try {
      const r = await fetch("/api/v1/account/iam/" + path, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        }),
        v = await r.json();
      if (!r.ok) throw new Error(v.error?.message ?? "Request failed.");
      if (v.data.secret) setSecret(v.data.secret);
      setMessage(
        v.data.message ??
          "Authenticator verified. Sensitive actions are available for five minutes.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <div role="status">{message}</div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <section className="card">
        <h2>Authenticator</h2>
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => send("mfa/enroll", {})}
        >
          Set up authenticator
        </button>
        {secret && (
          <p>
            Setup key: <code>{secret}</code>
          </p>
        )}
        <form
          className="business-form"
          onSubmit={(e) => {
            e.preventDefault();
            void send("mfa/verify", {
              code: new FormData(e.currentTarget).get("code"),
            });
          }}
        >
          <label>
            Six-digit code
            <input
              name="code"
              autoComplete="one-time-code"
              inputMode="numeric"
              pattern="[0-9]{6}"
              required
            />
          </label>
          <button className="button" disabled={busy}>
            Verify code
          </button>
        </form>
      </section>
      <section className="card">
        <h2>Temporary emergency access</h2>
        <p>
          Only independently designated local recovery administrators can
          activate access. Every activation is audited and expires within one
          hour.
        </p>
        <form
          className="business-form"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send("emergency/activate", {
              holdingSlug: f.get("holdingSlug") || undefined,
              companyId: f.get("companyId"),
              reason: f.get("reason"),
            });
          }}
        >
          <label>
            Holding slug (blank for platform)
            <input name="holdingSlug" />
          </label>
          <label>
            Company ID (or platform)
            <input name="companyId" required />
          </label>
          <label>
            Recovery reason
            <textarea name="reason" required minLength={10} />
          </label>
          <button className="button" disabled={busy}>
            Activate temporary access
          </button>
        </form>
      </section>
    </>
  );
}
