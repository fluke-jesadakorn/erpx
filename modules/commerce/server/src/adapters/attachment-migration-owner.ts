import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeCommerceReadFirst,RD_LEGACY_ATTACHMENT_SCOPE} from './native-commerce-read';
const ownerKinds:Record<string,string>={"commerce_order_delivery":RD_LEGACY_ATTACHMENT_SCOPE};
export function legacyAttachmentScopecommerce(db:SqlDatabase,kind:string,id:string){const ownerKind=ownerKinds[kind];if(!ownerKind)return Promise.resolve(null);return nativeCommerceReadFirst<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>(ownerKind,[id],db);}
