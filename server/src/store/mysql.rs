//! `SaveStore` 의 MySQL 구현.
//!
//! 열거형은 도메인의 serde 표현을 그대로 문자열 컬럼에 넣는다 (§4.3). 변환을 한 곳에
//! 모아 두어 API 응답과 DB 값이 갈라지지 않게 한다.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::MySqlPool;

use super::types::{SaveState, SaveStore};
use crate::character::Character;

/// 캐릭터를 만들기 전의 기본 자금 (원).
const INITIAL_CASH_KRW: i64 = 10_000_000;

pub struct MySqlSaveStore {
    pool: MySqlPool,
    /// 인증이 붙기 전까지 세이브는 하나다. 기동할 때 확정해 둔다 (§4.4).
    save_id: u64,
}

/// 세이브가 없으면 만들고, 그 세이브에 묶인 저장소를 돌려준다.
pub async fn create_mysql_save_store(pool: MySqlPool) -> Result<MySqlSaveStore> {
    let save_id = ensure_save(&pool)
        .await
        .context("세이브를 준비하지 못했습니다")?;

    Ok(MySqlSaveStore { pool, save_id })
}

async fn ensure_save(pool: &MySqlPool) -> Result<u64> {
    let existing: Option<(u64,)> = sqlx::query_as("SELECT id FROM save ORDER BY id LIMIT 1")
        .fetch_optional(pool)
        .await?;

    if let Some((id,)) = existing {
        return Ok(id);
    }

    let result = sqlx::query("INSERT INTO save (game_day, cash_krw, debt_krw) VALUES (0, ?, 0)")
        .bind(INITIAL_CASH_KRW)
        .execute(pool)
        .await?;

    Ok(result.last_insert_id())
}

#[async_trait]
impl SaveStore for MySqlSaveStore {
    async fn load(&self) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;
        let state = read_state(&mut tx, self.save_id).await?;
        tx.commit().await?;

        Ok(state)
    }

    async fn start_game(&self, character: &Character) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE save SET game_day = 0, cash_krw = ?, debt_krw = ? WHERE id = ?")
            .bind(character.cash_krw)
            .bind(character.debt_krw)
            .bind(self.save_id)
            .execute(&mut *tx)
            .await?;

        // 세이브당 캐릭터는 하나다. 갈아엎는 편이 upsert 보다 읽기 쉽다
        sqlx::query("DELETE FROM `character` WHERE save_id = ?")
            .bind(self.save_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO `character`
                 (save_id, name, age, gender, military, region, background,
                  education, career_years, certifications, health, dependents)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(self.save_id)
        .bind(&character.name)
        .bind(character.age)
        .bind(to_db_str(&character.gender)?)
        .bind(to_db_str(&character.military)?)
        .bind(to_db_str(&character.region)?)
        .bind(to_db_str(&character.background)?)
        .bind(to_db_str(&character.education)?)
        .bind(character.career_years)
        .bind(character.certifications)
        .bind(to_db_str(&character.health)?)
        .bind(character.dependents)
        .execute(&mut *tx)
        .await?;

        let state = read_state(&mut tx, self.save_id).await?;
        tx.commit().await?;

        Ok(state)
    }

    async fn advance(&self, days: u32) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;

        // 읽고-더하고-쓰지 않고 DB 에서 더한다 — 동시 요청이 서로의 전진을 덮어쓰지 않게
        sqlx::query("UPDATE save SET game_day = game_day + ? WHERE id = ?")
            .bind(days)
            .bind(self.save_id)
            .execute(&mut *tx)
            .await?;

        let state = read_state(&mut tx, self.save_id).await?;
        tx.commit().await?;

        Ok(state)
    }
}

#[derive(sqlx::FromRow)]
struct SaveRow {
    game_day: u32,
    cash_krw: i64,
    debt_krw: i64,
}

#[derive(sqlx::FromRow)]
struct CharacterRow {
    name: String,
    age: u32,
    gender: String,
    military: String,
    region: String,
    background: String,
    education: String,
    career_years: u32,
    certifications: u32,
    health: String,
    dependents: u32,
}

async fn read_state(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
) -> Result<SaveState> {
    let save: Option<SaveRow> =
        sqlx::query_as("SELECT game_day, cash_krw, debt_krw FROM save WHERE id = ?")
            .bind(save_id)
            .fetch_optional(&mut **tx)
            .await?;

    let Some(save) = save else {
        bail!("세이브 {save_id} 이 사라졌습니다");
    };

    let character: Option<CharacterRow> = sqlx::query_as(
        "SELECT name, age, gender, military, region, background,
                education, career_years, certifications, health, dependents
         FROM `character` WHERE save_id = ?",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;

    let character = match character {
        Some(row) => Some(to_character(row, save.cash_krw, save.debt_krw)?),
        None => None,
    };

    Ok(SaveState {
        game_day: save.game_day,
        cash_krw: save.cash_krw,
        debt_krw: save.debt_krw,
        character,
    })
}

/// 캐릭터의 현금·부채는 세이브가 들고 있다 — 게임이 진행되면 변하는 값이라서다.
fn to_character(row: CharacterRow, cash_krw: i64, debt_krw: i64) -> Result<Character> {
    Ok(Character {
        name: row.name,
        age: row.age,
        gender: from_db_str(&row.gender)?,
        military: from_db_str(&row.military)?,
        region: from_db_str(&row.region)?,
        background: from_db_str(&row.background)?,
        education: from_db_str(&row.education)?,
        career_years: row.career_years,
        certifications: row.certifications,
        cash_krw,
        debt_krw,
        health: from_db_str(&row.health)?,
        dependents: row.dependents,
    })
}

/// 열거형 → 컬럼 문자열. serde 표현(camelCase)을 그대로 쓴다.
fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("문자열로 저장할 수 없는 값입니다: {other}"),
    }
}

/// 컬럼 문자열 → 열거형.
fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("알 수 없는 값이 저장되어 있습니다: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Education, FamilyBackground, Gender, MilitaryStatus, Region};

    /// DB 왕복은 테스트하지 않는다 (§테스트 정책). 조용히 틀릴 수 있는 것은
    /// 열거형 ↔ 컬럼 문자열 변환이라서 그것만 본다.
    mod context_enum_columns_round_trip {
        use super::*;

        #[test]
        fn given_an_enum_when_stored_and_read_back_then_it_is_unchanged() {
            let military = MilitaryStatus::Alternative;

            let stored = to_db_str(&military).expect("저장 표현으로 바꿀 수 있어야 한다");
            let restored: MilitaryStatus = from_db_str(&stored).expect("되읽을 수 있어야 한다");

            assert_eq!(restored, military);
        }

        #[test]
        fn given_a_multi_word_variant_when_stored_then_it_uses_the_serde_name() {
            let region = Region::CapitalArea;

            let stored = to_db_str(&region).expect("저장 표현으로 바꿀 수 있어야 한다");

            assert_eq!(stored, "capitalArea");
        }
    }

    mod context_a_stored_value_is_not_a_known_variant {
        use super::*;

        #[test]
        fn given_an_unknown_string_when_read_then_it_fails_instead_of_guessing() {
            let stored = "graduateSchool";

            let restored = from_db_str::<Education>(stored);

            assert!(restored.is_err());
        }
    }

    mod context_a_character_row_is_assembled {
        use super::*;

        fn given_row() -> CharacterRow {
            CharacterRow {
                name: "테스터".to_owned(),
                age: 25,
                gender: "male".to_owned(),
                military: "completed".to_owned(),
                region: "capitalArea".to_owned(),
                background: "independent".to_owned(),
                education: "bachelor".to_owned(),
                career_years: 1,
                certifications: 1,
                health: "normal".to_owned(),
                dependents: 0,
            }
        }

        #[test]
        fn given_a_row_when_assembled_then_enums_come_from_their_columns() {
            let row = given_row();

            let character = to_character(row, 10_000_000, 0).expect("조립할 수 있어야 한다");

            assert_eq!(
                (
                    character.gender,
                    character.military,
                    character.region,
                    character.background,
                    character.education
                ),
                (
                    Gender::Male,
                    MilitaryStatus::Completed,
                    Region::CapitalArea,
                    FamilyBackground::Independent,
                    Education::Bachelor
                )
            );
        }

        #[test]
        fn given_a_row_when_assembled_then_money_comes_from_the_save_not_the_row() {
            let row = given_row();

            let character = to_character(row, 7_000_000, 3_000_000).expect("조립할 수 있어야 한다");

            assert_eq!(
                (character.cash_krw, character.debt_krw),
                (7_000_000, 3_000_000)
            );
        }
    }
}
