"use client";
import styles from "@/app/front-access/front-access.module.css";
import Link from "next/link";

export function FrontError({ kind, reference, retry }: { kind: "not-found" | "unavailable"; reference?: string; retry?: () => void }) {
  return <main className={styles.page}><div className={styles.wrap}>
    <span className={styles.badge}>Front</span>
    <section className={styles.card} role="alert">
      <p className={styles.eyebrow}>งานหน้างาน</p>
      <h1 className={styles.title}>{kind === "not-found" ? "ไม่พบจุดงานนี้" : "เปิดหน้างานไม่ได้ในขณะนี้"}</h1>
      <p className={styles.lead}>{kind === "not-found" ? "ตรวจสอบลิงก์หรือเลือกบริษัทและจุดงานใหม่ หากเคยใช้งานได้ กรุณาแจ้งผู้ดูแลบริษัทเพื่อตรวจสิทธิ์" : "ลองโหลดอีกครั้ง หากยังมีปัญหา กรุณาแจ้งผู้ดูแลพร้อมรหัสอ้างอิง"}</p>
      {reference && <p>รหัสอ้างอิง: {reference}</p>}
      {retry && <button type="button" onClick={retry}>ลองอีกครั้ง</button>}
      <Link className={styles.back} href="/">เลือกจุดงาน</Link>
    </section>
  </div></main>;
}
