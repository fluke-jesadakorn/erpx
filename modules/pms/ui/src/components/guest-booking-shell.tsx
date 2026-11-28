"use client";
import { useState,type ReactNode } from 'react';
import Link from "@erpx/platform-workplace/components/workspace-link";
import { FrontLanguageContext,type FrontLanguage } from '@erpx/platform-front/components/front-language';
import { AppearanceSwitcher } from '@erpx/brand-ui/components/appearance-switcher';
import { Select } from '@erpx/platform-ui/components/foundation';
import type { AppearancePreference } from "@erpx/platform-shared-ui/lib/appearance-preference";
import {frontText} from "@erpx/platform-shared-ui/lib/front-language";
import styles from '@erpx/pms-ui/components/booking.module.css';
export function GuestBookingShell({slug,initialLanguage,initialTheme,children}:{slug?:string;initialLanguage:FrontLanguage;initialTheme:AppearancePreference;children:ReactNode}){
 const [language,setLanguage]=useState(initialLanguage);
 return <FrontLanguageContext.Provider value={language}><div className={styles.guestShell}>
  <nav className={styles.guestNav} aria-label={language==='th'?'เมนูการจอง':'Booking navigation'}>
   <Link href={slug?`/book/${slug}`:'/'} className={styles.brand}>{language==='th'?'พักผ่อนกับเรา':'Stay with us'}</Link>
   {slug&&<Link href={`/book/${slug}/manage`}>{language==='th'?'จัดการการจอง':'Manage booking'}</Link>}
   <Select appearance="standard" aria-label="Language / ภาษา" value={language} onChange={e=>{const next=e.target.value as FrontLanguage;setLanguage(next);document.cookie=`erpx_booking_language=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;}}><option value="th">ไทย</option><option value="en">English</option></Select>
   <AppearanceSwitcher appearance="standard" compact initialTheme={initialTheme} labels={{label:frontText('Theme',language),system:frontText('System',language),light:frontText('Light',language),dark:frontText('Dark',language)}}/>
  </nav>{children}
  <footer className={styles.guestFooter}>{language==='th'?'จองตรงกับโรงแรม · ไม่ต้องสมัครสมาชิก':'Book directly with the hotel · No account needed'}</footer>
 </div></FrontLanguageContext.Provider>;
}
