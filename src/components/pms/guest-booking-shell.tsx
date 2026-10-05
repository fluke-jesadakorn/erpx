"use client";
import { useState,type ReactNode } from 'react';
import Link from 'next/link';
import { FrontLanguageContext,FrontLocalizedTree,type FrontLanguage } from '@/components/front/front-language';
import { AppearanceSwitcher } from '@/components/brand/appearance-switcher';
import { Select } from '@/components/ui/foundation';
import type { AppearancePreference } from '@/lib/appearance-preference';
import styles from './booking.module.css';
export function GuestBookingShell({slug,initialLanguage,initialTheme,children}:{slug:string;initialLanguage:FrontLanguage;initialTheme:AppearancePreference;children:ReactNode}){
 const [language,setLanguage]=useState(initialLanguage);
 return <FrontLanguageContext.Provider value={language}><div className={styles.guestShell}>
  <nav className={styles.guestNav} aria-label={language==='th'?'เมนูการจอง':'Booking navigation'}>
   <Link href={`/book/${slug}`} className={styles.brand}>{language==='th'?'พักผ่อนกับเรา':'Stay with us'}</Link>
   <Link href={`/book/${slug}/manage`}>{language==='th'?'จัดการการจอง':'Manage booking'}</Link>
   <Select appearance="standard" aria-label="Language / ภาษา" value={language} onChange={e=>{const next=e.target.value as FrontLanguage;setLanguage(next);document.cookie=`erpx_booking_language=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;}}><option value="th">ไทย</option><option value="en">English</option></Select>
   <AppearanceSwitcher appearance="standard" compact initialTheme={initialTheme}/>
  </nav><FrontLocalizedTree>{children}</FrontLocalizedTree>
  <footer className={styles.guestFooter}>{language==='th'?'จองตรงกับโรงแรม · ไม่ต้องสมัครสมาชิก':'Book directly with the hotel · No account needed'}</footer>
 </div></FrontLanguageContext.Provider>;
}
