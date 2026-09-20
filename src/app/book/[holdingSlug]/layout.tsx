import Link from "next/link";
import type { Metadata } from "next";
import styles from "@/components/pms/booking.module.css";
export const metadata: Metadata = {
  title: "Book your stay | ERPX",
  referrer: "no-referrer",
};
export default async function BookingLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  return (
    <div className={styles.page}>
      <nav className={styles.nav} aria-label="Guest booking">
        <Link href={`/book/${holdingSlug}`}>
          <strong>STAYS</strong> · Find your next stay
        </Link>
        <Link href={`/book/${holdingSlug}/manage`}>Manage booking</Link>
      </nav>
      {children}
    </div>
  );
}
