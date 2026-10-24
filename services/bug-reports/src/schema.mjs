export const categories = ['permissions','accounts','gui','cli','runtime','data','backup_restore','notifications','updates','installation','ai_integration','security','other'];
export const severities = ['critical','high','medium','low'];
export const statuses = ['new','investigating','resolved'];
const fields = ['schema_version','report_id','app_version','category','severity','summary','steps','expected','actual','severity_reason','error_code','reporter','redacted','consent_version'];
export const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const sensitive = /(?:-----BEGIN [A-Z ]*PRIVATE KEY-----|\bBearer\s+[a-z0-9._-]+|\b(?:sk-proj-|sk-ant-|ghp_|gho_)[a-z0-9_-]+|(?:password|recovery[_ -]?code|access[_ -]?token|client[_ -]?secret)\s*[:=]\s*\S+|[a-z]:[\\/]|\\\\[^\\\s]+\\|\/(?:Users|home)\/[^\s/]+|[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,})/i;
function prose(value, max) {
  if (typeof value !== 'string' || !value.trim() || value.length>max || /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(value) || sensitive.test(value)) throw new Error('INVALID_OR_SENSITIVE_REPORT');
  return value.trim();
}
export function validateReport(value) {
  if (!value || Array.isArray(value) || typeof value!=='object' || Object.keys(value).some(k=>!fields.includes(k))) throw new Error('INVALID_REPORT');
  if (value.schema_version!==1 || value.consent_version!==1 || value.redacted!==true || typeof value.report_id!=='string' || !uuid.test(value.report_id) || typeof value.app_version!=='string' || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(value.app_version)) throw new Error('INVALID_REPORT');
  if (!categories.includes(value.category) || !severities.includes(value.severity)) throw new Error('INVALID_CLASSIFICATION');
  if (!Array.isArray(value.steps) || value.steps.length<1 || value.steps.length>12) throw new Error('INVALID_STEPS');
  if (!['ai','human'].includes(value.reporter)) throw new Error('INVALID_REPORTER');
  if (typeof value.error_code!=='string' || !/^[A-Z0-9_]{0,64}$/.test(value.error_code)) throw new Error('INVALID_ERROR_CODE');
  return { schema_version:1, report_id:value.report_id, app_version:prose(value.app_version,64), category:value.category, severity:value.severity,
    summary:prose(value.summary,160), steps:value.steps.map(step=>prose(step,500)), expected:prose(value.expected,2000), actual:prose(value.actual,2000),
    severity_reason:prose(value.severity_reason,500), error_code:value.error_code, reporter:value.reporter, redacted:true, consent_version:1 };
}
