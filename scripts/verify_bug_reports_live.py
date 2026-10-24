"""Explicit synthetic intake/triage smoke; cleans up only its own UUID fixture."""
import json
from pathlib import Path
import uuid
from urllib.request import Request, urlopen
from urllib.error import HTTPError
import bug_reports_admin as admin

def request(method,path,payload=None):
    raw=None if payload is None else json.dumps(payload).encode()
    req=Request('https://swarm-bug-reports.volvcane.workers.dev'+path,data=raw,method=method,headers={'Content-Type':'application/json','User-Agent':'Swarm-bug-report/1'})
    try:
        with urlopen(req,timeout=15) as response:return response.status,json.load(response)
    except HTTPError as error:return error.code,json.load(error)

def main():
    report=json.loads((admin.ROOT/'docs/BUG_REPORT_TEMPLATE.json').read_text())
    report.update(report_id=str(uuid.uuid4()),summary='SYNTHETIC intake verification',severity='high')
    id=report['report_id'];checks={}
    try:
        code,ack=request('POST','/v1/reports',report)
        assert code in (200,201) and ack.get('accepted') is True and ack.get('report_id')==id
        checks['intake']=True
        assert request('POST','/v1/reports',report)[0]==200
        different=dict(report,actual='Different synthetic observation')
        assert request('POST','/v1/reports',different)[0]==409
        checks['idempotency_and_conflict']=True
        for path in ['/v1/reports','/v1/reports/'+id]:assert request('GET',path)[0]==404
        checks['no_public_read']=True
        row=admin.show_report(id);assert row['severity']=='high' and row['status']=='new'
        assert any(row['id']==id for row in admin.list_reports(severity='high',category='gui'))
        assert admin.delete_resolved([{'id':id,'revision':1}],True)['deleted']==[]
        checks['owner_read_filter_and_unresolved_delete_guard']=True
        admin.update_report(id,1,'resolved','low','Synthetic verification complete')
        row=admin.show_report(id);assert row['reported_severity']=='high' and row['severity']=='low'
        assert admin.delete_resolved([{'id':id,'revision':1}],True)['deleted']==[]
        assert len(admin.delete_resolved([{'id':id,'revision':2}])['preview'])==1
        checks['triage_original_severity_revision_and_preview']=True
    finally:
        try:
            row=admin.show_report(id)
            if row['status']!='resolved':
                admin.update_report(id,row['revision'],'resolved',row['severity'],'Synthetic fixture cleanup')
                row=admin.show_report(id)
            result=admin.delete_resolved([{'id':id,'revision':row['revision']}],True)
            checks['own_fixture_deleted']=len(result['deleted'])==1
        except ValueError:pass
    assert checks.get('own_fixture_deleted')
    output=admin.ROOT/'target/bug-reporting/live-check.json';output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps({'synthetic_report_id':id,'checks':checks},indent=2)+'\n',encoding='utf-8')
    print(json.dumps(checks))

if __name__=='__main__':main()
