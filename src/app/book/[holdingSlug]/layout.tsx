import { cookies } from 'next/headers';
import { GuestBookingShell } from '@/components/pms/guest-booking-shell';
import { preferredAccountAppearance } from "@/server/modules/identity_access/public/account-preference-snapshot";
export default async function BookingLayout({children,params}:{children:React.ReactNode;params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,cookie=await cookies();
 return <GuestBookingShell slug={holdingSlug} initialLanguage={cookie.get('erpx_booking_language')?.value==='th'?'th':'en'} initialTheme={await preferredAccountAppearance(cookie.get('erpx_appearance')?.value)}>{children}</GuestBookingShell>;
}
