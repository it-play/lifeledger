use anyhow::{Context, Result, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use super::RankingPageCursor;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RankingCursorWire {
    after_tax_net_worth_krw: i64,
    insolvency_days: u32,
    player_command_count: u64,
    save_id: u64,
    run_revision: u32,
}

pub fn parse_ranking_cursor(value: &str) -> Result<RankingPageCursor> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .context("ranking cursor is not canonical base64url")?;
    ensure!(
        URL_SAFE_NO_PAD.encode(&decoded) == value,
        "ranking cursor is not canonical base64url"
    );
    let cursor: RankingCursorWire =
        serde_json::from_slice(&decoded).context("ranking cursor payload is invalid")?;
    ensure!(
        cursor.save_id > 0 && cursor.run_revision > 0,
        "ranking cursor identity is invalid"
    );

    Ok(RankingPageCursor {
        after_tax_net_worth_krw: cursor.after_tax_net_worth_krw,
        insolvency_days: cursor.insolvency_days,
        player_command_count: cursor.player_command_count,
        save_id: cursor.save_id,
        run_revision: cursor.run_revision,
    })
}

pub fn encode_ranking_cursor(cursor: RankingPageCursor) -> Result<String> {
    let encoded = serde_json::to_vec(&RankingCursorWire {
        after_tax_net_worth_krw: cursor.after_tax_net_worth_krw,
        insolvency_days: cursor.insolvency_days,
        player_command_count: cursor.player_command_count,
        save_id: cursor.save_id,
        run_revision: cursor.run_revision,
    })
    .context("failed to serialize a ranking cursor")?;

    Ok(URL_SAFE_NO_PAD.encode(encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_랭킹_페이지_커서가_정렬_위치를_담는_경우 {
        use super::*;

        #[test]
        fn given_정렬_tuple_when_직렬화_후_읽으면_then_같은_위치로_복원한다() {
            let cursor = RankingPageCursor {
                after_tax_net_worth_krw: 123_456_789,
                insolvency_days: 4,
                player_command_count: 88,
                save_id: 29,
                run_revision: 7,
            };

            let encoded = encode_ranking_cursor(cursor).expect("커서를 직렬화할 수 있어야 한다");
            let parsed = parse_ranking_cursor(&encoded).expect("커서를 다시 읽을 수 있어야 한다");

            assert_eq!(parsed, cursor);
        }
    }
}
