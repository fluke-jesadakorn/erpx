"use client";
import {usePosController, type PosConsoleProps} from "@/components/pos/pos-controller";
import {PosRecordDetail} from "./pos-record-detail";
import {Button} from "@/components/ui/button";
import type {ReactNode} from "react";
import {useRouter} from "next/navigation";
/** Receipt commands share the console controller without importing its screens. */
export function PosRecordConsole(props: PosConsoleProps & {initialSummary?:ReactNode}) {
  const router=useRouter();
  const {data, error, busy, send, refresh, online, api, registerId, setRegisterId, setSelected, pendingCommand} = usePosController({...props,recordOnly:true});
  return <div className="ui-stack">
    {error && <p role="alert">{error}<Button onClick={()=>void refresh().catch(()=>{})}>Retry</Button></p>}
    {pendingCommand && <Button disabled={busy} onClick={()=>void send(pendingCommand.op,pendingCommand.input,pendingCommand.select)}>Retry safely</Button>}
    {data.sale ? <PosRecordDetail key={String(data.sale.id)} initialSummary={data === props.initial ? props.initialSummary : undefined} data={data} api={api} erpBase={props.erpBase ?? props.base} busy={busy} send={send} registerId={registerId} setRegisterId={setRegisterId}
      canEdit={Boolean(data.permissions["pos.sell"])} canVerifyTransfer={!props.frontMode && Boolean(data.permissions["finance.approve"])} online={online}
      pickSale={row=>{setSelected(String(row.id));setRegisterId(String(row.register_id));const url=new URL(window.location.href);url.searchParams.set("detailKind","pos_sale");url.searchParams.set("detailId",String(row.id));window.history.pushState(null,"",url);}}
      openShifts={()=>router.push(`${props.base}/pos?tab=shifts`)}/>
      : <p role="alert">Receipt unavailable in this work scope.</p>}
  </div>;
}
