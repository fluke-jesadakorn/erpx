import { Check, ArrowRight, ShieldCheck } from "lucide-react";
import { LoginForm } from "./login-form";
import styles from "@/components/workplace/login.module.css";
export default function LoginPage() {
  return (
    <main className={styles.page}>
      <section className={styles.story}>
        <div className={styles.brand}>
          <span>E</span>erpx.
        </div>
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
          <div className={styles.brand}>
            <span>E</span>erpx.
          </div>
          <div className="eyebrow">Your day starts here</div>
          <h1>Welcome back</h1>
          <p>Sign in to your workspace and pick up where you left off.</p>
          <LoginForm />
          <p><a href="/signup">Join the free beta</a> · <a href="/forgot-password">Forgot password?</a> · <a href="/verify-email">Verify email</a></p>
          <small>Access is based on your assigned workspace and role.</small>
        </div>
      </section>
    </main>
  );
}
