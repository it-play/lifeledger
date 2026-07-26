use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use super::types::{
    CommittedGameState, DailyAdvanceResult, DailyCommandAdvanceResult, DailyPipeline,
    DailyStartGameResult,
};
use crate::market::{MarketDay, MarketWorld};
use crate::store::{
    AdvanceCommandStepResult, AdvanceDayResult, ManualAdvanceCommand, MarketStore,
    RecruitmentPostingStore, SaveCursor, SaveState, SaveStore, StartGameCommand, StartGameResult,
};

const MAX_CURSOR_RETRIES: usize = 3;
const MAX_START_GAME_RETRIES: usize = 3;
const MARKET_SETTLEMENT_LOOKAHEAD_DAYS: u32 = 14;

struct DefaultDailyPipeline {
    saves: Arc<dyn SaveStore>,
    markets: Arc<dyn MarketStore>,
    recruitment_postings: Arc<dyn RecruitmentPostingStore>,
}

pub fn create_daily_pipeline(
    saves: Arc<dyn SaveStore>,
    markets: Arc<dyn MarketStore>,
    recruitment_postings: Arc<dyn RecruitmentPostingStore>,
) -> Arc<dyn DailyPipeline> {
    Arc::new(DefaultDailyPipeline {
        saves,
        markets,
        recruitment_postings,
    })
}

#[async_trait]
impl DailyPipeline for DefaultDailyPipeline {
    async fn load(&self, user_id: u64) -> Result<CommittedGameState> {
        let save = self.saves.load(user_id).await?;
        let refresh_after_market_prepare =
            save.character.is_none() && save.m2d_assets.product_bundle.is_none();
        let state = self.assemble(save).await?;
        if refresh_after_market_prepare && state.world.index_product.is_some() {
            return self.assemble(self.saves.load(user_id).await?).await;
        }
        Ok(state)
    }

    async fn start_game(
        &self,
        user_id: u64,
        command: &StartGameCommand,
    ) -> Result<DailyStartGameResult> {
        for _ in 0..MAX_START_GAME_RETRIES {
            let active = self.saves.active_run_configuration().await?;
            let world = self
                .markets
                .load_world(active.market_world.world_id)
                .await?;
            let market = self
                .markets
                .ensure_day(active.market_world.world_id, 0)
                .await?;
            if world.id != active.market_world.world_id || market.game_day != 0 {
                bail!("prepared new-run market does not match the active world");
            }

            match self.saves.start_game(user_id, command, active).await? {
                StartGameResult::Applied { save, receipt } => {
                    return Ok(DailyStartGameResult::Applied {
                        state: Box::new(CommittedGameState {
                            save: *save,
                            world: world.world,
                            market,
                        }),
                        receipt,
                    });
                }
                StartGameResult::Replayed { save, receipt } => {
                    return Ok(DailyStartGameResult::Replayed {
                        state: Box::new(self.assemble(*save).await?),
                        receipt,
                    });
                }
                StartGameResult::Rejected(rejection) => {
                    return Ok(DailyStartGameResult::Rejected(rejection));
                }
                StartGameResult::ActiveWorldChanged => continue,
            }
        }

        bail!("active market world kept changing while starting a game")
    }

    async fn advance_one_day(&self, user_id: u64) -> Result<DailyAdvanceResult> {
        for _ in 0..MAX_CURSOR_RETRIES {
            let current = self.saves.load(user_id).await?;
            if current.character.is_none() {
                return Ok(DailyAdvanceResult::CharacterRequired);
            }
            let target_day = current
                .game_day
                .checked_add(1)
                .context("game day overflowed")?;
            let world = self.markets.load_world(current.market_world_id).await?;
            let market = self
                .market_for_settlement(current.market_world_id, &world.world, target_day)
                .await?;
            self.recruitment_postings
                .ensure_postings_for_user(user_id, target_day)
                .await?;

            match self
                .saves
                .advance_one_day(user_id, SaveCursor::from(&current), &market)
                .await?
            {
                AdvanceDayResult::Advanced(save) => {
                    if save.market_world_id != world.id || save.game_day != market.game_day {
                        bail!("committed save does not match the selected market day");
                    }
                    return Ok(DailyAdvanceResult::Advanced(Box::new(CommittedGameState {
                        save,
                        world: world.world,
                        market,
                    })));
                }
                AdvanceDayResult::CharacterRequired => {
                    return Ok(DailyAdvanceResult::CharacterRequired);
                }
                AdvanceDayResult::Stale(_) => continue,
            }
        }

        bail!("save cursor kept changing while advancing one day")
    }

    async fn advance_command_step(
        &self,
        user_id: u64,
        command: &ManualAdvanceCommand,
    ) -> Result<DailyCommandAdvanceResult> {
        for _ in 0..MAX_CURSOR_RETRIES {
            let current = self.saves.load(user_id).await?;
            let target_day = current
                .game_day
                .checked_add(1)
                .context("game day overflowed")?;
            let world = self.markets.load_world(current.market_world_id).await?;
            let market = self
                .market_for_settlement(current.market_world_id, &world.world, target_day)
                .await?;
            self.recruitment_postings
                .ensure_postings_for_user(user_id, target_day)
                .await?;

            match self
                .saves
                .advance_command_step(user_id, command, &market)
                .await?
            {
                AdvanceCommandStepResult::Advanced { save, receipt } => {
                    if save.market_world_id != world.id || save.game_day != market.game_day {
                        bail!("committed save does not match the selected market day");
                    }
                    return Ok(DailyCommandAdvanceResult::Advanced {
                        state: Box::new(CommittedGameState {
                            save: *save,
                            world: world.world,
                            market,
                        }),
                        receipt,
                    });
                }
                AdvanceCommandStepResult::Replayed { save, receipt } => {
                    return Ok(DailyCommandAdvanceResult::Replayed {
                        state: Box::new(self.assemble(*save).await?),
                        receipt,
                    });
                }
                AdvanceCommandStepResult::Rejected(rejection) => {
                    return Ok(DailyCommandAdvanceResult::Rejected(rejection));
                }
                AdvanceCommandStepResult::Stale(_) => continue,
            }
        }

        bail!("manual command cursor kept changing while advancing one day")
    }
}

impl DefaultDailyPipeline {
    async fn market_for_settlement(
        &self,
        world_id: u64,
        world: &MarketWorld,
        target_day: u32,
    ) -> Result<MarketDay> {
        if world.index_product.is_some() {
            let lookahead_day = settlement_lookahead_day(target_day);
            let lookahead = self.markets.ensure_day(world_id, lookahead_day).await?;
            if lookahead_day == target_day {
                return Ok(lookahead);
            }
        }

        self.markets.ensure_day(world_id, target_day).await
    }

    async fn assemble(&self, save: SaveState) -> Result<CommittedGameState> {
        let world = self.markets.load_world(save.market_world_id).await?;
        let market = self
            .markets
            .ensure_day(save.market_world_id, save.game_day)
            .await?;
        if world.id != save.market_world_id || market.game_day != save.game_day {
            bail!("save and market state are inconsistent");
        }

        Ok(CommittedGameState {
            save,
            world: world.world,
            market,
        })
    }
}

fn settlement_lookahead_day(target_day: u32) -> u32 {
    target_day.saturating_add(MARKET_SETTLEMENT_LOOKAHEAD_DAYS)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};

    use time::Duration;

    use super::*;
    use crate::character::{
        Character, CharacterDraft, Education, FamilyBackground, Gender, Health, MilitaryStatus,
        Region, create_character,
    };
    use crate::finance::{
        CommandCursor, CommandId, FinancialAccount, FinancialAccountStatus, FinancialAccountType,
        FinancialIncomeYear, PolicySet, PolicySetAssignment, ResourceId, RunId,
    };
    use crate::market::{
        IndexProductTerms, MarketDay, MarketRegime, default_market_calibration,
        default_market_world,
    };
    use crate::store::{
        ActiveMarketWorld, ActiveRunConfiguration, AdvanceCommandReceipt, CareerCatalogAssignment,
        GameCommandCursor, GameCommandRejection, MarketWorldState, StartGameReceipt,
    };

    const USER_ID: u64 = 7;
    const SAVE_ID: u64 = 11;
    const WORLD_ID: u64 = 13;
    const POLICY_SET_ID: u64 = 17;
    const DEFAULT_ACCOUNT_ID: u64 = 19;
    const NEXT_DEFAULT_ACCOUNT_ID: u64 = 23;
    const RUN_REVISION: u32 = 3;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum StoreCall {
        ActiveRunConfigurationLoaded(ActiveRunConfiguration),
        SaveLoaded(SaveCursor),
        WorldLoaded(u64),
        MarketDayEnsured {
            world_id: u64,
            target_game_day: u32,
        },
        SaveStarted(ActiveRunConfiguration),
        SaveAdvanced {
            expected: SaveCursor,
            market: MarketDay,
        },
        SaveCommandAdvanced {
            market: MarketDay,
        },
    }

    type SharedCalls = Arc<Mutex<Vec<StoreCall>>>;

    struct FakeSaveStore {
        state: Mutex<SaveState>,
        calls: SharedCalls,
        stale_once: AtomicBool,
        stale_start_once: AtomicBool,
        active_run_configuration: Mutex<ActiveRunConfiguration>,
    }

    impl FakeSaveStore {
        fn new(state: SaveState, calls: SharedCalls, stale_once: bool) -> Self {
            Self {
                state: Mutex::new(state),
                calls,
                stale_once: AtomicBool::new(stale_once),
                stale_start_once: AtomicBool::new(false),
                active_run_configuration: Mutex::new(given_active_run_configuration(1)),
            }
        }

        fn state(&self) -> SaveState {
            lock(&self.state).clone()
        }

        fn change_active_world_twice_on_next_start(&self) {
            self.stale_start_once.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl SaveStore for FakeSaveStore {
        async fn load(&self, _user_id: u64) -> Result<SaveState> {
            let state = self.state();
            lock(&self.calls).push(StoreCall::SaveLoaded(SaveCursor::from(&state)));

            Ok(state)
        }

        async fn active_run_configuration(&self) -> Result<ActiveRunConfiguration> {
            let active = *lock(&self.active_run_configuration);
            lock(&self.calls).push(StoreCall::ActiveRunConfigurationLoaded(active));

            Ok(active)
        }

        async fn start_game(
            &self,
            _user_id: u64,
            command: &StartGameCommand,
            expected: ActiveRunConfiguration,
        ) -> Result<StartGameResult> {
            lock(&self.calls).push(StoreCall::SaveStarted(expected));
            if self.stale_start_once.swap(false, Ordering::SeqCst) {
                let mut active = lock(&self.active_run_configuration);
                active.market_world.assignment_revision += 2;
                return Ok(StartGameResult::ActiveWorldChanged);
            }
            if *lock(&self.active_run_configuration) != expected {
                return Ok(StartGameResult::ActiveWorldChanged);
            }

            let character = match create_character(command.draft.clone()) {
                Ok(character) => character,
                Err(errors) => {
                    return Ok(StartGameResult::Rejected(
                        GameCommandRejection::InvalidCharacter(errors),
                    ));
                }
            };

            let mut state = lock(&self.state);
            state.market_world_id = expected.market_world.world_id;
            state.policy_set.id = expected.policy_set.policy_set_id;
            state.run_revision += 1;
            state.state_revision = 0;
            state.game_day = 0;
            state.cash_krw = character.cash_krw;
            state.debt_krw = character.debt_krw;
            state.accounts = vec![given_default_account(
                state.run_revision,
                NEXT_DEFAULT_ACCOUNT_ID,
            )];
            state.positions.clear();
            state.pending_settlements.clear();
            state.character = Some(character);

            let committed_cursor = GameCommandCursor::from(&*state);
            Ok(StartGameResult::Applied {
                save: Box::new(state.clone()),
                receipt: StartGameReceipt {
                    command_id: command.command_id.clone(),
                    committed_cursor,
                    replayed: false,
                },
            })
        }

        async fn advance_one_day(
            &self,
            _user_id: u64,
            expected: SaveCursor,
            market: &MarketDay,
        ) -> Result<AdvanceDayResult> {
            lock(&self.calls).push(StoreCall::SaveAdvanced {
                expected,
                market: market.clone(),
            });
            let mut state = lock(&self.state);

            if self.stale_once.swap(false, Ordering::SeqCst) {
                state.game_day = state
                    .game_day
                    .checked_add(1)
                    .expect("경쟁 커밋의 게임 날짜는 증가할 수 있어야 한다");
                state.state_revision = state
                    .state_revision
                    .checked_add(1)
                    .expect("경쟁 커밋의 상태 revision은 증가할 수 있어야 한다");
                return Ok(AdvanceDayResult::Stale(state.clone()));
            }
            if SaveCursor::from(&*state) != expected {
                return Ok(AdvanceDayResult::Stale(state.clone()));
            }

            let target_game_day = expected
                .game_day
                .checked_add(1)
                .expect("테스트 게임 날짜는 증가할 수 있어야 한다");
            if market.game_day != target_game_day {
                anyhow::bail!("daily market input does not match the target game day");
            }

            state.game_day = market.game_day;
            state.state_revision = state
                .state_revision
                .checked_add(1)
                .expect("테스트 상태 revision은 증가할 수 있어야 한다");
            Ok(AdvanceDayResult::Advanced(state.clone()))
        }

        async fn advance_command_step(
            &self,
            _user_id: u64,
            command: &ManualAdvanceCommand,
            market: &MarketDay,
        ) -> Result<AdvanceCommandStepResult> {
            lock(&self.calls).push(StoreCall::SaveCommandAdvanced {
                market: market.clone(),
            });
            let mut state = lock(&self.state);
            let initial = GameCommandCursor::from(command.cursor);
            if state.character.is_none() {
                return Ok(AdvanceCommandStepResult::Rejected(
                    GameCommandRejection::CharacterRequired,
                ));
            }
            if state.run_revision != initial.run_revision
                || state.state_revision < initial.state_revision
                || state.game_day < initial.game_day
            {
                return Ok(AdvanceCommandStepResult::Rejected(
                    GameCommandRejection::Busy,
                ));
            }
            let completed = state.game_day - initial.game_day;
            if completed >= command.days {
                return Ok(AdvanceCommandStepResult::Replayed {
                    save: Box::new(state.clone()),
                    receipt: AdvanceCommandReceipt {
                        command_id: command.command_id.clone(),
                        requested_days: command.days,
                        initial_cursor: initial,
                        committed_cursor: GameCommandCursor::from(&*state),
                        replayed: true,
                    },
                });
            }
            if market.game_day != state.game_day + 1 {
                return Ok(AdvanceCommandStepResult::Stale(Box::new(state.clone())));
            }

            state.game_day += 1;
            state.state_revision += 1;
            let committed_cursor = GameCommandCursor::from(&*state);
            let receipt = (completed + 1 == command.days).then(|| AdvanceCommandReceipt {
                command_id: command.command_id.clone(),
                requested_days: command.days,
                initial_cursor: initial,
                committed_cursor,
                replayed: false,
            });

            Ok(AdvanceCommandStepResult::Advanced {
                save: Box::new(state.clone()),
                receipt,
            })
        }
    }

    struct FakeMarketStore {
        world: MarketWorldState,
        calls: SharedCalls,
        failure_day: Option<u32>,
        cached_days: Mutex<Vec<u32>>,
    }

    impl FakeMarketStore {
        fn new(
            world: MarketWorldState,
            calls: SharedCalls,
            failure_day: Option<u32>,
            cached_days: Vec<u32>,
        ) -> Self {
            Self {
                world,
                calls,
                failure_day,
                cached_days: Mutex::new(cached_days),
            }
        }

        fn cached_days(&self) -> Vec<u32> {
            lock(&self.cached_days).clone()
        }
    }

    #[async_trait]
    impl MarketStore for FakeMarketStore {
        async fn load_world(&self, world_id: u64) -> Result<MarketWorldState> {
            lock(&self.calls).push(StoreCall::WorldLoaded(world_id));

            Ok(self.world.clone())
        }

        async fn ensure_day(&self, world_id: u64, target_game_day: u32) -> Result<MarketDay> {
            lock(&self.calls).push(StoreCall::MarketDayEnsured {
                world_id,
                target_game_day,
            });
            if self.failure_day == Some(target_game_day) {
                anyhow::bail!("injected market generation failure");
            }

            let mut cached_days = lock(&self.cached_days);
            let next_day = cached_days.last().copied().map_or(0, |day| day + 1);
            if next_day <= target_game_day {
                cached_days.extend(next_day..=target_game_day);
            }

            Ok(given_market_day(target_game_day))
        }

        async fn history_for_user(
            &self,
            _user_id: u64,
            _limit: u32,
        ) -> Result<crate::store::MarketHistoryState> {
            anyhow::bail!("not used by daily pipeline tests")
        }
    }

    struct FakeRecruitmentPostingStore;

    #[async_trait]
    impl RecruitmentPostingStore for FakeRecruitmentPostingStore {
        async fn ensure_postings_for_user(
            &self,
            _user_id: u64,
            _target_game_day: u32,
        ) -> Result<()> {
            Ok(())
        }
    }

    struct TestFixture {
        pipeline: Arc<dyn DailyPipeline>,
        saves: Arc<FakeSaveStore>,
        markets: Arc<FakeMarketStore>,
        calls: SharedCalls,
    }

    impl TestFixture {
        fn calls(&self) -> Vec<StoreCall> {
            lock(&self.calls).clone()
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn given_character() -> Character {
        Character {
            name: "테스터".to_owned(),
            age: 25,
            gender: Gender::Other,
            military: MilitaryStatus::Exempted,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 1,
            cash_krw: 10_000_000,
            debt_krw: 0,
            health: Health::Normal,
            dependents: 0,
        }
    }

    fn given_character_draft() -> CharacterDraft {
        CharacterDraft {
            name: "테스터".to_owned(),
            age: 25,
            gender: Gender::Other,
            military: MilitaryStatus::Exempted,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 1,
            starting_cash_krw: 10_000_000,
            student_loan_krw: 0,
            credit_loan_krw: 0,
            health: Health::Normal,
            dependents: 0,
        }
    }

    fn given_start_game_command() -> StartGameCommand {
        StartGameCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: RUN_REVISION,
                expected_state_revision: 9,
                expected_game_day: 9,
            },
            draft: given_character_draft(),
        }
    }

    fn given_advance_command(game_day: u32) -> ManualAdvanceCommand {
        ManualAdvanceCommand {
            command_id: CommandId::parse("b6a1cc9d-3c87-44a9-aebe-9ff46677f043")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: RUN_REVISION,
                expected_state_revision: u64::from(game_day),
                expected_game_day: game_day,
            },
            days: 1,
        }
    }

    fn given_save(game_day: u32) -> SaveState {
        SaveState {
            save_id: SAVE_ID,
            market_world_id: WORLD_ID,
            policy_set: given_policy_set(),
            run_revision: RUN_REVISION,
            state_revision: u64::from(game_day),
            game_day,
            cash_krw: 10_000_000,
            debt_krw: 0,
            accounts: vec![given_default_account(RUN_REVISION, DEFAULT_ACCOUNT_ID)],
            positions: Vec::new(),
            pending_settlements: Vec::new(),
            cma_accounts: Vec::new(),
            cash_contracts: Vec::new(),
            deposit_protection: Vec::new(),
            current_financial_income_year: FinancialIncomeYear::zero(2026),
            current_annual_tax_year: crate::store::AnnualTaxYearState::empty_not_applicable(2026),
            latest_financial_income_assessment: None,
            m2d_assets: crate::finance::M2dAssetSnapshot::default(),
            isa_accounts: Vec::new(),
            pension_accounts: Vec::new(),
            career: crate::store::CareerSnapshotState::empty("softwareEngineering".to_owned()),
            character: Some(given_character()),
        }
    }

    fn given_cursor(game_day: u32) -> SaveCursor {
        SaveCursor {
            market_world_id: WORLD_ID,
            policy_set_id: POLICY_SET_ID,
            run_revision: RUN_REVISION,
            state_revision: u64::from(game_day),
            game_day,
        }
    }

    fn given_active_run_configuration(assignment_revision: u64) -> ActiveRunConfiguration {
        ActiveRunConfiguration {
            market_world: ActiveMarketWorld {
                world_id: WORLD_ID,
                assignment_revision,
            },
            policy_set: PolicySetAssignment {
                policy_set_id: ResourceId::from_u64(POLICY_SET_ID),
                assignment_revision: 1,
            },
            product_bundle_id: None,
            career_catalog: CareerCatalogAssignment {
                bundle_id: ResourceId::from_u64(1),
                assignment_revision: 1,
            },
        }
    }

    fn given_policy_set() -> PolicySet {
        PolicySet {
            id: ResourceId::from_u64(POLICY_SET_ID),
            key: "2026-v1".to_owned(),
            basis_date: "2026-07-26".to_owned(),
            sealed: true,
        }
    }

    fn given_default_account(run_revision: u32, account_id: u64) -> FinancialAccount {
        FinancialAccount {
            id: ResourceId::from_u64(account_id),
            run: RunId {
                save_id: ResourceId::from_u64(SAVE_ID),
                run_revision,
            },
            account_type: FinancialAccountType::TaxableBrokerage,
            status: FinancialAccountStatus::Open,
            is_default: true,
            cash_krw: 0,
        }
    }

    fn given_world_state() -> MarketWorldState {
        MarketWorldState {
            id: WORLD_ID,
            world: default_market_world().expect("기본 시장 세계는 유효해야 한다"),
            calibration: default_market_calibration(),
        }
    }

    fn given_v4_world_state() -> MarketWorldState {
        let mut state = given_world_state();
        state.world.index_product = Some(IndexProductTerms {
            product_version_id: 1,
            product_key: "llx-domestic-equity-2026-v1".to_owned(),
            day0_close_krw: 100_000,
            annual_management_fee_ppm: 1_500,
            annual_distribution_rate_ppm: 20_000,
            day_count_denominator: 365,
            buy_fee_ppm: 0,
            sell_fee_ppm: 0,
            transaction_tax_ppm: 0,
        });
        state
    }

    fn given_market_day(game_day: u32) -> MarketDay {
        let world = default_market_world().expect("기본 시장 세계는 유효해야 한다");
        let market_date = world
            .start_date
            .checked_add(Duration::days(i64::from(game_day)))
            .expect("테스트 시장 날짜는 범위 안이어야 한다");

        MarketDay {
            game_day,
            market_date,
            market_open: true,
            session_index: game_day,
            regime: MarketRegime::Expansion,
            equity_close_krw: world.day0_equity_close_krw + i64::from(game_day),
            equity_return_ppm: 0,
            equity_variance_ppm2: 144_000_000,
            equity_residual_ppm: 0,
            rates: None,
            m2: None,
        }
    }

    fn given_pipeline(
        save: SaveState,
        failure_day: Option<u32>,
        stale_once: bool,
        cached_days: Vec<u32>,
    ) -> TestFixture {
        given_pipeline_for_world(
            given_world_state(),
            save,
            failure_day,
            stale_once,
            cached_days,
        )
    }

    fn given_v4_pipeline(
        save: SaveState,
        failure_day: Option<u32>,
        stale_once: bool,
        cached_days: Vec<u32>,
    ) -> TestFixture {
        given_pipeline_for_world(
            given_v4_world_state(),
            save,
            failure_day,
            stale_once,
            cached_days,
        )
    }

    fn given_pipeline_for_world(
        world: MarketWorldState,
        save: SaveState,
        failure_day: Option<u32>,
        stale_once: bool,
        cached_days: Vec<u32>,
    ) -> TestFixture {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let saves = Arc::new(FakeSaveStore::new(save, Arc::clone(&calls), stale_once));
        let markets = Arc::new(FakeMarketStore::new(
            world,
            Arc::clone(&calls),
            failure_day,
            cached_days,
        ));
        let pipeline = create_daily_pipeline(
            saves.clone(),
            markets.clone(),
            Arc::new(FakeRecruitmentPostingStore),
        );

        TestFixture {
            pipeline,
            saves,
            markets,
            calls,
        }
    }

    async fn when_advancing_one_day(fixture: &TestFixture) -> Result<DailyAdvanceResult> {
        fixture.pipeline.advance_one_day(USER_ID).await
    }

    async fn when_advancing_command_step(
        fixture: &TestFixture,
        game_day: u32,
    ) -> Result<DailyCommandAdvanceResult> {
        fixture
            .pipeline
            .advance_command_step(USER_ID, &given_advance_command(game_day))
            .await
    }

    async fn when_loading(fixture: &TestFixture) -> Result<CommittedGameState> {
        fixture.pipeline.load(USER_ID).await
    }

    async fn when_starting_game(fixture: &TestFixture) -> Result<CommittedGameState> {
        match fixture
            .pipeline
            .start_game(USER_ID, &given_start_game_command())
            .await?
        {
            DailyStartGameResult::Applied { state, .. }
            | DailyStartGameResult::Replayed { state, .. } => Ok(*state),
            DailyStartGameResult::Rejected(rejection) => {
                anyhow::bail!("start game was rejected: {rejection:?}")
            }
        }
    }

    mod context_a_new_run_is_prepared {
        use super::*;

        #[tokio::test]
        async fn given_day_zero_generation_fails_when_starting_then_the_existing_run_is_untouched()
        {
            let initial = given_save(9);
            let fixture = given_pipeline(initial.clone(), Some(0), false, vec![]);

            let error = when_starting_game(&fixture)
                .await
                .expect_err("day 0 준비 실패는 새 런 커밋 전에 반환되어야 한다");

            assert_eq!(error.to_string(), "injected market generation failure");
            assert_eq!(fixture.saves.state(), initial);
            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::ActiveRunConfigurationLoaded(given_active_run_configuration(1)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 0,
                    },
                ]
            );
        }

        #[tokio::test]
        async fn given_active_world_changes_a_to_b_to_a_when_committing_then_preparation_retries_and_run_increments_once()
         {
            let fixture = given_pipeline(given_save(9), None, false, vec![]);
            fixture.saves.change_active_world_twice_on_next_start();

            let committed = when_starting_game(&fixture)
                .await
                .expect("바뀐 활성 월드 세대를 다시 준비해 시작해야 한다");

            assert_eq!(committed.save.run_revision, RUN_REVISION + 1);
            assert_eq!(fixture.saves.state().run_revision, RUN_REVISION + 1);
            assert_eq!(committed.save.state_revision, 0);
            assert_eq!(committed.save.game_day, 0);
            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::ActiveRunConfigurationLoaded(given_active_run_configuration(1)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 0,
                    },
                    StoreCall::SaveStarted(given_active_run_configuration(1)),
                    StoreCall::ActiveRunConfigurationLoaded(given_active_run_configuration(3)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 0,
                    },
                    StoreCall::SaveStarted(given_active_run_configuration(3)),
                ]
            );
        }
    }

    mod context_a_daily_advance_is_committed {
        use super::*;

        #[tokio::test]
        async fn given_next_market_day_when_advancing_then_market_is_ensured_before_save_commit() {
            let fixture = given_pipeline(given_save(2), None, false, vec![0, 1, 2]);

            when_advancing_one_day(&fixture)
                .await
                .expect("시장일 준비 뒤 저장이 커밋되어야 한다");

            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(2)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 3,
                    },
                    StoreCall::SaveAdvanced {
                        expected: given_cursor(2),
                        market: given_market_day(3),
                    },
                ]
            );
        }

        #[tokio::test]
        async fn given_legacy_world_when_advancing_command_then_only_target_day_is_ensured() {
            let fixture = given_pipeline(given_save(2), None, false, vec![0, 1, 2]);

            when_advancing_command_step(&fixture, 2)
                .await
                .expect("보존 월드는 기존 시장일 준비 방식으로 진행되어야 한다");

            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(2)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 3,
                    },
                    StoreCall::SaveCommandAdvanced {
                        market: given_market_day(3),
                    },
                ]
            );
        }

        #[tokio::test]
        async fn given_v4_world_when_advancing_then_lookahead_is_ensured_before_save_commit() {
            let fixture = given_v4_pipeline(given_save(2), None, false, vec![0, 1, 2]);

            when_advancing_one_day(&fixture)
                .await
                .expect("미래 시장일 준비 뒤 저장이 커밋되어야 한다");

            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(2)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 17,
                    },
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 3,
                    },
                    StoreCall::SaveAdvanced {
                        expected: given_cursor(2),
                        market: given_market_day(3),
                    },
                ]
            );
        }

        #[tokio::test]
        async fn given_v4_world_when_advancing_command_then_lookahead_is_ensured_before_save_commit()
         {
            let fixture = given_v4_pipeline(given_save(2), None, false, vec![0, 1, 2]);

            when_advancing_command_step(&fixture, 2)
                .await
                .expect("미래 시장일 준비 뒤 수동 진행이 커밋되어야 한다");

            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(2)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 17,
                    },
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 3,
                    },
                    StoreCall::SaveCommandAdvanced {
                        market: given_market_day(3),
                    },
                ]
            );
        }

        #[test]
        fn given_target_near_maximum_when_calculating_lookahead_then_day_saturates() {
            let target_day = u32::MAX - 1;

            let lookahead_day = settlement_lookahead_day(target_day);

            assert_eq!(lookahead_day, u32::MAX);
        }
    }

    mod context_market_generation_fails {
        use super::*;

        #[tokio::test]
        async fn given_generation_error_when_advancing_then_save_cursor_and_commit_stay_untouched()
        {
            let initial = given_save(3);
            let fixture = given_pipeline(initial.clone(), Some(4), false, vec![0, 1, 2, 3]);

            let error = when_advancing_one_day(&fixture)
                .await
                .expect_err("시장 생성 실패는 진행 실패로 반환되어야 한다");

            assert_eq!(error.to_string(), "injected market generation failure");
            assert_eq!(fixture.saves.state(), initial);
            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(3)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 4,
                    },
                ]
            );
        }
    }

    mod context_the_save_cursor_becomes_stale {
        use super::*;

        #[tokio::test]
        async fn given_one_competing_commit_when_advancing_then_retry_converges_from_newest_day() {
            let fixture = given_pipeline(given_save(4), None, true, vec![0, 1, 2, 3, 4]);

            let result = when_advancing_one_day(&fixture)
                .await
                .expect("최신 저장 날짜에서 재시도해 성공해야 한다");

            let DailyAdvanceResult::Advanced(committed) = result else {
                panic!("캐릭터가 있는 저장은 진행되어야 한다");
            };
            assert_eq!(committed.save.game_day, 6);
            assert_eq!(fixture.saves.state().game_day, 6);
            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(4)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 5,
                    },
                    StoreCall::SaveAdvanced {
                        expected: given_cursor(4),
                        market: given_market_day(5),
                    },
                    StoreCall::SaveLoaded(given_cursor(5)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 6,
                    },
                    StoreCall::SaveAdvanced {
                        expected: given_cursor(5),
                        market: given_market_day(6),
                    },
                ]
            );
        }
    }

    mod context_the_current_market_day_is_not_cached {
        use super::*;

        #[tokio::test]
        async fn given_partial_market_cache_when_loading_then_current_day_is_backfilled_and_assembled()
         {
            let save = given_save(4);
            let fixture = given_pipeline(save.clone(), None, false, vec![0, 1]);

            let committed = when_loading(&fixture)
                .await
                .expect("현재 시장일까지 보충해 저장과 조립해야 한다");

            assert_eq!(committed.save, save);
            assert_eq!(committed.world, given_world_state().world);
            assert_eq!(committed.market, given_market_day(4));
            assert_eq!(fixture.markets.cached_days(), vec![0, 1, 2, 3, 4]);
            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(4)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 4,
                    },
                ]
            );
        }
    }

    mod context_v4_초기_세이브가_시장_캐시보다_먼저_생긴_경우 {
        use super::*;

        #[tokio::test]
        async fn given_캐릭터없는_v4_세이브_when_불러오면_then_day0_준비후_완전한_상태를_다시_읽는다()
         {
            let mut save = given_save(0);
            save.character = None;
            let fixture = given_v4_pipeline(save, None, false, vec![]);

            when_loading(&fixture)
                .await
                .expect("day 0 시장을 준비한 뒤 초기 상태를 다시 읽어야 한다");

            assert_eq!(
                fixture.calls(),
                vec![
                    StoreCall::SaveLoaded(given_cursor(0)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 0,
                    },
                    StoreCall::SaveLoaded(given_cursor(0)),
                    StoreCall::WorldLoaded(WORLD_ID),
                    StoreCall::MarketDayEnsured {
                        world_id: WORLD_ID,
                        target_game_day: 0,
                    },
                ]
            );
        }
    }
}
