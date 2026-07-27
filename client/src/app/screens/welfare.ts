import type {
  GameSnapshot,
  WelfareApplicationRequest,
  WelfareApplicationResponse,
  WelfareApplicationStatus,
  WelfareConditionOutcome,
  WelfarePaymentStatus,
  WelfareProgram,
  WelfareProgramsResponse,
} from '../../api/contracts.js';
import { type WelfareApi, WelfareCommandError, WelfareQueryError } from '../../api/welfare-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import { type AppState, paths } from '../state.js';
import { createWelfareApplicationRetryPolicy } from '../welfare-retry/index.js';

export interface WelfareDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: WelfareApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface WelfareConditionSlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
}

interface WelfareProgramSlot {
  readonly element: HTMLLIElement;
  readonly name: HTMLHeadingElement;
  readonly benefit: HTMLElement;
  readonly schedule: HTMLElement;
  readonly evaluation: HTMLElement;
  readonly fingerprint: HTMLElement;
  readonly availability: HTMLElement;
  readonly application: HTMLElement;
  readonly payment: HTMLElement;
  readonly conditions: readonly WelfareConditionSlot[];
  readonly apply: HTMLButtonElement;
}

const MAX_PROGRAMS = 16;
const MAX_CONDITIONS = 32;

const EVALUATION_LABEL = {
  eligible: '신청 자격 있음',
  ineligible: '신청 자격 없음',
  indeterminate: '현재 정보로 판정할 수 없음',
} as const;

const CONDITION_OUTCOME_LABEL: Record<WelfareConditionOutcome, string> = {
  passed: '통과',
  failed: '실패',
  unknown: '확인 불가',
};

const APPLICATION_STATUS_LABEL: Record<WelfareApplicationStatus, string> = {
  applied: '신청됨',
  approved: '승인됨',
  rejected: '거절됨',
  active: '지급 대기',
  exhausted: '지급 완료',
  terminated: '종료됨',
};

const PAYMENT_STATUS_LABEL: Record<WelfarePaymentStatus, string> = {
  pending: '지급 대기',
  paid: '지급 완료',
  cancelled: '취소됨',
};

/** M4-D server-authoritative welfare evaluation, application, and payment view. */
export function createWelfareView(deps: WelfareDeps): ViewFactory {
  const retries = createWelfareApplicationRetryPolicy({
    createCommandId: deps.createCommandId,
  });

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
      const programs = h.useSignal<WelfareProgramsResponse | undefined>(undefined);
      const commandBusy = h.useSignal(false);
      const commandFeedback = h.useSignal('');
      const programRequest = h.useAsync((signal) => deps.api.listPrograms(signal));
      const gameReady = h.useComputed(() => {
        const current = snapshot.get();
        return current !== undefined && current.characterName !== null;
      });
      const canIssueCommand = h.useComputed(() => {
        const current = snapshot.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          !commandBusy.get() &&
          programRequest.state.get().status === 'success'
        );
      });

      const requestStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const commandStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const refresh = el('button', { type: 'button' }, '복지 프로그램 다시 조회');
      const slots = Array.from({ length: MAX_PROGRAMS }, createProgramSlot);
      const programList = el('ol', {}, ...slots.map((slot) => slot.element));

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '복지 프로그램'),
          el(
            'p',
            {},
            '자격과 지급액은 서버가 현재 실행에 고정된 규칙으로 판정합니다. 이 화면은 서버 결과만 표시합니다.',
          ),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          requestStatus,
          commandStatus,
          refresh,
          programList,
        ),
      );

      h.bindText(requestStatus, () =>
        programRequestText(programRequest.state.get(), gameReady.get()),
      );
      h.bindText(commandStatus, () => commandFeedback.get());
      h.bindAttribute(
        refresh,
        'disabled',
        () => !gameReady.get() || programRequest.state.get().status === 'loading',
      );

      for (const [programIndex, slot] of slots.entries()) {
        h.bindAttribute(
          slot.element,
          'hidden',
          () => programAt(programs.get(), programIndex) === undefined,
        );
        h.bindText(slot.name, () => programAt(programs.get(), programIndex)?.displayName ?? '');
        h.bindText(slot.benefit, () => benefitText(programAt(programs.get(), programIndex)));
        h.bindText(slot.schedule, () => scheduleText(programAt(programs.get(), programIndex)));
        h.bindText(slot.evaluation, () => evaluationText(programAt(programs.get(), programIndex)));
        h.bindText(slot.fingerprint, () =>
          fingerprintText(programAt(programs.get(), programIndex)),
        );
        h.bindText(slot.availability, () =>
          availabilityText(programAt(programs.get(), programIndex)),
        );
        h.bindText(slot.application, () =>
          applicationText(programAt(programs.get(), programIndex)),
        );
        h.bindText(slot.payment, () => paymentText(programAt(programs.get(), programIndex)));
        h.bindAttribute(slot.apply, 'disabled', () => {
          const program = programAt(programs.get(), programIndex);
          return (
            program === undefined ||
            !program.applicationAvailable ||
            program.evaluationStatus !== 'eligible' ||
            !canIssueCommand.get()
          );
        });
        for (const [conditionIndex, conditionSlot] of slot.conditions.entries()) {
          h.bindAttribute(conditionSlot.element, 'hidden', () => {
            const condition = programAt(programs.get(), programIndex)?.conditions[conditionIndex];
            return condition === undefined;
          });
          h.bindText(conditionSlot.text, () => {
            const condition = programAt(programs.get(), programIndex)?.conditions[conditionIndex];
            return condition === undefined
              ? ''
              : `${condition.label}: ${CONDITION_OUTCOME_LABEL[condition.outcome]}`;
          });
        }
        h.useEventListener(slot.apply, 'click', () => {
          const program = programAt(programs.peek(), programIndex);
          if (program === undefined) return;
          void submitApplication(program).catch((error: unknown) => {
            commandFeedback.set(welfareDisplayError(error));
          });
        });
      }

      h.useEffect(() => {
        const state = programRequest.state.get();
        if (state.status === 'success') programs.set(state.value);
      });
      h.useWatch(snapshot, (next, previous) => {
        if (next === undefined || next.characterName === null) {
          programRequest.cancel();
          programs.set(undefined);
          return;
        }
        if (
          previous === undefined ||
          next.runRevision !== previous.runRevision ||
          next.stateRevision !== previous.stateRevision
        ) {
          programRequest.run();
        }
      });
      h.useEventListener(refresh, 'click', () => {
        if (gameReady.peek()) programRequest.run();
      });

      if (gameReady.peek()) programRequest.run();

      async function submitApplication(program: WelfareProgram): Promise<void> {
        if (
          !canIssueCommand.peek() ||
          !program.applicationAvailable ||
          program.evaluationStatus !== 'eligible'
        ) {
          return;
        }
        const current = commandSnapshot(deps);
        const request = retries.select(current, program.id);

        commandBusy.set(true);
        commandFeedback.set(`${program.displayName} 신청을 처리하는 중입니다.`);
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.apply(request);
          handleApplicationSuccess(program, request, response);
        } catch (error) {
          handleApplicationFailure(request, error);
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      function handleApplicationSuccess(
        program: WelfareProgram,
        request: WelfareApplicationRequest,
        response: WelfareApplicationResponse,
      ): void {
        retries.complete(request);
        deps.snapshots.apply(response.snapshot);
        commandFeedback.set(
          response.replayed
            ? `${program.displayName}의 기존 신청 결과를 확인했습니다.`
            : `${program.displayName} 신청이 승인되었습니다.`,
        );
        deps.toasts.show(`${program.displayName} 신청이 승인되었습니다.`, { tone: 'success' });
      }

      function handleApplicationFailure(request: WelfareApplicationRequest, error: unknown): void {
        retries.fail(request, error);
        commandFeedback.set(welfareDisplayError(error));
        if (error instanceof WelfareCommandError) programRequest.run();
      }
    },
    unmount() {},
  });
}

function createProgramSlot(): WelfareProgramSlot {
  const conditions = Array.from({ length: MAX_CONDITIONS }, () => {
    const text = el('span');
    const element = el('li', {}, text);
    element.hidden = true;
    return { element, text };
  });
  const name = el('h2');
  const benefit = el('dd');
  const schedule = el('dd');
  const evaluation = el('dd');
  const fingerprint = el('dd');
  const availability = el('dd');
  const application = el('dd');
  const payment = el('dd');
  const apply = el('button', { type: 'button' }, '신청');
  const element = el(
    'li',
    {},
    el(
      'article',
      {},
      name,
      el(
        'dl',
        {},
        el('dt', {}, '정액 급여'),
        benefit,
        el('dt', {}, '지급 일정'),
        schedule,
        el('dt', {}, '현재 판정'),
        evaluation,
        el('dt', {}, '판정 fingerprint'),
        fingerprint,
        el('dt', {}, '신청 가능 여부'),
        availability,
        el('dt', {}, '최근 신청'),
        application,
        el('dt', {}, '다음 지급'),
        payment,
      ),
      el('h3', {}, '공개 판정 조건'),
      el('ul', {}, ...conditions.map((condition) => condition.element)),
      apply,
    ),
  );
  element.hidden = true;
  return {
    element,
    name,
    benefit,
    schedule,
    evaluation,
    fingerprint,
    availability,
    application,
    payment,
    conditions,
    apply,
  };
}

function programAt(
  response: WelfareProgramsResponse | undefined,
  index: number,
): WelfareProgram | undefined {
  return response?.programs[index];
}

function benefitText(program: WelfareProgram | undefined): string {
  return program === undefined ? '' : formatWon(program.benefitKrw);
}

function scheduleText(program: WelfareProgram | undefined): string {
  return program === undefined ? '' : `신청 후 ${program.paymentDelayGameDays}게임일 뒤 한 번 지급`;
}

function evaluationText(program: WelfareProgram | undefined): string {
  return program === undefined ? '' : EVALUATION_LABEL[program.evaluationStatus];
}

function fingerprintText(program: WelfareProgram | undefined): string {
  return program?.factFingerprint ?? '';
}

function availabilityText(program: WelfareProgram | undefined): string {
  if (program === undefined) return '';
  return program.applicationAvailable ? '신청 가능' : '신청할 수 없음';
}

function applicationText(program: WelfareProgram | undefined): string {
  const application = program?.latestApplication;
  if (application === null || application === undefined) return '신청 기록 없음';
  const approval =
    application.approvalGameDay === null ? '' : `, 승인 ${application.approvalGameDay}일차`;
  return `${APPLICATION_STATUS_LABEL[application.status]} · 신청 ${application.applicationGameDay}일차${approval} · 지급 ${formatWon(application.paidKrw)}`;
}

function paymentText(program: WelfareProgram | undefined): string {
  const payment = program?.nextPayment;
  if (payment === null || payment === undefined) return '예정된 지급 없음';
  return `${payment.paymentNo}회차 ${formatWon(payment.amountKrw)} · ${payment.dueGameDay}일차 · ${PAYMENT_STATUS_LABEL[payment.status]}`;
}

function programRequestText(
  state: AsyncState<WelfareProgramsResponse>,
  gameReady: boolean,
): string {
  if (!gameReady) return '캐릭터를 만든 뒤 복지 프로그램을 조회할 수 있습니다.';
  switch (state.status) {
    case 'idle':
      return '복지 프로그램을 조회할 수 있습니다.';
    case 'loading':
      return '복지 프로그램을 조회하는 중입니다.';
    case 'success':
      return state.value.programs.length === 0
        ? '현재 실행에 활성화된 복지 프로그램이 없습니다.'
        : `${state.value.gameDay}일차 기준 ${state.value.programs.length}개 프로그램을 표시합니다.`;
    case 'error':
      return welfareQueryErrorText(state.error);
  }
}

function commandSnapshot(deps: WelfareDeps): GameSnapshot {
  const snapshot = deps.store.getState().game.snapshot;
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error('복지를 신청하려면 먼저 캐릭터를 만들어야 합니다.');
  }
  if (snapshot.autoSpeed !== null) {
    throw new Error('자동 진행을 멈춘 뒤 복지를 신청해 주세요.');
  }
  return snapshot;
}

function welfareQueryErrorText(error: unknown): string {
  if (error instanceof WelfareQueryError) return error.message;
  return error instanceof Error
    ? `복지 프로그램을 조회하지 못했습니다: ${error.message}`
    : '복지 프로그램을 조회하지 못했습니다.';
}

function welfareDisplayError(error: unknown): string {
  if (error instanceof WelfareCommandError) return error.message;
  return '신청 결과를 확인하지 못했습니다. 같은 프로그램의 신청 버튼을 눌러 다시 확인해 주세요.';
}
