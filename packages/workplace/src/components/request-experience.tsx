"use client";
import { useFrontLanguage } from "@erpx/front/components/front-language";
import { InlineNotice } from "@erpx/ui/components/foundation";
import type { RequestPreview } from "@erpx/feature-work_projection/lib/request-preview";
export function RequestSteps({reviewing}:{reviewing:boolean}) {
 const th=useFrontLanguage()==="th";
 return <ol className="request-steps" aria-label={th?"ขั้นตอนคำขอ":"Request steps"}><li aria-current={!reviewing?"step":undefined}><span>1</span>{th?"รายละเอียด":"Details"}</li><li aria-current={reviewing?"step":undefined}><span>2</span>{th?"ตรวจและส่ง":"Review & send"}</li></ol>;
}
export function RoutePreview({data,base}:{data:RequestPreview|null;base:string}) {
 const th=useFrontLanguage()==="th";
 if(!data)return null;
 return <section className="request-route" aria-label="Route preview"><h3>{th?"ผู้รับงานและขั้นตอนถัดไป":"Who receives this request"}</h3>{data.team&&<p>{data.team}</p>}<ol>{data.steps.map((s,i)=><li key={i}><strong>{s.person}</strong><span>{s.label}</span></li>)}</ol>{data.blockers.length>0&&<InlineNotice tone="info" role="status">{data.blockers.map((b,i)=><p key={i}>{b}</p>)}<a href={`${base}${data.setupPath??"/people"}`}>{th?"ดูข้อมูลพนักงานและผู้ดูแล":"Open People & setup"}</a></InlineNotice>}<p className="muted">{th?"ระบบตรวจสิทธิและเส้นทางอีกครั้งเมื่อส่งคำขอ":"Eligibility and routing are checked again when you send."}</p></section>;
}
