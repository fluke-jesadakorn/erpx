"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useSyncExternalStore, useState } from "react";
import s from "@erpx/feature-legal/components/legal.module.css";
const subscribe = (notify: () => void) => {
  window.addEventListener("hashchange", notify);
  return () => window.removeEventListener("hashchange", notify);
};
export function LegalEnrollment({centralEnrollmentHref}:{centralEnrollmentHref?:string}={}) {
  const token = useSyncExternalStore(
    subscribe,
    () => new URLSearchParams(window.location.hash.slice(1)).get("token") ?? "",
    () => "",
  );
  const [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false);
  if(centralEnrollmentHref)return <main className={s.workspace} style={{maxWidth:600,padding:32}}><h1>Accept your Legal invitation</h1><p>Continue on ERPX Account to create and verify the exact invited account. Your document is opened separately through its original invitation policy.</p><a href={token?`${centralEnrollmentHref}#token=${encodeURIComponent(token)}`:undefined} aria-disabled={!token}>Continue on ERPX Account</a>{!token&&<p>Open the complete invitation link from your email.</p>}</main>;
  return (
    <main className={s.workspace} style={{ maxWidth: 600, padding: 32 }}>
      <h1>Accept your Legal invitation</h1>
      <p>
        Create an account for the email address that received this invitation. Verify that email
        before opening shared documents. Signing also requires MFA.
      </p>
      <form
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            const v = Object.fromEntries(new FormData(e.currentTarget));
            const response = await holdingFetch("/api/legal-portal/enroll", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ token, ...v }),
            });
            const result = await response.json();
            setMessage(
              response.ok
                ? "Check your invited email address to verify the account and return to the portal."
                : (result.error?.message ?? result.message ?? "Unable to create the account."),
            );
          } catch {
            setMessage("Unable to complete enrollment. Try again.");
          } finally {
            setBusy(false);
          }
        }}
      >
        <Field appearance="standard" className={s.field}>
          Your name
          <TextInput appearance="standard" name="name" required maxLength={100} autoComplete="name" />
        </Field>
        <Field appearance="standard" className={s.field}>
          Choose a password
          <TextInput appearance="standard"
            name="password"
            type="password"
            required
            minLength={12}
            maxLength={200}
            autoComplete="new-password"
          />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy || !token}>
          Create invited account and send verification
        </Button>
      </form>
      <InlineNotice tone="info" role="status">
        {message || (!token ? "Open the complete invitation link from your email." : "")}
      </InlineNotice>
      <a href="/login">
        Already have an ERPX account? Sign in and open the portal link in your invitation.
      </a>
    </main>
  );
}
