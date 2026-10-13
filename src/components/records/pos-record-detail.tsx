"use client";
import dynamic from "next/dynamic";
const PosPendingTransfer = dynamic(() => import("@/components/pos/pos-pending-transfer").then(module => module.PosPendingTransfer));
const PosReceipt = dynamic(() => import("@/components/pos/pos-receipt").then(module => module.PosReceipt));
const PosSaleWorkspace = dynamic(() => import("@/components/pos/pos-sales").then(module => module.PosSaleWorkspace));
import { useState, type ComponentProps, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

import { minorToDecimal } from "@/lib/finance/finance-money";
export function PosRecordDetail({ canEdit, canVerifyTransfer, online, initialSummary, ...props }: ComponentProps<typeof PosSaleWorkspace> & { canEdit: boolean; canVerifyTransfer: boolean; online: boolean; initialSummary?: ReactNode }) {
 const [editing,setEditing]=useState(false),sale=props.data.sale;
 if(!sale)return null;
 return <div className="ui-stack"><h1>{String(sale.number)}</h1><p>{String(sale.state)} · {String(sale.business_date)} · {String(sale.currency)} {minorToDecimal(Number(sale.total_minor))}</p>
  {canEdit&&<Button appearance="standard" variant="secondary" onClick={event=>{const node=event.currentTarget.closest('[data-record-drawer-content]');if(editing&&node?.getAttribute('data-record-dirty')==='true'&&!window.confirm('Discard unsaved receipt changes?'))return;if(editing)window.dispatchEvent(new Event('erpx:record-saved'));setEditing(!editing);}}>{editing?'Back to details':'Work on receipt'}</Button>}
  {editing?<PosSaleWorkspace {...props}/>:initialSummary??<><PosReceipt receipt={props.data.receipt}/><h2>Items</h2>{props.data.lines.map(line=><p key={String(line.id)}>{String(line.product_name??line.name??line.sku)} · {Number(line.quantity_milli)/1000} × {minorToDecimal(Number(line.price_minor))} · {String(line.fulfillment_state)}</p>)}<h2>Payments</h2>{props.data.payments.map(payment=><p key={String(payment.id)}>{String(payment.method)} · {String(payment.state)} · {minorToDecimal(Number(payment.amount_minor))}</p>)}</>}
  {!editing && canVerifyTransfer && props.data.payments.filter(payment=>payment.method==='transfer'&&payment.state==='pending').map(payment=><PosPendingTransfer key={String(payment.id)} payment={payment} data={props.data} currency={String(sale.currency)} canVerify={true} canCancel={false} send={props.send} busy={props.busy} online={online}/>)}
 </div>;
}
