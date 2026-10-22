"use client";
import {useState} from "react";
import {Field,Select} from "@erpx/ui/components/foundation";
import {SourceSupportLink} from "@erpx/feature-support/components/source-support-link";
import { type UiAppearance } from "@erpx/ui/components/foundation";
export function SourceSupportChooser({ appearance = "legacy",base,records,selectedId}:({base:string;records:{id:string;version:number;label:string}[];selectedId?:string}) & { appearance?: UiAppearance }){
 const [selected,setSelected]=useState(selectedId??""),row=records.find(record=>record.id===selected);
 if(records.length===0)return null;
 return <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-form-grid" : "business-form"}><Field appearance={appearance}>Report a hotel issue<Select appearance={appearance} value={selected} onChange={event=>setSelected(event.target.value)}><option data-ui-appearance={appearance} value="">Choose an affected hotel record</option>{records.map(record=><option data-ui-appearance={appearance} value={record.id} key={record.id}>{record.label}</option>)}</Select></Field>{row&&<SourceSupportLink appearance={appearance} base={base} kind="pms_record" id={row.id} version={row.version}/>}</div>;
}
