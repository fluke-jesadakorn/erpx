"use client";
import {Button,InlineNotice} from "@/components/ui/foundation";
import {useAccountPreferences} from "@/components/identity_access/account-preferences-context";
export function ProjectStructureRecovery({pending,busy,retry}:{pending:boolean;busy:boolean;retry:()=>Promise<unknown>}){
 const th=useAccountPreferences()?.preference?.language==="th";if(!pending)return null;
 return <InlineNotice><p>{th?"ผลคำขอโครงสร้างโครงการเดิมยังไม่ยืนยัน เก็บแบบร่างใหม่ไว้และกู้คืนคำขอเดิมก่อน":"The original Project structure result is not confirmed. Keep your newer draft and recover the original request first."}</p><Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>void retry()}>{th?"กู้คืนคำขอโครงการเดิม":"Recover original Project request"}</Button></InlineNotice>;
}
