import type {CountRow} from "@erpx/server-inventory/public/inventory-counts";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
export type CountAllocation = {lotId?:string;lotCode:string;serialNumber:string;manufacturedDate:string;expiryDate:string;countedQuantity:string;unitCost:string};
export type CountItem = {productId:string;trackingMode:string;countedQuantity:string;unitCost:string;allocations:CountAllocation[]};
export type CountEditorDraft = {warehouse:string;items:CountItem[];adding:string;countDate:string;reason:string;source?:{id:string;version:number}};
export function countEditorDraft(options:{warehouses:CountRow[];products:CountRow[]},count?:CountRow,lines:CountRow[]=[],lotLines:CountRow[]=[]):CountEditorDraft {
  return {warehouse:String(count?.warehouse_id ?? options.warehouses[0]?.id ?? ""),adding:String(options.products[0]?.id ?? ""),countDate:String(count?.count_date ?? new Date().toISOString().slice(0,10)),reason:String(count?.reason ?? ""),
    ...(count?{source:{id:String(count.id),version:Number(count.version)}}:{}),
    items:lines.map(line=>({productId:String(line.product_id),trackingMode:String(line.tracking_mode ?? "none"),countedQuantity:String(Number(line.counted_quantity_milli)/1000),unitCost:line.unit_cost_minor == null ? "" : minorToDecimal(Number(line.unit_cost_minor)),allocations:lotLines.filter(lot=>lot.count_line_id===line.id).map(lot=>({...(lot.source_lot_id?{lotId:String(lot.source_lot_id)}:{}),lotCode:String(lot.lot_code ?? ""),serialNumber:String(lot.serial_number ?? ""),manufacturedDate:String(lot.manufactured_date ?? ""),expiryDate:String(lot.expiry_date ?? ""),countedQuantity:String(Number(lot.counted_quantity_milli)/1000),unitCost:lot.unit_cost_minor == null ? "" : minorToDecimal(Number(lot.unit_cost_minor))}))}))};
}
/** An acknowledgement advances identity/version without replacing newer local quantities. */
export function acknowledgedCountDraft(draft:CountEditorDraft,receipt:{id:string;version:number}):CountEditorDraft{return {...draft,source:{id:receipt.id,version:receipt.version}};}
export function currentCountDraft(baseline:CountEditorDraft,incoming:CountEditorDraft) {
  return baseline.source && (!incoming.source || incoming.source.id===baseline.source.id && incoming.source.version<baseline.source.version) ? baseline : incoming;
}
