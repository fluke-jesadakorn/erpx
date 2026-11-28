"use client";
import { lazy } from "react";
import { DeferredRegion } from "@erpx/work-projection-ui/components/home/deferred-region";
import { PlanLauncher } from "@erpx/work-projection-ui/components/home/tool-launchers";
const MyDayCard = lazy(() => import("@erpx/people-payroll-ui/components/self-service/home").then(module => ({ default: module.MyDayCard })));
export function HomeDay({ base, enabled }: { base: string; enabled: boolean }) {
  return <>
    <DeferredRegion label="My day" minHeight={260}>
      <MyDayCard base={base} enabled={enabled} plan={null} />
    </DeferredRegion>
    <PlanLauncher base={base} />
  </>;
}
