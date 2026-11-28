"use client";
import type { LucideIcon } from "lucide-react";
import { HospitalityGroupedNavigation } from "@erpx/platform-shared-ui/components/hospitality/grouped-navigation";
import { useFrontLanguage } from "@erpx/platform-front/components/front-language";

/** Permission filtering and query changes belong to the native owner. */
export function StandardPosBackofficeNavigation({items,current,label,onSelect}:{items:{id:string;label:string;description:string;icon:LucideIcon}[];current:string;label:string;onSelect:(id:string)=>void}) {
  return <HospitalityGroupedNavigation app="pos" items={items} current={current} label={label} language={useFrontLanguage()} onSelect={onSelect}/>;
}
