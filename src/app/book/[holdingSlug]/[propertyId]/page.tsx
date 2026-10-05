import { notFound } from "next/navigation";
import { bookingCatalog, bookingDatabase } from "@/server/pms-booking";
import { BookingFlow } from "@/components/pms/booking-flow";
import { BookingGallery } from "@/components/pms/booking-gallery";
import { BookingCopy } from "@/components/pms/booking-copy";
import styles from "@/components/pms/booking.module.css";
export const dynamic = "force-dynamic";
export default async function HotelPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; propertyId: string }>;
}) {
  if (process.env.PMS_STRIPE_BOOKING_ENABLED === 'false') notFound();
  const { holdingSlug, propertyId } = await params;
  const db = await bookingDatabase(holdingSlug).catch((e) => {
    if (e instanceof Error && e.message === "HOLDING_NOT_FOUND") notFound();
    throw e;
  });
  const hotel = (await bookingCatalog(db)).find((h) => h.id === propertyId);
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
      <BookingFlow
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
