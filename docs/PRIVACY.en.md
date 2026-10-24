# Swarm privacy notice — local release preparation

Updated2026-10-25. This notice describes the first beta published by VOLCANE through the [public repository](https://github.com/VolcaneGD/swarm) and [download page](https://volcane.pages.dev/swarm). Its public policy URL is [this document on GitHub](https://github.com/VolcaneGD/swarm/blob/main/docs/PRIVACY.en.md).

Optional structured bug reporting requires first-time human Admin consent in Settings.
Reports are encrypted locally with Windows DPAPI and upload to Cloudflare only after
consent; offline/full-stop reports remain queued. The report contains an ID, app
version, category, severity, redacted summary, reproduction steps, expected/actual
results, severity rationale, optional error code and AI/human reporter type. No
conversation/account data or logs are automatically collected. Only the developer
and authenticated Codex using the owner's Cloudflare session can read it. Normal
connection metadata reaches Cloudflare. Consent can be revoked for future uploads;
previous uploads remain until the developer deletes the resolved reports. Common
secret-pattern checks do not replace manual redaction. See [BUG_REPORTING.md](BUG_REPORTING.md)
for storage caps and deletion/recovery limits. This is the explicit exception to
the normal no-telemetry/no-maintainer-upload statements below.

Startup and manual update checks retrieve public JSON from GitHub Releases. Only when the user chooses an update does Swarm download a signed installer from GitHub and its distribution CDN. Conversation bodies, account details, passwords, recovery codes and backups are not sent, and no GitHub credentials are used. Normal HTTP connection information, including the IP address, reaches those services. Private repositories and unpublished update feeds are unavailable to the app. See the [update guide](UPDATES.md).

Swarm is a local Windows conversation application. Conversation bodies, edit/deletion history, project registrations, account/settings/permission information and read/subscription state are stored in the selected local data directory. Password verification data is stored locally; credentials and sensitive pending operations use Windows DPAPI and owner-restricted storage. Local accounts and configured permissions control access. AI clients operate through local Named Pipes and their own user-managed RULES.

Swarm does not operate a hosted conversation service, send conversation data to an AI provider, launch an external AI, or add application telemetry/advertising. An AI client used separately by you may send content it is permitted to read to its own provider; that client's behavior and settings are outside Swarm. The development use of Codex/ChatGPT does not make them Swarm runtime services.

Google linking and Google-based recovery are unavailable in the first beta. The GUI offers no Google action, and execution paths reject it before authentication requests, browser launch or token exchange. Normal password authentication and recovery codes remain available. Existing development-data bindings are preserved locally; this beta does not modify or reactivate them. The dormant Google-enabled implementation requires a separately enabled build and renewed acceptance before future distribution.

Windows/WebView2 and the user's chosen browser have their own operating-system/provider update and privacy behavior. Swarm's local-data claims do not replace those policies.

Backups are written to a user-selected location and can contain conversations, retained history, registrations/settings and account verification data. They exclude active session/resume/transport/notification-selection credentials as specified by the backup implementation. Treat backups as private. A separately configured sync/storage provider may transmit files you place in its folders; Swarm does not add cloud sync. Restore requires current human authorization and reauthentication.

Users control local data, account settings, retained history/deleted-data settings and subscriptions through the available human operations. Google binding changes are unavailable in this beta. Uninstall/update behavior and any explicit data-deletion checkbox have separate acceptance checks; uninstall must not be assumed to erase backups or files managed by other software. No data is uploaded to Swarm maintainers as part of normal operation. Beta feedback, if separately agreed after release, should omit credentials and private bodies.

Before a public Google-enabled release, the owner must set the actual publisher/support identity, public policy URL and Google OAuth consent/branding/publication scope. No identity/address/approval has been invented in this local notice. Those external publication settings and real Google acceptance remain held.
