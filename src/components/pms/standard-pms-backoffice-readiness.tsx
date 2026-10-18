'use client';
import {Badge,Card} from '@/components/ui/foundation';
import Link from '@/components/workplace/workspace-link';
import {useFrontLanguage} from '@/components/front/front-language';
import {frontText} from '@/lib/shared/front-language';
import styles from './standard-pms-backoffice-workspace.module.css';

/** Readiness remains the native owner's projection; setup links neither create
 * Finance/Stock authority nor replace the hotel's original opening action. */
export function StandardPmsBackofficeReadiness({title,items}:{title:string;items:{id:string;label:string;ready:boolean;owner:string;href:string;detail?:string}[]}){
 const language=useFrontLanguage(),tr=(value:string)=>frontText(value,language),missing=items.filter(item=>!item.ready).length;
 return <Card appearance="standard" aria-label={tr(title)} className={styles.readiness}>
  <header><h2>{tr(title)}</h2><Badge tone={missing?'warning':'success'}>{missing?(language==='th'?`เตรียม ${missing} รายการ`:`${missing} to prepare`):tr('Ready')}</Badge></header>
  <ul>{items.map(item=><li key={item.id}><span aria-hidden="true">{item.ready?'✓':'○'}</span><div><strong>{tr(item.label)}</strong><p>{tr(item.owner)}{item.detail?' · '+tr(item.detail):''}</p></div><Link href={item.href}>{tr(item.ready?'Review':'Set up')}</Link></li>)}</ul>
 </Card>;
}
