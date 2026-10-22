import type {ReactNode} from 'react';
import Link from 'next/link';
import {configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import {ProductBrand} from '@erpx/feature-brand/components/product-brand';
import {ProductNavigationBoundary} from '@erpx/workplace/components/product-navigation-boundary';
import styles from '@erpx/workplace/components/product-shell.module.css';

/** Personal Account navigation never depends on Company or provider authority. */
export function AccountShell({children,language='en',current}:{children?:ReactNode;language?:'en'|'th';current:'profile'|'access'|'security'}){
 const th=language==='th';
 return <ProductNavigationBoundary language={language}><div className={styles.shell}>
  <header className={styles.header}><Link href="/" prefetch={false} data-product-switch className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark}/><strong>ERPX Account</strong></Link>
   <nav className={styles.links} aria-label={th?'บัญชีส่วนตัว':'Personal account'}>
    <Link href="/my-profile" prefetch={false} data-product-switch aria-current={current==='profile'?'page':undefined}>{th?'โปรไฟล์ของฉัน':'My profile'}</Link>
    <Link href="/my-access" prefetch={false} data-product-switch aria-current={current==='access'?'page':undefined}>{th?'สิทธิ์ของฉัน':'My access'}</Link>
    <Link href="/security" prefetch={false} data-product-switch aria-current={current==='security'?'page':undefined}>{th?'ความปลอดภัย':'Security'}</Link>
    <a data-product-switch href={configuredProductHandoff('apps')!}>Apps</a>
    <a data-product-switch href="/login?next=%2F">{th?'เปลี่ยนบัญชี':'Switch account'}</a>
   </nav>
  </header>{children}
 </div></ProductNavigationBoundary>;
}
