import {test} from 'node:test';
import assert from 'node:assert/strict';
import {validateReport} from '../src/schema.mjs';
import worker from '../src/worker.mjs';
const report=()=>({schema_version:1,report_id:crypto.randomUUID(),app_version:'0.1.0',category:'gui',severity:'medium',summary:'Sample layout regression',steps:['Open sample settings','Resize sample window'],expected:'Controls remain visible',actual:'Controls overlap',severity_reason:'A workaround exists',error_code:'',reporter:'ai',redacted:true,consent_version:1});
test('classification and sensitive/unknown fields are rejected',()=>{
  assert.equal(validateReport(report()).severity,'medium');
  for(const patch of [{severity:'urgent'},{category:'unknown'},{actual:'password=secret'},{actual:'C:/private/project'},{actual:'Bearer abc123'},{raw_log:'private'},{redacted:false}])assert.throws(()=>validateReport({...report(),...patch}));
});
test('write-only intake, rate limit and idempotency',async()=>{
  const rows=new Map();
  const env={
    REPORT_LIMITER:{limit:async()=>({success:true})},
    REPORTS:{prepare(){return{bind(...args){return{
      first:async()=>rows.has(args[0])?{payload_digest:rows.get(args[0])}:null,
      run:async()=>{rows.set(args[0],args[8]);}
    };}};}}
  };
  const send=r=>worker.fetch(new Request('https://reports.test/v1/reports',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(r)}),env);
  const r=report(); assert.equal((await send(r)).status,201);assert.equal((await send(r)).status,200);
  assert.equal((await send({...r,actual:'Different observation'})).status,409);assert.equal(rows.size,1);
  assert.equal((await worker.fetch(new Request('https://reports.test/v1/reports'),env)).status,404);
  env.REPORT_LIMITER.limit=async()=>({success:false});assert.equal((await send(report())).status,429);
});
