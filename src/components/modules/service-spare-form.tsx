"use client";
import { InlineNotice } from "@/components/ui/foundation";


import { useState } from "react";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";
import { quantityMilli } from "@/lib/commerce-math";
import { minorToDecimal } from "@/lib/finance-money";
import type { availableServiceSpares } from "@/server/inventory-consumption";
import { FormLayout } from "@/components/ui/foundation";

type Spares = Awaited<ReturnType<typeof availableServiceSpares>>;
type Reservation = { id: string; warehouseId: string; productId: string; remainingMilli: number };

export function ServiceSpareForm({ spares, reservations, busy, onSubmit }: {
  spares: Spares; reservations?: Reservation[]; busy: boolean;
  onSubmit: (input: Record<string, unknown>) => void;
}) {
  const choices = reservations ? reservations.flatMap(r => {
    const stock = spares.stock.find(s => s.product_id === r.productId && s.warehouse_id === r.warehouseId);
    return stock ? [{ ...stock, selection: r.id, remaining: r.remainingMilli }] : [];
  }) : spares.stock.filter(s => s.available_milli > 0).map(s => ({ ...s, selection: `${s.warehouse_id}:${s.product_id}`, remaining: s.available_milli }));
  const [selected, setSelected] = useState(choices[0]?.selection ?? ""), [error, setError] = useState("");
  const choice = choices.find(s => s.selection === selected), lots = spares.lots.filter(l => l.warehouse_id === choice?.warehouse_id && l.product_id === choice?.product_id);
  if (!choices.length) return <p className="muted">No available spare stock. Receive or reserve the required parts before issuing them.</p>;
  return <FormLayout className="ui-panel ui-form-grid" onSubmit={event => {
    event.preventDefault(); setError("");
    if (!choice) return;
    const values = new FormData(event.currentTarget);
    try {
      const amount = quantityMilli(String(values.get("quantity"))), allocations = lots.flatMap(lot => {
        const amount = String(values.get(`lot:${lot.lot_id}`) ?? "").trim();
        return amount && Number(amount) > 0 ? [{ lotId: lot.lot_id, quantityMilli: quantityMilli(amount), balanceVersion: Number(lot.version) }] : [];
      });
      if (amount > choice.remaining) throw new Error("The quantity exceeds available spare stock.");
      if (choice.tracking_mode !== "none" && allocations.reduce((n,l)=>n+l.quantityMilli,0) !== amount) throw new Error("Allocate the full issued quantity to the selected lots or serials.");
      onSubmit({ warehouseId: choice.warehouse_id, productId: choice.product_id, ...(reservations ? { reservationId: selected } : {}), quantityMilli: amount,
        entryDate: values.get("entryDate"), allocations, reason: values.get("reason") });
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Review the spare quantities."); }
  }}>
    <h2>Issue spare parts</h2>
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    <Field appearance="standard">Spare and warehouse<Select appearance="standard" value={selected} onChange={event => setSelected(event.target.value)}>
      {choices.map(s => <option key={s.selection} value={s.selection}>{s.sku} · {s.product_name} · {s.warehouse_name} · {minorToDecimal(s.remaining,3)} {s.unit}</option>)}
    </Select></Field>
    <Field appearance="standard">Quantity {choice?.unit}<TextInput appearance="standard" name="quantity" type="number" min="0.001" step={choice?.tracking_mode === "serial" ? "1" : "0.001"} required /></Field>
    <Field appearance="standard">Used on<TextInput appearance="standard" name="entryDate" type="date" defaultValue={new Date().toISOString().slice(0,10)} required /></Field>
    {choice?.tracking_mode !== "none" && <fieldset><legend>Source lots or serials</legend>
      {lots.length ? lots.map(lot => <Field appearance="standard" key={lot.lot_id}>{lot.serial_number ?? lot.lot_code} · available {minorToDecimal(lot.quantity_milli,3)} {choice?.unit}
        <TextInput appearance="standard" name={`lot:${lot.lot_id}`} type="number" min="0" max={minorToDecimal(lot.quantity_milli,3)} step={choice?.tracking_mode === "serial" ? "1" : "0.001"} defaultValue="0" />
      </Field>) : <p>No eligible lot or serial is available.</p>}
    </fieldset>}
    <Field appearance="standard">Usage and evidence<TextArea appearance="standard" name="reason" minLength={3} required /></Field>
    <p className="muted">Finance reviews the spare cost before this work can close.</p>
    <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || choice?.tracking_mode !== "none" && !lots.length}>Issue parts</Button>
  </FormLayout>;
}
