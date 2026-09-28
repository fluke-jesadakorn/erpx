import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { frontAppForHost, frontVisiblePath } from "@/lib/front-host";
import { currentSession } from "@/server/authorization";
import { frontCompanyApp } from "@/server/front-app";
import { frontCompanyChoices, frontCompanyItems } from "@/server/front-quick-access";
import { readQuickAccessPreferences } from "@/server/quick-access-preferences";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import styles from "./front-access.module.css";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function FrontStart({ searchParams }: { searchParams: Promise<{ choose?: string }> }) {
  const app = frontAppForHost((await headers()).get("host"));
  if (!app) redirect("/login");
  let session;
  try { session = (await currentSession()).session; } catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED") redirect("/login");
    throw error;
  }
  const chooseCompany = (await searchParams).choose === "1";
  const remembered = (await cookies()).get("erpx_front_device_target")?.value;
  if (remembered && !chooseCompany) {
    const parts = remembered.split("/").filter(Boolean);
    if (parts.length >= 3) {
      const entry = await frontCompanyApp(parts[0], parts[1], app).catch(() => null);
      if (entry && entry.locations.some((location) => remembered.includes("/" + location.path + "/"))) redirect(remembered);
    }
  }
  const companies = await frontCompanyChoices(app);
  if (companies.length === 1) redirect(frontVisiblePath(companies[0].holding, companies[0].slug));
  const items = frontCompanyItems(companies);
  const preference = await readQuickAccessPreferences(session.user.id, "front-picker", app);
  return <main className={styles.page}><div className={styles.wrap}>
    <span className={styles.badge}>{app.toUpperCase()}</span>
    <p className={styles.eyebrow}>{app === "pms" ? "งานโรงแรม" : "งานหน้าร้าน"}</p>
    <h1 className={styles.title}>เลือกบริษัทเพื่อเริ่มงาน</h1>
    <p className={styles.lead}>เลือกบริษัทและจุดงานที่บัญชีของคุณมีสิทธิ์ใช้งาน เครื่องนี้จะจำจุดงานล่าสุดไว้</p>
    <a className={styles.skipLink} href="#front-company-list">ข้ามไปยังรายชื่อบริษัท</a>
    <div className={styles.pickerLayout}>
      <aside className={styles.quickAccessRail} aria-label="ทางลัดไปยังบริษัท">
        <QuickAccessShelf items={items} initialPreferences={preference} defaultIds={items.slice(0, 6).map((item) => item.id)} endpoint="/api/v1/front-access/quick-access" title="เข้าถึงบริษัทด่วน" requestMode="plain" locale="th" placement="rail" embedded />
      </aside>
      <section id="front-company-list" tabIndex={-1} className={styles.card}><div className={styles.list}>{companies.map((company) => <Link className={styles.choice} key={company.holding + company.slug} href={frontVisiblePath(company.holding,company.slug)}>{company.label}<span aria-hidden="true">→</span></Link>)}
        {!companies.length && <p>ไม่พบจุดงานที่บัญชีนี้ใช้งานได้ กรุณาติดต่อผู้ดูแลบริษัท</p>}</div></section>
    </div>
  </div></main>;
}
