import { cookies } from "next/headers";
import { appearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import { publicBookingDirectory } from "@erpx/server-pms/public/public-booking-directory";
import { ActionAnchor } from "@erpx/ui/components/foundation";
import { GuestBookingShell } from "@erpx/feature-pms/components/guest-booking-shell";
import { BookingCopy } from "@erpx/feature-pms/components/booking-copy";
import { BookingGallery } from "@erpx/feature-pms/components/booking-gallery";
import styles from "@erpx/feature-pms/components/booking.module.css";

export async function PublicBookingDirectory() {
  const [hotels, cookie] = await Promise.all([publicBookingDirectory(), cookies()]);
  return <GuestBookingShell initialLanguage={cookie.get("erpx_booking_language")?.value === "th" ? "th" : "en"} initialTheme={appearancePreference(cookie.get("erpx_appearance")?.value)}>
    <main className={styles.wrap}>
      <header className={styles.hero}>
        <BookingCopy className={styles.eyebrow}>A place for your next chapter</BookingCopy>
        <BookingCopy as="h1">Find a stay that feels right.</BookingCopy>
        <BookingCopy as="p">Choose a hotel to see available rooms, reserve a stay or manage an existing booking.</BookingCopy>
      </header>
      <section id="hotels" className={styles.surface}>
        <BookingCopy as="h2">Our hotels</BookingCopy>
        {hotels.length ? hotels.map(hotel => {
          const path = `/book/${encodeURIComponent(hotel.slug)}`;
          return <article className={styles.offer} key={`${hotel.slug}:${hotel.id}`}>
            <div><BookingGallery slug={hotel.slug} compact images={hotel.images}/><h3><a href={`${path}/${encodeURIComponent(hotel.id)}`}>{hotel.name}</a></h3><p>{hotel.description}</p><p>{hotel.address}</p>
              <a href={`${path}/manage`}><BookingCopy>Manage booking</BookingCopy></a>
            </div>
            <ActionAnchor appearance="standard" variant="secondary" href={`${path}/${encodeURIComponent(hotel.id)}`}><BookingCopy>View hotel</BookingCopy></ActionAnchor>
          </article>;
        }) : <><BookingCopy as="h2">Online reservations are not open yet</BookingCopy><BookingCopy as="p">Our hotels will appear here when they are ready to accept direct bookings.</BookingCopy></>}
      </section>
    </main>
  </GuestBookingShell>;
}
