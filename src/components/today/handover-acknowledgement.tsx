"use client";

import { useEffect, useState } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { HandoverAcknowledgement as Acknowledgement } from "@/server/handover";
import styles from "./handover-composer.module.css";

export function HandoverAcknowledgement({ api, postId, version,language="en" }: { api:string;postId:string;version:number;language?:"th"|"en" }) {
  const t=(th:string,en:string)=>language==="th"?th:en;
  const [state,setState]=useState<Acknowledgement|null>(null);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const endpoint=`${api}/handover/${encodeURIComponent(postId)}/acknowledgement`;
  useEffect(()=>{
    let active=true;
    betaFetch<Acknowledgement>(endpoint).then(result=>{if(active)setState(result);})
      .catch(()=>{if(active)setError("Confirmation status is unavailable.");});
    return ()=>{active=false;};
  },[endpoint,version]);
  async function confirm(){
    if(busy)return;
    setBusy(true);setError("");
    try { setState(await betaFetch<Acknowledgement>(endpoint,{})); }
    catch(cause){setError(cause instanceof Error?cause.message:"Confirmation failed. Try again.");}
    finally {setBusy(false);}
  }
  if(!state&&!error)return <p className={styles.ack} role="status">{t("กำลังโหลดผลรับเวร…","Loading handover receipt…")}</p>;
  return <div className={styles.ack} aria-label={t("ผลรับเวร","Handover receipt")}>
    {state && <span>{t(`รับทราบแล้ว ${state.acknowledgedCount} จาก ${state.recipientCount} คน${state.acknowledged?" · คุณรับทราบแล้ว":""}`,`${state.acknowledgedCount} of ${state.recipientCount} recipients confirmed receipt${state.acknowledged?" · You confirmed":""}.`)}</span>}
    {state?.canAcknowledge&&!state.acknowledged && <button type="button" disabled={busy} onClick={()=>void confirm()}>{busy?t("กำลังยืนยัน…","Confirming…"):t("ยืนยันรับเวร","Confirm receipt")}</button>}
    {error && <span role="alert">{error}</span>}
  </div>;
}
