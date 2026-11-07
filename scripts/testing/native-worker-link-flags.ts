/** Fixed native compiler link configuration, never a runtime loader override. */
import { isAbsolute, relative, resolve } from 'node:path';
const separator='\u001f';
function directory(value:string|undefined) {
 if(!value||!isAbsolute(value)||value.length>4096||/[\x00-\x1f\x7f]/.test(value))throw Error('NATIVE_WORKER_LINK_DIRECTORY_REQUIRED');return resolve(value);
}
export function nativeWorkerLinkFlags(platform:string,libdir:string):string {
 const path=directory(libdir);
 if(platform==='darwin')return '';
 if(platform!=='linux')throw Error('NATIVE_WORKER_PLATFORM_UNSUPPORTED');
 // ELF uses ':' as a search-list separator and expands '$' loader tokens.
 // Spaces/commas remain single arguments through Cargo's encoded flag format.
 if(path.includes(':')||path.includes('$'))throw Error('NATIVE_WORKER_LITERAL_LINUX_LINK_PATH_REQUIRED');
 return ['-C','link-arg=-Xlinker','-C','link-arg=--enable-new-dtags','-C','link-arg=-Xlinker','-C','link-arg=-rpath','-C','link-arg=-Xlinker','-C',`link-arg=${path}`].join(separator);
}
export function qualifiedNativeLinkEnvironment(environment:Readonly<Record<string,string|undefined>>,platform=process.platform):Record<string,string> {
 const values=[environment.ERPX_NATIVE_VIPS_PREFIX,environment.ERPX_NATIVE_VIPS_LIBDIR,environment.CARGO_ENCODED_RUSTFLAGS];
 if(values.every(value=>value===undefined))return {};
 const prefix=directory(values[0]),libdir=directory(values[1]),path=relative(prefix,libdir);
 if(path==='..'||path.startsWith('../')||isAbsolute(path)||values[2]!==nativeWorkerLinkFlags(platform,libdir))throw Error('NATIVE_WORKER_LINK_FLAGS_MISMATCH');
 return {ERPX_NATIVE_VIPS_PREFIX:prefix,ERPX_NATIVE_VIPS_LIBDIR:libdir,CARGO_ENCODED_RUSTFLAGS:values[2]!};
}
export function nativeElfRunpath(dynamic:string,libdir:string) {
 const actual=/\(RUNPATH\)[^\n]*\[([^\n]*)\]/.exec(dynamic)?.[1];
 if(actual!==directory(libdir)||!dynamic.includes('Shared library: [libvips.so.42]'))throw Error('NATIVE_WORKER_EXACT_ELF_RUNPATH_REQUIRED');
 return actual;
}
