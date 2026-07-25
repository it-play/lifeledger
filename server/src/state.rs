use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::character::{Character, net_worth_krw};

/// 시작 게임 날짜. 나중에 월드 시드 설정으로 옮긴다.
const START_DATE: &str = "2026-01-01";
/// 캐릭터를 만들기 전의 기본 자금 (원). 캐릭터가 생기면 그 값으로 대체된다.
const INITIAL_CASH_KRW: i64 = 10_000_000;

/// 클라이언트에 보내는 게임 상태 스냅샷.
///
/// 날짜 문자열을 매번 계산해 보내지 않고 시작일 + 경과일만 보낸다.
/// 표시용 날짜 계산은 결정론적이라 클라이언트가 해도 권위 문제가 없다.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug)]
pub struct AppState {
    inner: Mutex<Inner>,
    ticks: broadcast::Sender<GameSnapshot>,
}

#[derive(Debug)]
struct Inner {
    game_day: u32,
    cash_krw: i64,
    debt_krw: i64,
    character: Option<Character>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let (ticks, _) = broadcast::channel(256);
        Arc::new(Self {
            inner: Mutex::new(Inner {
                game_day: 0,
                cash_krw: INITIAL_CASH_KRW,
                debt_krw: 0,
                character: None,
            }),
            ticks,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GameSnapshot> {
        self.ticks.subscribe()
    }

    pub fn snapshot(&self) -> GameSnapshot {
        let inner = self.inner.lock().expect("state mutex poisoned");
        Self::snapshot_of(&inner)
    }

    /// 게임일을 하루씩 전진시키며 매일의 스냅샷을 방송한다.
    /// 1개월 스텝이나 배속은 이 루프를 서버가 연속 실행하는 것으로 표현된다.
    pub fn advance(&self, days: u32) -> GameSnapshot {
        let mut last = self.snapshot();
        for _ in 0..days {
            let snapshot = {
                let mut inner = self.inner.lock().expect("state mutex poisoned");
                inner.game_day += 1;
                // TODO(M1): 시장 일봉 조회 → 평가 → 정산. 지금은 자리만 잡아 둔다.
                Self::snapshot_of(&inner)
            };
            // 구독자가 없으면 오류가 나는데, 그건 정상 상황이므로 무시한다
            let _ = self.ticks.send(snapshot.clone());
            last = snapshot;
        }
        last
    }

    /// 캐릭터를 확정하고 게임을 시작 상태로 되돌린다 (게임일 0).
    pub fn start_game(&self, character: Character) -> GameSnapshot {
        let snapshot = {
            let mut inner = self.inner.lock().expect("state mutex poisoned");
            inner.game_day = 0;
            inner.cash_krw = character.cash_krw;
            inner.debt_krw = character.debt_krw;
            inner.character = Some(character);
            Self::snapshot_of(&inner)
        };
        let _ = self.ticks.send(snapshot.clone());
        snapshot
    }

    fn snapshot_of(inner: &Inner) -> GameSnapshot {
        GameSnapshot {
            game_day: inner.game_day,
            start_date: START_DATE,
            cash_krw: inner.cash_krw,
            debt_krw: inner.debt_krw,
            net_worth_krw: net_worth_krw(inner.cash_krw, inner.debt_krw),
            character_name: inner.character.as_ref().map(|c| c.name.clone()),
        }
    }
}
