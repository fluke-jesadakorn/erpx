import { InlineNotice } from "@erpx/ui/components/foundation";

import {routedConnection} from '@erpx/server-identity_access/public/directory-lookup';
import { DirectoryLogin } from "@erpx/workplace/routes/login/directory-login";
import { LoginForm } from "@erpx/workplace/routes/login/login-form";
import { AuthShell } from "@erpx/workplace/components/auth-shell";
import styles from "@erpx/workplace/components/login.module.css";
import { safeReturnPath } from "@erpx/feature-shared/lib/access-problem";
import { publicBetaEnabled } from "@erpx/server-runtime/public/presentation-configuration";
import { headers } from "next/headers";
import { productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import {accountWorkspaceLoginReturn} from '@erpx/server-identity_access/public/rules/account-workspace';
import {legalEnrollmentReturnPath} from '@erpx/server-identity_access/public/rules/public-surface-entry';

export default async function LoginPage({ searchParams }: { searchParams: Promise<{ reason?: string; next?: string; connection?:string; sso?: string; state?: string; challenge?: string }> }) {
  const query = await searchParams;
  const account = productHostForHost((await headers()).get("host")) === "account";
  const transaction = account && /^[a-f0-9-]{36}$/.test(query.sso ?? "") && /^[A-Za-z0-9_-]{43}$/.test(query.state ?? "") && /^[A-Za-z0-9_-]{43}$/.test(query.challenge ?? "");
  const next = transaction ? `/sso/authorize?${new URLSearchParams({ transaction: query.sso!, state: query.state!, challenge: query.challenge! })}` : account ? accountWorkspaceLoginReturn(query.next)??legalEnrollmentReturnPath(query.next) : safeReturnPath(query.next);
  let directory=null;if(query.connection&&/^[a-f0-9-]{36}$/i.test(query.connection)){try{const {connection}=await routedConnection(query.connection);if(connection.status==="active")directory=connection;}catch{/* Unavailable sign-in options are omitted. */}}
  const registration = !account && await publicBetaEnabled();
  return <AuthShell eyebrow={account ? "ERPX ACCOUNT" : "ERPX CONSOLE"} title="Welcome back." description="Sign in to your workspace and pick up where you left off.">
    {query.reason === "sign_in" && <InlineNotice tone="info" className={styles.notice} role="status">Your session has expired. Sign in to continue to the requested page.</InlineNotice>}
    {directory&&<DirectoryLogin id={directory.id} name={directory.name} mode={directory.config.mode} next={next??"/"}/>}
    <LoginForm appearance="standard" next={next ?? undefined} />
    <div className={styles.links}>{registration && <a href="/signup">Create an account</a>}<a href="/forgot-password">Forgot password?</a><a href="/verify-email">Verify email</a></div>
    <small>Access is based on your assigned workspace and role.</small>
  </AuthShell>;
}
