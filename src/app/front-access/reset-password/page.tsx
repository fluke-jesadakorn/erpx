import { BetaAuthForm } from "@/components/beta/auth-form";
import styles from "../front-access.module.css";
export default async function FrontResetPassword({searchParams}:{searchParams:Promise<{token?:string;error?:string}>}) {
  const query=await searchParams;
  return <main className={styles.page}><div className={styles.wrap}><p className={styles.eyebrow}>Staff account</p><h1 className={styles.title}>Set a new password</h1><section className={styles.card}><BetaAuthForm mode="reset" token={query.token} invalid={Boolean(query.error)} frontMode/></section></div></main>;
}
