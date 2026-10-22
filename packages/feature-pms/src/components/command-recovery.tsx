'use client';
import {Button,InlineNotice} from '@erpx/ui/components/foundation';
import {useFrontLanguage} from '@erpx/front/components/front-language';
export function PmsCommandRecovery({busy,retry}:{busy:boolean;retry:()=>Promise<boolean>}){
 const th=useFrontLanguage()==='th';
 return <InlineNotice tone="warning" role="status"><p>{th?'ยังไม่ทราบผลคำสั่งโรงแรมเดิม กรุณาตรวจผลเดิมก่อนเริ่มรายการอื่น ข้อมูลที่แก้ใหม่ยังเก็บไว้':'Confirm the original hotel action before starting another one. Your newer entries are retained.'}</p><Button appearance="standard" variant="secondary" disabled={busy} onClick={()=>void retry()}>{th?'ตรวจผลคำสั่งเดิม':'Retry original action'}</Button></InlineNotice>;
}
