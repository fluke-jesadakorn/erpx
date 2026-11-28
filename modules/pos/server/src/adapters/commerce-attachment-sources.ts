import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativePosCommerceAttachmentImageStatement} from './native-pos-core-statement';
/** Original generic product-image fallback remains POS-owned. This private
 * witness exposes no catalog or register-management permission. */
export function commerceAttachmentPosSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pos_catalog,pos_register_catalog,pos_registers IN SHARE ROW EXCLUSIVE MODE');}
export function commerceAttachmentPosSourceImage(database:SqlDatabase,companyId:string,productId:string){return nativePosCommerceAttachmentImageStatement<{image:string}>(database,companyId,productId).then(row=>row?.image??null);}
