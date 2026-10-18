import {configuredProductHandoff} from '@/lib/runtime/product-hosts';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Suspense, type ReactNode } from "react";
import Link from "@/components/workplace/workspace-link";
import { workAttention } from "@/lib/work_projection/work-attention";
import { WorkAttention } from "@/components/workplace/work-attention";
import { StartActions } from "@/components/work_projection/today/start-actions";
import { HomeDay } from "@/components/work_projection/home/home-day";
import { HomeVisualPrototype } from "@/components/work_projection/home/home-visual-prototype";
import { TodayTools, HomePriorities, MobileAttentionCount, MobileAttentionNotice } from "@/components/collaboration/social-home-controls";
import styles from "@/components/collaboration/social.module.css";

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
