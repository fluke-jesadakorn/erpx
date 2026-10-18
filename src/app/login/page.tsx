import { InlineNotice } from "@/components/ui/foundation";
import { routedConnection } from "@/server/modules/identity_access/public/iam-directories";
import { DirectoryLogin } from "./directory-login";
import { LoginForm } from "./login-form";
import { AuthShell } from "@/components/workplace/auth-shell";
import styles from "@/components/workplace/login.module.css";
import { safeReturnPath } from "@/lib/shared/access-problem";
import { publicBetaEnabled } from "@/server/modules/runtime/public/presentation-configuration";
import { headers } from "next/headers";
import { productHostForHost } from "@/lib/runtime/product-hosts";
import {accountWorkspaceLoginReturn} from '@/server/modules/identity_access/public/rules/account-workspace';
import {legalEnrollmentReturnPath} from '@/server/modules/identity_access/public/rules/public-surface-entry';

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
