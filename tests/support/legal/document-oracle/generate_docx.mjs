// Development-only independent oracle. Never imported by a runtime artifact.
// Run with the pinned Node from the repository root; output is frozen evidence.
import fs from 'node:fs';
import crypto from 'node:crypto';
import mammoth from 'mammoth';
import Zip from 'jszip';
const packageVersion = name => JSON.parse(fs.readFileSync(new URL(import.meta.resolve(`${name}/package.json`)),'utf8')).version;
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const p = text => `<w:p><w:r><w:t>${text}</w:t></w:r></w:p>`;
const cell = (text, props='') => `<w:tc><w:tcPr>${props}</w:tcPr>${p(text)}</w:tc>`;
const row = (cells, props='') => `<w:tr><w:trPr>${props}</w:trPr>${cells}</w:tr>`;
const specs = [
  ['thai-entities', p('สัญญา A &amp; B &#x1F600; &#169; &lt;ok&gt;')],
  ['runs-tabs-breaks', '<w:p><w:r><w:t>A</w:t><w:tab/><w:t>B</w:t><w:br/><w:t>C</w:t><w:br w:type="page"/><w:t>D</w:t><w:cr/><w:t>E</w:t></w:r></w:p>'],
  ['hyphens', '<w:p><w:r><w:t>a</w:t><w:noBreakHyphen/><w:t>b</w:t><w:softHyphen/><w:t>c</w:t></w:r></w:p>'],
  ['hyperlink', '<w:p><w:hyperlink w:anchor="section"><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p>'],
  ['tracked-runs', '<w:p><w:del><w:r><w:delText>deleted</w:delText></w:r></w:del><w:ins><w:r><w:t>inserted</w:t></w:r></w:ins><w:moveFrom><w:r><w:t>old</w:t></w:r></w:moveFrom><w:moveTo><w:r><w:t>new</w:t></w:r></w:moveTo></w:p>'],
  ['deleted-paragraph-merge', '<w:p><w:pPr><w:rPr><w:del/></w:rPr></w:pPr><w:r><w:t>before</w:t></w:r></w:p>'+p('after')],
  ['deleted-final-paragraph', p('kept')+'<w:p><w:pPr><w:rPr><w:del/></w:rPr></w:pPr><w:r><w:t>gone</w:t></w:r></w:p>'],
  ['table', '<w:tbl>'+row(cell('A')+cell('B'))+row(cell('C')+cell('D'))+'</w:tbl>'],
  ['deleted-row', '<w:tbl>'+row(cell('gone'),'<w:del/>')+row(cell('kept'))+'</w:tbl>'],
  ['vertical-merge', '<w:tbl>'+row(cell('top','<w:vMerge w:val="restart"/>')+cell('R1'))+row(cell('hidden','<w:vMerge/>')+cell('R2'))+'</w:tbl>'],
  ['orphan-vertical-merge', '<w:tbl>'+row(cell('first','<w:vMerge/>'))+'</w:tbl>'],
  ['grid-span-vertical', '<w:tbl>'+row(cell('wide','<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>')+cell('last'))+row(cell('hidden','<w:gridSpan w:val="2"/><w:vMerge/>')+cell('end'))+'</w:tbl>'],
  ['sdt', '<w:sdt><w:sdtPr/><w:sdtContent>'+p('content')+'</w:sdtContent></w:sdt>'],
  ['sdt-checkbox', '<w:p><w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val="1"/></w14:checkbox></w:sdtPr><w:sdtContent><w:r><w:t>☒</w:t><w:t>label</w:t></w:r></w:sdtContent></w:sdt></w:p>'],
  ['sdt-checkbox-tab', '<w:p><w:sdt><w:sdtPr><w14:checkbox/></w:sdtPr><w:sdtContent><w:r><w:tab/></w:r></w:sdtContent></w:sdt></w:p>'],
  ['wrappers', '<w:customXml><w:smartTag>'+p('wrapped')+'</w:smartTag></w:customXml>'],
  ['alternate-content', '<mc:AlternateContent><mc:Choice Requires="w14">'+p('choice')+'</mc:Choice><mc:Fallback>'+p('fallback')+'</mc:Fallback></mc:AlternateContent>'],
  ['vml-textbox', '<w:p><w:r><w:t>head</w:t><w:pict><v:shape><v:textbox><w:txbxContent>'+p('box')+'</w:txbxContent></v:textbox></v:shape></w:pict><w:t>tail</w:t></w:r></w:p>'],
  ['unknown-elements', '<w:unknown>'+p('ignored')+'</w:unknown>'+p('visible')],
  ['symbols', '<w:p><w:r><w:sym w:font="Wingdings" w:char="F0FC"/><w:sym w:font="Symbol" w:char="0061"/><w:sym w:font="Unknown" w:char="0061"/></w:r></w:p>'],
  ['literal-entity', p('&amp;lt; &amp;amp;')],
  ['empty-paragraph', '<w:p/>'+p('tail')],
  ['field-result', '<w:p><w:r><w:fldChar w:fldCharType="begin"/><w:instrText>DATE</w:instrText><w:fldChar w:fldCharType="separate"/><w:t>2026-10-27</w:t><w:fldChar w:fldCharType="end"/></w:r></w:p>'],
  ['footnote-ignored', '<w:p><w:r><w:t>A</w:t><w:footnoteReference w:id="2"/><w:t>B</w:t></w:r></w:p>'],
  ['alternate-prefix', p('alias').replaceAll('w:', 'x:')],
  ['strict-word-namespace', p('strict'), {strict:true}],
  ['custom-main-part', p('custom'), {target:'custom/main.xml'}],
  ['unicode-near-limit', p('😀'.repeat(99_999))],
  ['text-over-limit', p('A'.repeat(200_000))],
];
(async()=>{
  const cases=[];
  for(const [id,body,options={}] of specs){
    const zip=new Zip();
    const word=options.strict?'http://purl.oclc.org/ooxml/wordprocessingml/main':'http://schemas.openxmlformats.org/wordprocessingml/2006/main';
    const target=options.target||'word/document.xml';
    const xml=`<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="${word}" xmlns:x="${word}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:v="urn:schemas-microsoft-com:vml"><w:body>${body}<w:sectPr/></w:body></w:document>`;
    zip.file('[Content_Types].xml',`<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/${target}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`);
    zip.file('_rels/.rels',`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="${target}"/></Relationships>`);
    zip.file(target,xml);
    for(const entry of Object.values(zip.files))entry.date=new Date('2026-10-27T00:00:00Z');
    const bytes=await zip.generateAsync({type:'nodebuffer',compression:'DEFLATE'});
    const result=await mammoth.extractRawText({buffer:bytes});
    cases.push({id,docxHex:bytes.toString('hex'),sha256:sha(bytes),expected:result.value.length>200000?{error:'DOCX_TEXT_TOO_LARGE'}:{text:result.value},messages:result.messages});
  }
  const sources=['scripts/legal/extract_docx.mjs','node_modules/mammoth/lib/docx/body-reader.js','node_modules/mammoth/lib/raw-text.js','node_modules/mammoth/lib/docx/office-xml-reader.js'];
  const receipt={schema:1,oracle:{node:process.version,mammoth:packageVersion('mammoth'),jszip:packageVersion('jszip'),sources:sources.map(path=>({path,sha256:sha(fs.readFileSync(path))}))},cases};
  fs.mkdirSync('fixtures/legal/documents',{recursive:true});
  fs.writeFileSync('fixtures/legal/documents/docx-conformance.v1.json',JSON.stringify(receipt,null,2)+'\n');
  console.log(`Captured ${cases.length} independent Mammoth cases`);
})().catch(e=>{console.error(e.message);process.exitCode=1});
