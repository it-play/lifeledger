import {
  type CharacterDraft,
  CharacterDraftSchema,
  type CharacterPresetVersion,
  type LoanProductCatalog,
  type PointBudgetEvaluation,
  type PointSelection,
  type RunMode,
  type RunOptions,
  type RunStartDraft,
  type SeasonLeagues,
} from '../../api/contracts.js';
import {
  CharacterRejectedError,
  type GameApi,
  GameCommandError,
  RunRequestError,
} from '../../api/game-api.js';
import type { LoanApi } from '../../api/loan-api.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { el } from '../../lib/dom/index.js';
import { type FieldSpec, type FormHandle, renderForm } from '../../lib/form/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import {
  type CharacterStartDraftBuilder,
  createCharacterStartDraftBuilder,
} from '../character-start/index.js';
import {
  createRunStartRetryPolicy,
  type RunStartRetryPolicy,
} from '../game-command-retry/index.js';
import type { GameStateWriter } from '../game-state/index.js';
import type { AppState } from '../state.js';

export interface CharacterCreateDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly loanApi: LoanApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface RunSubmitDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly retries: RunStartRetryPolicy;
  readonly currentForm: () => FormHandle | undefined;
  readonly navigate: (to: string) => void;
}

interface SandboxSubmitDeps extends RunSubmitDeps {
  readonly startDrafts: CharacterStartDraftBuilder;
  readonly currentLoanCatalog: () => LoanProductCatalog | undefined;
  readonly currentRunOptions: () => RunOptions | undefined;
}

const FIELDS: readonly FieldSpec[] = [
  { name: 'name', label: '이름', kind: 'text' },
  { name: 'age', label: '나이', kind: 'number', help: '19 ~ 50' },
  {
    name: 'gender',
    label: '성별',
    kind: 'select',
    options: [
      { value: 'male', label: '남' },
      { value: 'female', label: '여' },
      { value: 'other', label: '기타' },
    ],
  },
  {
    name: 'military',
    label: '병역',
    kind: 'select',
    help: '특례복무는 자격증 또는 석사 이상이 필요합니다',
    options: [
      { value: 'notServed', label: '미필' },
      { value: 'serving', label: '복무중' },
      { value: 'completed', label: '필' },
      { value: 'exempted', label: '면제' },
      { value: 'alternative', label: '특례복무' },
    ],
  },
  {
    name: 'education',
    label: '학력',
    kind: 'select',
    options: [
      { value: 'highSchool', label: '고졸' },
      { value: 'associate', label: '전문대' },
      { value: 'bachelor', label: '학사' },
      { value: 'master', label: '석사' },
      { value: 'doctorate', label: '박사' },
    ],
  },
  {
    name: 'region',
    label: '출신 지역',
    kind: 'select',
    options: [
      { value: 'capitalArea', label: '수도권' },
      { value: 'metropolitan', label: '광역시' },
      { value: 'smallCity', label: '중소도시' },
      { value: 'rural', label: '군' },
    ],
  },
  {
    name: 'background',
    label: '가정 배경',
    kind: 'select',
    options: [
      { value: 'supportive', label: '지원형' },
      { value: 'independent', label: '독립형' },
      { value: 'dependent', label: '부양형' },
    ],
  },
  { name: 'careerYears', label: '경력 (년)', kind: 'number' },
  { name: 'certifications', label: '보유 자격증 수', kind: 'number' },
  { name: 'startingCashKrw', label: '시작 자금 (원)', kind: 'number' },
  { name: 'studentLoanKrw', label: '학자금 부채 (원)', kind: 'number' },
  { name: 'creditLoanKrw', label: '신용 부채 (원)', kind: 'number' },
  {
    name: 'health',
    label: '건강',
    kind: 'select',
    options: [
      { value: 'good', label: '상' },
      { value: 'normal', label: '중' },
      { value: 'poor', label: '하' },
    ],
  },
  { name: 'dependents', label: '부양가족 수', kind: 'number' },
];

const validator = asFormValidator(CharacterDraftSchema);

/** The M5-A run creation screen. Server-side validation remains authoritative. */
export function createCharacterCreateView(deps: CharacterCreateDeps): ViewFactory {
  const retries = createRunStartRetryPolicy({ createCommandId: deps.createCommandId });
  const startDrafts = createCharacterStartDraftBuilder();

  return (): View => {
    let form: FormHandle | undefined;
    let loanCatalog: LoanProductCatalog | undefined;

    return {
      mount(host, ctx) {
        const { store, snapshots, api, toasts } = deps;
        const h = createHooks(ctx.bag);
        const mode = h.useSignal<RunMode>('sandbox');
        const runOptions = h.useSignal<RunOptions | undefined>(undefined);
        const seasonLeagues = h.useSignal<SeasonLeagues | undefined>(undefined);
        const catalogFailed = h.useSignal(false);
        const seasonFailed = h.useSignal(false);
        const selectedPresetId = h.useSignal('');
        const selectedBudgetId = h.useSignal('');
        const rankedBusy = h.useSignal(false);
        const previewBusy = h.useSignal(false);
        const pointInputs = new Map<string, ReadonlyMap<string, HTMLInputElement>>();

        const rankedPresetOption = el('option', { value: 'rankedPreset' }, '랭크 프리셋');
        const rankedCustomOption = el('option', { value: 'rankedCustom' }, '랭크 커스텀');
        const sandboxOption = el('option', { value: 'sandbox' }, '샌드박스');
        const modeSelect = el(
          'select',
          { name: 'runMode', attrs: { 'aria-label': '실행 모드' } },
          rankedPresetOption,
          rankedCustomOption,
          sandboxOption,
        );
        modeSelect.value = 'sandbox';
        const modeStatus = el('p', {}, '실행 설정을 불러오는 중…');
        const seasonStatus = el('p', {}, '시즌 정보를 불러오는 중…');
        const leagueList = el('ul');
        const seasonSection = el(
          'section',
          {},
          el('h2', {}, '시즌과 리그'),
          seasonStatus,
          leagueList,
        );

        const presetSelect = el(
          'select',
          { name: 'characterPresetVersionId', attrs: { 'aria-label': '랭크 프리셋' } },
          el('option', { value: '' }, '프리셋을 선택하세요'),
        );
        const presetStartButton = el('button', { type: 'button' }, '이 프리셋으로 시작');
        const rankedPresetSection = el(
          'section',
          {},
          el('h2', {}, '랭크 프리셋'),
          el('p', {}, '게시된 조건을 수정하지 않고 그대로 시작한다.'),
          presetSelect,
          presetStartButton,
        );

        const budgetSelect = el(
          'select',
          { name: 'pointBudgetVersionId', attrs: { 'aria-label': '포인트 예산' } },
          el('option', { value: '' }, '포인트 예산을 선택하세요'),
        );
        const pointFields = el('div');
        const previewButton = el('button', { type: 'button' }, '포인트 계산');
        const customStartButton = el('button', { type: 'button' }, '이 선택으로 시작');
        const previewOutput = el('p', {}, '수량을 고른 뒤 포인트를 계산하세요.');
        const rankedCustomSection = el(
          'section',
          {},
          el('h2', {}, '랭크 커스텀'),
          el('p', {}, '서버가 선택 비용과 조합 규칙을 계산한다.'),
          budgetSelect,
          pointFields,
          previewButton,
          customStartButton,
          previewOutput,
        );

        const sandboxPresets = el('div', { class: 'presets' }, '프리셋 불러오는 중…');
        const loanBar = el('p', {}, '시작 대출 상품 불러오는 중…');
        const sandboxSection = el(
          'section',
          {},
          el('h2', {}, '샌드박스'),
          el('p', {}, '시작 조건을 직접 정한다. 이 실행은 랭킹에서 제외된다.'),
          sandboxPresets,
          loanBar,
        );

        const container = el(
          'section',
          { class: 'character-create' },
          el('h1', {}, '새 실행 만들기'),
          el('label', {}, '실행 모드 ', modeSelect),
          modeStatus,
          seasonSection,
          rankedPresetSection,
          rankedCustomSection,
          sandboxSection,
        );
        host.replaceChildren(container);

        form = renderForm(
          { fields: FIELDS, validator, submitLabel: '이 조건으로 시작' },
          {
            initial: {
              name: '',
              age: 25,
              gender: 'male',
              military: 'completed',
              education: 'bachelor',
              region: 'capitalArea',
              background: 'independent',
              careerYears: 1,
              certifications: 1,
              startingCashKrw: 10_000_000,
              studentLoanKrw: 20_000_000,
              creditLoanKrw: 0,
              health: 'normal',
              dependents: 0,
            },
            onSubmit: (draft) =>
              submitSandbox(
                {
                  store,
                  snapshots,
                  api,
                  toasts,
                  retries,
                  startDrafts,
                  currentForm: () => form,
                  currentLoanCatalog: () => loanCatalog,
                  currentRunOptions: () => runOptions.peek(),
                  navigate: ctx.navigate,
                },
                draft,
              ),
          },
        );
        ctx.bag.add(form);
        sandboxSection.appendChild(form.element);

        h.useEventListener(modeSelect, 'change', () => mode.set(readRunMode(modeSelect.value)));
        h.useEventListener(presetSelect, 'change', () => selectedPresetId.set(presetSelect.value));
        h.useEventListener(budgetSelect, 'change', () => {
          selectedBudgetId.set(budgetSelect.value);
          previewOutput.replaceChildren('수량을 고른 뒤 포인트를 계산하세요.');
        });

        h.bindAttribute(rankedPresetSection, 'hidden', () => mode.get() !== 'rankedPreset');
        h.bindAttribute(rankedCustomSection, 'hidden', () => mode.get() !== 'rankedCustom');
        h.bindAttribute(sandboxSection, 'hidden', () => mode.get() !== 'sandbox');
        h.bindAttribute(rankedPresetOption, 'disabled', () => {
          const options = runOptions.get();
          return options !== undefined && !options.modes.includes('rankedPreset');
        });
        h.bindAttribute(rankedCustomOption, 'disabled', () => {
          const options = runOptions.get();
          return options !== undefined && !options.modes.includes('rankedCustom');
        });
        h.bindAttribute(
          sandboxOption,
          'disabled',
          () => runOptions.get()?.sandboxAvailable === false,
        );
        h.bindAttribute(presetStartButton, 'disabled', () => {
          const options = runOptions.get();
          return (
            rankedBusy.get() ||
            options === undefined ||
            !options.modes.includes('rankedPreset') ||
            options.activeSeasonId === null ||
            selectedPresetId.get() === ''
          );
        });
        h.bindAttribute(customStartButton, 'disabled', () => {
          const options = runOptions.get();
          return (
            rankedBusy.get() ||
            options === undefined ||
            !options.modes.includes('rankedCustom') ||
            options.activeSeasonId === null ||
            selectedBudgetId.get() === ''
          );
        });
        h.bindAttribute(previewButton, 'disabled', () => {
          return previewBusy.get() || selectedBudgetId.get() === '';
        });
        h.bindText(modeStatus, () => {
          return catalogFailed.get()
            ? '실행 설정을 불러오지 못했습니다.'
            : runModeStatus(mode.get(), runOptions.get());
        });
        h.bindText(seasonStatus, () => {
          if (seasonFailed.get()) return '시즌·리그 정보를 불러오지 못했습니다.';
          const catalog = seasonLeagues.get();
          if (catalog !== undefined) return formatSeasonStatus(catalog);
          return runOptions.get()?.activeSeasonId === null
            ? '현재 등록 가능한 랭크 시즌이 없습니다.'
            : '시즌 정보를 불러오는 중…';
        });

        h.useEventListener(presetStartButton, 'click', () => {
          const options = runOptions.peek();
          const presetId = selectedPresetId.peek();
          if (!rankedCanStart(options, 'rankedPreset') || presetId === '') return;
          void withBusy(rankedBusy, async () => {
            await submitRun(
              {
                store,
                snapshots,
                api,
                toasts,
                retries,
                currentForm: () => form,
                navigate: ctx.navigate,
              },
              { mode: 'rankedPreset', characterPresetVersionId: presetId },
            );
          });
        });

        h.useEventListener(previewButton, 'click', () => {
          const budgetId = selectedBudgetId.peek();
          const inputs = pointInputs.get(budgetId);
          const selections = inputs === undefined ? undefined : readPointSelections(inputs);
          if (selections === undefined) {
            toasts.show('포인트 수량은 0 이상 정수로 입력해 주세요.', { tone: 'error' });
            return;
          }
          void withBusy(previewBusy, async () => {
            try {
              const evaluation = await api.previewPointBudget({
                pointBudgetVersionId: budgetId,
                selections,
              });
              previewOutput.replaceChildren(formatPointPreview(evaluation));
            } catch (error) {
              previewOutput.replaceChildren(pointPreviewErrorMessage(error));
            }
          });
        });

        h.useEventListener(customStartButton, 'click', () => {
          const options = runOptions.peek();
          const budgetId = selectedBudgetId.peek();
          const inputs = pointInputs.get(budgetId);
          const selections = inputs === undefined ? undefined : readPointSelections(inputs);
          if (
            !rankedCanStart(options, 'rankedCustom') ||
            budgetId === '' ||
            selections === undefined
          )
            return;
          void withBusy(rankedBusy, async () => {
            try {
              const evaluation = await api.previewPointBudget({
                pointBudgetVersionId: budgetId,
                selections,
              });
              previewOutput.replaceChildren(formatPointPreview(evaluation));
              if (!evaluation.valid) {
                toasts.show('포인트 조합을 먼저 맞춰 주세요.', { tone: 'error' });
                return;
              }
              await submitRun(
                {
                  store,
                  snapshots,
                  api,
                  toasts,
                  retries,
                  currentForm: () => form,
                  navigate: ctx.navigate,
                },
                { mode: 'rankedCustom', pointBudgetVersionId: budgetId, selections },
              );
            } catch (error) {
              previewOutput.replaceChildren(pointPreviewErrorMessage(error));
            }
          });
        });

        const buildPresetCatalog = (options: RunOptions): void => {
          sandboxPresets.replaceChildren();
          for (const preset of options.presets) {
            presetSelect.appendChild(
              el(
                'option',
                { value: preset.id },
                `${preset.displayName} · v${String(preset.version)}`,
              ),
            );
            const button = presetButton(preset);
            h.useEventListener(button, 'click', () => form?.setValues(toFormValues(preset.draft)));
            sandboxPresets.appendChild(button);
          }
          const first = options.presets[0];
          if (first === undefined) {
            sandboxPresets.append('사용 가능한 프리셋이 없습니다.');
            return;
          }
          presetSelect.value = first.id;
          selectedPresetId.set(first.id);
        };

        const buildPointBudgetCatalog = (options: RunOptions): void => {
          for (const budget of options.pointBudgets) {
            budgetSelect.appendChild(
              el(
                'option',
                { value: budget.id },
                `${budget.displayName} · ${String(budget.totalPoints)}점`,
              ),
            );
            const fieldset = el('fieldset', {}, el('legend', {}, budget.displayName));
            const inputs = new Map<string, HTMLInputElement>();
            for (const option of budget.options) {
              const input = pointOptionInput(option.id, option.maximumQuantity);
              inputs.set(option.id, input);
              fieldset.appendChild(
                el(
                  'label',
                  {},
                  `${option.displayName} (${pointCostLabel(option.pointDeltaPerUnit)}) `,
                  input,
                  el('small', {}, ` ${option.description}`),
                ),
              );
            }
            pointInputs.set(budget.id, inputs);
            h.bindAttribute(fieldset, 'hidden', () => selectedBudgetId.get() !== budget.id);
            pointFields.appendChild(fieldset);
          }
          const first = options.pointBudgets[0];
          if (first === undefined) {
            pointFields.append('사용 가능한 포인트 예산이 없습니다.');
            return;
          }
          budgetSelect.value = first.id;
          selectedBudgetId.set(first.id);
        };

        let catalogsBuilt = false;
        let seasonBuilt = false;
        const seasonCatalog = h.useAsync((signal) => {
          const seasonId = runOptions.peek()?.activeSeasonId;
          if (seasonId === undefined || seasonId === null) {
            return Promise.reject(new Error('active season is unavailable'));
          }
          return api.listSeasonLeagues(seasonId, signal);
        });
        h.useEffect(() => {
          const state = seasonCatalog.state.get();
          if (state.status === 'error') {
            seasonFailed.set(true);
            return;
          }
          if (state.status !== 'success') return;
          seasonFailed.set(false);
          seasonLeagues.set(state.value);
          if (seasonBuilt) return;
          seasonBuilt = true;
          appendLeagueStatuses(leagueList, state.value);
        });
        const catalogs = h.useAsync((signal) => api.listRunOptions(signal));
        h.useEffect(() => {
          const state = catalogs.state.get();
          if (state.status === 'error') {
            catalogFailed.set(true);
            return;
          }
          if (state.status !== 'success') return;
          catalogFailed.set(false);
          runOptions.set(state.value);
          loadActiveSeason(state.value, () => seasonCatalog.run());
          if (catalogsBuilt) return;
          catalogsBuilt = true;
          buildPresetCatalog(state.value);
          buildPointBudgetCatalog(state.value);
        });
        catalogs.run();

        const products = h.useAsync((signal) => deps.loanApi.listProducts(signal));
        h.useEffect(() => {
          const state = products.state.get();
          if (state.status === 'error') {
            loanCatalog = undefined;
            loanBar.replaceChildren(
              '시작 대출 상품을 불러오지 못했습니다. 부채 0원으로는 시작할 수 있습니다.',
            );
            return;
          }
          if (state.status !== 'success') return;
          loanCatalog = state.value;
          const count = state.value.products.filter((product) => product.startingEligible).length;
          loanBar.replaceChildren(`시작 대출 상품 ${String(count)}개 확인됨`);
        });
        products.run();
      },

      unmount() {
        form = undefined;
      },
    };
  };
}

async function submitSandbox(deps: SandboxSubmitDeps, draft: CharacterDraft): Promise<void> {
  const options = deps.currentRunOptions();
  if (options === undefined) {
    deps.toasts.show('실행 설정을 불러온 뒤 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  if (!options.sandboxAvailable) {
    deps.toasts.show('현재 샌드박스 실행을 만들 수 없습니다.', { tone: 'error' });
    return;
  }
  const built = deps.startDrafts.build(draft, deps.currentLoanCatalog());
  if (!built.ok) {
    deps.currentForm()?.setErrors(built.errors);
    deps.toasts.show('시작 대출 상품과 부채 금액을 확인해 주세요.', { tone: 'error' });
    return;
  }
  await submitRun(deps, { mode: 'sandbox', ...built.value });
}

async function submitRun(deps: RunSubmitDeps, draft: RunStartDraft): Promise<void> {
  const current = deps.store.getState().game.snapshot;
  if (current === undefined) {
    deps.toasts.show('현재 게임 상태를 확인한 뒤 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  const request = deps.retries.select(current, draft);
  try {
    const response = await deps.api.createRun(request);
    deps.retries.clear(request);
    deps.snapshots.apply(response.snapshot);
    deps.navigate('/');
    const name = response.snapshot.characterName ?? '캐릭터';
    deps.toasts.show(`${name}의 새 실행을 시작합니다`, { tone: 'success' });
  } catch (error) {
    if (handleKnownRunFailure(deps, request, error)) return;
    deps.retries.retain(request);
    deps.toasts.show('시작 결과를 확인하지 못했습니다. 같은 조건으로 다시 시도해 주세요.', {
      tone: 'error',
    });
  }
}

function handleKnownRunFailure(
  deps: RunSubmitDeps,
  request: Parameters<RunStartRetryPolicy['clear']>[0],
  error: unknown,
): boolean {
  if (error instanceof CharacterRejectedError) {
    deps.retries.clear(request);
    deps.currentForm()?.setErrors(error.fieldErrors);
    return true;
  }
  if (error instanceof GameCommandError) {
    deps.retries.clear(request);
    deps.toasts.show(error.message, { tone: 'error' });
    return true;
  }
  return false;
}

async function withBusy(
  busy: { peek(): boolean; set(value: boolean): void },
  task: () => Promise<void>,
): Promise<void> {
  if (busy.peek()) return;
  busy.set(true);
  try {
    await task();
  } finally {
    busy.set(false);
  }
}

function readRunMode(value: string): RunMode {
  if (value === 'rankedPreset' || value === 'rankedCustom') return value;
  return 'sandbox';
}

function rankedCanStart(options: RunOptions | undefined, mode: RunMode): options is RunOptions {
  if (options === undefined) return false;
  return options.modes.includes(mode) && options.activeSeasonId !== null;
}

function runModeStatus(mode: RunMode, options: RunOptions | undefined): string {
  if (options === undefined) return '실행 설정을 불러오는 중…';
  if (!options.modes.includes(mode)) return '현재 이 실행 모드를 사용할 수 없다.';
  if (mode === 'sandbox') {
    return options.sandboxAvailable
      ? '샌드박스는 랭킹에서 제외되며 현재 활성 규칙 버전을 고정한다.'
      : '현재 샌드박스 실행을 만들 수 없다.';
  }
  return options.activeSeasonId === null
    ? '게시된 랭크 시즌이 없어 설정 확인과 포인트 미리보기만 가능하다.'
    : `활성 시즌 ${options.activeSeasonId}에 고정해 시작한다.`;
}

function formatSeasonStatus(catalog: SeasonLeagues): string {
  const statusLabels: Readonly<Record<SeasonLeagues['season']['status'], string>> = {
    draft: '준비 중',
    registrationOpen: '참가 등록 중',
    active: '진행 중',
    locked: '잠김',
    finalized: '결산 완료',
    archived: '보관됨',
  };
  const season = catalog.season;
  return `${season.displayName} · ${statusLabels[season.status]} · 목표 게임일 ${String(season.targetGameDay)}일`;
}

function formatLeagueStatus(league: SeasonLeagues['leagues'][number]): string {
  const modeLabel = league.mode === 'rankedPreset' ? '프리셋' : '커스텀';
  const publication = league.provisional ? '잠정 집계' : '공식 집계';
  return `${league.displayName} · ${modeLabel} · 참가 ${String(league.participantCount)}명 / 최소 ${String(league.minimumParticipants)}명 · ${publication}`;
}

function appendLeagueStatuses(list: HTMLUListElement, catalog: SeasonLeagues): void {
  if (catalog.leagues.length === 0) {
    list.appendChild(el('li', {}, '게시된 리그가 없습니다.'));
    return;
  }
  for (const league of catalog.leagues) {
    list.appendChild(el('li', {}, formatLeagueStatus(league)));
  }
}

function loadActiveSeason(options: RunOptions, load: () => void): void {
  if (options.activeSeasonId !== null) load();
}

function readPointSelections(
  inputs: ReadonlyMap<string, HTMLInputElement>,
): PointSelection[] | undefined {
  const selections: PointSelection[] = [];
  for (const [optionId, input] of inputs) {
    if (input.value.trim() === '' || input.value === '0') continue;
    const quantity = Number(input.value);
    if (!Number.isInteger(quantity) || quantity < 1 || quantity > 1_000_000) return undefined;
    selections.push({ optionId, quantity });
  }
  selections.sort((left, right) => {
    const leftId = BigInt(left.optionId);
    const rightId = BigInt(right.optionId);
    return leftId < rightId ? -1 : leftId > rightId ? 1 : 0;
  });
  return selections;
}

function formatPointPreview(evaluation: PointBudgetEvaluation): string {
  const spent = evaluation.spentPoints === null ? '-' : String(evaluation.spentPoints);
  const remaining = evaluation.remainingPoints === null ? '-' : String(evaluation.remainingPoints);
  if (evaluation.valid) {
    return `유효한 조합 · 총 ${String(evaluation.totalPoints)}점 · 사용 ${spent}점 · 남음 ${remaining}점`;
  }
  const failures = evaluation.failures.map((failure) => pointFailureLabel(failure.code)).join(', ');
  return `조합을 확인하세요 · ${failures || '알 수 없는 규칙 오류'}`;
}

function pointFailureLabel(code: PointBudgetEvaluation['failures'][number]['code']): string {
  const labels: Readonly<Record<typeof code, string>> = {
    unknownOption: '알 수 없는 선택',
    duplicateOption: '중복 선택',
    invalidQuantity: '수량 범위 오류',
    missingExclusiveGroup: '필수 그룹 미선택',
    multipleExclusiveGroup: '한 그룹에서 여러 항목 선택',
    requiredOptionMissing: '필수 선행 선택 누락',
    forbiddenOptionSelected: '함께 고를 수 없는 항목',
    requiredFactMissing: '필수 조건 누락',
    forbiddenFactMatched: '금지 조건 충족',
    conflictingFact: '서로 충돌하는 조건',
    pointOverflow: '포인트 계산 범위 초과',
    budgetExceeded: '포인트 예산 초과',
    invalidCatalog: '게시된 카탈로그 오류',
  };
  return labels[code];
}

function pointPreviewErrorMessage(error: unknown): string {
  return error instanceof RunRequestError
    ? error.message
    : '포인트 계산 결과를 확인하지 못했습니다. 다시 시도해 주세요.';
}

function pointCostLabel(delta: number | null): string {
  if (delta === null) return '구간별 비용';
  return `${delta > 0 ? '+' : ''}${String(delta)}점/단위`;
}

function pointOptionInput(optionId: string, maximumQuantity: number): HTMLInputElement {
  return el('input', {
    type: 'number',
    name: `point-${optionId}`,
    value: '0',
    attrs: { min: '0', max: String(maximumQuantity), step: '1' },
  });
}

function presetButton(preset: CharacterPresetVersion): HTMLButtonElement {
  return el(
    'button',
    { type: 'button', class: 'preset', attrs: { title: preset.summary } },
    `${preset.displayName} · v${String(preset.version)}`,
  );
}

function toFormValues(draft: CharacterDraft): Record<string, unknown> {
  return { ...draft };
}
