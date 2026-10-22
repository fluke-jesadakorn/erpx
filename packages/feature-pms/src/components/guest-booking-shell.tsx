"use client";
import { useState,type ReactNode } from 'react';
import Link from "@erpx/workplace/components/workspace-link";
import { FrontLanguageContext,FrontLocalizedTree,type FrontLanguage } from '@erpx/front/components/front-language';
import { AppearanceSwitcher } from '@erpx/feature-brand/components/appearance-switcher';
import { Select } from '@erpx/ui/components/foundation';
import type { AppearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import {frontText} from "@erpx/feature-shared/lib/front-language";
import styles from '@erpx/feature-pms/components/booking.module.css';
export function GuestBookingShell({slug,initialLanguage,initialTheme,children}:{slug?:string;initialLanguage:FrontLanguage;initialTheme:AppearancePreference;children:ReactNode}){
 const [language,setLanguage]=useState(initialLanguage);
 return <FrontLanguageContext.Provider value={language}><div className={styles.guestShell}>
  <nav className={styles.guestNav} aria-label={language==='th'?'เมนูการจอง':'Booking navigation'}>
   <Link href={slug?`/book/${slug}`:'/'} className={styles.brand}>{language==='th'?'พักผ่อนกับเรา':'Stay with us'}</Link>
   {slug&&<Link href={`/book/${slug}/manage`}>{language==='th'?'จัดการการจอง':'Manage booking'}</Link>}
   <Select appearance="standard" aria-label="Language / ภาษา" value={language} onChange={e=>{const next=e.target.value as FrontLanguage;setLanguage(next);document.cookie=`erpx_booking_language=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;}}><option value="th">ไทย</option><option value="en">English</option></Select>
   <AppearanceSwitcher appearance="standard" compact initialTheme={initialTheme} labels={{label:frontText('Theme',language),system:frontText('System',language),light:frontText('Light',language),dark:frontText('Dark',language)}}/>
  </nav><FrontLocalizedTree>{children}</FrontLocalizedTree>
  <footer className={styles.guestFooter}>{language==='th'?'จองตรงกับโรงแรม · ไม่ต้องสมัครสมาชิก':'Book directly with the hotel · No account needed'}</footer>
 </div></FrontLanguageContext.Provider>;
}
