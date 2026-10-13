"use client";
import { lazy, Suspense, type ComponentProps } from "react";
import { ChevronDown, PenLine } from "lucide-react";
import { Button } from "@/components/ui/foundation";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { useToolActivation } from "@/components/work_projection/home/tool-activation";
import styles from "@/components/work_projection/today/handover-composer.module.css";
const Composer = lazy(() => import("@/components/collaboration/social-composer").then(module => ({ default: module.SocialComposer })));
const Handover = lazy(() => import("@/components/work_projection/today/handover-composer").then(module => ({ default: module.HandoverComposer })));
const Plan = lazy(() => import("@/components/work_projection/today/daily-plan").then(module => ({ default: module.DailyPlan })));
const loading = <p role="status">Opening editor…</p>;
export function ComposerLauncher(props: ComponentProps<typeof Composer>) {
  const { activated, activate } = useToolActivation("compose");
  return activated ? <Suspense fallback={loading}><Composer {...props} initialOpen /></Suspense> :
    <button type="button" className="ui-panel beta-composer beta-composer-launcher" onClick={activate}>
      <PersonAvatar name={props.name} /><span className="beta-composer-prompt"><strong>Share an update</strong>
      <small>Ask a question or share progress with your team</small></span>
      <PenLine className="beta-compose-icon" size={18} aria-hidden="true" />
      <ChevronDown className="beta-compose-chevron" size={16} aria-hidden="true" />
    </button>;
}
export function HandoverLauncher(props: ComponentProps<typeof Handover>) {
  const { activated, activate } = useToolActivation("handover");
  return activated ? <Suspense fallback={loading}><Handover {...props} initialOpen /></Suspense> :
    <section className={styles.handover} id="handover" aria-label="Pass on work">
      <h2>Pass on work</h2><Button appearance="standard" variant="ghost" type="button" onClick={activate}>Draft handover</Button>
    </section>;
}
export function PlanLauncher(props: ComponentProps<typeof Plan>) {
  const { activated, activate } = useToolActivation("plan");
  return activated ? <Suspense fallback={loading}><Plan {...props} initialOpen /></Suspense> :
    <section id="plan" tabIndex={-1} aria-label="Plan my day">
      <Button appearance="standard" variant="ghost" type="button" onClick={activate}>Plan my day</Button>
    </section>;
}
