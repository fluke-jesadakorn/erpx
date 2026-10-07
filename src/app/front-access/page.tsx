import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { frontAppForHost, frontVisiblePath } from "@/lib/front-host";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { frontCompanyApp } from "@/server/modules/work_projection/public/front-app";
import { frontCompanyChoices, frontCompanyItems } from "@/server/modules/work_projection/public/front-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import styles from "./front-access.module.css";
import Link from "next/link";
import { consoleOrigin } from "@/lib/erpx-hosts";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";

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
  const erpHref = (await workspaceChoices()).length ? consoleOrigin()?.toString() : null;
  return <main className={styles.page}><div className={styles.wrap}>
    {erpHref && <a className={styles.erpLink} href={erpHref}>← Back to ERP</a>}
    <span className={styles.badge}>{app.toUpperCase()}</span>
    <p className={styles.eyebrow}>{app === "pms" ? "Hotel operations" : "Point of sale"}</p>
    <h1 className={styles.title}>Choose a company to begin</h1>
    <p className={styles.lead}>Choose a company and location your account can access. This device will remember your last location.</p>
    <a className={styles.skipLink} href="#front-company-list">Skip to company list</a>
    <div className={styles.pickerLayout}>
      <aside className={styles.quickAccessRail} aria-label="Company shortcuts">
        <QuickAccessShelf items={items} initialPreferences={preference} defaultIds={items.slice(0, 6).map((item) => item.id)} endpoint="/api/v1/front-access/quick-access" title="Quick access" requestMode="plain" locale="en" placement="rail" embedded />
      </aside>
      <section id="front-company-list" tabIndex={-1} className={styles.card}><div className={styles.list}>{companies.map((company) => <Link className={styles.choice} key={company.holding + company.slug} href={frontVisiblePath(company.holding,company.slug)}>{company.label}<span aria-hidden="true">→</span></Link>)}
        {!companies.length && <p>No available locations for this account. Contact your company administrator.</p>}</div></section>
    </div>
  </div></main>;
}
