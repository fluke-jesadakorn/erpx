"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@erpx/platform-front/components/front-language";
import type {FrontPosDailyContext} from "@erpx/pos-ui/components/pos-front-console";


const RoomChargeReversalReviews = dynamic(() => import("@erpx/platform-shared-ui/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function FrontPosDailyHistoryReversalsScreen({context}: {context: FrontPosDailyContext}) {
  const {api,data} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={api.slice("/api/v1".length)} owner={{userId:data.userId,companyId:data.companyId}}/>}</FrontLocalizedTree>;
}
