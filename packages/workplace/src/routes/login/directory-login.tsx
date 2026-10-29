"use client";
import {createAccountDirectoryHttpPort} from "@erpx/feature-identity_access/public/client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {deviceAccountReturn} from "@erpx/server-identity_access/public/rules/device-account-return";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import {accountWorkspaceLoginReturn} from '@erpx/server-identity_access/public/rules/account-workspace';
import {legalEnrollmentReturnPath} from '@erpx/server-identity_access/public/rules/public-surface-entry';
import { Card } from "@erpx/ui/components/foundation";
const directoryClient=createAccountDirectoryHttpPort();
export function DirectoryLogin({
  id,
  name,
  mode,
  next,
}: {
  id: string;
  name: string;
  mode: string;
  next: string;
}) {
  const [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    router = useRouter();
  async function signIn(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setPending(true);
    setError("");
    try {
      let redirect: string | null = null;
      if(mode === "oidc") {
        const response=await holdingFetch("/api/auth/sign-in/social",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({provider:"directory-"+id,callbackURL:next})});
        const result=await response.json();
        if(!response.ok)throw new Error(result.message??"Directory sign-in failed.");
        redirect=typeof result.url==='string'?result.url:null;
      } else {
        const result=await directoryClient.signIn({connectionId:id,username:String(f.get("username")??''),password:String(f.get("password")??'')});
        if(result.error)throw new Error(result.error.message);
      }
      if (redirect) window.location.assign(redirect);
      // The SSO exchange is single-use; do not follow it with RSC push/refresh.
      else if(next.startsWith('/sso/authorize?')||accountWorkspaceLoginReturn(next)===next||deviceAccountReturn(next)===next||legalEnrollmentReturnPath(next)===next)window.location.assign(next);
      else {
        router.push(next);
        router.refresh();
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Sign-in failed.");
    } finally {
      setPending(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{name}</h2>
      <p>Use your approved corporate identity.</p>
      <form className="form" onSubmit={signIn}>
        {mode === "ldaps" && (
          <>
            <Field appearance="standard" className="field">
              Directory username
              <TextInput appearance="standard" name="username" autoComplete="username" required />
            </Field>
            <Field appearance="standard" className="field">
              Directory password
              <TextInput appearance="standard" name="password" type="password" autoComplete="current-password" required />
            </Field>
          </>
        )}
        {error && (
          <InlineNotice tone="danger" className="error" role="alert">
            {error}
          </InlineNotice>
        )}
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
          {pending
            ? "Connecting…"
            : mode === "oidc"
              ? "Continue to corporate sign-in"
              : "Sign in with directory"}
        </Button>
      </form>
    </Card>
  );
}
