import { GuestBookingManagementWorkspace } from "@/components/pms/guest-booking-management-workspace";
import { BookingCopy } from "@/components/pms/booking-copy";
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
        <BookingCopy className={styles.eyebrow}>Your stay</BookingCopy>
        <BookingCopy as="h1">Manage your booking.</BookingCopy>
        <BookingCopy as="p">
          Review your reservation, change dates and rooms, or cancel selected rooms within the accepted policy. Track payments and refunds here.
        </BookingCopy>
      </header>
      <GuestBookingManagementWorkspace slug={holdingSlug} />
    </main>
  );
}
