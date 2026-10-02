import { headers } from "next/headers";
import { frontAppForHost, safeFrontReturnPath } from "@/lib/front-host";
import { LoginForm } from "@/app/login/login-form";
import { currentSession } from "@/server/authorization";
import { workspaceChoices } from "@/server/workspace-choices";
import { consoleOrigin } from "@/lib/erpx-hosts";
import styles from "../front-access.module.css";

export default async function FrontLogin({ searchParams }: { searchParams: Promise<{ next?: string; reason?: string }> }) {
  const app = frontAppForHost((await headers()).get("host"));
  const query = await searchParams;
  const next = safeFrontReturnPath(query.next);
  const session = await currentSession().catch(() => null);
  const hasErpRole = session ? (await workspaceChoices().catch(() => [])).length > 0 : false;
  const erpHref = hasErpRole ? consoleOrigin()?.toString() : null;
  const isPos = app === "pos";
  return <main className={styles.loginPage}>
    <div className={styles.loginFrame}>
      <header className={styles.loginHeader}>
        <span className={styles.loginWordmark}><span className={styles.loginMark}>{isPos ? "P" : "H"}</span><span>ERPX <strong>{isPos ? "POS" : "PMS"}</strong></span></span>
        {erpHref && <a className={styles.erpLink} href={erpHref}>← Back to ERP</a>}
      </header>
      <div className={styles.loginLayout}>
        <section className={styles.loginIntro}>
          <span className={styles.loginKicker}>{isPos ? "POINT OF SALE" : "PROPERTY MANAGEMENT"}</span>
          <h1>Ready for<br/><em>{isPos ? "the next sale" : "the next guest"}</em><br/>today?</h1>
          <p>Sign in with your staff account to pick up work at a location you are authorized to use.</p>
          <div className={styles.loginFeatures}><span>01 <strong>One ERPX account</strong></span><span>02 <strong>Access by role and location</strong></span></div>
        </section>
        <section className={styles.loginCard} aria-labelledby="front-login-title">
          <span className={styles.loginCardIcon}>{isPos ? "POS" : "PMS"}</span>
          <p className={styles.loginCardKicker}>WELCOME BACK</p>
          <h2 id="front-login-title">Staff sign in</h2>
          <p className={styles.loginCardLead}>Enter your account details to start working on this device.</p>
          {query.reason && <p className={styles.loginNotice} role="status">Your session has expired. Please sign in again.</p>}
          <div className={styles.loginForm}><LoginForm next={next} /></div>
          <a className={styles.loginForgot} href="/forgot-password">Forgot password?</a>
        </section>
      </div>
      <footer className={styles.loginFooter}><span>ERPX · Work connected</span><span>Secure access by role</span></footer>
    </div>
  </main>;
}
