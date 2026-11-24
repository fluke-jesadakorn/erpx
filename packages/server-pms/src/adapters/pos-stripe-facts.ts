import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {nativePmsOwnerWrite} from "@erpx/feature-pms/public/native-pms";
import {nativePmsReadFirst, PMS_RD_POS_STRIPE_PAYMENT, PMS_RD_POS_BOOKING_PAYMENT} from './native-pms-read';
import {pmsPosPropertyConfigSql} from './pms-read-tail-r33-sql';
import {pmsRefundPaymentJoinHeadSql} from './pms-read-tail-r34-sql';

export function posStripePmsFacts(db:SqlDatabase,companyId = ''){return {
readPmsStripePayments1:(facts:{counterId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>nativePmsReadFirst<Record<string,string|number|boolean|null>>(PMS_RD_POS_STRIPE_PAYMENT,[facts.counterId,facts.company_id,facts.account_id,facts.livemode],db),
writePmsStripePayments1:(facts:{provider_created_at:string | number | true;counterId:string;})=>nativePmsOwnerWrite(companyId,'pms_w098',[facts.provider_created_at,facts.counterId]),
readPmsBookingPayments1:(facts:{bookingId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>nativePmsReadFirst<Record<string,string|number|boolean|null>>(PMS_RD_POS_BOOKING_PAYMENT,[facts.bookingId,facts.company_id,facts.account_id,facts.livemode],db),
writePmsBookingPayments1:(facts:{provider_created_at:string | number | true;bookingId:string;})=>nativePmsOwnerWrite(companyId,'pms_w099',[facts.provider_created_at,facts.bookingId]),
readPmsStripePayments2:(facts:{counterId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>nativePmsReadFirst<Record<string,string|number|boolean|null>>(PMS_RD_POS_STRIPE_PAYMENT,[facts.counterId,facts.company_id,facts.account_id,facts.livemode],db),
readPmsBookingPayments2:(facts:{bookingId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>nativePmsReadFirst<Record<string,string|number|boolean|null>>(PMS_RD_POS_BOOKING_PAYMENT,[facts.bookingId,facts.company_id,facts.account_id,facts.livemode],db),
readPmsBookingPayments3:(facts:{bookingId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>one(db,"SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND status IN ('confirmed','cancelled','refund_review')",facts.bookingId,facts.company_id,facts.account_id,facts.livemode),
readPmsStripePayments3:(facts:{counterId:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>one(db,"SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",facts.counterId,facts.company_id,facts.account_id,facts.livemode),
readPmsBookingRefundRequests1:(facts:{source:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>one(db,`${pmsRefundPaymentJoinHeadSql('requestId')} WHERE r.provider_id=? AND p.company_id=? AND p.account_id=? AND p.livemode=?`,facts.source,facts.company_id,facts.account_id,facts.livemode),
readPmsStripePayments4:(facts:{source:string;company_id:string | number | boolean | null;account_id:string | number | boolean | null;livemode:string | number | boolean | null;})=>one(db,"SELECT id FROM pms_stripe_payments WHERE refund_id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",facts.source,facts.company_id,facts.account_id,facts.livemode),
readPmsProperties1:(facts:{companyId:string;})=>one(db,pmsPosPropertyConfigSql('booking'),facts.companyId),
readPmsProperties2:(facts:{companyId:string;})=>one(db,pmsPosPropertyConfigSql('stripe'),facts.companyId)
};}
