import {
  type CharacterDraft,
  CharacterDraftSchema,
  type LoanProductCatalog,
  type Preset,
} from '../../api/contracts.js';
import { CharacterRejectedError, type GameApi, GameCommandError } from '../../api/game-api.js';
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
  type CharacterStartRetryPolicy,
  createCharacterStartRetryPolicy,
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

interface CharacterSubmitDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly retries: CharacterStartRetryPolicy;
  readonly startDrafts: CharacterStartDraftBuilder;
  readonly currentForm: () => FormHandle | undefined;
  readonly currentLoanCatalog: () => LoanProductCatalog | undefined;
  readonly navigate: (to: string) => void;
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

/**
 * The character creation screen (§3).
 *
 * The server is the authority on combination checks. This screen filters field shapes
 * with zod, then plants the server's 422 field errors straight into the form, so the
 * rules never live in two places.
 */
export function createCharacterCreateView(deps: CharacterCreateDeps): ViewFactory {
  const retries = createCharacterStartRetryPolicy({ createCommandId: deps.createCommandId });
  const startDrafts = createCharacterStartDraftBuilder();

  return (): View => {
    let form: FormHandle | undefined;
    let loanCatalog: LoanProductCatalog | undefined;

    return {
      async mount(host, ctx) {
        const { store, snapshots, api, toasts } = deps;
        const h = createHooks(ctx.bag);

        const presetBar = el('div', { class: 'presets' }, '프리셋 불러오는 중…');
        const loanBar = el('p', {}, '시작 대출 상품 불러오는 중…');
        const container = el(
          'section',
          { class: 'character-create' },
          el('h1', {}, '캐릭터 생성'),
          el('p', {}, '시작 조건이 이 게임의 난이도다. 프리셋으로 시작해 원하는 값만 바꾸면 된다.'),
          presetBar,
          loanBar,
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
              submitCharacter(
                {
                  store,
                  snapshots,
                  api,
                  toasts,
                  retries,
                  startDrafts,
                  currentForm: () => form,
                  currentLoanCatalog: () => loanCatalog,
                  navigate: ctx.navigate,
                },
                draft,
              ),
          },
        );
        ctx.bag.add(form);
        container.appendChild(form.element);

        // Presets fill in after mount, so a slow list does not hold up the form
        const presets = h.useAsync(() => api.listPresets());
        h.useEffect(() => {
          const state = presets.state.get();
          if (state.status === 'error') {
            presetBar.replaceChildren('프리셋을 불러오지 못했습니다');
            return;
          }
          if (state.status !== 'success') return;
          presetBar.replaceChildren(
            ...state.value.map((preset) =>
              presetButton(preset, () => form?.setValues(toFormValues(preset))),
            ),
          );
        });
        presets.run();

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
          const startingProductCount = state.value.products.filter(
            (product) => product.startingEligible,
          ).length;
          loanBar.replaceChildren(`시작 대출 상품 ${startingProductCount}개 확인됨`);
        });
        products.run();
      },

      unmount() {
        form = undefined;
      },
    };
  };
}

async function submitCharacter(deps: CharacterSubmitDeps, draft: CharacterDraft): Promise<void> {
  const current = deps.store.getState().game.snapshot;
  if (current === undefined) {
    deps.toasts.show('현재 게임 상태를 확인한 뒤 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  const built = deps.startDrafts.build(draft, deps.currentLoanCatalog());
  if (!built.ok) {
    deps.currentForm()?.setErrors(built.errors);
    deps.toasts.show('시작 대출 상품과 부채 금액을 확인해 주세요.', { tone: 'error' });
    return;
  }
  const request = deps.retries.select(current, built.value);
  try {
    const response = await deps.api.createCharacter(request);
    deps.retries.clear(request);
    deps.snapshots.apply(response.snapshot);
    deps.navigate('/');
    // The host lives outside #app, so this survives the screen swap.
    const currentName = deps.store.getState().game.snapshot?.characterName;
    const name = currentName ?? response.snapshot.characterName ?? '캐릭터';
    deps.toasts.show(`${name}의 인생을 시작합니다`, { tone: 'success' });
  } catch (error) {
    if (handleKnownCharacterFailure(deps, request, error)) return;
    deps.retries.retain(request);
    deps.toasts.show('시작 결과를 확인하지 못했습니다. 같은 조건으로 다시 시도해 주세요.', {
      tone: 'error',
    });
  }
}

function handleKnownCharacterFailure(
  deps: CharacterSubmitDeps,
  request: Parameters<CharacterStartRetryPolicy['clear']>[0],
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

function presetButton(preset: Preset, onPick: () => void): HTMLElement {
  const button = el(
    'button',
    { type: 'button', class: 'preset', attrs: { title: preset.summary } },
    `${preset.label} · ${preset.age}세`,
  );
  button.addEventListener('click', onPick);
  return button;
}

function toFormValues(preset: Preset): Record<string, unknown> {
  return {
    name: preset.label,
    age: preset.age,
    military: preset.military,
    education: preset.education,
    region: preset.region,
    background: preset.background,
    careerYears: preset.careerYears,
    certifications: preset.certifications,
    startingCashKrw: preset.startingCashKrw,
    studentLoanKrw: preset.studentLoanKrw,
    creditLoanKrw: preset.creditLoanKrw,
    health: preset.health,
    dependents: preset.dependents,
  };
}
