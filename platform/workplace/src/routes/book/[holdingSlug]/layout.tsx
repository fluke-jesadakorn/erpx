import { cookies } from 'next/headers';
import { GuestBookingShell } from '@erpx/pms-ui/components/guest-booking-shell';
import { preferredAccountAppearance } from "@erpx/identity-access-server/public/account-preference-snapshot";
export default async function BookingLayout({children,params}:{children:React.ReactNode;params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,cookie=await cookies();
 return <GuestBookingShell slug={holdingSlug} initialLanguage={cookie.get('erpx_booking_language')?.value==='th'?'th':'en'} initialTheme={await preferredAccountAppearance(cookie.get('erpx_appearance')?.value)}>{children}</GuestBookingShell>;
}
