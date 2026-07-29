import {
  type BusinessContract,
  type BusinessMonthlyPlanDraft,
  BusinessMonthlyPlanDraftSchema,
  type BusinessOperationRequest,
  type BusinessOperationsResponse,
  type BusinessPosition,
  type CorporationCreateDraft,
  CorporationCreateDraftSchema,
  type CorporationCreateRequest,
  type CorporationOperatingMonth,
  type CorporationOperatingMonthPageResponse,
  type CorporationPayoutDraft,
  CorporationPayoutDraftSchema,
  type CorporationPayoutRequest,
  type CorporationSettingsDraft,
  CorporationSettingsDraftSchema,
  type CorporationSettingsRequest,
  type CorporationSummary,
  type CorporationTemplate,
  type CorporationTemplatesResponse,
  type GameSnapshot,
} from '../../api/contracts.js';
import {
  type CorporationApi,
  CorporationCommandError,
  CorporationQueryError,
} from '../../api/corporation-api.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { el } from '../../lib/dom/index.js';
import { type FormHandle, renderForm } from '../../lib/form/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import { type AppState, paths } from '../state.js';

export interface CorporationDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: CorporationApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface PendingCommand<T> {
  readonly key: string;
  readonly request: T;
}

type BusinessOperationActionDraft =
  | { readonly action: 'acceptContract'; readonly contractId: string }
  | { readonly action: 'cancelContract'; readonly contractId: string }
  | { readonly action: 'hirePosition'; readonly positionId: string }
  | { readonly action: 'terminatePosition'; readonly positionId: string }
  | {
      readonly action: 'setMonthlyPlan';
      readonly marketingBandId: string;
      readonly cashBufferKrw: number;
      readonly contractPriorityIds: string[];
    };

interface MonthSlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
}

interface ContractSlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
  readonly accept: HTMLButtonElement;
  readonly cancel: HTMLButtonElement;
}

interface PositionSlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
  readonly action: HTMLButtonElement;
}

const TEMPLATE_CAPACITY = 3;
const SCALE_CAPACITY = 3;
const MONTH_CAPACITY = 20;
const MARKETING_CAPACITY = 8;
const CONTRACT_CAPACITY = 50;
const POSITION_CAPACITY = 32;

const PLACEHOLDER_TEMPLATE_OPTIONS = Array.from({ length: TEMPLATE_CAPACITY }, (_, index) => ({
  value: String(index + 1),
  label: '업종 템플릿 조회 중',
}));

const PLACEHOLDER_SCALE_OPTIONS = Array.from({ length: SCALE_CAPACITY }, (_, index) => ({
  value: String(index + 1),
  label: '운영 규모 조회 중',
}));

const PLACEHOLDER_MARKETING_OPTIONS = Array.from({ length: MARKETING_CAPACITY }, (_, index) => ({
  value: String(index + 1),
  label: '마케팅 정책 조회 중',
}));

const CREATE_FIELDS = [
  {
    name: 'industryTemplateId',
    label: '업종',
    kind: 'select',
    options: PLACEHOLDER_TEMPLATE_OPTIONS,
  },
  { name: 'name', label: '법인명', kind: 'text', help: '2~40자' },
  { name: 'capitalKrw', label: '자본금', kind: 'number', help: '개인 지갑에서 출자합니다.' },
] as const;

const SETTINGS_FIELDS = [
  {
    name: 'operatingScaleId',
    label: '다음 달 운영 규모',
    kind: 'select',
    options: PLACEHOLDER_SCALE_OPTIONS,
  },
  {
    name: 'officerGrossSalaryKrw',
    label: '다음 달 대표 급여(세전)',
    kind: 'number',
    help: '0원이면 대표 급여를 지급하지 않습니다.',
  },
] as const;

const PAYOUT_FIELDS = [
  {
    name: 'grossDividendKrw',
    label: '세전 배당액',
    kind: 'number',
    help: '최근 결산 후 배당가능이익과 법인 현금 범위에서 지급합니다.',
  },
] as const;

const BUSINESS_PLAN_FIELDS = [
  {
    name: 'marketingBandId',
    label: '다음 달 마케팅',
    kind: 'select',
    options: PLACEHOLDER_MARKETING_OPTIONS,
  },
  {
    name: 'cashBufferKrw',
    label: '보호할 현금',
    kind: 'number',
    help: '계약 수행비보다 우선해 남겨 둘 법인 현금입니다.',
  },
  {
    name: 'contractPriorityText',
    label: '계약 우선순위 ID',
    kind: 'text',
    help: '수행할 계약 ID를 쉼표로 구분해 우선순위대로 입력합니다.',
  },
] as const;

/** Functional corporation console. Visual design is intentionally deferred. */
export function createCorporationView(deps: CorporationDeps): ViewFactory {
  return (): View => ({
    mount(host, ctx) {
      const h = createHooks(ctx.bag);
      const snapshot = h.useStoreValue(
        deps.store,
        paths.gameSnapshot,
        (state) => state.game.snapshot,
      );
      const advancing = h.useStoreValue(
        deps.store,
        paths.gameAdvancing,
        (state) => state.game.advancing,
      );
      const ordering = h.useStoreValue(
        deps.store,
        paths.gameOrdering,
        (state) => state.game.ordering,
      );
      const commandBusy = h.useSignal(false);
      const commandFeedback = h.useSignal('');
      const templates = h.useSignal<CorporationTemplatesResponse | undefined>(undefined);
      const monthCursor = h.useSignal<string | undefined>(undefined);
      const currentCorporation = h.useComputed(
        () => snapshot.get()?.life.corporation.current ?? null,
      );
      const corporationId = h.useComputed(() => currentCorporation.get()?.id);
      const gameReady = h.useComputed(
        () => snapshot.get()?.characterName !== null && snapshot.get() !== undefined,
      );
      const canIssueCommand = h.useComputed(() => {
        const current = snapshot.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          !commandBusy.get()
        );
      });
      const templateRequest = h.useAsync((signal) => deps.api.templates(signal));
      const monthRequest = h.useAsync<CorporationOperatingMonthPageResponse>(async (signal) => {
        const id = corporationId.peek();
        if (id === undefined) return { months: [], nextCursor: null };
        return deps.api.months(id, monthCursor.peek(), signal);
      });
      const operationsRequest = h.useAsync<BusinessOperationsResponse>(async (signal) => {
        const id = corporationId.peek();
        if (id === undefined) throw new Error('법인 운영 정보를 조회할 법인이 없습니다.');
        return deps.api.operations(id, signal);
      });

      let pendingCreate: PendingCommand<CorporationCreateRequest> | undefined;
      let pendingSettings: PendingCommand<CorporationSettingsRequest> | undefined;
      let pendingPayout: PendingCommand<CorporationPayoutRequest> | undefined;
      let pendingOperation: PendingCommand<BusinessOperationRequest> | undefined;
      let lastSettingKey = '';
      let lastPlanKey = '';

      const creationForm = renderForm<CorporationCreateDraft>(
        {
          fields: CREATE_FIELDS,
          validator: asFormValidator(CorporationCreateDraftSchema),
          submitLabel: '법인 설립',
          idPrefix: 'corporation-create',
        },
        {
          initial: { industryTemplateId: '1', name: '', capitalKrw: 1 },
          onSubmit: submitCreate,
        },
      );
      const settingsForm = renderForm<CorporationSettingsDraft>(
        {
          fields: SETTINGS_FIELDS,
          validator: asFormValidator(CorporationSettingsDraftSchema),
          submitLabel: '다음 달 설정 저장',
          idPrefix: 'corporation-settings',
        },
        {
          initial: { operatingScaleId: '1', officerGrossSalaryKrw: 0 },
          onSubmit: submitSettings,
        },
      );
      const payoutForm = renderForm<CorporationPayoutDraft>(
        {
          fields: PAYOUT_FIELDS,
          validator: asFormValidator(CorporationPayoutDraftSchema),
          submitLabel: '배당 지급',
          idPrefix: 'corporation-payout',
        },
        { initial: { grossDividendKrw: 1 }, onSubmit: submitPayout },
      );
      const planForm = renderForm<BusinessMonthlyPlanDraft>(
        {
          fields: BUSINESS_PLAN_FIELDS,
          validator: asFormValidator(BusinessMonthlyPlanDraftSchema),
          submitLabel: '다음 달 사업 계획 저장',
          idPrefix: 'corporation-business-plan',
        },
        {
          initial: { marketingBandId: '1', cashBufferKrw: 0, contractPriorityText: '' },
          onSubmit: submitBusinessPlan,
        },
      );
      ctx.bag.add(creationForm);
      ctx.bag.add(settingsForm);
      ctx.bag.add(payoutForm);
      ctx.bag.add(planForm);

      const templateSelect = formSelect(creationForm, 'industryTemplateId');
      const scaleSelect = formSelect(settingsForm, 'operatingScaleId');
      const createSubmit = formSubmit(creationForm);
      const settingsSubmit = formSubmit(settingsForm);
      const payoutSubmit = formSubmit(payoutForm);
      const marketingSelect = formSelect(planForm, 'marketingBandId');
      const planSubmit = formSubmit(planForm);
      const queryStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const commandStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const summaryStatus = el('dd');
      const summaryName = el('dd');
      const summaryTemplate = el('dd');
      const summaryCash = el('dd');
      const summaryRetained = el('dd');
      const summaryOperatingPayable = el('dd');
      const summaryTaxPayable = el('dd');
      const summaryDistributable = el('dd');
      const summaryNextSetting = el('dd');
      const refreshMonths = el('button', { type: 'button' }, '첫 페이지 새로고침');
      const nextMonths = el('button', { type: 'button' }, '다음 20개월');
      const monthSlots = Array.from({ length: MONTH_CAPACITY }, createMonthSlot);
      const contractSlots = Array.from({ length: CONTRACT_CAPACITY }, createContractSlot);
      const positionSlots = Array.from({ length: POSITION_CAPACITY }, createPositionSlot);
      const operationsSummary = el('p');
      const latestBusinessMonth = el('p');
      const refreshOperations = el('button', { type: 'button' }, '사업 운영 새로고침');
      const createSection = el(
        'section',
        {},
        el('h2', {}, '법인 설립'),
        el('p', {}, '한 실행에서 법인 하나를 설립할 수 있습니다.'),
        creationForm.element,
      );
      const manageSection = el(
        'section',
        {},
        el('h2', {}, '법인 현황'),
        el(
          'dl',
          {},
          el('dt', {}, '상태'),
          summaryStatus,
          el('dt', {}, '법인명'),
          summaryName,
          el('dt', {}, '업종'),
          summaryTemplate,
          el('dt', {}, '법인 현금'),
          summaryCash,
          el('dt', {}, '이익잉여금'),
          summaryRetained,
          el('dt', {}, '운영 미지급금'),
          summaryOperatingPayable,
          el('dt', {}, '법인세 미지급금'),
          summaryTaxPayable,
          el('dt', {}, '배당가능이익'),
          summaryDistributable,
          el('dt', {}, '다음 달 설정'),
          summaryNextSetting,
        ),
        el('h2', {}, '다음 달 운영 설정'),
        settingsForm.element,
        el('h2', {}, '계약·인력 운영'),
        refreshOperations,
        operationsSummary,
        el('h3', {}, '고객 계약'),
        el('ol', {}, ...contractSlots.map((slot) => slot.element)),
        el('h3', {}, '직원 자리'),
        el('ol', {}, ...positionSlots.map((slot) => slot.element)),
        el('h3', {}, '다음 달 사업 계획'),
        planForm.element,
        el('h3', {}, '최근 사업 실적'),
        latestBusinessMonth,
        el('h2', {}, '배당'),
        payoutForm.element,
        el('h2', {}, '월별 손익'),
        refreshMonths,
        nextMonths,
        el('ol', {}, ...monthSlots.map((slot) => slot.element)),
      );

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '법인 경영'),
          el('p', {}, '법인 시간·손익·세금·배당은 서버가 확정합니다.'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          queryStatus,
          commandStatus,
          createSection,
          manageSection,
        ),
      );

      h.bindText(queryStatus, () =>
        queryStatusText(
          templateRequest.state.get(),
          monthRequest.state.get(),
          operationsRequest.state.get(),
        ),
      );
      h.bindText(commandStatus, () => commandFeedback.get());
      h.bindAttribute(createSection, 'hidden', () => currentCorporation.get() !== null);
      h.bindAttribute(manageSection, 'hidden', () => currentCorporation.get() === null);
      h.bindAttribute(createSubmit, 'disabled', () => {
        const catalog = templates.get();
        return (
          !canIssueCommand.get() ||
          catalog?.availability !== 'active' ||
          catalog.templates.length === 0
        );
      });
      h.bindAttribute(settingsSubmit, 'disabled', () => {
        const current = currentCorporation.get();
        return !canIssueCommand.get() || current === null || current.status !== 'active';
      });
      h.bindAttribute(payoutSubmit, 'disabled', () => {
        const current = currentCorporation.get();
        return (
          !canIssueCommand.get() ||
          current === null ||
          current.status !== 'active' ||
          current.distributableProfitKrw <= 0
        );
      });
      h.bindAttribute(planSubmit, 'disabled', () => {
        const state = operationsRequest.state.get();
        return (
          !canIssueCommand.get() ||
          state.status !== 'success' ||
          state.value.availability !== 'active' ||
          state.value.marketingBands.length === 0
        );
      });
      h.bindAttribute(refreshOperations, 'disabled', () => {
        return (
          corporationId.get() === undefined || operationsRequest.state.get().status === 'loading'
        );
      });
      h.bindAttribute(
        refreshMonths,
        'disabled',
        () => corporationId.get() === undefined || monthRequest.state.get().status === 'loading',
      );
      h.bindAttribute(nextMonths, 'disabled', () => {
        const state = monthRequest.state.get();
        return state.status !== 'success' || state.value.nextCursor === null;
      });
      h.bindText(summaryStatus, () => currentCorporation.get()?.status ?? '—');
      h.bindText(summaryName, () => currentCorporation.get()?.name ?? '—');
      h.bindText(summaryTemplate, () => currentCorporation.get()?.templateDisplayName ?? '—');
      h.bindText(summaryCash, () => money(currentCorporation.get()?.cashKrw));
      h.bindText(summaryRetained, () => money(currentCorporation.get()?.retainedEarningsKrw));
      h.bindText(summaryOperatingPayable, () =>
        money(currentCorporation.get()?.operatingPayableKrw),
      );
      h.bindText(summaryTaxPayable, () => money(currentCorporation.get()?.corporateTaxPayableKrw));
      h.bindText(summaryDistributable, () =>
        money(currentCorporation.get()?.distributableProfitKrw),
      );
      h.bindText(summaryNextSetting, () => settingText(currentCorporation.get()));
      h.bindText(operationsSummary, () => operationsSummaryText(operationsRequest.state.get()));
      h.bindText(latestBusinessMonth, () => latestBusinessMonthText(operationsRequest.state.get()));

      for (const [index, slot] of monthSlots.entries()) {
        h.bindAttribute(
          slot.element,
          'hidden',
          () => monthAt(monthRequest.state.get(), index) === undefined,
        );
        h.bindText(slot.text, () => monthText(monthAt(monthRequest.state.get(), index)));
      }

      for (const [index, slot] of contractSlots.entries()) {
        h.bindAttribute(
          slot.element,
          'hidden',
          () => operationContractAt(operationsRequest.state.get(), index) === undefined,
        );
        h.bindText(slot.text, () =>
          contractText(operationContractAt(operationsRequest.state.get(), index)),
        );
        h.bindAttribute(
          slot.accept,
          'hidden',
          () => operationContractAt(operationsRequest.state.get(), index)?.status !== 'offered',
        );
        h.bindAttribute(slot.accept, 'disabled', () => !canIssueCommand.get());
        h.bindAttribute(slot.cancel, 'hidden', () => {
          const status = operationContractAt(operationsRequest.state.get(), index)?.status;
          return status !== 'offered' && status !== 'accepted';
        });
        h.bindAttribute(slot.cancel, 'disabled', () => !canIssueCommand.get());
        h.useEventListener(slot.accept, 'click', () => {
          const contract = operationContractAt(operationsRequest.state.peek(), index);
          if (contract !== undefined) {
            void submitOperation({ action: 'acceptContract', contractId: contract.id }).catch(
              showButtonError,
            );
          }
        });
        h.useEventListener(slot.cancel, 'click', () => {
          const contract = operationContractAt(operationsRequest.state.peek(), index);
          if (contract !== undefined) {
            void submitOperation({ action: 'cancelContract', contractId: contract.id }).catch(
              showButtonError,
            );
          }
        });
      }

      for (const [index, slot] of positionSlots.entries()) {
        h.bindAttribute(
          slot.element,
          'hidden',
          () => operationPositionAt(operationsRequest.state.get(), index) === undefined,
        );
        h.bindText(slot.text, () =>
          positionText(operationPositionAt(operationsRequest.state.get(), index)),
        );
        h.bindText(slot.action, () =>
          positionActionText(operationPositionAt(operationsRequest.state.get(), index)),
        );
        h.bindAttribute(slot.action, 'hidden', () => {
          const status = operationPositionAt(operationsRequest.state.get(), index)?.status;
          return status !== 'vacant' && status !== 'hired' && status !== 'active';
        });
        h.bindAttribute(slot.action, 'disabled', () => !canIssueCommand.get());
        h.useEventListener(slot.action, 'click', () => {
          const position = operationPositionAt(operationsRequest.state.peek(), index);
          if (position === undefined) return;
          void submitOperation(
            position.status === 'vacant'
              ? { action: 'hirePosition', positionId: position.id }
              : { action: 'terminatePosition', positionId: position.id },
          ).catch(showButtonError);
        });
      }

      h.useEffect(() => {
        const state = templateRequest.state.get();
        if (state.status !== 'success') return;
        templates.set(state.value);
        syncTemplateOptions(templateSelect, state.value.templates);
        if (state.value.minimumCapitalKrw !== null) {
          creationForm.setValues({ capitalKrw: state.value.minimumCapitalKrw });
        }
      });
      h.useEffect(() => {
        const catalog = templates.get();
        const current = currentCorporation.get();
        if (catalog === undefined || current === null) return;
        const template = catalog.templates.find(
          (candidate) => candidate.id === current.industryTemplateId,
        );
        syncScaleOptions(scaleSelect, template?.operatingScales ?? []);
        const setting = current.nextMonthSetting;
        const settingKey = `${current.id}:${setting.settingId ?? 'initial'}:${setting.operatingScaleId}:${setting.officerGrossSalaryKrw}`;
        if (settingKey !== lastSettingKey) {
          lastSettingKey = settingKey;
          settingsForm.setValues({
            operatingScaleId: setting.operatingScaleId,
            officerGrossSalaryKrw: setting.officerGrossSalaryKrw,
          });
        }
      });
      h.useEffect(() => {
        const state = operationsRequest.state.get();
        if (state.status !== 'success' || state.value.availability !== 'active') return;
        const operations = state.value;
        syncOptions(
          marketingSelect,
          operations.marketingBands.map((band) => ({
            value: band.id,
            label: `${band.displayName} · 월 ${formatWon(band.monthlyCostKrw)}`,
          })),
        );
        const plan = operations.plan;
        const planKey = `${operations.revision}:${plan?.id ?? 'initial'}:${plan?.planRevision ?? 0}`;
        if (planKey !== lastPlanKey) {
          lastPlanKey = planKey;
          planForm.setValues({
            marketingBandId: plan?.marketingBandId ?? operations.marketingBands[0]?.id ?? '1',
            cashBufferKrw: plan?.cashBufferKrw ?? 0,
            contractPriorityText: plan?.contractPriorityIds.join(', ') ?? '',
          });
        }
      });
      h.useWatch(gameReady, (ready) => {
        if (ready) templateRequest.run();
      });
      h.useWatch(corporationId, (next, previous) => {
        if (next === previous || next === undefined) return;
        monthCursor.set(undefined);
        monthRequest.run();
        operationsRequest.run();
      });
      h.useEventListener(refreshOperations, 'click', () => operationsRequest.run());
      h.useEventListener(refreshMonths, 'click', () => {
        monthCursor.set(undefined);
        monthRequest.run();
      });
      h.useEventListener(nextMonths, 'click', () => {
        const state = monthRequest.state.peek();
        if (state.status !== 'success' || state.value.nextCursor === null) return;
        monthCursor.set(state.value.nextCursor);
        monthRequest.run();
      });

      if (gameReady.peek()) templateRequest.run();
      if (corporationId.peek() !== undefined) {
        monthRequest.run();
        operationsRequest.run();
      }

      async function submitCreate(draft: CorporationCreateDraft): Promise<void> {
        const current = commandSnapshot(deps, '법인을 설립');
        const catalog = templates.peek();
        if (
          catalog === undefined ||
          catalog.minimumCapitalKrw === null ||
          catalog.maximumCapitalKrw === null ||
          draft.capitalKrw < catalog.minimumCapitalKrw ||
          draft.capitalKrw > catalog.maximumCapitalKrw
        ) {
          throw new Error('자본금은 현재 설립 가능 범위 안에서 입력해 주세요.');
        }
        const key = JSON.stringify(draft);
        const request =
          pendingCreate?.key === key
            ? pendingCreate.request
            : { ...commandCursor(current, deps.createCommandId()), ...draft };
        pendingCreate = { key, request };
        await runCommand('법인을 설립하는 중입니다.', async () => {
          try {
            const response = await deps.api.create(request);
            pendingCreate = undefined;
            deps.snapshots.apply(response.snapshot);
            commandFeedback.set(
              response.replayed ? '기존 법인 설립 결과를 복원했습니다.' : '법인을 설립했습니다.',
            );
            deps.toasts.show('법인을 설립했습니다.', { tone: 'success' });
          } catch (error) {
            pendingCreate = retainUnknownOutcome(pendingCreate, error);
            throw displayError(error, '법인 설립');
          }
        });
      }

      async function submitSettings(draft: CorporationSettingsDraft): Promise<void> {
        const current = commandSnapshot(deps, '법인 설정을 변경');
        const corporation = requireCorporation(current);
        const key = `${corporation.id}:${JSON.stringify(draft)}`;
        const request =
          pendingSettings?.key === key
            ? pendingSettings.request
            : { ...commandCursor(current, deps.createCommandId()), ...draft };
        pendingSettings = { key, request };
        await runCommand('다음 달 운영 설정을 저장하는 중입니다.', async () => {
          try {
            const response = await deps.api.updateSettings(corporation.id, request);
            pendingSettings = undefined;
            deps.snapshots.apply(response.snapshot);
            commandFeedback.set(
              response.replayed
                ? '기존 운영 설정 결과를 복원했습니다.'
                : '다음 달 운영 설정을 저장했습니다.',
            );
            deps.toasts.show('다음 달 운영 설정을 저장했습니다.', { tone: 'success' });
          } catch (error) {
            pendingSettings = retainUnknownOutcome(pendingSettings, error);
            throw displayError(error, '법인 설정 변경');
          }
        });
      }

      async function submitPayout(draft: CorporationPayoutDraft): Promise<void> {
        const current = commandSnapshot(deps, '배당을 지급');
        const corporation = requireCorporation(current);
        if (draft.grossDividendKrw > corporation.distributableProfitKrw) {
          throw new Error('배당액이 현재 배당가능이익보다 큽니다.');
        }
        const key = `${corporation.id}:${JSON.stringify(draft)}`;
        const request =
          pendingPayout?.key === key
            ? pendingPayout.request
            : {
                ...commandCursor(current, deps.createCommandId()),
                kind: 'dividend' as const,
                ...draft,
              };
        pendingPayout = { key, request };
        await runCommand('배당을 지급하는 중입니다.', async () => {
          try {
            const response = await deps.api.payDividend(corporation.id, request);
            pendingPayout = undefined;
            deps.snapshots.apply(response.snapshot);
            payoutForm.reset();
            commandFeedback.set(
              `${response.replayed ? '기존 배당 결과를 복원했습니다.' : '배당을 지급했습니다.'} 실수령 ${formatWon(response.result.netDividendKrw)}`,
            );
            deps.toasts.show('배당을 지급했습니다.', { tone: 'success' });
          } catch (error) {
            pendingPayout = retainUnknownOutcome(pendingPayout, error);
            throw displayError(error, '배당 지급');
          }
        });
      }

      async function submitBusinessPlan(draft: BusinessMonthlyPlanDraft): Promise<void> {
        const state = operationsRequest.state.peek();
        if (state.status !== 'success' || state.value.availability !== 'active') {
          throw new Error('사업 운영 정보를 먼저 새로고침해 주세요.');
        }
        const ids = draft.contractPriorityText
          .split(',')
          .map((id) => id.trim())
          .filter((id) => id.length > 0);
        if (ids.length > 50 || new Set(ids).size !== ids.length) {
          throw new Error('계약 우선순위는 중복 없이 50개까지 입력할 수 있습니다.');
        }
        const eligible = new Set(
          state.value.contracts
            .filter((contract) => contract.status === 'accepted' || contract.status === 'active')
            .map((contract) => contract.id),
        );
        if (ids.some((id) => !eligible.has(id))) {
          throw new Error('수락되어 다음 달 수행 가능한 계약 ID만 입력해 주세요.');
        }
        await submitOperation({
          action: 'setMonthlyPlan',
          marketingBandId: draft.marketingBandId,
          cashBufferKrw: draft.cashBufferKrw,
          contractPriorityIds: ids,
        });
      }

      async function submitOperation(action: BusinessOperationActionDraft): Promise<void> {
        const current = commandSnapshot(deps, '사업 운영 명령을 실행');
        const corporation = requireCorporation(current);
        const state = operationsRequest.state.peek();
        if (
          state.status !== 'success' ||
          state.value.availability !== 'active' ||
          state.value.revision < 1
        ) {
          throw new Error('사업 운영 정보를 먼저 새로고침해 주세요.');
        }
        const key = `${corporation.id}:${state.value.revision}:${JSON.stringify(action)}`;
        const request: BusinessOperationRequest =
          pendingOperation?.key === key
            ? pendingOperation.request
            : {
                ...commandCursor(current, deps.createCommandId()),
                expectedRevision: state.value.revision,
                ...action,
              };
        pendingOperation = { key, request };
        await runCommand('사업 운영 명령을 처리하는 중입니다.', async () => {
          try {
            const response = await deps.api.manageOperations(corporation.id, request);
            pendingOperation = undefined;
            deps.snapshots.apply(response.snapshot);
            operationsRequest.run();
            monthCursor.set(undefined);
            monthRequest.run();
            const message = response.replayed
              ? '기존 사업 운영 결과를 복원했습니다.'
              : `${operationActionLabel(request.action)} 처리를 완료했습니다.`;
            commandFeedback.set(message);
            deps.toasts.show(message, { tone: 'success' });
          } catch (error) {
            pendingOperation = retainUnknownOutcome(pendingOperation, error);
            throw displayError(error, '사업 운영 명령');
          }
        });
      }

      function showButtonError(error: unknown): void {
        commandFeedback.set(
          error instanceof Error ? error.message : '사업 운영 명령에 실패했습니다.',
        );
      }

      async function runCommand(status: string, command: () => Promise<void>): Promise<void> {
        commandBusy.set(true);
        commandFeedback.set(status);
        deps.store.set(paths.gameOrdering, true);
        try {
          await command();
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }
    },
    unmount() {},
  });
}

function formSelect(form: FormHandle, name: string): HTMLSelectElement {
  const select = form.element.querySelector<HTMLSelectElement>(`select[name="${name}"]`);
  if (select === null) throw new Error(`${name} 선택 필드가 없습니다.`);
  return select;
}

function formSubmit(form: FormHandle): HTMLButtonElement {
  const submit = form.element.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (submit === null) throw new Error('제출 버튼이 없습니다.');
  return submit;
}

function syncTemplateOptions(
  select: HTMLSelectElement,
  templates: readonly CorporationTemplate[],
): void {
  syncOptions(
    select,
    templates.map((template) => ({ value: template.id, label: template.displayName })),
  );
}

function syncScaleOptions(
  select: HTMLSelectElement,
  scales: readonly {
    readonly id: string;
    readonly scaleKey: string;
    readonly scaleOrder: number;
  }[],
): void {
  syncOptions(
    select,
    scales.map((scale) => ({
      value: scale.id,
      label: `${scale.scaleOrder}단계 · ${scale.scaleKey}`,
    })),
  );
}

function syncOptions(
  select: HTMLSelectElement,
  values: readonly { readonly value: string; readonly label: string }[],
): void {
  for (const [index, option] of Array.from(select.options).entries()) {
    const value = values[index];
    option.hidden = value === undefined;
    option.disabled = value === undefined;
    option.value = value?.value ?? String(index + 1);
    option.textContent = value?.label ?? '';
  }
  const first = values[0];
  if (first !== undefined && !values.some((value) => value.value === select.value)) {
    select.value = first.value;
  }
}

function createMonthSlot(): MonthSlot {
  const text = el('span');
  return { element: el('li', {}, text), text };
}

function createContractSlot(): ContractSlot {
  const text = el('span');
  const accept = el('button', { type: 'button' }, '수락');
  const cancel = el('button', { type: 'button' }, '취소');
  return { element: el('li', {}, text, ' ', accept, ' ', cancel), text, accept, cancel };
}

function createPositionSlot(): PositionSlot {
  const text = el('span');
  const action = el('button', { type: 'button' });
  return { element: el('li', {}, text, ' ', action), text, action };
}

function monthAt(
  state: AsyncState<CorporationOperatingMonthPageResponse>,
  index: number,
): CorporationOperatingMonth | undefined {
  return state.status === 'success' ? state.value.months[index] : undefined;
}

function monthText(month: CorporationOperatingMonth | undefined): string {
  if (month === undefined) return '';
  return `${month.operatingYear}년 ${month.operatingMonth}월 · 매출 ${formatWon(month.revenueKrw)} · 비용 ${formatWon(month.operatingExpenseKrw)} · 급여비 ${formatWon(month.totalPayrollCostKrw)} · 세전손익 ${formatWon(month.preTaxProfitKrw)} · 현금 ${formatWon(month.cashAfterKrw)} · 급여 ${month.payrollStatus}`;
}

function operationContractAt(
  state: AsyncState<BusinessOperationsResponse>,
  index: number,
): BusinessContract | undefined {
  return state.status === 'success' ? state.value.contracts[index] : undefined;
}

function operationPositionAt(
  state: AsyncState<BusinessOperationsResponse>,
  index: number,
): BusinessPosition | undefined {
  return state.status === 'success' ? state.value.positions[index] : undefined;
}

function contractText(contract: BusinessContract | undefined): string {
  if (contract === undefined) return '';
  return `#${contract.id} ${contract.displayName} · ${contract.serviceYear}년 ${contract.serviceMonth}월 · 매출 ${formatWon(contract.revenueKrw)} · 필요 역량 ${contract.requiredCapacityUnits} · ${contract.status}`;
}

function positionText(position: BusinessPosition | undefined): string {
  if (position === undefined) return '';
  return `#${position.id} ${position.displayName} · 역량 ${position.capacityUnits} · 월 급여 ${formatWon(position.monthlyGrossWageKrw)} · ${position.status}`;
}

function positionActionText(position: BusinessPosition | undefined): string {
  return position?.status === 'vacant' ? '채용' : '종료';
}

function operationsSummaryText(state: AsyncState<BusinessOperationsResponse>): string {
  if (state.status !== 'success') return '';
  if (state.value.availability !== 'active') return '현재 실행에는 상세 사업 운영 규칙이 없습니다.';
  const operations = state.value;
  return `${operations.nextOperatingYear ?? '—'}년 ${operations.nextOperatingMonth ?? '—'}월 · 운영 revision ${operations.revision} · 계약 ${operations.contracts.length}건 · 인력 자리 ${operations.positions.length}개`;
}

function latestBusinessMonthText(state: AsyncState<BusinessOperationsResponse>): string {
  if (state.status !== 'success' || state.value.latestMonth === null)
    return '아직 확정된 상세 사업 실적이 없습니다.';
  const month = state.value.latestMonth;
  return `${month.operatingYear}년 ${month.operatingMonth}월 · 계약 매출 ${formatWon(month.contractRevenueKrw)} · 계약 변동비 ${formatWon(month.contractVariableCostKrw)} · 마케팅 ${formatWon(month.marketingCostKrw)} · 직원 비용 ${formatWon(month.employeeCostKrw)} · 역량 ${month.usedCapacityUnits}/${month.totalCapacityUnits}`;
}

function queryStatusText(
  templates: AsyncState<CorporationTemplatesResponse>,
  months: AsyncState<CorporationOperatingMonthPageResponse>,
  operations: AsyncState<BusinessOperationsResponse>,
): string {
  if (templates.status === 'loading') return '법인 규칙을 조회하는 중입니다.';
  if (templates.status === 'error') return errorMessage(templates.error, '법인 규칙 조회');
  if (months.status === 'loading') return '월별 손익을 조회하는 중입니다.';
  if (months.status === 'error') return errorMessage(months.error, '월별 손익 조회');
  if (operations.status === 'loading') return '계약·인력 운영 정보를 조회하는 중입니다.';
  if (operations.status === 'error') return errorMessage(operations.error, '계약·인력 운영 조회');
  return '';
}

function operationActionLabel(action: BusinessOperationRequest['action']): string {
  switch (action) {
    case 'acceptContract':
      return '계약 수락';
    case 'cancelContract':
      return '계약 취소';
    case 'hirePosition':
      return '직원 채용';
    case 'terminatePosition':
      return '직원 종료';
    case 'setMonthlyPlan':
      return '사업 계획 저장';
  }
}

function settingText(corporation: CorporationSummary | null): string {
  if (corporation === null) return '—';
  const setting = corporation.nextMonthSetting;
  return `${setting.effectiveYear}년 ${setting.effectiveMonth}월 · ${setting.scaleKey} · 대표 급여 ${formatWon(setting.officerGrossSalaryKrw)}`;
}

function money(value: number | undefined): string {
  return value === undefined ? '—' : formatWon(value);
}

function commandSnapshot(deps: CorporationDeps, action: string): GameSnapshot {
  const current = deps.store.getState().game.snapshot;
  if (current === undefined || current.characterName === null) {
    throw new Error(`${action}하려면 진행 중인 캐릭터가 필요합니다.`);
  }
  if (current.autoSpeed !== null || deps.store.getState().game.advancing) {
    throw new Error(`${action}하기 전에 자동 진행을 멈춰 주세요.`);
  }
  return current;
}

function requireCorporation(snapshot: GameSnapshot): CorporationSummary {
  const corporation = snapshot.life.corporation.current;
  if (corporation === null) throw new Error('먼저 법인을 설립해 주세요.');
  return corporation;
}

function commandCursor(snapshot: GameSnapshot, commandId: string) {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function displayError(error: unknown, action: string): Error {
  if (error instanceof CorporationCommandError) return new Error(error.message);
  return new Error(
    `${action} 결과를 확인하지 못했습니다. 같은 입력으로 다시 시도하면 중복 처리되지 않습니다.`,
  );
}

function retainUnknownOutcome<T>(pending: T, error: unknown): T | undefined {
  return error instanceof CorporationCommandError ? undefined : pending;
}

function errorMessage(error: unknown, action: string): string {
  if (error instanceof CorporationQueryError) return error.message;
  return `${action}에 실패했습니다. 잠시 후 다시 시도해 주세요.`;
}
