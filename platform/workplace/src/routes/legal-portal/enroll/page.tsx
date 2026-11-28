import { LegalEnrollment } from "@erpx/legal-ui/components/legal-enrollment";
import {headers} from 'next/headers';
import {configuredProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
export default async function Page() {
  const accountOrigin=configuredProductOrigin('account'),host=(await headers()).get('host');
  return <LegalEnrollment centralEnrollmentHref={accountOrigin&&host!==new URL(accountOrigin).host?new URL('/legal-portal/enroll',accountOrigin).href:undefined}/>;
}
