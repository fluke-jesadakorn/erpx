"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useCallback, useEffect, useRef, useState } from "react";
import { Card, FormLayout } from "@erpx/ui/components/foundation";
import {createAccountSecurityHttpPort} from '@erpx/feature-identity_access/public/client';
import {contextGeneration} from '@erpx/feature-work_projection/lib/context-switch';
type EmergencySeed={access:{companyId:string;holdingSlug:string|null;scopeLabel:string;reason:string;expiresAt:number}[];unavailableScopes:string[]};
export function AccountSecurity({initialEmergency}:{initialEmergency?:EmergencySeed}) {
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
  const mfa=useRef(createAccountSecurityHttpPort(holdingFetch)),mfaLifecycle=useRef({epoch:0,active:true}),[invalidated,setInvalidated]=useState(false);
  useEffect(()=>{
    const state=mfaLifecycle.current;state.active=true;
    const changed=()=>{state.epoch++;setSecret('');setInvalidated(true);setBusy(false);setError('Your account changed. Reload the page before continuing.');};
    window.addEventListener('erpx:context-switch-started',changed);window.addEventListener('erpx:work-context-changed',changed);
    return()=>{state.active=false;state.epoch++;window.removeEventListener('erpx:context-switch-started',changed);window.removeEventListener('erpx:work-context-changed',changed);};
  },[]);
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
    if(busy||invalidated)return;
    const generation=contextGeneration(),epoch=++mfaLifecycle.current.epoch;
    const current=()=>mfaLifecycle.current.active&&mfaLifecycle.current.epoch===epoch&&contextGeneration()===generation;
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
      if(path==='mfa/enroll'||path==='mfa/verify'){
        if(path==='mfa/enroll'){
          const result=await mfa.current.enroll();if(!current())return;setSecret(result.secret);
        }else{
          if(!body||typeof body!=='object'||!('code'in body)||typeof body.code!=='string')throw Error('Enter a six-digit authenticator code.');
          await mfa.current.verify({code:body.code});if(!current())return;setSecret('');
        }
        window.dispatchEvent(new CustomEvent('erpx:account-security-updated'));
        setMessage(path==='mfa/enroll'?'Add the setup key to your authenticator, then verify a code.':'Authenticator verified. Sensitive actions are available for five minutes.');return;
      }
      const r = await holdingFetch("/api/v1/account/iam/" + path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...(idempotencyKey ? { "Idempotency-Key": idempotencyKey } : {}),
          },
          body: JSON.stringify(body),
        }),
        v = await r.json();
      if(!current())return;
      if (!r.ok) throw new Error(v.error?.message ?? "Request failed.");
      if (emergencyCommand) {
        commandKeys.current.delete(commandIdentity);
        await refreshEmergencyAccess();
      }
      setMessage(
        v.data.message ??
          "Authenticator verified. Sensitive actions are available for five minutes.",
      );
    } catch (e) {
      if(current())setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      if(current())setBusy(false);
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

          disabled={busy||invalidated}
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
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||invalidated}>
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
