"use client";
import { lazy } from "react";
import { DeferredRegion } from "./deferred-region";
import { PlanLauncher } from "./tool-launchers";
const MyDayCard = lazy(() => import("@/components/self-service/home").then(module => ({ default: module.MyDayCard })));
export function HomeDay({ base, enabled }: { base: string; enabled: boolean }) {
  return <>
    <DeferredRegion label="My day" minHeight={260}>
      <MyDayCard base={base} enabled={enabled} plan={null} />
    </DeferredRegion>
    <PlanLauncher base={base} />
  </>;
}
