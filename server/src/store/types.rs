//! 저장소 계약. 구현(MySQL)은 이 파일을 모르고, 상위 계층은 구현을 모른다.

use anyhow::Result;
use async_trait::async_trait;

use crate::character::Character;

/// 세이브 하나의 지속 상태. DB 에 있는 것과 같은 값이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveState {
    pub game_day: u32,
    pub cash_krw: i64,
    pub debt_krw: i64,
    /// 아직 캐릭터를 만들지 않았으면 `None`.
    pub character: Option<Character>,
}

/// 세이브의 읽기·쓰기.
///
/// 인증이 붙기 전까지 서버는 세이브 하나만 다룬다 (§4.4). 그래서 메서드에
/// 세이브 식별자가 없다 — 계정이 생기면 여기에 인자로 들어온다.
#[async_trait]
pub trait SaveStore: Send + Sync + 'static {
    /// 현재 세이브를 읽는다. 없으면 초기 상태로 만들어 반환한다.
    async fn load(&self) -> Result<SaveState>;

    /// 캐릭터를 확정하고 세이브를 그 시작 조건으로 되돌린다.
    async fn start_game(&self, character: &Character) -> Result<SaveState>;

    /// 게임일을 `days` 만큼 전진시킨다. 되감기는 없다 (§2).
    async fn advance(&self, days: u32) -> Result<SaveState>;
}
