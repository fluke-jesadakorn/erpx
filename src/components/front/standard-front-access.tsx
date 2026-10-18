import Link from "next/link";
import { LoginForm } from "@/app/login/login-form";
import { ActionAnchor, Badge, Card, InlineNotice, SectionHeader } from "@/components/ui/foundation";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import { frontVisiblePath, type FrontHostApp } from "@/lib/shared/front-host";
import type { QuickAccessItem, QuickAccessPreference } from "@/lib/work_projection/quick-access";
import type { FrontCompanyChoice } from "@/server/modules/work_projection/public/front-quick-access";
import styles from "./standard-front-access.module.css";

function FrontAccessHeader({ isPos, erpHref }: { isPos: boolean; erpHref?: string | null }) {
  return <header className={styles.header}>
    <span className={styles.brand}><span className={styles.mark} aria-hidden="true">{isPos ? "P" : "H"}</span><span>ERPX <strong>{isPos ? "POS" : "PMS"}</strong></span></span>
    {erpHref && <ActionAnchor appearance="standard" href={erpHref}>← Back to ERP</ActionAnchor>}
  </header>;
}

export function StandardFrontSignIn({ isPos, next, reason, erpHref }: {
  isPos: boolean;
  next: string;
  reason?: string;
  erpHref?: string | null;
}) {
  return <main className={styles.page} aria-labelledby="front-access-title">
    <div className={styles.frame}>
      <FrontAccessHeader isPos={isPos} erpHref={erpHref} />
      <div className={styles.signInLayout}>
        <section className={styles.introduction}>
          <Badge tone="info">{isPos ? "POINT OF SALE" : "PROPERTY MANAGEMENT"}</Badge>
          <h1 id="front-access-title">Ready for {isPos ? "the next sale" : "the next guest"} today?</h1>
          <p>Sign in with your staff account to pick up work at a location you are authorized to use.</p>
          <ol className={styles.features}><li>One ERPX account</li><li>Access by role and location</li></ol>
        </section>
        <Card appearance="standard" className={styles.signInCard} aria-labelledby="front-login-title">
          <SectionHeader><div><p className={styles.eyebrow}>WELCOME BACK</p><h2 id="front-login-title">Staff sign in</h2><p className={styles.description}>Enter your account details to start working on this device.</p></div></SectionHeader>
          {reason && <InlineNotice className={styles.notice} role="status">Your session has expired. Please sign in again.</InlineNotice>}
          <LoginForm appearance="standard" next={next} />
          <ActionAnchor appearance="standard" variant="ghost" className={styles.forgot} href="/forgot-password">Forgot password?</ActionAnchor>
        </Card>
      </div>
      <footer className={styles.footer}><span>ERPX · Work connected</span><span>Secure access by role</span></footer>
    </div>
  </main>;
}

export function StandardFrontCompanyPicker({ app, companies, items, preference, erpHref }: {
  app: FrontHostApp;
  companies: FrontCompanyChoice[];
  items: QuickAccessItem[];
  preference: QuickAccessPreference;
  erpHref?: string | null;
}) {
  return <main className={styles.page} aria-labelledby="front-access-title">
    <a className={styles.skipLink} href="#front-company-list">Skip to company list</a>
    <div className={styles.frame}>
      <FrontAccessHeader isPos={app === "pos"} erpHref={erpHref} />
      <SectionHeader className={styles.pickerHeader}><div><Badge tone="info">{app.toUpperCase()}</Badge><p className={styles.eyebrow}>{app === "pms" ? "Hotel operations" : "Point of sale"}</p><h1 id="front-access-title">Choose a company to begin</h1><p className={styles.description}>Choose a company and location your account can access. This device will remember your last location.</p></div></SectionHeader>
      <div className={styles.pickerLayout}>
        <aside className={styles.shortcuts} aria-label="Company shortcuts">
          <QuickAccessShelf appearance="standard" items={items} initialPreferences={preference} defaultIds={items.slice(0, 6).map((item) => item.id)} endpoint="/api/v1/front-access/quick-access" title="Quick access" requestMode="plain" locale="en" placement="rail" embedded />
        </aside>
        <Card appearance="standard" id="front-company-list" tabIndex={-1} aria-label="Available companies" className={styles.companyCard}>
          {companies.length ? <ul className={styles.companies}>{companies.map((company) => <li key={company.holding + company.slug}><Link className={styles.companyLink} href={frontVisiblePath(company.holding, company.slug)}><span>{company.label}</span><span aria-hidden="true">→</span></Link></li>)}</ul> : <InlineNotice>No available locations for this account. Contact your company administrator.</InlineNotice>}
        </Card>
      </div>
    </div>
  </main>;
}
