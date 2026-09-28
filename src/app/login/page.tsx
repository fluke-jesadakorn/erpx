import { routedConnection } from "@/server/iam-directories";
import { DirectoryLogin } from "./directory-login";
import { LoginForm } from "./login-form";
import { AuthShell } from "@/components/workplace/auth-shell";
import styles from "@/components/workplace/login.module.css";
import { safeReturnPath } from "@/lib/access-problem";
import { runtimeEnv } from "@/server/runtime";

export default async function LoginPage({ searchParams }: { searchParams: Promise<{ reason?: string; next?: string; connection?:string }> }) {
  const query = await searchParams;
  const next = safeReturnPath(query.next);
  let directory=null;if(query.connection&&/^[a-f0-9-]{36}$/i.test(query.connection)){try{const {connection}=await routedConnection(query.connection);if(connection.status==="active")directory=connection;}catch{/* Unavailable sign-in options are omitted. */}}
  const registration = (await runtimeEnv()).ERPX_PUBLIC_BETA === "true";
  return <AuthShell eyebrow="ERPX CONSOLE" title="Welcome back." description="Sign in to your workspace and pick up where you left off.">
    {query.reason === "sign_in" && <p className={styles.notice} role="status">Your session has expired. Sign in to continue to the requested page.</p>}
    {directory&&<DirectoryLogin id={directory.id} name={directory.name} mode={directory.config.mode} next={next??"/"}/>}
    <LoginForm next={next ?? undefined} />
    <div className={styles.links}>{registration && <a href="/signup">Create an account</a>}<a href="/forgot-password">Forgot password?</a><a href="/verify-email">Verify email</a></div>
    <small>Access is based on your assigned workspace and role.</small>
  </AuthShell>;
}
