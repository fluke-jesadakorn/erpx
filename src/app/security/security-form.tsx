"use client";
import { useCallback, useEffect, useRef, useState } from "react";
export function AccountSecurity() {
  const [message, setMessage] = useState(""),
    [secret, setSecret] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [emergencyAccess, setEmergencyAccess] = useState<{
      companyId: string;
      holdingSlug: string | null;
      scopeLabel: string;
      reason: string;
      expiresAt: number;
    }[]>([]),
    [emergencyUnavailable, setEmergencyUnavailable] = useState<string[]>([]),
    [emergencyLoading, setEmergencyLoading] = useState(true),
    [emergencyError, setEmergencyError] = useState(""),
    commandKeys = useRef(new Map<string, string>());
  const refreshEmergencyAccess = useCallback(async () => {
    setEmergencyLoading(true);
    setEmergencyError("");
    try {
      const r = await fetch("/api/v1/account/iam/emergency/active", {
          cache: "no-store",
        }),
        v = await r.json();
      if (!r.ok)
        throw new Error(v.error?.message ?? "Unable to load temporary access.");
      setEmergencyAccess(v.data.access ?? []);
      setEmergencyUnavailable(v.data.unavailableScopes ?? []);
    } catch (e) {
      setEmergencyError(
        e instanceof Error ? e.message : "Unable to load temporary access.",
      );
    } finally {
      setEmergencyLoading(false);
    }
  }, []);
  useEffect(() => {
    let current = true;
    void fetch("/api/v1/account/iam/emergency/active", { cache: "no-store" })
      .then(async (r) => {
        const v = await r.json();
        if (!r.ok)
          throw new Error(v.error?.message ?? "Unable to load temporary access.");
        return v.data;
      })
      .then((data) => {
        if (!current) return;
        setEmergencyAccess(data.access ?? []);
        setEmergencyUnavailable(data.unavailableScopes ?? []);
      })
      .catch((e: unknown) => {
        if (current)
          setEmergencyError(
            e instanceof Error ? e.message : "Unable to load temporary access.",
          );
      })
      .finally(() => {
        if (current) setEmergencyLoading(false);
      });
    return () => {
      current = false;
    };
  }, []);
  async function send(path: string, body: unknown) {
    setBusy(true);
    setError("");
    const emergencyCommand = path.startsWith("emergency/"),
      commandIdentity = `${path}:${JSON.stringify(body)}`;
    let idempotencyKey: string | undefined;
    if (emergencyCommand) {
      idempotencyKey = commandKeys.current.get(commandIdentity);
      if (!idempotencyKey) {
        idempotencyKey = crypto.randomUUID();
        commandKeys.current.set(commandIdentity, idempotencyKey);
      }
    }
    try {
      const r = await fetch("/api/v1/account/iam/" + path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...(idempotencyKey ? { "Idempotency-Key": idempotencyKey } : {}),
          },
          body: JSON.stringify(body),
        }),
        v = await r.json();
      if (!r.ok) throw new Error(v.error?.message ?? "Request failed.");
      if (emergencyCommand) {
        commandKeys.current.delete(commandIdentity);
        await refreshEmergencyAccess();
      }
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
          activate access. Every activation and revocation is audited and
          expires within one hour.
        </p>
        <h3>Active in this browser session</h3>
        {emergencyLoading ? (
          <p role="status">Checking temporary access…</p>
        ) : emergencyError ? (
          <p role="alert" className="error">
            {emergencyError}
          </p>
        ) : emergencyAccess.length ? (
          <ul>
            {emergencyAccess.map((access) => (
              <li key={`${access.holdingSlug ?? "platform"}:${access.companyId}`}>
                <strong>{access.scopeLabel}</strong>
                <p>Reason: {access.reason}</p>
                <p>Expires: {new Date(access.expiresAt).toLocaleString()}</p>
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={() =>
                    void send("emergency/revoke", {
                      holdingSlug: access.holdingSlug ?? undefined,
                      companyId: access.companyId,
                      reason: "Temporary access is no longer required.",
                    })
                  }
                >
                  Deactivate for this session
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p>No temporary emergency access is active in this session.</p>
        )}
        {emergencyUnavailable.length > 0 && (
          <p role="status">
            Status could not be checked for: {emergencyUnavailable.join(", ")}
          </p>
        )}
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
