import { bookingPresentationQueries } from "@erpx/server-pms/public/presentation-context-queries";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { notFound } from "next/navigation";
import Link from "next/link";

import { GuestBookingVisualPrototype } from "@erpx/feature-pms/components/visual-prototype";
import { PublicBookingReservation } from "@erpx/feature-pms/components/public-booking-reservation";
import { BookingGallery } from "@erpx/feature-pms/components/booking-gallery";
import { BookingCopy } from "@erpx/feature-pms/components/booking-copy";
import styles from "@erpx/feature-pms/components/booking.module.css";
export const dynamic = "force-dynamic";
export default async function BookingPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ prototype?: string }>;
}) {
  if (
    process.env.PMS_STRIPE_BOOKING_ENABLED === "false"
  )
    notFound();
  const { holdingSlug } = await params;
  const queries = await bookingPresentationQueries(holdingSlug).catch((e) => {
    if (e instanceof Error && e.message === "HOLDING_NOT_FOUND") notFound();
    throw e;
  });
  if (process.env.NODE_ENV !== "production" && (await searchParams).prototype === "1") return <GuestBookingVisualPrototype />;
  const hotels = await queries.bookingCatalog();
  return (
    <main className={styles.wrap}>
      <header className={styles.hero}>
        <BookingCopy className={styles.eyebrow}>A place for your next chapter</BookingCopy>
        <BookingCopy as="h1">Find a stay that feels right.</BookingCopy>
        <BookingCopy as="p">
          Explore our hotels, compare room options and reserve your stay. Book directly and complete
          payment securely online.
        </BookingCopy>
      </header>
      {hotels.length ? (
        <>
          <PublicBookingReservation
            slug={holdingSlug}
            hotels={hotels}
            today={new Intl.DateTimeFormat("en-CA", {
              timeZone: "Asia/Bangkok",
            }).format(new Date())}
          />
          <section className={styles.surface}>
            <BookingCopy as="h2">Our hotels</BookingCopy>
            {hotels.map((h) => (
              <article className={styles.offer} key={h.id}>
                <div>
                  <BookingGallery slug={holdingSlug} compact images={h.images}/>
                  <h3>
                    <Link href={`/book/${holdingSlug}/${h.id}`}>{h.name}</Link>
                  </h3>
                  <p>{h.description}</p>
                  <p>{h.address}</p>
                </div>
                <UiActionLink
                  variant="secondary"
                  className={styles.secondary}
                  href={`/book/${holdingSlug}/${h.id}`}
                >
                  <BookingCopy>View hotel</BookingCopy>
                </UiActionLink>
              </article>
            ))}
          </section>
        </>
      ) : (
        <section className={styles.surface}>
          <BookingCopy as="h2">Online reservations are not open yet</BookingCopy>
          <BookingCopy as="p">Our hotels will appear here when they are ready to accept direct bookings.</BookingCopy>
        </section>
      )}
    </main>
  );
}
