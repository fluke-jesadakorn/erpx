"use client";
import { Button, Field, Select } from "@/components/ui/foundation";
import { type FormEvent } from "react";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";
import type { PosRow } from "@/server/modules/pos/public/pos-core";
import { formatPosMoney } from "@/server/modules/pos/public/rules/pos";
import { useFrontLanguage } from "@/components/front/front-language";
import styles from "./pos.module.css";
type Row = PosRow;

const s = (row: Row, key: string) => String(row[key] ?? "");

const n = (row: Row, key: string) => Number(row[key] ?? 0);

const form = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};

export type PosSend = (
  op: string,
  input: unknown,
  select?: boolean,
) => Promise<Record<string, unknown> | undefined>;

export function PosPendingTransfer({payment,data,currency,canVerify,canCancel,send,busy,online}:{
  payment:Row;data:PosWorkspace;currency:string|null;canVerify:boolean;canCancel:boolean;send:PosSend;busy:boolean;online:boolean;
}) {
  const language=useFrontLanguage(),t=(th:string,en:string)=>language==='th'?th:en;
  const money=(value:number)=>formatPosMoney(value,currency);
  return <section className={styles.alert}>
    <strong>{t('บันทึกการโอนแล้ว · รอตรวจสอบ','Bank transfer recorded — awaiting verification')}</strong>
    <p>{t('เลขอ้างอิง','Reference')} {s(payment,'reference')} · {money(n(payment,'amount_minor'))}. {t('บิลยังไม่ชำระจนกว่า Finance จะจับคู่หลักฐานธนาคารที่นำเข้า','This bill is not settled until Finance matches imported bank evidence.')}</p>
    {canVerify&&<form className={styles.fields} onSubmit={event=>{
      const fields=form(event),bank=data.transferEvidence.find(row=>row.id===fields.get('bank'));
      if(bank)void send('payment.verify_transfer',{id:payment.id,expectedVersion:payment.version,bankTransactionId:bank.id,bankVersion:bank.version});
    }}>
      <Field>{t('หลักฐานธนาคารที่นำเข้า','Imported bank evidence')}<Select name="bank" required>
        <option value="">{t('เลือกรายการที่ตรงกัน','Select exact matching row')}</option>
        {data.transferEvidence.map(bank=><option key={s(bank,'id')} value={s(bank,'id')}>{s(bank,'transaction_date')} · {s(bank,'external_id')} · {money(n(bank,'amount_minor'))}</option>)}
      </Select></Field>
      <Button clientManaged type="submit" disabled={busy||!online||!data.transferEvidence.length}>{t('ตรวจยืนยันการโอน','Verify transfer')}</Button>
      {!data.transferEvidence.length&&<p>{t('ยังไม่มีรายการธนาคารที่ตรงกัน ให้นำเข้ารายการธนาคารใน Finance ก่อน','No matching imported bank row. Import the bank statement in Finance first.')}</p>}
    </form>}
    {canCancel&&<Button variant="secondary" type="button" disabled={busy||!online} onClick={()=>void send('payment.cancel_transfer',{id:payment.id,expectedVersion:payment.version})}>{t('ยกเลิกเลขอ้างอิงที่ยังไม่ยืนยัน','Cancel unverified reference')}</Button>}
  </section>;
}
