import { BetaAuthForm } from "@/components/beta/auth-form";
import styles from "../front-access.module.css";
export default function FrontVerifyEmail() {
  return <main className={styles.page}><div className={styles.wrap}><p className={styles.eyebrow}>Staff account</p><h1 className={styles.title}>Verify your email</h1><section className={styles.card}><BetaAuthForm mode="verify" frontMode /></section></div></main>;
}
