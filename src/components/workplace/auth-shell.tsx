import { Card } from "@/components/ui/foundation";
import type { ReactNode } from "react";
import { ArrowUpRight, Check, Layers3 } from "lucide-react";
import { cookies } from "next/headers";
import { AppearanceSwitcher } from "@/components/brand/appearance-switcher";
import { marketingOrigin } from "@/lib/runtime/erpx-hosts";
import { preferredAccountAppearance } from "@/server/modules/identity_access/public/account-preference-snapshot";
import { ProductBrand } from "@/components/brand/product-brand";
import styles from "./login.module.css";

export async function AuthShell({ eyebrow, title, description, children }: { eyebrow: string; title: string; description: string; children: ReactNode }) {
  const site = marketingOrigin()?.toString();
  const theme = await preferredAccountAppearance((await cookies()).get("erpx_appearance")?.value);
  return <main className={styles.page}>
    <section className={styles.story} aria-label="ERPX introduction">
      <div className={styles.storyTop}><ProductBrand appearance="standard" inverse className={styles.brand} /><span>ONE CONNECTED WORKSPACE</span></div>
      <div className={styles.storyMiddle}><span className={styles.previewBadge}>EARLY-ACCESS PREVIEW</span><h2>Your day,<br/><em>more connected.</em></h2><p>Bring work, people, operations, and finance into focus in one shared place.</p>
        <div className={styles.storyCard}><div className={styles.cardTitle}><span className={styles.cardIcon}><Layers3 size={18}/></span><span>Workspace overview</span><ArrowUpRight size={16}/></div><div className={styles.storyRow}><span><Check size={13}/></span> Decisions with context <b>08</b></div><div className={styles.storyRow}><span><Check size={13}/></span> Work moving forward <b>24</b></div><small>Illustrative preview · Synthetic data</small></div>
      </div>
      <div className={styles.storyBottom}>ERPX · Work connected</div>
    </section>
    <Card as="section" appearance="standard" className={styles.panel}><div className={styles.formPanel}>
      <div className={styles.panelTop}><div className={styles.mobileBrand}><ProductBrand appearance="standard" className={styles.brand}/></div><AppearanceSwitcher appearance="standard" initialTheme={theme}/></div>
      <span className={styles.eyebrow}>{eyebrow}</span><h1>{title}</h1><p className={styles.lead}>{description}</p>
      {children}
      {site && <a className={styles.siteLink} href={site}>Explore ERPX <ArrowUpRight size={15}/></a>}
    </div></Card>
  </main>;
}
