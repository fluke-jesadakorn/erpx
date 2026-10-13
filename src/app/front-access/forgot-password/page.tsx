import { BetaAuthForm } from "@/components/collaboration/auth-form";
import styles from "../front-access.module.css";
export default function FrontForgotPassword() {
  return <main className={styles.page}><div className={styles.wrap}><p className={styles.eyebrow}>Staff account</p><h1 className={styles.title}>Reset your password</h1><section className={styles.card}><BetaAuthForm mode="forgot" frontMode /></section></div></main>;
}
