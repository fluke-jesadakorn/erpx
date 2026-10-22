import { describe, expect, it } from "vitest";
import { aiMarkdownRichContent, plainRichContent, validateRichContent } from "@erpx/feature-shared/lib/home-rich-content";
import type { Mention } from "@erpx/server-collaboration/public/rules/social-text";

describe("Home rich documents",()=>{
  it("keeps Thai text and the stable AI mention target through a document round trip",()=>{
    const body="@AI ช่วยอ่านตาราง\nยอดขาย ๒๐๐";
    const mention:Mention={kind:"agent",targetId:"erpx-assistant",label:"AI",start:0,end:3};
    const result=validateRichContent(plainRichContent(body,[mention]),30000);
    expect(result.body).toBe(body);
    expect(result.mentions).toEqual([mention]);
  });
  it("retains list and table structure in an AI answer",()=>{
    const result=validateRichContent(aiMarkdownRichContent("งานค้าง\n- ตรวจสต็อก\n- โทรกลับ\n| รายการ | จำนวน |\n| --- | --- |\n| A | 2 |"),10000);
    const doc=result.content.blocks[0];
    expect(doc.type).toBe("text");
    if(doc.type==="text")expect(doc.doc.content?.map(node=>node.type)).toEqual(["paragraph","bulletList","table"]);
    expect(result.body).toContain("A\t2");
  });
  it("accepts the table cell attributes emitted by Tiptap 3",()=>{
    const content={version:1,blocks:[{type:"text",doc:{type:"doc",content:[{type:"table",content:[{type:"tableRow",content:[{type:"tableHeader",attrs:{colspan:1,rowspan:1,colwidth:null,align:null},content:[{type:"paragraph"}]}]}]}]}}]};
    expect(validateRichContent(content,30000).content.blocks[0].type).toBe("text");
  });
  it("rejects unsafe links, unsupported attributes and oversized text",()=>{
    const doc=plainRichContent("open");
    const text=doc.blocks[0].type==="text"?doc.blocks[0].doc.content?.[0].content?.[0]:undefined;
    text!.marks=[{type:"link",attrs:{href:"javascript:alert(1)"}}];
    expect(()=>validateRichContent(doc,30000)).toThrow("Invalid link");
    text!.marks=[];text!.attrs={onclick:"alert(1)"};
    expect(()=>validateRichContent(doc,30000)).toThrow("Unsupported document attribute");
    expect(()=>validateRichContent(plainRichContent("x".repeat(30001)),30000)).toThrow("Message is too long");
  });
  it("allows bounded image widths and rejects attributes outside the allowlist",()=>{
    const image={type:"image" as const,slotId:"s1",attachmentId:"a1",name:"photo.png",width:50};
    expect(validateRichContent({version:1,blocks:[image]},10000).content.blocks[0]).toEqual(image);
    expect(()=>validateRichContent({version:1,blocks:[{...image,width:101}]},10000)).toThrow("Invalid image");
  });
});
