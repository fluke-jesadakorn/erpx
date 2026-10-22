import {configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Suspense, type ReactNode } from "react";
import Link from "@erpx/workplace/components/workspace-link";
import { workAttention } from "@erpx/feature-work_projection/lib/work-attention";
import { WorkAttention } from "@erpx/workplace/components/work-attention";
import { StartActions } from "@erpx/feature-work_projection/components/today/start-actions";
import { HomeDay } from "@erpx/feature-work_projection/components/home/home-day";
import { HomeVisualPrototype } from "@erpx/feature-work_projection/components/home/home-visual-prototype";
import { TodayTools, HomePriorities, MobileAttentionCount, MobileAttentionNotice } from "@erpx/feature-collaboration/components/social-home-controls";
import styles from "@erpx/feature-collaboration/components/social.module.css";

export function SocialHomeShell({ base, name, single, sourceOnly=false, calendarEnabled, contextKey, canRequestLegal, children }: {
  base: string; name: string; single: boolean; sourceOnly?:boolean; calendarEnabled: boolean;
  contextKey?: string; canRequestLegal: boolean; children: ReactNode;
}) {
  return (
    <HomeVisualPrototype surface="workspace" compactHero={!single}>
      <div className={`content ${styles.home}`}>
        <header className="beta-heading">
          <div>
            <span className="beta-hero-kicker">
              {sourceOnly?"Included conversation":<>Your workspace <span aria-hidden="true">/</span> Today</>}
            </span>
            <h1>{single ? "Team conversation" : "Today"}</h1>
            <p>
              {single
                ? "Follow this conversation."
                : `Good to see you, ${name.split(" ")[0]}. Catch up with your team.`}
            </p>
          </div>
          <div className="beta-hero-art" aria-hidden="true">
            <span className="beta-hero-orbit" />
            <span className="beta-hero-pane beta-hero-pane-back" />
            <span className="beta-hero-pane beta-hero-pane-front" />
          </div>
          {single ? <Link href={sourceOnly?configuredProductHandoff("apps")??"/":canonicalUiHref(`${base}/home`)}>{sourceOnly?"Back to Apps":"Back to Today"}</Link> : null}
        </header>
        {!single && <Suspense><MobileAttentionNotice base={base} /></Suspense>}
        <div className={single ? styles.singleLayout : styles.homeLayout}>
          {!single && (
            <TodayTools summary={<><Link href={canonicalUiHref(`${base}/work?view=actions`)}>Needs you <strong>
              <Suspense fallback="…"><MobileAttentionCount /></Suspense></strong></Link>
              {calendarEnabled && <Link href={canonicalUiHref(`${base}/calendar`)}>Calendar</Link>}</>}>
              <Suspense fallback={<WorkAttention base={base} attention={workAttention([])} compact rail loading />}>
                <HomePriorities base={base} />
              </Suspense>
              <div className={styles.railStart}>
                <StartActions base={base} rail canRequestLegal={canRequestLegal} />
              </div>
              <HomeDay key={contextKey ?? base} base={base} enabled={calendarEnabled} />
            </TodayTools>
          )}
          {children}
        </div>
      </div>
    </HomeVisualPrototype>
  );
}
