'use client';
import {useFrontLanguage} from '@erpx/front/components/front-language';
import type {BookingPolicy} from "@erpx/server-pms/public/rules/pms-model";
export function BookingPolicySummary({policy}:{policy?:BookingPolicy}){
 const language=useFrontLanguage();
 if(!policy)return <p>{language==='th'?'ติดต่อโรงแรมเพื่อสอบถามเงื่อนไขการจอง':'Contact the hotel for booking terms'}</p>;
 const legacyPayAtHotel=!policy.collection&&/^Free cancellation until \d{4}-\d{2}-\d{2} \d{2}:\d{2} \([^)]+\)\. After this deadline, contact the hotel\. Pay at the hotel\.$/.test(policy.text);
 if(language!=='th'||!policy.collection&&!legacyPayAtHotel)return <p>{policy.text}</p>;
 const collection:NonNullable<BookingPolicy['collection']>=policy.collection??{method:'pay_at_hotel',depositPercent:0,changesAllowed:true};
 const deadline=new Intl.DateTimeFormat('th-TH',{timeZone:policy.timezone,dateStyle:'medium',timeStyle:'short'}).format(policy.deadline),fee=collection.cancellationFeePercent??0;
 return <p>{collection.cancellationsAllowed===false?'การยกเลิกให้ติดต่อโรงแรม':`${fee?`ค่าธรรมเนียมยกเลิก ${fee}% ของยอดรวมการเข้าพักที่ยอมรับแล้ว`:'ยกเลิกได้โดยไม่มีค่าธรรมเนียม'} ก่อน ${deadline} (${policy.timezone}) หลังจากนั้นให้ติดต่อโรงแรม`}{' · '}{collection.method==='full'?'ชำระเต็มจำนวนออนไลน์':collection.method==='deposit'?`ชำระเงินค่าห้องล่วงหน้า ${collection.depositPercent}% ยอดที่เหลือชำระที่โรงแรม เงินค่าห้องล่วงหน้าแยกจากเงินประกันความเสียหาย`:'ชำระที่โรงแรม'}{collection.changesAllowed===false?' · การเปลี่ยนการจองให้ติดต่อโรงแรม':''}</p>;
}
