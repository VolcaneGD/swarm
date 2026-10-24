"""Triage rules against actual SQLite, without Cloudflare or private data."""
import json
import sqlite3
import unittest
from unittest.mock import patch
import bug_reports_admin as admin

class TriageTests(unittest.TestCase):
    def setUp(self):
        self.db=sqlite3.connect(':memory:')
        self.db.row_factory=sqlite3.Row
        self.db.executescript((admin.ROOT/'services/bug-reports/migrations/0001_reports.sql').read_text())
        self.ids=['00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000002']
        for id,severity in zip(self.ids,['low','critical']):
            self.db.execute("INSERT INTO reports(id,schema_version,app_version,category,severity,reported_severity,summary,payload,payload_digest) VALUES(?,1,'0.1.0','gui',?,?,'Synthetic example','{}','test')",(id,severity,severity))
        self.mock=patch.object(admin,'execute',lambda sql,local=False:[dict(row) for row in self.db.execute(sql).fetchall()])
        self.mock.start()
    def tearDown(self):
        self.mock.stop();self.db.close()
    def test_severity_filter_original_rating_and_guarded_deletion(self):
        self.assertEqual(admin.list_reports()[0]['severity'],'critical')
        self.assertEqual(len(admin.list_reports(severity='low')),1)
        items=[{'id':self.ids[0],'revision':1}]
        self.assertEqual(admin.delete_resolved(items,True)['deleted'],[])
        admin.update_report(self.ids[0],1,'resolved','high',"Owner's synthetic note")
        row=admin.show_report(self.ids[0]);self.assertEqual(row['reported_severity'],'low')
        self.assertEqual(row['severity'],'high')
        with self.assertRaises(ValueError):admin.update_report(self.ids[0],1,'resolved','low')
        self.assertEqual(admin.delete_resolved(items,True)['deleted'],[])
        items[0]['revision']=2
        self.assertEqual(len(admin.delete_resolved(items)['preview']),1)
        admin.update_report(self.ids[0],2,'investigating','high')
        self.assertEqual(admin.delete_resolved(items,True)['deleted'],[])
        admin.update_report(self.ids[0],3,'resolved','high')
        self.assertEqual(len(admin.delete_resolved([{'id':self.ids[0],'revision':4}],True)['deleted']),1)
        self.assertEqual(len(admin.list_reports()),1)
    def test_filters_and_ids_cannot_inject_sql(self):
        with self.assertRaises(ValueError):admin.list_reports(severity="high'; DROP TABLE reports;--")
        with self.assertRaises(ValueError):admin.show_report("' OR 1=1--")
        self.assertEqual(len(admin.list_reports()),2)

if __name__=='__main__':unittest.main()
