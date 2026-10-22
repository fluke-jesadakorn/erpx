import {z} from 'zod';
export const customerNotificationSchema=z.object({expectedVersion:z.number().int().positive(),note:z.string().trim().min(3).max(500)});
export const customerNotificationRetrySchema=z.object({expectedVersion:z.number().int().positive()});
export const customerNotificationSettingsSchema=z.object({expectedVersion:z.number().int().min(0),adapter:z.enum(['disabled','sandbox','resend'])});


export type CustomerMail={id:string;company_id:string;fulfillment_id:string;order_id:string;party_id:string;party_version:number;recipient_email:string;subject:string;body_text:string;adapter:string;attempts:number};

export type CustomerNotification=z.infer<typeof customerNotificationSchema>;
export type CustomerNotificationRetry=z.infer<typeof customerNotificationRetrySchema>;
export type CustomerNotificationSettings=z.infer<typeof customerNotificationSettingsSchema>;
export type NotificationRow=Record<string,string|number|null>;
export function customerNotificationInvalid(mail:CustomerMail,current:NotificationRow|null){return !current?'CUSTOMER_NOTIFICATION_SOURCE_CHANGED':current.status!=='active'||current.notification_consent!=='granted'||String(current.notification_email??'').toLowerCase()!==mail.recipient_email?'CUSTOMER_CONSENT_OR_EMAIL_CHANGED':current.adapter!==mail.adapter?'CUSTOMER_NOTIFICATION_PROVIDER_CHANGED':null;}
export function customerNotificationRetryAt(now:number,attempts:number){return now+Math.min(3600000,1000*2**Math.min(attempts,10));}
export function customerNotificationMessage(row:NotificationRow,lines:NotificationRow[],note:string){return {subject:`Delivery ${String(row.fulfillment_number)} for order ${String(row.order_number)}`,body:[`Hello ${String(row.party_name)},`,`Your delivery ${String(row.fulfillment_number)} for order ${String(row.order_number)} was recorded on ${String(row.fulfillment_date)}.`,...lines.map(line=>`${String(line.sku)}: ${Number(line.quantity_milli)/1000} ${String(line.unit)}`),`Update from our team: ${note}`].join('\n\n')};}
