"use client";
import type { LucideIcon } from "lucide-react";
import { HospitalityGroupedNavigation } from "@erpx/feature-shared/components/hospitality/grouped-navigation";
import { useFrontLanguage } from "@erpx/front/components/front-language";

/** Owners supply their original permission-filtered items and exact scoped hrefs. */
export function StandardPmsBackofficeNavigation({items,current,label}:{
 items:{id:string;label:string;description:string;icon:LucideIcon;href:string}[];
 current:string;label:string;
}){
 return <HospitalityGroupedNavigation app="pms" items={items} current={current} label={label} language={useFrontLanguage()}/>;
}
