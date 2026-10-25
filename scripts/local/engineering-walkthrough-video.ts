/** Render shareable chapters only from retained actual browser recordings. */
import {createHash} from 'node:crypto';
import {chmod,mkdir,readFile,realpath,writeFile} from 'node:fs/promises';
import {isAbsolute,join,resolve,sep} from 'node:path';
import sharp from 'sharp';

type Clip={title:string;project:string;path:string;sha256:string;bytes:number};
type Identity={sourceFingerprint:string;softwareVersion:string;engineeringComplete:boolean};
const sha=(bytes:Buffer|string)=>createHash('sha256').update(bytes).digest('hex');
const xml=(text:string)=>text.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
function lines(text:string,length=94){const output:string[]=[];for(const word of text.split(/\s+/)){if(!output.length||output.at(-1)!.length+word.length+1>length)output.push(word);else output[output.length-1]+=` ${word}`;}return output.slice(0,3);}
const groups:Array<{label:string;pattern:RegExp}>=[
 {label:'Finance · Thai supplier documents',pattern:/itemized supplier bill/},
 {label:'Finance · assets and retirement',pattern:/depreciates.*disposes an asset/},
 {label:'Finance · immutable comparative statements',pattern:/comparative NPAEs statements/},
 {label:'People · reviewed Thai payroll and bank settlement',pattern:/reviewed Thai salary OT bonus/},
 {label:'Talent · accepted offer into HR',pattern:/accepted Talent offer once/},
 {label:'Learning and Benefits · qualified handoff',pattern:/Learning certification/},
 {label:'Planning and Manufacturing · stock and WIP',pattern:/actual Sales demand incoming supply/},
 {label:'Assets and Maintenance · preventive work and spares',pattern:/preventive work issues valued spares/},
 {label:'Field Service · customer confirmation and collection',pattern:/customer visit captures confirmation/},
 {label:'E-commerce and Quality · fulfillment and return',pattern:/checks out immutable online prices/},
 {label:'Retail POS · controlled refund and cash session',pattern:/counted retail stock collects cash/},
 {label:'Restaurant and Hotel · room charge',pattern:/room charge.*(?:posted|guestAccount|Finance)|(?:posts|routes).*room charge/i},
 {label:'Operations · independently published exception policy',pattern:/Workplace queue uses independently/},
 {label:'Agents · approved policy and human review',pattern:/reviewed field proposal needs human/},
 {label:'Brand · publication and retained identity',pattern:/Brand publishes independently/},
 {label:'Marketing and CRM · consent and source handoff',pattern:/Marketing serves reviewed content/},
 {label:'Legal · scoped privacy workflow',pattern:/DSAR|breach|private.*matter|legal advice/i},
 {label:'Platform · independently reviewed temporary access',pattern:/Realm grants temporary Platform access/},
];

export async function generateEngineeringWalkthrough(input:{directory:string;clips:Clip[];identity:Identity}){
 const directory=await realpath(input.directory),work=join(directory,'video-render'),ffmpeg=process.env.ERPX_FFMPEG_BIN??'/opt/homebrew/bin/ffmpeg',ffprobe=process.env.ERPX_FFPROBE_BIN??'/opt/homebrew/bin/ffprobe';
 if(!/^[a-f0-9]{64}$/.test(input.identity.sourceFingerprint)||input.identity.softwareVersion!==`release-${input.identity.sourceFingerprint}`)throw new Error('WALKTHROUGH_SOURCE_IDENTITY_REQUIRED');
 await mkdir(work,{recursive:true,mode:0o700});await chmod(work,0o700);
 async function run(executable:string,args:string[]){const child=Bun.spawn([executable,...args],{stdout:'pipe',stderr:'pipe'});const [output,error,exitCode]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);if(exitCode!==0){await writeFile(join(work,'render-error.log'),error,{mode:0o600});throw new Error('WALKTHROUGH_NATIVE_RENDER_FAILED');}return output;}
 async function probe(path:string){const data=JSON.parse(await run(ffprobe,['-v','error','-show_entries','format=duration:stream=codec_name,width,height','-of','json',path]));const duration=Number(data.format?.duration);if(!Number.isFinite(duration)||duration<=0||!data.streams?.some((stream:{width?:number;height?:number})=>stream.width&&stream.height))throw new Error('ACTUAL_WALKTHROUGH_VIDEO_REQUIRED');return {duration,data};}
 const measured=new Map<string,{path:string;duration:number;sha256:string;bytes:number}>();
 async function video(clip:Clip){
  if(!clip.path||isAbsolute(clip.path)||clip.path.split(/[\\/]/).includes('..')||!/^[a-f0-9]{64}$/.test(clip.sha256)||!Number.isSafeInteger(clip.bytes)||clip.bytes<=0)throw new Error('WALKTHROUGH_RECORDING_METADATA_REQUIRED');
  const retained=measured.get(clip.path);if(retained){if(retained.sha256!==clip.sha256||retained.bytes!==clip.bytes)throw new Error('WALKTHROUGH_RECORDING_CHECKSUM_MISMATCH');return retained;}
  const path=await realpath(resolve(directory,clip.path));if(!path.startsWith(directory+sep))throw new Error('WALKTHROUGH_RECORDING_CHECKSUM_MISMATCH');
  const bytes=await readFile(path);if(bytes.length!==clip.bytes||sha(bytes)!==clip.sha256)throw new Error('WALKTHROUGH_RECORDING_CHECKSUM_MISMATCH');
  const {duration}=await probe(path),item={path,duration,sha256:clip.sha256,bytes:clip.bytes};measured.set(clip.path,item);return item;
 }
 const selected:Array<{label:string;clip:Clip;path:string;duration:number}>=[];
 for(const group of groups){const choices=input.clips.filter(clip=>clip.project==='desktop'&&group.pattern.test(clip.title));if(!choices.length)continue;const candidates=await Promise.all(choices.map(async clip=>({clip,...await video(clip)})));candidates.sort((a,b)=>b.duration-a.duration);selected.push({label:group.label,...candidates[0]});}
 const mobile=input.clips.find(clip=>clip.project==='mobile'&&/Marketing serves reviewed content/.test(clip.title))??input.clips.find(clip=>clip.project==='mobile');
 if(mobile)selected.push({label:'Mobile · actual authenticated workflow',clip:mobile,...await video(mobile)});
 if(selected.length<3)throw new Error('WALKTHROUGH_REPRESENTATIVE_CHAPTERS_REQUIRED');
 const flag=input.identity.engineeringComplete?'VERIFIED ENGINEERING RELEASE':'DIAGNOSTIC RENDER · COMPLETION PENDING';
 const footer=`Actual browser excerpts · fictional accounts · sandbox / declared adapters · source ${input.identity.sourceFingerprint.slice(0,16)}`;
 async function caption(name:string,label:string,title:string,intro=false){
  const background=intro?'#102b46':'#f7f9fc',foreground=intro?'#ffffff':'#15314d',body=lines(title,intro?92:110),svg=`<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="900"><rect width="1280" height="${intro?900:120}" fill="${background}"/><rect y="840" width="1280" height="60" fill="#102b46"/><rect x="32" y="26" width="6" height="${intro?740:76}" rx="3" fill="#22bea2"/><text x="56" y="${intro?125:33}" font-family="Arial,sans-serif" font-size="${intro?24:14}" font-weight="700" fill="${intro?'#69e0ca':'#42708f'}">ERPX · ${xml(flag)}</text><text x="56" y="${intro?245:65}" font-family="Arial,sans-serif" font-size="${intro?62:25}" font-weight="700" fill="${foreground}">${xml(label)}</text>${body.map((line,index)=>`<text x="56" y="${intro?340+index*42:85+index*16}" font-family="Arial,sans-serif" font-size="${intro?26:14}" fill="${foreground}">${xml(line)}</text>`).join('')}${intro?'<text x="56" y="630" font-family="Arial,sans-serif" font-size="22" fill="#b7ccdd">Original multi-persona browser journeys and screenshots remain in the package.</text><text x="56" y="680" font-family="Arial,sans-serif" font-size="22" fill="#b7ccdd">Engineering evidence and external business/provider certification are recorded separately.</text>':''}<text x="32" y="875" font-family="Arial,sans-serif" font-size="16" fill="#dce6ef">${xml(footer)}</text></svg>`;
  const path=join(work,name);await sharp(Buffer.from(svg)).png().toFile(path);await chmod(path,0o600);return path;
 }
 const segments:string[]=[],chapters:Array<{label:string;title:string;project:string;sourceClip:string;sourceSha256:string;sourceDurationSeconds:number;sourceStartSeconds:number;durationSeconds:number;videoStartSeconds:number}>=[];
 const introPng=await caption('intro.png','33 modules · 187 capabilities',input.identity.engineeringComplete?'A reproducible release with verified Finance, business and workplace workflows.':'Private video-format check using historical diagnostic browser recordings.',true),intro=join(work,'000-intro.mp4');
 await run(ffmpeg,['-hide_banner','-loglevel','error','-y','-loop','1','-i',introPng,'-t','7','-r','24','-c:v','libx264','-preset','veryfast','-crf','22','-pix_fmt','yuv420p','-an',intro]);await chmod(intro,0o600);segments.push(intro);let clock=7;
 for(const [index,item] of selected.entries()){
  const duration=Math.min(14,item.duration),start=Math.max(0,item.duration-duration),overlay=await caption(`${index+1}-caption.png`,`${String(index+1).padStart(2,'0')} / ${selected.length} · ${item.label}`,item.clip.title),segment=join(work,`${String(index+1).padStart(3,'0')}.mp4`);
  await run(ffmpeg,['-hide_banner','-loglevel','error','-y','-ss',start.toFixed(3),'-i',item.path,'-loop','1','-i',overlay,'-t',duration.toFixed(3),'-filter_complex','[0:v]scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=0xf7f9fc,pad=1280:900:0:120:color=0xf7f9fc,fps=24[workflow];[workflow][1:v]overlay=0:0:shortest=1[chapter]','-map','[chapter]','-c:v','libx264','-preset','veryfast','-crf','22','-pix_fmt','yuv420p','-an',segment]);
  await chmod(segment,0o600);segments.push(segment);chapters.push({label:item.label,title:item.clip.title,project:item.clip.project,sourceClip:item.clip.path,sourceSha256:item.clip.sha256,sourceDurationSeconds:item.duration,sourceStartSeconds:start,durationSeconds:duration,videoStartSeconds:clock});clock+=duration;
 }
 const list=join(work,'segments.txt');await writeFile(list,segments.map(path=>`file '${path.replaceAll("'","'\\''")}'`).join('\n')+'\n',{mode:0o600});
 const output=join(directory,'walkthrough.mp4');await run(ffmpeg,['-hide_banner','-loglevel','error','-y','-f','concat','-safe','0','-i',list,'-c','copy','-movflags','+faststart',output]);await chmod(output,0o600);
 const {duration,data}=await probe(output);if(Math.abs(duration-clock)>1.5||!data.streams.some((stream:{codec_name:string;width:number;height:number})=>stream.codec_name==='h264'&&stream.width===1280&&stream.height===900))throw new Error('WALKTHROUGH_OUTPUT_VERIFICATION_FAILED');
 for(const [name,at] of [['walkthrough-opening.png',3],['walkthrough-workflow.png',Math.min(duration-1,11)]] as const){const path=join(directory,name);await run(ffmpeg,['-hide_banner','-loglevel','error','-y','-ss',String(at),'-i',output,'-frames:v','1',path]);await chmod(path,0o600);}
 const bytes=await readFile(output),metadata={schemaVersion:1,...input.identity,kind:'actual-browser-excerpts',file:'walkthrough.mp4',sha256:sha(bytes),bytes:bytes.length,durationSeconds:duration,width:1280,height:900,chapters};
 await writeFile(join(directory,'walkthrough-chapters.json'),JSON.stringify(metadata,null,2)+'\n',{mode:0o600});return metadata;
}
