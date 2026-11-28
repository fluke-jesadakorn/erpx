"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@erpx/platform-front/components/front-language";
import type {BackofficePosConsoleContext} from "@erpx/pos-ui/components/standard-pos-backoffice-console";


const RoomChargeReversalReviews = dynamic(() => import("@erpx/platform-shared-ui/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function BackofficePosConsoleHistoryReversalsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {erpBase,data} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={erpBase} owner={{userId:data.userId,companyId:data.companyId}}/>}</FrontLocalizedTree>;
}
