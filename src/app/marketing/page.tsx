import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";
import type { Metadata } from "next";
import Link from "next/link";
import { cookies, headers } from "next/headers";
import {
  ArrowRight,
  CalendarDays,
  Check,
  CircleDollarSign,
  Layers3,
  UsersRound,
} from "lucide-react";
import { AppearanceSwitcher } from "@/components/brand/appearance-switcher";
import { ProductBrand } from "@/components/brand/product-brand";
import { appearancePreference } from "@/lib/shared/appearance-preference";
import { marketingConsoleUrl, marketingIsIndexable } from "@/lib/runtime/erpx-hosts";
import styles from "./site.module.css";

export const dynamic = "force-dynamic";

export async function generateMetadata(): Promise<Metadata> {
  const indexable = marketingIsIndexable((await headers()).get("host"));
  const canonical = indexable ? "https://erpx.jesadakorn.com/" : "https://dev-erpx.jesadakorn.com/";
  return {
    title: "ERPX | Bring the whole day into view",
    description:
      "ERPX brings work, people, operations, and finance into one connected workspace. Explore the early-access preview.",
    alternates: { canonical },
    robots: { index: indexable, follow: indexable },
    openGraph: {
      title: "ERPX | Bring the whole day into view",
      description:
        "One connected place for work, people, operations, and finance. Early-access preview.",
      url: canonical,
      type: "website",
    },
  };
}

const areas = [
  {
    id: "work",
    number: "01",
    icon: Layers3,
    title: "Work",
    line: "Keep the next step in sight.",
    body: "Bring requests, decisions, and follow-through into a shared view, so teams can see what needs attention.",
  },
  {
    id: "people",
    number: "02",
    icon: UsersRound,
    title: "People",
    line: "Make room for better teamwork.",
    body: "Connect roles, schedules, and everyday people processes with the work happening around them.",
  },
  {
    id: "operations",
    number: "03",
    icon: CalendarDays,
    title: "Operations",
    line: "See the day as it unfolds.",
    body: "Give teams a clearer view of service, stock, and operational handoffs across their workspace.",
  },
  {
    id: "finance",
    number: "04",
    icon: CircleDollarSign,
    title: "Finance",
    line: "Bring context to every number.",
    body: "Follow spending, approvals, and financial activity alongside the decisions that move them forward.",
  },
];

function Preview() {
  return (
    <div
      className={styles.preview}
      aria-label="Illustrative ERPX console preview with synthetic data"
    >
      <div className={styles.previewBar}>
        <span className={styles.previewLogo}>E</span>
        <span>
          ERPX <b>/</b> Northstar Studio
        </span>
        <span className={styles.previewAvatar}>NS</span>
      </div>
      <div className={styles.previewBody}>
        <aside className={styles.previewNav}>
          <span className={styles.navActive}>Overview</span>
          <span>Work</span>
          <span>People</span>
          <span>Operations</span>
          <span>Finance</span>
        </aside>
        <div className={styles.previewContent}>
          <div className={styles.previewHeading}>
            <div>
              <small>MONDAY · SAMPLE WORKSPACE</small>
              <strong>A clearer day starts here.</strong>
            </div>
            <span>Overview</span>
          </div>
          <div className={styles.statGrid}>
            <div>
              <small>IN MOTION</small>
              <strong>24</strong>
              <span>items across teams</span>
            </div>
            <div>
              <small>READY TO REVIEW</small>
              <strong>08</strong>
              <span>decisions to make</span>
            </div>
            <div>
              <small>COMING UP</small>
              <strong>12</strong>
              <span>planned activities</span>
            </div>
          </div>
          <div className={styles.previewRows}>
            <div>
              <b>Today’s focus</b>
              <span>
                View work <ArrowRight size={13} />
              </span>
            </div>
            <div>
              <span className={styles.rowIcon}>✓</span>
              <span>Review the weekly team plan</span>
              <small>People</small>
            </div>
            <div>
              <span className={styles.rowIcon}>↗</span>
              <span>Confirm supplier request</span>
              <small>Operations</small>
            </div>
            <div>
              <span className={styles.rowIcon}>●</span>
              <span>Check pending expenses</span>
              <small>Finance</small>
            </div>
          </div>
        </div>
      </div>
      <span className={styles.sample}>Illustrative preview · Synthetic data</span>
    </div>
  );
}

export default async function MarketingPage() {
  const consoleHref = marketingConsoleUrl().toString();
  const theme = appearancePreference((await cookies()).get("erpx_appearance")?.value);
  return (
    <div className={styles.site}>
      <a className={styles.skip} href="#main">
        Skip to content
      </a>
      <header className={styles.header}>
        <div className={styles.container}>
          <Link href="/" className={styles.brandLink} aria-label="ERPX home">
            <ProductBrand className={styles.brand} />
          </Link>
          <nav aria-label="Sections">
            <a href="#work">Work</a>
            <a href="#people">People</a>
            <a href="#operations">Operations</a>
            <a href="#finance">Finance</a>
          </nav>
          <AppearanceSwitcher initialTheme={theme} compact />
          <UiActionAnchor variant="secondary" className={styles.headerAction} href={consoleHref}>
            Open console <ArrowRight size={16} />
          </UiActionAnchor>
        </div>
      </header>
      <main id="main">
        <section className={styles.hero}>
          <div className={styles.container}>
            <div className={styles.heroCopy}>
              <span className={styles.eyebrow}>
                <span className={styles.dot} /> EARLY-ACCESS PREVIEW
              </span>
              <h1>
                Bring the whole day <em>into view.</em>
              </h1>
              <p>
                Work moves better when everyone can see how it connects. ERPX gives your people,
                operations, and finances one place to move forward together.
              </p>
              <div className={styles.heroActions}>
                <UiActionAnchor
                  variant="primary"
                  className={styles.primaryAction}
                  href={consoleHref}
                >
                  Open console <ArrowRight size={18} />
                </UiActionAnchor>
                <UiActionAnchor variant="ghost" className={styles.textAction} href="#explore">
                  Explore ERPX <span aria-hidden="true">↓</span>
                </UiActionAnchor>
              </div>
              <div className={styles.heroNote}>
                <Check size={16} /> A connected workspace, taking shape with early users.
              </div>
            </div>
            <Preview />
          </div>
        </section>
        <section id="explore" className={styles.intro}>
          <div className={styles.container}>
            <span className={styles.sectionLabel}>ONE CONNECTED WORKSPACE</span>
            <h2>
              Less searching for context.
              <br />
              <span>More room to do good work.</span>
            </h2>
            <p>
              ERPX brings the parts of a working day closer together. Explore how teams can move
              from an update to a decision, and from a decision to action.
            </p>
          </div>
        </section>
        <section className={styles.areas} aria-label="Explore ERPX capabilities">
          <div className={styles.container}>
            <div className={styles.areaGrid}>
              {areas.map(({ id, number, icon: Icon, title, line, body }) => (
                <article id={id} className={styles.area} key={id}>
                  <div className={styles.areaTop}>
                    <span>{number} / 04</span>
                    <Icon size={25} strokeWidth={1.7} />
                  </div>
                  <h3>{title}</h3>
                  <strong>{line}</strong>
                  <p>{body}</p>
                </article>
              ))}
            </div>
          </div>
        </section>
        <section className={styles.closing}>
          <div className={styles.container}>
            <span className={styles.sectionLabel}>THE NEXT STEP</span>
            <h2>
              See the work behind
              <br />
              the work.
            </h2>
            <p>Take a look around the early-access console and see how the pieces fit together.</p>
            <UiActionAnchor variant="primary" className={styles.primaryAction} href={consoleHref}>
              Open console <ArrowRight size={18} />
            </UiActionAnchor>
          </div>
        </section>
      </main>
      <footer className={styles.footer}>
        <div className={styles.container}>
          <ProductBrand inverse className={styles.footerBrand} />
          <span>ERPX · Early-access preview</span>
          <a href={consoleHref}>
            Open console <ArrowRight size={15} />
          </a>
        </div>
      </footer>
    </div>
  );
}
