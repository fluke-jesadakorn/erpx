"use client";
/* eslint-disable @next/next/no-html-link-for-pages -- Recovery needs document navigation to re-read access cookies and leave failed route boundaries. */
import { useSyncExternalStore } from "react";
import { ArrowRight, FileQuestion, LockKeyhole, RefreshCw, ShieldAlert, WifiOff } from "lucide-react";
import { accessProblems, type AccessProblem } from "@/lib/access-problem";
import styles from "./access-state.module.css";
import { ProductBrand } from "@/components/brand/product-brand";
import { FrontError } from "@/components/front/front-error";

function subscribe(callback: () => void) { window.addEventListener("online", callback); window.addEventListener("offline", callback); return () => { window.removeEventListener("online", callback); window.removeEventListener("offline", callback); }; }
export function AccessState({ problem, email, reference, embedded = false, retry, returnTo }: {
  problem: AccessProblem; email?: string; reference?: string; embedded?: boolean; retry?: () => void; returnTo?: string;
}) {
  const online = useSyncExternalStore(subscribe, () => navigator.onLine, () => true);
  const frontHost = useSyncExternalStore(() => () => {}, () => /^(?:(?:dev|stage)-)?(?:pms|pos)$/.test(window.location.hostname.split(".")[0]), () => false);
  if (frontHost) return <FrontError kind={problem === "not_found" ? "not-found" : "unavailable"} reference={reference} retry={retry} />;
  const reason = online ? problem : "offline";
  const content = accessProblems[reason];
  const Icon = reason === "offline" ? WifiOff : reason === "not_found" || reason === "workspace" ? FileQuestion : reason === "sign_in" ? LockKeyhole : ShieldAlert;
  const canRetry = ["unavailable", "offline", "conflict", "rate_limited"].includes(reason);
  const Surface = embedded ? "div" : "main";
  return <Surface className={`${styles.page} ${embedded ? styles.embedded : ""}`}>
    <section className={styles.card} aria-labelledby="access-state-title" data-access-reason={reason}>
      <a className={styles.brand} href="/" aria-label="ERPX home"><ProductBrand /></a>
      <div className={styles.icon}><Icon size={26} aria-hidden="true" /></div>
      <p className={styles.label}>{content.label}</p>
      <h1 id="access-state-title">{content.title}</h1>
      <p className={styles.description}>{content.description}</p>
      <div className={styles.next}><strong>What you can do</strong><p>{content.next}</p></div>
      {email && <p className={styles.identity}>Signed in as <strong>{email}</strong></p>}
      <div className={styles.actions}>
        {reason === "sign_in" ? <a className={styles.primary} href={`/login?reason=sign_in${returnTo ? `&next=${encodeURIComponent(returnTo)}` : ""}`}>Sign in <ArrowRight size={16} /></a>
          : reason === "available" && returnTo ? <a className={styles.primary} href={returnTo}>Open requested page <ArrowRight size={16} /></a>
          : canRetry ? <button className={styles.primary} onClick={() => retry ? retry() : window.location.reload()}><RefreshCw size={16} />Try again</button>
          : <a className={styles.primary} href="/">Open my workspace <ArrowRight size={16} /></a>}
        {canRetry && <a href="/">Open my workspace</a>}
        {!canRetry && !["sign_in", "available", "not_found"].includes(reason) && <button onClick={() => window.location.reload()}>Check access again</button>}
        {reason !== "sign_in" && <a href="/login">Sign in with another account</a>}
      </div>
      <p className={styles.reference}>Reference: {reference && /^[a-zA-Z0-9_-]{1,100}$/.test(reference) ? reference : `ERPX_${reason.toUpperCase()}`}</p>
    </section>
  </Surface>;
}
