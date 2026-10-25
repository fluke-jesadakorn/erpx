"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {FrontPosDailyContext} from "./pos-front-console";


const RoomChargeReversalReviews = dynamic(() => import("@/components/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function FrontPosDailyHistoryReversalsScreen({context}: {context: FrontPosDailyContext}) {
  const {api} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={api.slice("/api/v1".length)}/>}</FrontLocalizedTree>;
}
