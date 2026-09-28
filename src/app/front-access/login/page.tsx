import { headers } from "next/headers";
import { frontAppForHost, safeFrontReturnPath } from "@/lib/front-host";
import { LoginForm } from "@/app/login/login-form";
import styles from "../front-access.module.css";

export default async function FrontLogin({ searchParams }: { searchParams: Promise<{ next?: string; reason?: string }> }) {
  const app = frontAppForHost((await headers()).get("host"));
  const query = await searchParams;
  const next = safeFrontReturnPath(query.next);
  return <main className={styles.page}><div className={styles.wrap}>
    <span className={styles.badge}>{app?.toUpperCase() ?? "Front"}</span>
    <p className={styles.eyebrow}>{app === "pms" ? "PMS · งานโรงแรม" : "POS · งานหน้าร้าน"}</p>
    <h1 className={styles.title}>เข้าสู่ระบบพนักงาน</h1>
    <p className={styles.lead}>ใช้บัญชีพนักงานเดิมของคุณเพื่อเริ่มงานบนเครื่องนี้</p>
    <section className={`${styles.card} ${styles.form}`}>
      {query.reason && <p role="status">เซสชันหมดอายุ กรุณาเข้าสู่ระบบอีกครั้ง</p>}
      <LoginForm next={next} frontMode />
      <a className={styles.back} href="/forgot-password">ลืมรหัสผ่าน?</a>
    </section>
  </div></main>;
}
