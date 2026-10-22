import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { thaiBahtWords } from "@erpx/feature-finance/lib/thai-finance-tax-pack";

export type ThaiDocumentIdentity = { id:string;legal_name:string;tax_id:string;address:string;branch_code:string;signer_name:string|null;signature_data_uri:string|null;signature_sha256:string|null };
export type ThaiDocumentSnapshot = {
  schema:"finance.thai.document.v1";kind:"withholding_certificate"|"tax_invoice";documentNumber:string;issueDate:string;paymentDate?:string;
  payer:ThaiDocumentIdentity;payee:ThaiDocumentIdentity;formKind?:string;incomeMinor:number;taxMinor:number;grossMinor:number;
  items:{description:string;quantity?:string;unitPriceMinor?:number;incomeMinor:number;taxMinor:number;section?:string;category?:string;rateBps?:number}[];
  source:{invoiceId?:string;paymentId?:string;assessmentId?:string;rulePackVersion:string;rulePackDigest:string;[key:string]:unknown};
  previousDocumentNumber?:string;edition:number;reason:string;
};
function escape(value:unknown){return String(value??"").replace(/[&<>"']/g,char=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[char]!));}
function thaiDate(date:string){const [year,month,day]=date.split("-");return `${day}/${month}/${Number(year)+543}`;}

export function renderThaiDocument(snapshot:ThaiDocumentSnapshot){
  const wht=snapshot.kind==="withholding_certificate", title=wht?"หนังสือรับรองการหักภาษี ณ ที่จ่าย":"ใบกำกับภาษี";
  const copies=wht?["ฉบับที่ 1 สำหรับผู้ถูกหักภาษี ณ ที่จ่าย ใช้แนบพร้อมกับแบบแสดงรายการ","ฉบับที่ 2 สำหรับผู้ถูกหักภาษี ณ ที่จ่าย เก็บไว้เป็นหลักฐาน","สำเนาคู่ฉบับ สำหรับผู้หักภาษี ณ ที่จ่าย เก็บไว้เป็นหลักฐาน"]:["ต้นฉบับ สำหรับผู้ซื้อสินค้าหรือผู้รับบริการ","สำเนา สำหรับผู้ประกอบการจดทะเบียน เก็บไว้เป็นหลักฐาน"];
  const identity=(value:ThaiDocumentIdentity,label:string)=>`<section><h2>${label}</h2><p>${escape(value.legal_name)}</p><p>เลขประจำตัวผู้เสียภาษีอากร ${escape(value.tax_id)} · ${value.branch_code==="00000"?"สำนักงานใหญ่":`สาขาที่ ${escape(value.branch_code)}`}</p><p>ที่อยู่ ${escape(value.address)}</p></section>`;
  const html=copies.map((copy,index)=>`<article data-copy="${index+1}"><p class="copy">${copy}</p><h1>${title}</h1>${wht?"<p>ตามมาตรา 50 ทวิ แห่งประมวลรัษฎากร</p>":""}<p>เลขที่ ${escape(snapshot.documentNumber)} · วันที่ออก ${thaiDate(snapshot.issueDate)}</p>${snapshot.previousDocumentNumber?`<p>ออกแทนฉบับเลขที่ ${escape(snapshot.previousDocumentNumber)} · เหตุผล ${escape(snapshot.reason)}</p>`:""}
    ${identity(snapshot.payer,wht?"ผู้มีหน้าที่หักภาษี ณ ที่จ่าย":"ผู้ประกอบการจดทะเบียน")}${identity(snapshot.payee,wht?"ผู้ถูกหักภาษี ณ ที่จ่าย":"ผู้ซื้อสินค้าหรือผู้รับบริการ")}
    ${wht?`<p>ลำดับที่ในการยื่นแบบ ${escape(snapshot.documentNumber)} · แบบแสดงรายการภาษีที่ใช้ ${snapshot.formKind==="PND3"?"☑ ภ.ง.ด.3 ☐ ภ.ง.ด.53":"☐ ภ.ง.ด.3 ☑ ภ.ง.ด.53"}</p>`:""}
    <table><thead><tr><th>${wht?"ประเภทเงินได้พึงประเมินที่จ่าย":"ชื่อ ชนิด หรือประเภทสินค้า/บริการ"}</th><th>${wht?"วัน เดือน ปี ที่จ่าย":"ปริมาณ / ราคาต่อหน่วย"}</th><th>${wht?"จำนวนเงินที่จ่าย":"มูลค่าก่อนภาษี"}</th><th>${wht?"ภาษีที่หักและนำส่งไว้":"ภาษีมูลค่าเพิ่ม"}</th></tr></thead><tbody>${snapshot.items.map(item=>`<tr><td>${escape(item.description)}${item.section?` · มาตรา ${escape(item.section)}`:""}</td><td>${wht?thaiDate(snapshot.paymentDate!):`${escape(item.quantity??"")} / ${minorToDecimal(item.unitPriceMinor??0)}`}</td><td>${minorToDecimal(item.incomeMinor)}</td><td>${minorToDecimal(item.taxMinor)}</td></tr>`).join("")}</tbody><tfoot><tr><th colspan="2">รวม</th><td>${minorToDecimal(snapshot.incomeMinor)}</td><td>${minorToDecimal(snapshot.taxMinor)}</td></tr></tfoot></table>
    <p>${wht?"รวมเงินภาษีที่หักนำส่ง (ตัวอักษร)":"จำนวนเงินรวมทั้งสิ้น"}: ${wht?thaiBahtWords(snapshot.taxMinor):`${minorToDecimal(snapshot.grossMinor)} บาท (${thaiBahtWords(snapshot.grossMinor)})`}</p>
    ${wht?`<p>ผู้จ่ายเงิน ☑ หัก ณ ที่จ่าย ☐ ออกให้ตลอดไป ☐ ออกให้ครั้งเดียว ☐ อื่น ๆ</p><p>ขอรับรองว่าข้อความและตัวเลขดังกล่าวข้างต้นถูกต้องตรงกับความจริงทุกประการ</p><img class="signature" alt="ลายมือชื่อผู้จ่ายเงิน" src="${escape(snapshot.payer.signature_data_uri)}"/><p>ลงชื่อ ${escape(snapshot.payer.signer_name)} (ผู้จ่ายเงิน) · ${thaiDate(snapshot.issueDate)}</p><p>คำเตือน: ผู้มีหน้าที่ออกหนังสือรับรองการหักภาษี ณ ที่จ่าย ต้องปฏิบัติตามมาตรา 50 ทวิ แห่งประมวลรัษฎากร</p>`:""}
    <small>รุ่นเอกสาร ${snapshot.edition} · หลักฐานต้นทาง ${escape(snapshot.source.assessmentId??snapshot.source.invoiceId)} · กฎ ${escape(snapshot.source.rulePackVersion)}</small></article>`).join("");
  return `<!doctype html><html lang="th"><head><meta charset="utf-8"><title>${title} ${escape(snapshot.documentNumber)}</title><style>body{font-family:"Noto Sans Thai",Tahoma,sans-serif;color:#111;margin:0}article{padding:24px;max-width:760px;margin:auto;break-after:page}h1{text-align:center;font-size:24px}h2{font-size:15px;margin-bottom:4px}p{font-size:13px;line-height:1.5;margin:5px 0}section{border:1px solid #444;padding:10px;margin:10px 0}table{border-collapse:collapse;width:100%;margin:14px 0;font-size:12px}th,td{border:1px solid #444;padding:8px;text-align:left}td:nth-last-child(-n+2){text-align:right}.signature{max-width:200px;max-height:70px}.copy{text-align:right;font-size:11px}small{font-size:10px}@page{size:A4;margin:10mm}@media print{article{padding:0}}</style></head><body>${html}</body></html>`;
}
