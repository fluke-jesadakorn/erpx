import { bookingPresentationQueries, bookingPaymentPresentationQueries } from "@erpx/server-pms/public/presentation-context-queries";
import { notFound } from "next/navigation";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BookingPaymentReturn} from "@erpx/feature-pms/components/booking-payment-return";

import { PublicBookingReservation } from "@erpx/feature-pms/components/public-booking-reservation";
import { BookingGallery } from "@erpx/feature-pms/components/booking-gallery";
import { BookingCopy } from "@erpx/feature-pms/components/booking-copy";
import styles from "@erpx/feature-pms/components/booking.module.css";
export const dynamic = "force-dynamic";
export default async function HotelPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; propertyId: string }>;
  searchParams?: Promise<{attempt?:string|string[]}>;
}) {
  if (process.env.PMS_STRIPE_BOOKING_ENABLED === 'false') notFound();
  const { holdingSlug, propertyId } = await params;
  const attempt = (await searchParams)?.attempt;
  const queries = await (attempt === undefined
    ? bookingPresentationQueries(holdingSlug)
    : bookingPaymentPresentationQueries(holdingSlug)).catch((e) => {
    if (e instanceof Error && e.message === "HOLDING_NOT_FOUND") notFound();
    throw e;
  });

  if ("bookingPaymentStatus" in queries) {
    if(typeof attempt!=="string"||!attempt||attempt.length>1000)notFound();
    let status:Awaited<ReturnType<typeof queries.bookingPaymentStatus>>;
    try{status=await queries.bookingPaymentStatus(attempt);}
    catch(error){
      if(error instanceof ApiError&&[401,403,404].includes(error.status))notFound();
      throw error;
    }
    if(status.hotel.id!==propertyId)notFound();
    return <main className={styles.wrap}><BookingPaymentReturn key={JSON.stringify([holdingSlug,propertyId,attempt,status.state,status.paymentState,status.refundState])} slug={holdingSlug} propertyId={propertyId} attempt={attempt} initial={{number:status.number,state:status.state,paymentState:status.paymentState,checkoutUrl:status.checkoutUrl,managementToken:status.managementToken,refundState:status.refundState}}/></main>;
  }
  const hotel = (await queries.bookingCatalog()).find((h) => h.id === propertyId);
  if (!hotel) notFound();
  return (
    <main className={styles.wrap}>
      <header className={styles.hero}>
        <BookingCopy className={styles.eyebrow}>Book directly · Pay securely online</BookingCopy>
        <h1>{hotel.name}</h1>
        <p>{hotel.description}</p>
        <p>{hotel.address}</p>
        <p>
          <BookingCopy>Check-in</BookingCopy> {hotel.checkIn} · <BookingCopy>Check-out</BookingCopy> {hotel.checkOut} ·{" "}
          {hotel.timezone}
        </p>
        <a href={`mailto:${hotel.contactEmail}`}>{hotel.contactEmail}</a> ·{" "}
        {hotel.contactPhone}
      </header>
      <BookingGallery slug={holdingSlug} images={hotel.images}/>
      <PublicBookingReservation
        slug={holdingSlug}
        hotels={[hotel]}
        propertyId={hotel.id}
        today={new Intl.DateTimeFormat("en-CA", {
          timeZone: hotel.timezone,
        }).format(new Date())}
      />
    </main>
  );
}
