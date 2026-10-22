import "server-only";
import type {PosWorkspace} from "@erpx/server-pos/public/pos";
import {PosReceiptView} from "@erpx/feature-pos/components/pos-receipt-view";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
export function PosRecordSummary({data}:{data:PosWorkspace}) {return <><PosReceiptView receipt={data.receipt}/><h2>Items</h2>{data.lines.map(line=><p key={String(line.id)}>{String(line.product_name??line.name??line.sku)} · {Number(line.quantity_milli)/1000} × {minorToDecimal(Number(line.price_minor))} · {String(line.fulfillment_state)}</p>)}<h2>Payments</h2>{data.payments.map(payment=><p key={String(payment.id)}>{String(payment.method)} · {String(payment.state)} · {minorToDecimal(Number(payment.amount_minor))}</p>)}</>;}
