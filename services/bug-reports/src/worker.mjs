import {validateReport} from './schema.mjs';
function response(status, value) {
  return Response.json(value,{status,headers:{'Cache-Control':'no-store','X-Content-Type-Options':'nosniff','Content-Security-Policy':"default-src 'none'; frame-ancestors 'none'"}});
}
async function boundedBody(request) {
  if (!request.body) throw new Error('INVALID_REPORT');
  const reader=request.body.getReader(), chunks=[]; let size=0;
  while(true){ const {done,value}=await reader.read(); if(done)break; size+=value.byteLength; if(size>16384){await reader.cancel();throw new Error('TOO_LARGE');}chunks.push(value); }
  const bytes=new Uint8Array(size); let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
  return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
}
export default {
  async fetch(request,env) {
    const url=new URL(request.url);
    if(url.pathname==='/health' && request.method==='GET')return response(200,{service:'swarm-bug-reports',schema_version:1});
    // No public read, status update or delete API. The developer uses authenticated D1 access.
    if(url.pathname!=='/v1/reports' || request.method!=='POST')return response(404,{error:'NOT_FOUND'});
    if(!request.headers.get('content-type')?.toLowerCase().startsWith('application/json'))return response(415,{error:'JSON_REQUIRED'});
    if(Number(request.headers.get('content-length'))>16384)return response(413,{error:'TOO_LARGE'});
    const ip=request.headers.get('CF-Connecting-IP') || 'unknown';
    const hashed=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(ip));
    const rateKey=Array.from(new Uint8Array(hashed),b=>b.toString(16).padStart(2,'0')).join('');
    if(!(await env.REPORT_LIMITER.limit({key:rateKey})).success)return response(429,{error:'RATE_LIMITED'});
    let report;
    try{report=validateReport(await boundedBody(request));}catch(error){return response(error.message==='TOO_LARGE'?413:400,{error:'INVALID_OR_SENSITIVE_REPORT'});}
    const payload=JSON.stringify(report);
    const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(payload))),b=>b.toString(16).padStart(2,'0')).join('');
    try{
      const old=await env.REPORTS.prepare('SELECT payload_digest FROM reports WHERE id=?').bind(report.report_id).first();
      if(old)return old.payload_digest===digest?response(200,{report_id:report.report_id,accepted:true}):response(409,{error:'REPORT_ID_CONFLICT'});
      await env.REPORTS.prepare('INSERT INTO reports(id,schema_version,app_version,category,severity,reported_severity,summary,payload,payload_digest) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING')
        .bind(report.report_id,1,report.app_version,report.category,report.severity,report.severity,report.summary,payload,digest).run();
      // Resolve a concurrent duplicate without acknowledging different content.
      const stored=await env.REPORTS.prepare('SELECT payload_digest FROM reports WHERE id=?').bind(report.report_id).first();
      return stored?.payload_digest===digest?response(201,{report_id:report.report_id,accepted:true}):response(409,{error:'REPORT_ID_CONFLICT'});
    }catch(error){return response(String(error).includes('REPORT_CAPACITY')?507:503,{error:'REPORT_STORAGE_UNAVAILABLE'});}
  }
};
