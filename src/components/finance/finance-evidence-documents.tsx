"use client";
import {RecordDocumentPanel} from "@/components/files/record-document-panel";
export function FinanceEvidenceDocuments({base,row,targetType,canWrite}:{base:string;row:Record<string,unknown>;targetType:string;canWrite:boolean}){
 return <RecordDocumentPanel appearance="standard" base={base} targetType={targetType} targetId={String(row.id)} expectedTargetVersion={Number(row.version)} editable={canWrite&&["draft",...(targetType==="finance_bank_match_group"?["proposed"]:[])].includes(String(row.status))} purposes={[{key:"finance_support",label:"Finance source documents",description:"Retain supporting files with this record. Bind clean scanned files before independent approval; reviewed evidence keeps its original checksum."}]}/>;
}
