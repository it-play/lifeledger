//! MySQL implementation of `SaveStore`.
//!
//! Enums go into string columns using their domain serde representation (§4.3). Keeping
//! the conversion in one place stops API responses and stored values from drifting apart.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::MySqlPool;

use super::types::{SaveState, SaveStore};
use crate::character::Character;

/// Starting cash before a character exists, in KRW.
const INITIAL_CASH_KRW: i64 = 10_000_000;

pub struct MySqlSaveStore {
    pool: MySqlPool,
}

pub const fn create_mysql_save_store(pool: MySqlPool) -> MySqlSaveStore {
    MySqlSaveStore { pool }
}

/// Finds the account's save, creating it if absent. One save per account (§4.5).
async fn ensure_save(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, user_id: u64) -> Result<u64> {
    let existing: Option<(u64,)> =
        sqlx::query_as("SELECT id FROM save WHERE user_id = ? ORDER BY id LIMIT 1")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?;

    if let Some((id,)) = existing {
        return Ok(id);
    }

    let result =
        sqlx::query("INSERT INTO save (user_id, game_day, cash_krw, debt_krw) VALUES (?, 0, ?, 0)")
            .bind(user_id)
            .bind(INITIAL_CASH_KRW)
            .execute(&mut **tx)
            .await?;

    Ok(result.last_insert_id())
}

#[async_trait]
impl SaveStore for MySqlSaveStore {
    async fn load(&self, user_id: u64) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;
        let state = read_state(&mut tx, save_id).await?;
        tx.commit().await?;

        Ok(state)
    }

    async fn start_game(&self, user_id: u64, character: &Character) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;

        sqlx::query("UPDATE save SET game_day = 0, cash_krw = ?, debt_krw = ? WHERE id = ?")
            .bind(character.cash_krw)
            .bind(character.debt_krw)
            .bind(save_id)
            .execute(&mut *tx)
            .await?;

        // One character per save; replacing reads more clearly than an upsert
        sqlx::query("DELETE FROM `character` WHERE save_id = ?")
            .bind(save_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO `character`
                 (save_id, name, age, gender, military, region, background,
                  education, career_years, certifications, health, dependents)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
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

        let state = read_state(&mut tx, save_id).await?;
        tx.commit().await?;

        Ok(state)
    }

    async fn advance(&self, user_id: u64, days: u32) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;

        // Add in the database rather than read-modify-write, so concurrent requests
        // cannot overwrite each other's advance
        sqlx::query("UPDATE save SET game_day = game_day + ? WHERE id = ?")
            .bind(days)
            .bind(save_id)
            .execute(&mut *tx)
            .await?;

        let state = read_state(&mut tx, save_id).await?;
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
        bail!("save {save_id} disappeared");
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
        save_id,
        game_day: save.game_day,
        cash_krw: save.cash_krw,
        debt_krw: save.debt_krw,
        character,
    })
}

/// Cash and debt live on the save, not the character row: they change as the game runs.
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

/// Enum -> column string, reusing the serde (camelCase) representation.
fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

/// Column string -> enum.
fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown value stored: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Education, FamilyBackground, Gender, MilitaryStatus, Region};

    /// Database round trips are out of scope for tests. What can silently go wrong is the
    /// enum <-> column string conversion, so that is what is covered here.
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
