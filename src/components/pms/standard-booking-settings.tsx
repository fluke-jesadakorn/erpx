"use client";
import WorkspaceLink from '@/components/workplace/workspace-link';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {FrontLocalizedTree} from '@/components/front/front-language';
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { BookingRatePolicy,BookingMediaSetup,BookingMediaPicker } from "./booking-policy-settings";
import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { useState, useRef } from "react";
import {BookingGallery} from "./booking-gallery";
import { records, type Snapshot, type Rate, type BookingSettings as Settings } from "@/server/modules/pms/public/rules/pms-model";
import { decimalToMinor, minorToDecimal } from "@/lib/finance/finance-money";
import styles from "./standard-booking-settings.module.css";
type Save = (operation: string, input: unknown, version?: number) => Promise<boolean>;
function ChildPrices({
  id,
  rate,
  version,
  save,
}: {
  id: string;
  rate: Rate;
  version: number;
  save: Save;
}) {
  const [bands, setBands] = useState(
      (rate.childBands ?? []).map((b) => ({
        ...b,
        displayPrice: minorToDecimal(b.price),
      })),
    ),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const editingVersion = useRef<number | undefined>(undefined);
  return (
    <FrontLocalizedTree><FormDisclosure appearance="standard" className={styles.surface} dismissDisabled={busy}>
      <summary>Child prices · {rate.name}</summary>
      <p className={styles.small}>
        Each child counts toward capacity. The configured charge is added per child, per night,
        before the rate’s tax treatment. Ages without a band are not offered.
      </p>
      <form
        onChange={() => {
          editingVersion.current ??= version;
        }}
        onClickCapture={() => {
          editingVersion.current ??= version;
        }}
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            const ok = await save(
              "rate.save",
              {
                ...rate,
                id,
                childBands: bands.map(({ displayPrice, ...b }) => ({
                  ...b,
                  price: decimalToMinor(displayPrice),
                })),
              },
              editingVersion.current ?? version,
            );
            if (ok) editingVersion.current = undefined;
            setMessage(ok ? "Child prices saved" : "Not saved. Check the message above");
          } finally {
            setBusy(false);
          }
        }}
      >
        {bands.map((b, i) => (
          <div className={styles.grid} key={i}>
            <Field appearance="standard">
              From age
              <TextInput appearance="standard"
                type="number"
                min={0}
                max={17}
                required
                value={b.minAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, minAge: Number(e.target.value) } : r)),
                  )
                }
              />
            </Field>
            <Field appearance="standard">
              Through age
              <TextInput appearance="standard"
                type="number"
                min={b.minAge}
                max={17}
                required
                value={b.maxAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, maxAge: Number(e.target.value) } : r)),
                  )
                }
              />
            </Field>
            <Field appearance="standard">
              Charge per night (THB)
              <TextInput appearance="standard"
                type="number"
                min={0}
                step="0.01"
                required
                value={b.displayPrice}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, displayPrice: e.target.value } : r)),
                  )
                }
              />
            </Field>
            <Button appearance="standard"
              variant="secondary"
              className={styles.secondary}
              type="button"
              onClick={() => setBands(bands.filter((_, j) => i !== j))}
            >
              Remove band
            </Button>
          </div>
        ))}
        <div className={styles.actions}>
          <Button appearance="standard"
            variant="secondary"
            className={styles.secondary}
            type="button"
            onClick={() =>
              setBands([...bands, { minAge: 0, maxAge: 17, price: 0, displayPrice: "0.00" }])
            }
            disabled={bands.length >= 18}
          >
            Add age band
          </Button>
          <Button appearance="standard" clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Save child prices
          </Button>
        </div>
        <p role="status">{message}</p>
      </form>
    </FormDisclosure></FrontLocalizedTree>
  );
}
export function StandardBookingSettings({
  snapshot,
  save,
  slug,
  canReview = false,
  readiness,
}: {
  snapshot: Snapshot;
  save: Save;
  slug: string;
  canReview?: boolean;
  readiness?:{email:boolean;accounting:boolean;onlineAccounting:boolean;fees:boolean;connect:boolean};
}) {
  const b = snapshot.property.config.booking,
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const editingVersion = useRef<number | undefined>(undefined),formRef=useRef<HTMLFormElement>(null);
  const [preview,setPreview]=useState<Settings|null>(null);
  function formSettings(f:FormData):Settings{
   const rateIds=f.getAll('rateIds').map(String),typeIds=new Set(records(snapshot,'rate').filter(r=>rateIds.includes(r.id)).map(r=>r.data.typeId));
   const images=(name:string,fallback:string)=>f.getAll(name).map(String).map(publicationId=>({publicationId,alt:String(f.get(`alt-${publicationId}`)??fallback)}));
   return {published:f.get('published')==='on',description:String(f.get('description')),address:String(f.get('address')),contactEmail:String(f.get('email')),contactPhone:String(f.get('phone')),cancellationDays:Number(f.get('days')),cancellationTime:String(f.get('time')),rateIds,images:images('hotel-images',snapshot.property.name),roomContent:records(snapshot,'room_type').filter(r=>typeIds.has(r.id)).map(r=>({typeId:r.id,description:String(f.get(`room-${r.id}`)),beds:String(f.get(`beds-${r.id}`)??''),sizeSqm:Number(f.get(`size-${r.id}`))||undefined,amenities:String(f.get(`amenities-${r.id}`)??''),images:images(`images-${r.id}`,r.data.name)}))};
  }
  return (
    <FrontLocalizedTree><div className={styles.wrap}>
      <FormDisclosure appearance="standard" dismissDisabled={busy} className={styles.surface}>
        <summary>Guest booking website · content and publishing</summary>
        <p className={styles.small}>
          Publish this hotel on the{" "}
          <WorkspaceLink href={`/book/${slug}`} target="_blank" rel="noreferrer">
            shared booking website
          </WorkspaceLink>
          . Only selected rates and the room descriptions below are public. New hotels are
          unpublished.
        </p>
        <form ref={formRef}
          onChange={() => {
            editingVersion.current ??= snapshot.property.version;
          }}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            setBusy(true);
            try {
              const payload=formSettings(f);
              if(payload.published&&JSON.stringify(preview)!==JSON.stringify(payload)){setMessage("Preview the latest content and readiness before publishing");return;}
              const ok=await save('booking.save',payload,editingVersion.current??snapshot.property.version);
              if (ok) editingVersion.current = undefined;
              setMessage(
                ok ? "Booking website settings saved" : "Not saved. Check the message above",
              );
            } finally {
              setBusy(false);
            }
          }}
        >
          <Field appearance="standard" className={styles.field}>
            Hotel introduction
            <TextArea appearance="standard" name="description" defaultValue={b?.description} required maxLength={3000} />
          </Field>
          <div className={styles.grid}>
            <Field appearance="standard">
              Public address
              <TextInput appearance="standard" name="address" defaultValue={b?.address} required maxLength={500} />
            </Field>
            <Field appearance="standard">
              Guest contact email
              <TextInput appearance="standard" type="email" name="email" defaultValue={b?.contactEmail} required />
            </Field>
            <Field appearance="standard">
              Guest contact phone
              <TextInput appearance="standard" name="phone" defaultValue={b?.contactPhone} required />
            </Field>
            <Field appearance="standard">
              Free cancellation: days before arrival
              <TextInput appearance="standard"
                type="number"
                name="days"
                min={0}
                max={365}
                defaultValue={b?.cancellationDays ?? 1}
                required
              />
            </Field>
            <Field appearance="standard">
              Deadline in hotel timezone
              <TextInput appearance="standard"
                type="time"
                name="time"
                defaultValue={b?.cancellationTime ?? "18:00"}
                required
              />
            </Field>
          </div>
          <BookingMediaPicker base={canonicalUiHref(`/holdings/${slug}`)} targetType="pms_property" targetId={snapshot.property.id} name="hotel-images" selected={b?.images}/>
          <h3>Public rates</h3>
          {records(snapshot, "rate")
            .filter((r) => r.data.market === "public")
            .map((r) => (
              <Field appearance="standard" orientation="inline" key={r.id} className={styles.check}>
                <TextInput appearance="standard"
                  type="checkbox"
                  name="rateIds"
                  value={r.id}
                  defaultChecked={b?.rateIds.includes(r.id)}
                />
                {r.data.name} ·{" "}
                {records(snapshot, "room_type").find((t) => t.id === r.data.typeId)?.data.name}
              </Field>
            ))}
          <h3>Room descriptions</h3>
          {records(snapshot, "room_type").map((r) => (
            <div key={r.id}><Field appearance="standard" className={styles.field}>
              {r.data.name}
              <TextArea appearance="standard"
                name={`room-${r.id}`}
                defaultValue={b?.roomContent.find((c) => c.typeId === r.id)?.description}
                maxLength={2000}
                placeholder="Describe the room for guests. Required when its rates are selected."
              />
            </Field><div className={styles.grid}><Field appearance="standard">Bed configuration<TextInput appearance="standard" name={`beds-${r.id}`} defaultValue={b?.roomContent.find(c=>c.typeId===r.id)?.beds}/></Field><Field appearance="standard">Room size (m²)<TextInput appearance="standard" type="number" min="1" max="10000" name={`size-${r.id}`} defaultValue={b?.roomContent.find(c=>c.typeId===r.id)?.sizeSqm}/></Field><Field appearance="standard">Amenities<TextInput appearance="standard" name={`amenities-${r.id}`} defaultValue={b?.roomContent.find(c=>c.typeId===r.id)?.amenities??r.data.amenities}/></Field></div><BookingMediaPicker base={canonicalUiHref(`/holdings/${slug}`)} targetType="pms_room_type" targetId={r.id} name={`images-${r.id}`} selected={b?.roomContent.find(c=>c.typeId===r.id)?.images}/></div>
          ))}
          <Field appearance="standard" orientation="inline" className={styles.check}>
            <TextInput appearance="standard" type="checkbox" name="published" defaultChecked={b?.published ?? false} />
            Publish this hotel for online booking
          </Field>
          <p className={styles.small}>
            Publishing requires an open hotel, selected rates, room descriptions and configured
            email delivery. Prepaid rates also require approved liability mappings and a ready Company Connect account. Late cancellations go to staff.
          </p>
          <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{if(formRef.current){setPreview(formSettings(new FormData(formRef.current)));setMessage('Review the guest preview below before saving');}}}>Preview and check readiness</Button>
          {preview&&<section aria-label="Guest website preview"><h2>{snapshot.property.name}</h2><BookingGallery slug={slug} images={preview.images}/><p>{preview.description}</p><p>{preview.address} · {preview.contactPhone} · {preview.contactEmail}</p><ul>{[["Hotel open",snapshot.property.state==='open'],["Room rates selected",preview.rateIds.length>0],["Finance setup approved",readiness?.accounting],["Booking email ready",readiness?.email],["Company Connect ready for selected online rates",preview.rateIds.every(id=>!records(snapshot,'rate').find(r=>r.id===id)?.data.collection?.method||records(snapshot,'rate').find(r=>r.id===id)?.data.collection?.method==='pay_at_hotel')||readiness?.connect&&readiness?.onlineAccounting],["Cancellation fee accounting approved",preview.rateIds.every(id=>!(records(snapshot,'rate').find(r=>r.id===id)?.data.collection?.cancellationFeePercent))||readiness?.fees]].map(([label,ready])=><li key={String(label)}>{label}: {ready?'Ready':'Needs setup'}</li>)}</ul>{preview.roomContent.map(room=><article key={room.typeId}><h3>{records(snapshot,'room_type').find(t=>t.id===room.typeId)?.data.name}</h3><BookingGallery compact slug={slug} images={room.images}/><p>{room.description}</p><p>{room.beds} · {room.sizeSqm?`${room.sizeSqm} m²`:''} · {room.amenities}</p>{records(snapshot,'rate').filter(r=>preview.rateIds.includes(r.id)&&r.data.typeId===room.typeId).map(rate=><p key={rate.id}>{rate.data.name} · {rate.data.collection?.method==='deposit'?`${rate.data.collection.depositPercent}% room advance`:rate.data.collection?.method==='full'?'Pay in full online':'Pay at hotel'}</p>)}</article>)}</section>}
          <Button appearance="standard" clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Save booking website settings
          </Button>
          <p role="status">{message}</p>
        </form>
      </FormDisclosure>
      <BookingMediaSetup base={canonicalUiHref(`/holdings/${slug}`)} targetType="pms_property" targetId={snapshot.property.id} version={snapshot.property.version} title={snapshot.property.name} canReview={canReview}/>
      {records(snapshot,'room_type').map(r=><BookingMediaSetup key={r.id} base={canonicalUiHref(`/holdings/${slug}`)} targetType="pms_room_type" targetId={r.id} version={r.version} title={r.data.name} canReview={canReview}/>)}
      {records(snapshot,'rate').map(r=><BookingRatePolicy key={`policy-${r.id}-${r.version}`} id={r.id} rate={r.data} version={snapshot.property.version} save={save}/>)}
      {records(snapshot, "rate").map((r) => (
        <ChildPrices
          key={`${r.id}-${r.version}`}
          id={r.id}
          rate={r.data}
          version={snapshot.property.version}
          save={save}
        />
      ))}
    </div></FrontLocalizedTree>
  );
}
