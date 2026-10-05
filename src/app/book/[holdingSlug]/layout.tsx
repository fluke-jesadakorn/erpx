import { cookies } from 'next/headers';
import { GuestBookingShell } from '@/components/pms/guest-booking-shell';
import { appearancePreference } from '@/lib/appearance-preference';
export default async function BookingLayout({children,params}:{children:React.ReactNode;params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,cookie=await cookies();
 return <GuestBookingShell slug={holdingSlug} initialLanguage={cookie.get('erpx_booking_language')?.value==='th'?'th':'en'} initialTheme={appearancePreference(cookie.get('erpx_appearance')?.value)}>{children}</GuestBookingShell>;
}
