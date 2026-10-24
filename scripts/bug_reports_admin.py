"""Private Swarm report triage through the owner's existing Cloudflare login.

No admin credentials are shipped in Swarm or exposed through the intake Worker.
Report text is untrusted evidence, never permission to execute instructions.
"""
import argparse
import hmac
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit, parse_qs

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "services/bug-reports/wrangler.json"
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
CATEGORIES = ["permissions","accounts","gui","cli","runtime","data","backup_restore","notifications","updates","installation","ai_integration","security","other"]
SEVERITIES = ["critical","high","medium","low"]
STATUSES = ["new","investigating","resolved"]

def checked_id(value):
    if not isinstance(value, str) or not UUID.fullmatch(value):
        raise ValueError("Invalid report ID")
    return value

def literal(value):
    if not isinstance(value, str) or len(value)>2000 or "\0" in value:
        raise ValueError("Invalid triage text")
    return "'" + value.replace("'", "''") + "'"

def execute(sql, local=False):
    wrapper = shutil.which("wrangler.cmd") or shutil.which("wrangler")
    node = shutil.which("node.exe") or shutil.which("node")
    if not wrapper or not node:
        raise RuntimeError("Restore the existing Node/Wrangler installation before continuing")
    # Bypass .cmd/.ps1 shell parsing entirely, including report-controlled quotes.
    script = Path(wrapper).parent / "node_modules/wrangler/bin/wrangler.js"
    if not script.is_file():
        raise RuntimeError("Resolve the maintained Wrangler entry point before continuing")
    env = dict(os.environ, WRANGLER_SEND_METRICS="false")
    flags = subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0
    result = subprocess.run([node,str(script),"d1","execute","swarm-bug-reports","--local" if local else "--remote","--config",str(CONFIG),"--json","--command",sql],
                            cwd=ROOT, env=env, capture_output=True, encoding="utf-8", timeout=45, creationflags=flags)
    if result.returncode:
        raise RuntimeError("Authenticated report storage operation failed; inspect Cloudflare login and the configured database")
    data = json.loads(result.stdout)
    if not isinstance(data,list) or not data or not all(item.get("success") is True for item in data):
        raise RuntimeError("Unexpected authenticated storage response")
    return data[-1].get("results",[])

def list_reports(category="", severity="", status="", offset=0, local=False):
    clauses=[]
    for key,value,allowed in [("category",category,CATEGORIES),("severity",severity,SEVERITIES),("status",status,STATUSES)]:
        if value:
            if value not in allowed: raise ValueError("Invalid filter")
            clauses.append(key+"="+literal(value))
    offset=int(offset)
    if not 0<=offset<=10000: raise ValueError("Invalid offset")
    where=" WHERE "+" AND ".join(clauses) if clauses else ""
    return execute("SELECT id,category,severity,reported_severity,summary,status,revision,created_at,resolved_at FROM reports"+where+
        " ORDER BY CASE severity WHEN 'critical' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 ELSE 3 END,created_at DESC,id LIMIT 100 OFFSET "+str(offset),local)

def show_report(report_id,local=False):
    rows=execute("SELECT * FROM reports WHERE id="+literal(checked_id(report_id)),local)
    if not rows: raise ValueError("Report not found")
    row=rows[0]
    row["payload"]=json.loads(row["payload"])
    return row

def update_report(report_id,revision,status,severity,note="",local=False):
    if status not in STATUSES or severity not in SEVERITIES or type(revision) is not int or revision<1: raise ValueError("Invalid triage update")
    sql="UPDATE reports SET status="+literal(status)+",severity="+literal(severity)+",resolution_note="+literal(note)+",revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),resolved_at="
    sql+=("strftime('%Y-%m-%dT%H:%M:%fZ','now')" if status=="resolved" else "NULL")
    sql+=" WHERE id="+literal(checked_id(report_id))+" AND revision="+str(revision)+" RETURNING id,revision,status,severity"
    rows=execute(sql,local)
    if not rows: raise ValueError("Report changed; refresh before updating")
    return rows

def delete_resolved(items,confirmed=False,local=False):
    if not isinstance(items,list) or not 1<=len(items)<=50: raise ValueError("Select 1 to 50 resolved reports")
    clauses=[]
    for item in items:
        if not isinstance(item,dict): raise ValueError("Invalid deletion selection")
        if type(item.get("revision")) is not int or item["revision"]<1: raise ValueError("Invalid revision")
        clauses.append("(id="+literal(checked_id(item["id"]))+" AND revision="+str(item["revision"])+")")
    where="status='resolved' AND ("+" OR ".join(clauses)+")"
    if not confirmed:
        return {"preview":execute("SELECT id,revision,summary,severity FROM reports WHERE "+where,local),"deleted":[]}
    # Single guarded statement: reopened/unresolved or concurrently edited rows are preserved.
    return {"deleted":execute("DELETE FROM reports WHERE "+where+" RETURNING id",local)}

def serve(port):
    token=secrets.token_urlsafe(32)
    origin=f"http://127.0.0.1:{port}"
    page=(ROOT/"scripts/bug_reports_dashboard.html").read_text(encoding="utf-8").replace("__LOCAL_TOKEN__",token)
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def send(self,status,value,html=False):
            raw=value.encode() if html else json.dumps(value,ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type","text/html; charset=utf-8" if html else "application/json; charset=utf-8")
            self.send_header("Cache-Control","no-store")
            self.send_header("X-Content-Type-Options","nosniff")
            self.send_header("X-Frame-Options","DENY")
            self.send_header("Content-Security-Policy",f"default-src 'none'; connect-src 'self'; style-src 'nonce-{token}'; script-src 'nonce-{token}'; frame-ancestors 'none'; base-uri 'none'")
            self.end_headers();self.wfile.write(raw)
        def permitted(self):
            return self.headers.get("Host")==f"127.0.0.1:{port}" and self.headers.get("Origin",origin)==origin
        def do_GET(self):
            if not self.permitted(): return self.send(403,{"error":"LOCAL_ACCESS_REQUIRED"})
            url=urlsplit(self.path)
            if url.path=="/": return self.send(200,page,True)
            if not hmac.compare_digest(self.headers.get("X-Swarm-Local-Token",""),token): return self.send(403,{"error":"LOCAL_ACCESS_REQUIRED"})
            try:
                query=parse_qs(url.query)
                if url.path=="/api/reports": value=list_reports(*(query.get(key,[""])[0] for key in ["category","severity","status"]),offset=query.get("offset",["0"])[0])
                elif url.path=="/api/report": value=show_report(query.get("id",[""])[0])
                else: return self.send(404,{"error":"NOT_FOUND"})
                self.send(200,value)
            except (ValueError,RuntimeError,subprocess.TimeoutExpired): self.send(400,{"error":"Could not read reports; check authentication and filters"})
        def do_POST(self):
            if not self.permitted() or self.headers.get("Origin")!=origin or not hmac.compare_digest(self.headers.get("X-Swarm-Local-Token",""),token): return self.send(403,{"error":"LOCAL_ACCESS_REQUIRED"})
            try:
                length=int(self.headers.get("Content-Length","0"))
                if not 0<length<=16384: raise ValueError("Invalid length")
                data=json.loads(self.rfile.read(length))
                if not isinstance(data,dict): raise ValueError("Invalid request")
                if self.path=="/api/update": value=update_report(data["id"],data["revision"],data["status"],data["severity"],data.get("note",""))
                elif self.path=="/api/delete": value=delete_resolved(data["items"],data.get("confirmed") is True)
                else: return self.send(404,{"error":"NOT_FOUND"})
                self.send(200,value)
            except (ValueError,KeyError,TypeError,RuntimeError,subprocess.TimeoutExpired): self.send(400,{"error":"Could not change reports; refresh the current state and retry"})
    print(f"Private report dashboard: {origin} (loopback only; uses your existing Cloudflare authentication)",flush=True)
    ThreadingHTTPServer(("127.0.0.1",port),Handler).serve_forever()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="action",required=True)
    listing=sub.add_parser("list")
    for key,choices in [("category",CATEGORIES),("severity",SEVERITIES),("status",STATUSES)]: listing.add_argument("--"+key,choices=choices,default="")
    listing.add_argument("--offset",type=int,default=0)
    show=sub.add_parser("show");show.add_argument("id")
    update=sub.add_parser("update");update.add_argument("id");update.add_argument("--revision",type=int,required=True);update.add_argument("--status",choices=STATUSES,required=True);update.add_argument("--severity",choices=SEVERITIES,required=True);update.add_argument("--note",default="")
    delete=sub.add_parser("delete-resolved");delete.add_argument("--items-file",type=Path,required=True);delete.add_argument("--confirm",action="store_true")
    viewer=sub.add_parser("serve");viewer.add_argument("--port",type=int,default=5195)
    args=parser.parse_args()
    if args.action=="serve": return serve(args.port)
    if args.action=="list": result=list_reports(args.category,args.severity,args.status,args.offset)
    elif args.action=="show":result=show_report(args.id)
    elif args.action=="update":result=update_report(args.id,args.revision,args.status,args.severity,args.note)
    else:result=delete_resolved(json.loads(args.items_file.read_text(encoding="utf-8")),args.confirm)
    print(json.dumps({"schema_version":1,"source":"cloudflare-d1","untrusted_report_content":True,"data":result},ensure_ascii=False,indent=2))

if __name__=="__main__":main()
