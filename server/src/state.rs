use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

use crate::character::{Character, net_worth_krw};
use crate::store::{SaveState, SaveStore};

/// 시작 게임 날짜. 나중에 월드 시드 설정으로 옮긴다.
const START_DATE: &str = "2026-01-01";

/// 클라이언트에 보내는 게임 상태 스냅샷.
///
/// 날짜 문자열을 매번 계산해 보내지 않고 시작일 + 경과일만 보낸다.
/// 표시용 날짜 계산은 결정론적이라 클라이언트가 해도 권위 문제가 없다.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub game_day: u32,
    pub start_date: &'static str,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub net_worth_krw: i64,
    /// 캐릭터를 아직 만들지 않았으면 None — 클라이언트는 생성 화면으로 보낸다.
    pub character_name: Option<String>,
}

/// 게임일 전진의 권위는 서버에 있다 (§4.2). 클라이언트는 "얼마나" 만 요청한다.
///
/// 상태 자체는 DB 가 들고 있다 (§4.4). 여기서는 저장소 호출을 조립하고,
/// 커밋된 결과만 스트림으로 흘린다.
pub struct AppState {
    store: Arc<dyn SaveStore>,
    ticks: broadcast::Sender<GameSnapshot>,
}

impl AppState {
    pub fn new(store: Arc<dyn SaveStore>) -> Arc<Self> {
        let (ticks, _) = broadcast::channel(256);
        Arc::new(Self { store, ticks })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GameSnapshot> {
        self.ticks.subscribe()
    }

    pub async fn snapshot(&self) -> Result<GameSnapshot> {
        Ok(to_snapshot(&self.store.load().await?))
    }

    /// 게임일을 전진시키고 결과를 방송한다.
    ///
    /// 지금은 마지막 상태 하나만 흘린다. 일별 정산(M1)이 붙으면 하루가 실제로
    /// 값을 바꾸므로, 그때 하루 단위 틱으로 나눈다.
    pub async fn advance(&self, days: u32) -> Result<GameSnapshot> {
        let snapshot = to_snapshot(&self.store.advance(days).await?);
        self.broadcast(&snapshot);

        Ok(snapshot)
    }

    /// 캐릭터를 확정하고 게임을 시작 상태로 되돌린다 (게임일 0).
    pub async fn start_game(&self, character: Character) -> Result<GameSnapshot> {
        let snapshot = to_snapshot(&self.store.start_game(&character).await?);
        self.broadcast(&snapshot);

        Ok(snapshot)
    }

    fn broadcast(&self, snapshot: &GameSnapshot) {
        // 구독자가 없으면 오류가 나는데, 그건 정상 상황이므로 무시한다
        let _ = self.ticks.send(snapshot.clone());
    }
}

fn to_snapshot(state: &SaveState) -> GameSnapshot {
    GameSnapshot {
        game_day: state.game_day,
        start_date: START_DATE,
        cash_krw: state.cash_krw,
        debt_krw: state.debt_krw,
        net_worth_krw: net_worth_krw(state.cash_krw, state.debt_krw),
        character_name: state.character.as_ref().map(|c| c.name.clone()),
    }
}
