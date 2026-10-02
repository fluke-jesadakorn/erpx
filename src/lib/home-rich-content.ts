import type { Mention } from "./social-text";

export type RichNode = { type:string; attrs?:Record<string,unknown>; text?:string; marks?:Array<{type:string;attrs?:Record<string,unknown>}>; content?:RichNode[] };
export type RichContent = { version:1; blocks:Array<{type:"text";doc:RichNode}|{type:"image";slotId:string;attachmentId:string|null;name:string;alt?:string;width?:number}> };
const nodes=new Set(["doc","paragraph","heading","blockquote","bulletList","orderedList","listItem","table","tableRow","tableCell","tableHeader","hardBreak","text","horizontalRule"]);
const marks=new Set(["bold","italic","underline","strike","code","link","textStyle","highlight","mention"]);
const alignments=new Set(["left","right","center","justify"]);
const fonts=new Set(["Arial","Georgia","Times New Roman","Noto Sans Thai","Noto Serif Thai","Courier New"]);
const colors=/^#[0-9a-fA-F]{6}$/;
const sizes=/^(?:1[0-9]|2[0-9]|3[0-6])px$/;
const lineHeights=new Set(["1","1.15","1.5","2"]);
export function safeRichUrl(value:unknown):string|null {
  if(typeof value!=="string" || value.length>2048) return null;
  try { const url=new URL(value); return ["http:","https:"].includes(url.protocol)?url.href:null; } catch { return null; }
}
function checkNode(node:RichNode,depth:number):void {
  if(depth>20||!node||!nodes.has(node.type)) throw Error("Unsupported document element.");
  const attrs=node.attrs??{};
  const allowed:Record<string,string[]>={doc:[],paragraph:["textAlign"],heading:["level","textAlign"],blockquote:[],bulletList:[],orderedList:["start","type"],listItem:[],table:[],tableRow:[],tableCell:["colspan","rowspan","colwidth","align","background"],tableHeader:["colspan","rowspan","colwidth","align","background"],hardBreak:[],text:[],horizontalRule:[]};
  if(Object.keys(attrs).some(key=>!allowed[node.type].includes(key)))throw Error("Unsupported document attribute.");
  if(node.type==="text") { if(typeof node.text!=="string") throw Error("Invalid document text."); }
  else if(node.text!==undefined) throw Error("Unexpected document text.");
  if(node.type==="heading"&&(!Number.isInteger(attrs.level)||Number(attrs.level)<1||Number(attrs.level)>3)) throw Error("Invalid heading.");
  if((node.type==="paragraph"||node.type==="heading")&&attrs.textAlign!=null&&!alignments.has(String(attrs.textAlign))) throw Error("Invalid alignment.");
  if(node.type==="orderedList"&&attrs.start!=null&&(!Number.isInteger(attrs.start)||Number(attrs.start)<1||Number(attrs.start)>100000))throw Error("Invalid list.");
  if(["tableCell","tableHeader"].includes(node.type)) {
    if(attrs.align!=null&&!["left","center","right"].includes(String(attrs.align)))throw Error("Invalid cell alignment.");
    if(attrs.colspan!=null&&(!Number.isInteger(attrs.colspan)||Number(attrs.colspan)>10||Number(attrs.colspan)<1)) throw Error("Invalid table span.");
    if(attrs.rowspan!=null&&(!Number.isInteger(attrs.rowspan)||Number(attrs.rowspan)>30||Number(attrs.rowspan)<1)) throw Error("Invalid table span.");
    if(attrs.colwidth!=null&&(!Array.isArray(attrs.colwidth)||attrs.colwidth.some(v=>!Number.isInteger(v)||v<40||v>1200))) throw Error("Invalid column width.");
    if(attrs.background!=null&&!colors.test(String(attrs.background)))throw Error("Invalid cell color.");
  }
  if(node.marks) for(const mark of node.marks) {
    if(!marks.has(mark.type)) throw Error("Unsupported text style.");
    const a=mark.attrs??{};
    const markAllowed:Record<string,string[]>={bold:[],italic:[],underline:[],strike:[],code:[],link:["href","target","rel","class"],textStyle:["fontFamily","fontSize","color","backgroundColor","lineHeight"],highlight:["color"],mention:["kind","targetId","label"]};
    if(Object.keys(a).some(key=>!markAllowed[mark.type].includes(key)))throw Error("Unsupported text attribute.");
    if(mark.type==="link"&&!safeRichUrl(a.href)) throw Error("Invalid link.");
    if(mark.type==="textStyle") {
      if(a.fontFamily!=null&&!fonts.has(String(a.fontFamily))) throw Error("Invalid font.");
      if(a.fontSize!=null&&!sizes.test(String(a.fontSize))) throw Error("Invalid font size.");
      if(a.color!=null&&!colors.test(String(a.color))) throw Error("Invalid color.");
      if(a.backgroundColor!=null&&!colors.test(String(a.backgroundColor))) throw Error("Invalid highlight.");
      if(a.lineHeight!=null&&!lineHeights.has(String(a.lineHeight))) throw Error("Invalid line height.");
    }
    if(mark.type==="mention"&&(typeof a.targetId!=="string"||typeof a.label!=="string"||!["agent","person","record"].includes(String(a.kind)))) throw Error("Invalid mention.");
  }
  if(node.content) { if(!Array.isArray(node.content)||node.content.length>1000) throw Error("Document is too long.");node.content.forEach(child=>checkNode(child,depth+1)); }
}
export function validateRichContent(input:unknown,maxChars:number):{content:RichContent;body:string;mentions:Mention[]} {
  const json=JSON.stringify(input);
  if(!json||new TextEncoder().encode(json).length>1048576) throw Error("Document exceeds 1 MB.");
  const content=input as RichContent;
  if(content?.version!==1||!Array.isArray(content.blocks)||content.blocks.length>100) throw Error("Unsupported document version.");
  let body="";const mentions:Mention[]=[];let textBlocks=0;let images=0;
  for(const block of content.blocks) {
    if(block.type==="image") {if(typeof block.slotId!=="string"||block.slotId.length<1||block.slotId.length>160||typeof block.name!=="string"||block.name.length<1||block.name.length>255||(block.attachmentId!==null&&(typeof block.attachmentId!=="string"||block.attachmentId.length>160))||(block.alt!==undefined&&(typeof block.alt!=="string"||block.alt.length>500))||(block.width!==undefined&&(!Number.isInteger(block.width)||block.width<25||block.width>100))||Object.keys(block).some(key=>!["type","slotId","attachmentId","name","alt","width"].includes(key)))throw Error("Invalid image.");images++;continue;}
    if(block.type!=="text"||block.doc?.type!=="doc") throw Error("Invalid document.");
    checkNode(block.doc,0);
    if(textBlocks++)body+="\n";
    const walk=(node:RichNode):void=>{
      if(node.type==="text") {
        const start=body.length;
        body+=node.text??"";
        const mark=node.marks?.find(m=>m.type==="mention");
        if(mark) mentions.push({kind:mark.attrs?.kind as Mention["kind"],targetId:String(mark.attrs?.targetId),label:String(mark.attrs?.label),start,end:body.length});
        return;
      }
      if(node.type==="hardBreak") {body+="\n";return;}
      for(const [index,child] of (node.content??[]).entries()) {
        if(index>0&&["doc","blockquote","bulletList","orderedList","listItem","table"].includes(node.type))body+="\n";
        if(index>0&&node.type==="tableRow")body+="\t";
        walk(child);
      }
    };
    walk(block.doc);
  }
  if(images&&!body.trim())body="";
  if(body.length>maxChars)throw Error("Message is too long.");
  return {content,body,mentions};
}
export function plainRichContent(body:string,mentions:Mention[]=[]):RichContent {
  let offset=0;
  return {version:1,blocks:[{type:"text",doc:{type:"doc",content:body.split("\n").map(line=>{
    const content:RichNode[]=[];let pos=0;
    for(const mention of mentions.filter(m=>m.start>=offset&&m.end<=offset+line.length)) {
      if(mention.start-offset>pos)content.push({type:"text",text:line.slice(pos,mention.start-offset)});
      content.push({type:"text",text:line.slice(mention.start-offset,mention.end-offset),marks:[{type:"mention",attrs:{kind:mention.kind,targetId:mention.targetId,label:mention.label}}]});
      pos=mention.end-offset;
    }
    if(pos<line.length)content.push({type:"text",text:line.slice(pos)});
    offset+=line.length+1;
    return {type:"paragraph",content};
  })}}]};
}
export function aiMarkdownRichContent(markdown:string):RichContent {
  const lines=markdown.split(/\r?\n/),content:RichNode[]=[];
  const text=(value:string):RichNode[]=>value?[{type:"text",text:value}]:[];
  for(let i=0;i<lines.length;){
    const line=lines[i];
    if(/^\|.*\|$/.test(line)&&i+1<lines.length&&/^\|?\s*:?-{3,}/.test(lines[i+1])){
      const rows:RichNode[]=[];const cells=(value:string,header:boolean):RichNode=>({type:header?"tableHeader":"tableCell",attrs:{colspan:1,rowspan:1,colwidth:null},content:[{type:"paragraph",content:text(value.trim())}]});
      const split=(value:string)=>value.replace(/^\||\|$/g,"").split("|");
      rows.push({type:"tableRow",content:split(line).map(value=>cells(value,true))});i+=2;
      while(i<lines.length&&/^\|.*\|$/.test(lines[i])){rows.push({type:"tableRow",content:split(lines[i]).map(value=>cells(value,false))});i++;}
      content.push({type:"table",content:rows});continue;
    }
    const bullet=/^\s*[-*]\s+(.+)$/.exec(line),ordered=/^\s*\d+[.)]\s+(.+)$/.exec(line);
    if(bullet||ordered){const kind=bullet?"bulletList":"orderedList",items:RichNode[]=[];
      while(i<lines.length){const match=kind==="bulletList"?/^\s*[-*]\s+(.+)$/.exec(lines[i]):/^\s*\d+[.)]\s+(.+)$/.exec(lines[i]);if(!match)break;items.push({type:"listItem",content:[{type:"paragraph",content:text(match[1])}]});i++;}
      content.push({type:kind,content:items});continue;
    }
    if(line.trim())content.push({type:"paragraph",content:text(line)});
    i++;
  }
  return {version:1,blocks:[{type:"text",doc:{type:"doc",content:content.length?content:[{type:"paragraph",content:[]}]}}]};
}
