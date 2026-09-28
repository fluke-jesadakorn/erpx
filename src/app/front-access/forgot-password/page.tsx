import { BetaAuthForm } from "@/components/beta/auth-form";
import styles from "../front-access.module.css";
export default function FrontForgotPassword() {
  return <main className={styles.page}><div className={styles.wrap}><p className={styles.eyebrow}>บัญชีพนักงาน</p><h1 className={styles.title}>กู้รหัสผ่าน</h1><section className={styles.card}><BetaAuthForm mode="forgot" frontMode /></section></div></main>;
}
