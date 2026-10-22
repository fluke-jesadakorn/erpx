import { LegalEnrollment } from "@erpx/feature-legal/components/legal-enrollment";
import {headers} from 'next/headers';
import {configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
export default async function Page() {
  const accountOrigin=configuredProductOrigin('account'),host=(await headers()).get('host');
  return <LegalEnrollment centralEnrollmentHref={accountOrigin&&host!==new URL(accountOrigin).host?new URL('/legal-portal/enroll',accountOrigin).href:undefined}/>;
}
