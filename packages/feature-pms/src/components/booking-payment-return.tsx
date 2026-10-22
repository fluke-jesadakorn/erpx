"use client";

import {useEffect,useState} from "react";
import {FrontLocalizedTree} from "@erpx/front/components/front-language";
import {ActionAnchor,InlineNotice} from "@erpx/ui/components/foundation";
import type {bookingPaymentStatus} from "@erpx/server-pms/public/pms-booking";
import styles from "@erpx/feature-pms/components/booking.module.css";

type PaymentStatus=Awaited<ReturnType<typeof bookingPaymentStatus>>;
export type BookingPaymentReturnStatus=Pick<PaymentStatus,"number"|"state"|"paymentState"|"checkoutUrl"|"managementToken"|"refundState">;

/** The original signed payment return retains history independently of public
 * hotel publication. Expired payments can still reconcile, and failed refunds
 * remain retryable until the original owner confirms successful settlement. */
export function BookingPaymentReturn({slug,propertyId,attempt,initial}:{slug:string;propertyId:string;attempt:string;initial:BookingPaymentReturnStatus}){
 const [status,setStatus]=useState(initial),[error,setError]=useState("");
 useEffect(()=>{
  const pendingPayment=status.state==="payment_pending"||status.state==="expired";
  const pendingRefund=(status.state==="refund_review"||!!status.refundState)&&status.refundState!=="succeeded";
  if(!pendingPayment&&!pendingRefund)return;
  const controller=new AbortController();let pending=false;
  const timer=window.setInterval(async()=>{
   if(pending)return;pending=true;
   try{
    const response=await fetch(`/api/public/booking/${encodeURIComponent(slug)}/reservations?`+new URLSearchParams({attempt}),{cache:"no-store",signal:controller.signal}),body=await response.json();
    if(!response.ok)throw Error("Payment status unavailable");
    const next=body?.data as PaymentStatus|undefined;
    if(!next||next.attemptToken!==attempt||next.hotel?.id!==propertyId||next.number!==initial.number||typeof next.state!=="string")throw Error("Payment status unavailable");
    if(!controller.signal.aborted){setStatus({number:next.number,state:next.state,paymentState:next.paymentState,checkoutUrl:next.checkoutUrl,managementToken:next.managementToken,refundState:next.refundState});setError("");}
   }catch(error){if(!controller.signal.aborted)setError(error instanceof Error?error.message:"Payment status unavailable");}
   finally{pending=false;}
  },5000);
  return()=>{controller.abort();window.clearInterval(timer);};
 },[slug,propertyId,attempt,initial.number,status.state,status.refundState]);
 const heading=status.state==="payment_pending"?"Waiting for payment":status.state==="expired"?"Payment period expired":status.state==="refund_review"?"Payment needs review":status.state==="confirmed"?"Booking confirmed":status.state==="cancelled"?"Booking cancelled":"Checking booking status";
 return <FrontLocalizedTree><section data-ui-appearance="standard" className={styles.notice} aria-label="Booking payment status">
  <div role="status"><h1>{heading} · {status.number}</h1><p>{status.state==="payment_pending"?"Your room is held for 30 minutes. Complete payment in Stripe Checkout.":status.state==="confirmed"?status.paymentState==="received"?"Payment confirmed. A confirmation email and private management link are on the way.":"Your room is reserved. Pay the balance at the hotel.":status.state==="refund_review"?"The hold expired before payment confirmation. Finance will review a refund.":"The reservation is no longer held."}</p></div>
  {status.state==="payment_pending"&&status.checkoutUrl&&<ActionAnchor appearance="standard" variant="secondary" href={status.checkoutUrl}>Continue to secure payment</ActionAnchor>}
  {status.managementToken&&<ActionAnchor appearance="standard" variant="secondary" href={`/book/${encodeURIComponent(slug)}/manage#token=${encodeURIComponent(status.managementToken)}`}>Manage booking</ActionAnchor>}
  {status.refundState&&<p>Refund: {status.refundState}</p>}
  {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
 </section></FrontLocalizedTree>;
}
