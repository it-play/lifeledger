import { CharacterDraftSchema, type Preset } from '../../api/contracts.js';
import { CharacterRejectedError, type GameApi } from '../../api/game-api.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { el } from '../../lib/dom/index.js';
import { type FieldSpec, type FormHandle, renderForm } from '../../lib/form/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { type AppState, paths } from '../state.js';

export interface CharacterCreateDeps {
  readonly store: Store<AppState>;
  readonly api: GameApi;
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
  return (): View => {
    let form: FormHandle | undefined;

    return {
      async mount(host, ctx) {
        const { store, api } = deps;
        const h = createHooks(ctx.bag);

        const presetBar = el('div', { class: 'presets' }, '프리셋 불러오는 중…');
        const container = el(
          'section',
          { class: 'character-create' },
          el('h1', {}, '캐릭터 생성'),
          el('p', {}, '시작 조건이 이 게임의 난이도다. 프리셋으로 시작해 원하는 값만 바꾸면 된다.'),
          presetBar,
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
            onSubmit: async (draft) => {
              try {
                const snapshot = await api.createCharacter(draft);
                store.set(paths.gameSnapshot, snapshot);
                ctx.navigate('/');
              } catch (error) {
                // Show the server's contradiction findings on the fields themselves
                if (error instanceof CharacterRejectedError) {
                  form?.setErrors(error.fieldErrors);
                  return;
                }
                throw error;
              }
            },
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
      },

      unmount() {
        form = undefined;
      },
    };
  };
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
