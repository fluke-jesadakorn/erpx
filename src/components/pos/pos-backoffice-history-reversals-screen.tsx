"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./standard-pos-backoffice-console";


const RoomChargeReversalReviews = dynamic(() => import("@/components/shared/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));
export function BackofficePosConsoleHistoryReversalsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {erpBase,data} = context;
  return <FrontLocalizedTree>{<RoomChargeReversalReviews base={erpBase} owner={{userId:data.userId,companyId:data.companyId}}/>}</FrontLocalizedTree>;
}
