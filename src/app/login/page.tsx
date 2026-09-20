import { routedConnection } from "@/server/iam-directories";
import { DirectoryLogin } from "./directory-login";
import { Check, ArrowRight, ShieldCheck } from "lucide-react";
import { LoginForm } from "./login-form";
import styles from "@/components/workplace/login.module.css";
import { safeReturnPath } from "@/lib/access-problem";
import { ProductBrand } from "@/components/brand/product-brand";
export default async function LoginPage({ searchParams }: { searchParams: Promise<{ reason?: string; next?: string; connection?:string }> }) {
  const query = await searchParams;
  const next = safeReturnPath(query.next);
  let directory=null;if(query.connection&&/^[a-f0-9-]{36}$/i.test(query.connection)){try{const {connection}=await routedConnection(query.connection);if(connection.status==="active")directory=connection;}catch{/* Unavailable sign-in options are omitted. */}}
  return (
    <main className={styles.page}>
      <section className={styles.story}>
        <ProductBrand className={styles.brand} />
        <div>
          <div className="eyebrow" style={{ color: "var(--muted)" }}>
            A more connected way to work
          </div>
          <h1>
            Good work.
            <br />
            Greater clarity.
          </h1>
          <p>
            Bring your people, decisions, and everyday operations together. Make
            room for what comes next.
          </p>
          <div className={styles.art}>
            <div>
              <span>
                <Check size={14} />
              </span>
              Work, in focus
            </div>
            <ArrowRight size={16} />
            <div>
              <span>
                <Check size={14} />
              </span>
              Progress, together
            </div>
          </div>
        </div>
        <div className={styles.footer}>
          <ShieldCheck size={15} />
          Your workspace. Your people. Connected.
        </div>
      </section>
      <section className={styles.panel}>
        <div className={styles.formPanel}>
          <ProductBrand className={styles.brand} />
          <div className="eyebrow">Your day starts here</div>
          <h1>Welcome back</h1>
          <p>Sign in to your workspace and pick up where you left off.</p>
          {query.reason === "sign_in" && <p className={styles.notice} role="status">You are signed out, or your session has expired. Sign in to continue to the requested page.</p>}
          {directory&&<DirectoryLogin id={directory.id} name={directory.name} mode={directory.config.mode} next={next??"/"}/>}
          <LoginForm next={next ?? undefined} />
          <p><a href="/signup">Join the free beta</a> · <a href="/forgot-password">Forgot password?</a> · <a href="/verify-email">Verify email</a></p>
          <small>Access is based on your assigned workspace and role.</small>
        </div>
      </section>
    </main>
  );
}
