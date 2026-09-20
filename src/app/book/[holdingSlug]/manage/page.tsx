import { GuestBookingManager } from "@/components/pms/guest-booking-manager";
import styles from "@/components/pms/booking.module.css";
export default async function ManageBooking({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  return (
    <main className={styles.wrap}>
      <header className={styles.hero}>
        <span className={styles.eyebrow}>Your stay</span>
        <h1>Manage your booking.</h1>
        <p>
          View your reservation or cancel before its free-cancellation deadline.
          Contact the hotel for changes to rooms or dates.
        </p>
      </header>
      <GuestBookingManager slug={holdingSlug} />
    </main>
  );
}
