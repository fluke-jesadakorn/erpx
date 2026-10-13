"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";


const RoomChargeReversalReviews = dynamic(() => import("@/components/shared/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function BackofficePosConsoleHistoryReversalsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {erpBase} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={erpBase}/>}</FrontLocalizedTree>;
}
