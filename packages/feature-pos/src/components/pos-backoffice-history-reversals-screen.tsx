"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";


const RoomChargeReversalReviews = dynamic(() => import("@erpx/feature-shared/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function BackofficePosConsoleHistoryReversalsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {erpBase,data} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={erpBase} owner={{userId:data.userId,companyId:data.companyId}}/>}</FrontLocalizedTree>;
}
