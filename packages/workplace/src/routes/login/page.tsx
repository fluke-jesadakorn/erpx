import {deviceAccountReturn} from "@erpx/server-identity_access/public/rules/device-account-return";
import {DeviceAccountLoginChoices} from '@erpx/feature-identity_access/components/device-account-login-choices';
import { InlineNotice } from "@erpx/ui/components/foundation";
import { ActionAnchor } from "@erpx/ui/components/action-anchor";
import { ArrowRight, Clock3, Grid2X2 } from "lucide-react";

import {routedConnection} from '@erpx/server-identity_access/public/directory-lookup';
import { DirectoryLogin } from "@erpx/workplace/routes/login/directory-login";
import { LoginForm } from "@erpx/workplace/routes/login/login-form";
import { AuthShell } from "@erpx/workplace/components/auth-shell";
import styles from "@erpx/workplace/components/login.module.css";
import { safeReturnPath } from "@erpx/feature-shared/lib/access-problem";
import { publicBetaEnabled } from "@erpx/server-runtime/public/presentation-configuration";
import { headers } from "next/headers";
import { configuredProductHandoff, configuredProductOrigin, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { isProductHostKey, product } from "@erpx/server-commercial/public/product-registry";
import {accountWorkspaceLoginReturn} from '@erpx/server-identity_access/public/rules/account-workspace';
import {legalEnrollmentReturnPath} from '@erpx/server-identity_access/public/rules/public-surface-entry';

export default async function LoginPage({ searchParams }: { searchParams: Promise<{ reason?: string; next?: string; connection?:string; sso?: string; state?: string; challenge?: string; app?: string }> }) {
  const query = await searchParams;
  const account = productHostForHost((await headers()).get("host")) === "account";
  if (account && query.reason === "sso_expired") {
    const apps = configuredProductHandoff("apps")!;
    const retryApp = query.app && query.app !== "account" && isProductHostKey(query.app) ? query.app : null;
    const retry = retryApp ? configuredProductHandoff(retryApp) : null;
    const label = retryApp ? product(retryApp)?.label.en ?? (retryApp === "apps" ? "Apps" : "your app") : null;
    return <AuthShell eyebrow="SIGN-IN RECOVERY" title="Let’s get you back in." description="This sign-in link has expired or could not be verified. You can start again with a fresh link.">
      <div className={styles.recoveryNotice} role="status">
        <span className={styles.recoveryIcon}><Clock3 size={22} aria-hidden="true" /></span>
        <div><strong>{retry ? `Continue to ${label}` : "Start again from Apps"}</strong><p>{retry ? "Try signing in again to return to your app." : "Open Apps and choose where you want to work. We’ll create a new sign-in link for you."}</p></div>
      </div>
      <div className={styles.recoveryActions}>
        <ActionAnchor appearance="standard" variant="primary" href={retry ?? apps}>{retry ? "Try sign-in again" : "Open Apps"}<ArrowRight size={17} aria-hidden="true" /></ActionAnchor>
        <ActionAnchor appearance="standard" href={retry ? apps : "/login"}>{retry ? <><Grid2X2 size={17} aria-hidden="true" />Open Apps</> : "Sign in to Account"}</ActionAnchor>
      </div>
      <small>If this keeps happening, allow cookies for ERPX and start sign-in from the app you want to open.</small>
    </AuthShell>;
  }
  const transaction = account && /^[a-f0-9-]{36}$/.test(query.sso ?? "") && /^[A-Za-z0-9_-]{43}$/.test(query.state ?? "") && /^[A-Za-z0-9_-]{43}$/.test(query.challenge ?? "");
  const next = transaction ? `/sso/authorize?${new URLSearchParams({ transaction: query.sso!, state: query.state!, challenge: query.challenge! })}` : account ? accountWorkspaceLoginReturn(query.next)??deviceAccountReturn(query.next)??legalEnrollmentReturnPath(query.next) : safeReturnPath(query.next);
  let directory=null;if(query.connection&&/^[a-f0-9-]{36}$/i.test(query.connection)){try{const {connection}=await routedConnection(query.connection);if(connection.status==="active")directory=connection;}catch{/* Unavailable sign-in options are omitted. */}}
  const registration = !account && await publicBetaEnabled();
  return <AuthShell eyebrow={account ? "ERPX ACCOUNT" : "ERPX CONSOLE"} title="Welcome back." description="Sign in to your workspace and pick up where you left off.">
    {query.reason === "sign_in" && <InlineNotice tone="info" className={styles.notice} role="status">Your session has expired. Sign in to continue to the requested page.</InlineNotice>}
    {directory&&<DirectoryLogin id={directory.id} name={directory.name} mode={directory.config.mode} next={next??"/"}/>}
    <LoginForm appearance="standard" next={next ?? undefined} />
    {account&&<DeviceAccountLoginChoices next={next??'/'}/>}
    <div className={styles.links}>{registration && <a href="/signup">Create an account</a>}<a href="/forgot-password">Forgot password?</a><a href="/verify-email">Verify email</a></div>
    <small>Access is based on your assigned workspace and role.</small>
  </AuthShell>;
}
