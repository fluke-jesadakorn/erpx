"use client";
import { useState, type ComponentProps } from "react";
import { Button } from "@/components/ui/button";
import { PosReceipt, PosSaleWorkspace } from "@/components/pos/pos-sales";
import { minorToDecimal } from "@/lib/finance-money";
export function PosRecordDetail({ canEdit, ...props }: ComponentProps<typeof PosSaleWorkspace> & { canEdit: boolean }) {
 const [editing,setEditing]=useState(false),sale=props.data.sale;
 if(!sale)return null;
 return <div className="ui-stack"><h1>{String(sale.number)}</h1><p>{String(sale.state)} · {String(sale.business_date)} · {String(sale.currency)} {minorToDecimal(Number(sale.total_minor))}</p>
  {canEdit&&<Button appearance="standard" variant="secondary" onClick={event=>{const node=event.currentTarget.closest('[data-record-drawer-content]');if(editing&&node?.getAttribute('data-record-dirty')==='true'&&!window.confirm('Discard unsaved receipt changes?'))return;if(editing)window.dispatchEvent(new Event('erpx:record-saved'));setEditing(!editing);}}>{editing?'Back to details':'Work on receipt'}</Button>}
  {editing?<PosSaleWorkspace {...props}/>:<><PosReceipt receipt={props.data.receipt}/><h2>Items</h2>{props.data.lines.map(line=><p key={String(line.id)}>{String(line.product_name??line.name??line.sku)} · {Number(line.quantity_milli)/1000} × {minorToDecimal(Number(line.price_minor))} · {String(line.fulfillment_state)}</p>)}<h2>Payments</h2>{props.data.payments.map(payment=><p key={String(payment.id)}>{String(payment.method)} · {String(payment.state)} · {minorToDecimal(Number(payment.amount_minor))}</p>)}</>}
 </div>;
}
