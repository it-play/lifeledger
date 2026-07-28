# LifeLedger SQLx transport patch

This directory is the crates.io `sqlx-core` 0.9.0 package with checksum
`05b44e85bf579a8eeb4ceaa77a3a523baf2bf0e9bac7e40f405d537b5d2d5ccb`.
The upstream MIT and Apache-2.0 license files are preserved.

LifeLedger changes only `src/net/socket/mod.rs`: Tokio and async-io TCP streams call
`set_nodelay(true)` immediately after connecting. This restores the behavior from upstream
SQLx 0.8.6 / pull request #3055 that regressed in SQLx 0.9.0. Remove this path patch after an
upstream release restores `TCP_NODELAY`, then repeat the production one-day and 30-day checks.
