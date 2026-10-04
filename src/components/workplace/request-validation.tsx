"use client";
import { useRef,useState,type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useFrontLanguage } from "@/components/front/front-language";
type Invalid={field:HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement;label:string;message:string};
/** Native constraints still prevent submission; errors remain next to their fields and in a summary. */
export function RequestValidation({children}:{children:ReactNode}){
 const th=useFrontLanguage()==="th",[errors,setErrors]=useState<Invalid[]>([]),first=useRef<HTMLElement|null>(null);
 return <div onInvalidCapture={event=>{
  const field=event.target as Invalid["field"];if(!field.validity)return;
  event.preventDefault();
  const label=field.getAttribute("aria-label")??[...field.labels?.[0]?.childNodes??[]].filter(n=>n.nodeType===3).map(n=>n.textContent?.trim()).filter(Boolean).join(" ")??"";
  const message=field.validity.valueMissing?(th?"กรุณากรอกข้อมูลนี้":"Complete this field."):field.validity.rangeUnderflow||field.validity.rangeOverflow?(th?"ตรวจสอบค่าที่อนุญาต":"Check the allowed value."):field.validity.tooShort?(th?"เพิ่มรายละเอียดให้ครบ":"Add the required detail."):(th?"ตรวจสอบรูปแบบข้อมูล":"Check this value's format.");
  field.setAttribute("aria-invalid","true");
  setErrors(previous=>[...previous.filter(e=>e.field!==field),{field,label:label||field.name|| (th?"ข้อมูลคำขอ":"Request field"),message}]);
  if(!first.current){first.current=field;field.focus();}
 }} onInputCapture={event=>{const field=event.target as Invalid["field"];field.removeAttribute?.("aria-invalid");first.current=null;setErrors(previous=>previous.filter(e=>e.field!==field));}}>
  {!!errors.length&&<section className="request-validation-summary" role="alert"><strong>{th?"ตรวจสอบข้อมูลก่อนดำเนินการ":"Check these details before continuing"}</strong><ul>{errors.map((e,i)=><li key={i}><button type="button" onClick={()=>e.field.focus()}>{e.label}: {e.message}</button></li>)}</ul></section>}
  {children}
  {errors.map((e,i)=>e.field.parentElement&&createPortal(<small className="request-field-error" role="alert">{e.message}</small>,e.field.parentElement,String(i)))}
 </div>;
}
