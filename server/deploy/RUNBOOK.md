# LifeLedger production runbook

This runbook covers the M5-F development-production environment. Commands run from
server/deploy/ on the home server. They must not print app.env, DATABASE_URL, OAuth credentials,
cookies, session tokens, user identifiers, character values, or money amounts.

The first read-only check is always:

~~~sh
./scripts/observe.sh
~~~

The schema 2 report contains only aggregate counts and stable alert codes. It does not run migrations or
change database state. Do not create a separate database, clone, or recovery dump during the
current development stage. With no external participants, recovery means rebuilding an empty
database from immutable migrations and accepting loss of development data. This is not a
backup/restore rehearsal. Preserving external participant data later requires a separately approved
encrypted location, retention period, and deletion plan.

## Deployment verification

1. Confirm the GitHub Server Deploy run completed successfully.
2. Run docker compose ps and require both lifeledger-server and lifeledger-offline-worker to be
   healthy.
3. Run ./scripts/observe.sh.
4. Require migrations.failedCount=0 and the expected latestSuccessfulVersion.
5. Check logs from the new containers:

   ~~~sh
   docker compose logs --since 15m server offline-worker
   ~~~

6. Confirm no new ERROR, panic, migration failure, or restart loop. A non-empty alert list is
   investigated below; it is not deleted or hidden to make validation pass.

## Migration failure

### Confirm

- Keep the previously healthy API serving until the failed rollout is understood.
- Inspect the failed deployment log and the new server container startup log.
- Read _sqlx_migrations through the production database console and identify only the failed
  version, checksum, and success flag. Do not print the database URL or unrelated rows.
- Inspect which objects from that exact migration were created before the failure.

### Mitigate

- Stop the newly failing server container if it is restart-looping. Do not stop MySQL or delete
  existing immutable bundles and runs.
- Fix the migration source. Never edit the checksum of a migration that completed successfully.
- When MySQL DDL left partial objects, remove only objects proven to belong to the failed version,
  in reverse dependency order, and remove only that failed _sqlx_migrations row.

### Recover

- Commit and push the correction through the normal Server Deploy workflow.
- Let API startup apply the migration. Do not apply an uncommitted replacement by hand.

### Verify

- Run ./scripts/observe.sh; require failed migration count zero and the expected latest version.
- Check API/worker health, startup logs, immutable catalog counts, existing run counts, and open
  InnoDB transactions.

## Worker backlog or paused progress

### Confirm

- Read offlineProgress.pendingRunCount, pendingDayCount, oldestAccrualAgeSeconds, pausedRunCount,
  and the recent committed/failed counts.
- Check worker health and logs. Use stable error codes; do not inspect player payloads.
- Confirm the active worker image engine version matches the pinned offline policy.

### Mitigate

- Leave online commands authoritative. Do not increase a run's pending days or edit game day.
- If the worker is unhealthy, restart only offline-worker.
- If failures are a permanent domain/policy error, keep the setting pausedBySystem until the
  underlying versioned rule is fixed. Do not clear the error merely to resume throughput.

### Recover

- Deploy the compatible worker image and allow normal lease acquisition to resume from the last
  committed day.
- Change poll/batch configuration only after a measured baseline; never bypass one-day commits.

### Verify

- Pending days decrease, recent committed count increases, failed count stops increasing, and
  manual/online progress remains available.
- A paused setting is resumed only through its typed control path after the cause is removed.

## Stuck or expired worker lease

### Confirm

- Check activeWorkerLeaseCount and expiredWorkerLeaseCount.
- Confirm no live worker owns the reported generation before considering cleanup.
- Compare DB UTC time, lease expiry, worker logs, and container process state.

### Mitigate and recover

- Restart an unhealthy worker and let expiry fencing reject the old holder.
- Do not rewrite a live lease, generation, game day, command receipt, or ledger row.
- An expired lease row may remain as audit state; a new holder replaces it only through the
  normal acquisition transaction.

### Verify

- The new worker commits with a higher generation, duplicate day/receipt counts remain zero, and
  the expired lease alert clears after normal acquisition.

## Season lock and provisional ranking

### Confirm

- Check season status counts, ranked run count, and finalization completed/failed counts.
- A provisional empty ranking is expected when minimum participants or completed finalizations
  are absent.
- For a suspected integrity fault, identify the affected sealed season/release/rule hashes
  without changing them.

### Mitigate and recover

- Move an affected season only through the permitted active or registrationOpen to locked
  transition and close new ranked starts before changing any assignment.
- Never mutate a sealed bundle, finalization, or ranking evidence in place.
- Publish a corrected version/season. Existing unsupported runs remain in maintenance state.

### Verify

- New starts cannot enter the locked season, existing manifests keep their original hashes, and
  rankings read only completed immutable finalizations with stable cursor ordering.

## OAuth provider outage

### Confirm

- Check provider-specific callback failures without logging authorization codes, tokens, email,
  or cookies.
- Verify /api/health independently. One provider outage must not be mistaken for API failure.

### Mitigate and recover

- Disable a provider by removing both its client ID and secret from the deployment secret, then
  redeploy. Never leave only one credential configured.
- Keep existing sessions subject to their normal expiry; do not manufacture a bypass session.
- Restore both credentials and the registered callback origin, then redeploy.

### Verify

- The login screen exposes only fully configured providers and callback failure logs contain no
  credentials or user profile data.

## Database recovery

### Confirm

- Declare the exact incident scope and recovery point before authorizing any restore.
- Lock ranked season registration if integrity or ordering may be affected.

### Mitigate and recover

- Do not create an ad-hoc dump under the repository or deployment directory.
- Use only the separately approved encrypted backup location and retention policy.
- Restore into the approved target, verify migration checksums and immutable hashes, then switch
  API/worker connectivity through the deployment secret. Never overwrite production based on an
  unverified copy.

### Verify

- Run the sanitized report, health checks, immutable manifest/bundle hash checks, ledger balance
  checks, command replay checks, and finalization/ranking checks before reopening a season.

## Playtest consent withdrawal or feedback deletion

### Confirm

- Require the authenticated owner to use GET /api/playtest/feedback. Do not locate a report by
  asking for an email, OAuth profile, save/run identifier, session token, message text, or money
  amount.
- Confirm only the current consent status/revision and aggregate active feedback count. Operators
  do not read or copy category, severity, message, manifest hash, or finalization hash.
- Distinguish one-report deletion from full consent withdrawal. Account deletion is a separate
  procedure below and must not be inferred from either action.

### Mitigate and recover

- For one report, use DELETE /api/playtest/feedback/{feedbackId} through the authenticated owner
  UI/API. For all active reports, use PUT /api/playtest/consent with action withdraw and the exact
  current policy/revision.
- Never hard-delete or manually null feedback columns. The application transaction changes the
  row to a withdrawn tombstone and clears category, severity, message, run revision, manifest
  hash, and finalization hash together.
- Do not export a dump, copy feedback into a ticket or analytics store, or change consent event
  history. A failed or unknown POST is resolved by refreshing the owner list before any retry.

### Verify

- Through the owner API, require zero active rows for a full withdrawal or absence of the one
  deleted report. A repeated DELETE of the same owned tombstone may return the same withdrawn
  result.
- Through an aggregate database query, verify the affected active count decreased and every
  withdrawn row has null content/evidence fields. Do not print public UUIDs or owner identifiers.
- Confirm no open transaction, error log, temporary session, report container, dump, or analytics
  copy remains. Consent withdrawal does not delete the account; approved account deletion uses
  the foreign-key cascade described below.

## Expired playtest feedback retention

### Confirm

- Read only feedbackRetention.activeCount, expiredCount, overdueActiveCount and the stable
  expiredFeedbackRetention alert from ops-report. Do not query or print owner IDs, public UUIDs,
  category, severity, message, or run hashes.
- Confirm the offline worker is healthy. It checks the sealed policy retention value at most once
  every 60 seconds; policy v2 fixes the maximum at 90 days.

### Mitigate and recover

- Restart only offline-worker when the retention pass is not running. Do not edit created_at,
  policy manifests, consent events, or feedback content by hand.
- Let the application store transition overdue active rows to expired tombstones. The transition
  clears category, severity, message, run revision, manifest hash, and finalization hash together.

### Verify

- Require overdueActiveCount=0 and the alert to clear. expiredCount may remain as content-free
  deletion evidence until its owner deletes the account.
- Confirm the worker log reports only the aggregate purged count and never an owner or feedback ID.

## Privacy or deletion request

### Confirm

- Authenticate the requester through the supported account path. Do not request real asset,
  income, health, or identity documents.
- Identify the account scope without copying raw session tokens or OAuth profiles into tickets.

### Mitigate and recover

- The owner uses the dashboard's double confirmation, which sends
  `DELETE /api/auth/account` with the exact `{"confirmation":"deleteAccount"}` body.
- The server pauses that account's automatic progress under its save operation lock, deletes the
  `user` row, lets FK cascades delete every session, save, consent event, and feedback row, removes
  the runtime cache entry, and clears the session cookie. Do not run manual DELETE statements.
- Aggregate operations reports and logs must remain free of user/save/run identifiers, character
  values, command IDs, and money amounts.

### Verify

- Confirm the response is 204, the old cookie receives 401, and aggregate counts show no orphaned
  rows. A smoke test creates a disposable account and deletes only that account; it never uses the
  standing QA account.
- Confirm operational logs contain no newly introduced personal data. The deletion is permanent;
  the current no-backup development policy provides no recovery path.

## Public notice and incident contact

- The dashboard is the release notice authority visible to a signed-in participant. It states that
  all assets and characters are fictional, investment/legal/insurance models are simplified and
  are not advice, analytics is disabled, feedback retention is at most 90 days, and deletion paths
  are available.
- Known issues, outage reports, and deletion questions use
  https://github.com/it-play/lifeledger/issues. The reporter must not include an email address,
  session token, actual financial information, or feedback message content.
