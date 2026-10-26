"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useCallback, useEffect, useRef, useState } from "react";
import { Card, FormLayout } from "@erpx/ui/components/foundation";
import type {currentAccountSecurity} from '@erpx/server-identity_access/public/account-security';
export function AccountSecurity({initialEmergency}:{initialEmergency?:Awaited<ReturnType<typeof currentAccountSecurity>>['emergency']}) {
  const [message, setMessage] = useState(""),
    [secret, setSecret] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [emergencyAccess, setEmergencyAccess] = useState<
      {
        companyId: string;
        holdingSlug: string | null;
        scopeLabel: string;
        reason: string;
        expiresAt: number;
      }[]
    >(initialEmergency?.access??[]),
    [emergencyUnavailable, setEmergencyUnavailable] = useState<string[]>(initialEmergency?.unavailableScopes??[]),
    [emergencyLoading, setEmergencyLoading] = useState(initialEmergency===undefined),
    [emergencyError, setEmergencyError] = useState(""),
    commandKeys = useRef(new Map<string, string>());
  const refreshEmergencyAccess = useCallback(async () => {
    setEmergencyLoading(true);
    setEmergencyError("");
    try {
      const r = await holdingFetch("/api/v1/account/iam/emergency/active", {
          cache: "no-store",
        }),
        v = await r.json();
      if (!r.ok) throw new Error(v.error?.message ?? "Unable to load temporary access.");
      setEmergencyAccess(v.data.access ?? []);
      setEmergencyUnavailable(v.data.unavailableScopes ?? []);
    } catch (e) {
      setEmergencyError(e instanceof Error ? e.message : "Unable to load temporary access.");
    } finally {
      setEmergencyLoading(false);
    }
  }, []);
  useEffect(() => {
    if(initialEmergency!==undefined)return;
    let current = true;
    void holdingFetch("/api/v1/account/iam/emergency/active", { cache: "no-store" })
      .then(async (r) => {
        const v = await r.json();
        if (!r.ok) throw new Error(v.error?.message ?? "Unable to load temporary access.");
        return v.data;
      })
      .then((data) => {
        if (!current) return;
        setEmergencyAccess(data.access ?? []);
        setEmergencyUnavailable(data.unavailableScopes ?? []);
      })
      .catch((e: unknown) => {
        if (current)
          setEmergencyError(e instanceof Error ? e.message : "Unable to load temporary access.");
      })
      .finally(() => {
        if (current) setEmergencyLoading(false);
      });
    return () => {
      current = false;
    };
  }, [initialEmergency]);
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
      const r = await holdingFetch("/api/v1/account/iam/" + path, {
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
      if(path==='mfa/verify'||path==='mfa/enroll')window.dispatchEvent(new CustomEvent('erpx:account-security-updated'));
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
      <InlineNotice tone="info" role="status">{message}</InlineNotice>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Authenticator</h2>
        <Button appearance="standard"
          type="submit"
          variant="secondary"

          disabled={busy}
          onClick={() => send("mfa/enroll", {})}
        >
          Set up authenticator
        </Button>
        {secret && (
          <p>
            Setup key: <code>{secret}</code>
          </p>
        )}
        <FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send("mfa/verify", {
              code: new FormData(e.currentTarget).get("code"),
            });
          }}
        >
          <Field appearance="standard">
            Six-digit code
            <TextInput appearance="standard"
              name="code"
              autoComplete="one-time-code"
              inputMode="numeric"
              pattern="[0-9]{6}"
              required
            />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Verify code
          </Button>
        </FormLayout>
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Temporary emergency access</h2>
        <p>
          Only independently designated local recovery administrators can activate access. Every
          activation and revocation is audited and expires within one hour.
        </p>
        <h3>Active in this browser session</h3>
        {emergencyLoading ? (
          <InlineNotice tone="info" role="status">Checking temporary access…</InlineNotice>
        ) : emergencyError ? (
          <InlineNotice tone="danger" role="alert" className="error">
            {emergencyError}
          </InlineNotice>
        ) : emergencyAccess.length ? (
          <ul>
            {emergencyAccess.map((access) => (
              <li key={`${access.holdingSlug ?? "platform"}:${access.companyId}`}>
                <strong>{access.scopeLabel}</strong>
                <p>Reason: {access.reason}</p>
                <p>
                  Expires: <TimeText appearance="standard" value={access.expiresAt} />
                </p>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"

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
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <p>No temporary emergency access is active in this session.</p>
        )}
        {emergencyUnavailable.length > 0 && (
          <InlineNotice tone="info" role="status">Status could not be checked for: {emergencyUnavailable.join(", ")}</InlineNotice>
        )}
        <FormLayout
          className="ui-form-grid"
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
          <Field appearance="standard">
            Holding slug (blank for platform)
            <TextInput appearance="standard" name="holdingSlug" />
          </Field>
          <Field appearance="standard">
            Company ID (or platform)
            <TextInput appearance="standard" name="companyId" required />
          </Field>
          <Field appearance="standard">
            Recovery reason
            <TextArea appearance="standard" name="reason" required minLength={10} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Activate temporary access
          </Button>
        </FormLayout>
      </Card>
    </>
  );
}
