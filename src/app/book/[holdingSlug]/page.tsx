import { notFound } from "next/navigation";
import Link from "next/link";
import { bookingCatalog, bookingDatabase } from "@/server/pms-booking";
import { BookingFlow } from "@/components/pms/booking-flow";
import styles from "@/components/pms/booking.module.css";
export const dynamic = "force-dynamic";
export default async function BookingPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  if (process.env.NODE_ENV === "production") notFound();
  const { holdingSlug } = await params;
  const db = await bookingDatabase(holdingSlug).catch((e) => {
    if (e instanceof Error && e.message === "HOLDING_NOT_FOUND") notFound();
    throw e;
  });
  const hotels = await bookingCatalog(db);
  return (
    <main className={styles.wrap}>
      <header className={styles.hero}>
        <span className={styles.eyebrow}>A place for your next chapter</span>
        <h1>Find a stay that feels right.</h1>
        <p>
          Explore our hotels, compare room options and reserve your stay. Book
          directly and pay at the hotel.
        </p>
      </header>
      {hotels.length ? (
        <>
          <BookingFlow
            slug={holdingSlug}
            hotels={hotels}
            today={new Intl.DateTimeFormat("en-CA", {
              timeZone: "Asia/Bangkok",
            }).format(new Date())}
          />
          <section className={styles.surface}>
            <h2>Our hotels</h2>
            {hotels.map((h) => (
              <article className={styles.offer} key={h.id}>
                <div>
                  <h3>
                    <Link href={`/book/${holdingSlug}/${h.id}`}>{h.name}</Link>
                  </h3>
                  <p>{h.description}</p>
                  <p>{h.address}</p>
                </div>
                <Link
                  className={styles.secondary}
                  href={`/book/${holdingSlug}/${h.id}`}
                >
                  View hotel
                </Link>
              </article>
            ))}
          </section>
        </>
      ) : (
        <section className={styles.surface}>
          <h2>Online reservations are not open yet</h2>
          <p>
            Our hotels will appear here when they are ready to accept direct
            bookings.
          </p>
        </section>
      )}
    </main>
  );
}
