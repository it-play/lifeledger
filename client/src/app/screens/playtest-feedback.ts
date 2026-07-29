import {
  type PlaytestConsentUpdate,
  type PlaytestFeedback,
  type PlaytestFeedbackCategory,
  type PlaytestFeedbackDraft,
  PlaytestFeedbackDraftSchema,
  type PlaytestFeedbackOverview,
  type PlaytestFeedbackSeverity,
} from '../../api/contracts.js';
import { type PlaytestApi, PlaytestRequestError } from '../../api/playtest-api.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { el } from '../../lib/dom/index.js';
import { renderForm } from '../../lib/form/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { type AppState, paths } from '../state.js';

export interface PlaytestFeedbackDeps {
  readonly store: Store<AppState>;
  readonly api: PlaytestApi;
}

interface FeedbackSlot {
  readonly element: HTMLLIElement;
  readonly summary: HTMLParagraphElement;
  readonly message: HTMLParagraphElement;
  readonly evidence: HTMLPreElement;
  readonly remove: HTMLButtonElement;
}

const CATEGORY_LABEL: Readonly<Record<PlaytestFeedbackCategory, string>> = {
  bug: '버그',
  balance: '밸런스',
  usability: '사용성',
  performance: '성능',
  rules: '규칙',
  other: '기타',
};

const SEVERITY_LABEL: Readonly<Record<PlaytestFeedbackSeverity, string>> = {
  blocking: '진행 불가',
  major: '주요 문제',
  minor: '경미한 문제',
  suggestion: '제안',
};

const CONSENT_STATUS_LABEL = {
  notGranted: '동의하지 않음',
  granted: '현재 고지에 동의함',
  withdrawn: '동의 철회됨',
  policyChanged: '고지가 바뀌어 다시 동의해야 함',
} as const;

const FEEDBACK_FIELDS = [
  {
    name: 'category',
    label: '분류',
    kind: 'select',
    options: [
      { value: 'bug', label: '버그' },
      { value: 'balance', label: '밸런스' },
      { value: 'usability', label: '사용성' },
      { value: 'performance', label: '성능' },
      { value: 'rules', label: '규칙' },
      { value: 'other', label: '기타' },
    ],
  },
  {
    name: 'severity',
    label: '중요도',
    kind: 'select',
    options: [
      { value: 'blocking', label: '진행 불가' },
      { value: 'major', label: '주요 문제' },
      { value: 'minor', label: '경미한 문제' },
      { value: 'suggestion', label: '제안' },
    ],
  },
  {
    name: 'message',
    label: '피드백',
    kind: 'textarea',
    help: '500자 이내. 실제 재산·소득·건강·캐릭터 값과 원 단위 금액을 적지 마세요.',
  },
  {
    name: 'runRevision',
    label: '재현할 실행 revision(선택)',
    kind: 'number',
    help: '본인 실행만 선택할 수 있으며 manifest와 완료된 결산 hash는 서버가 직접 붙입니다.',
  },
  {
    name: 'privacyConfirmed',
    label: '개인정보와 실제 금융정보를 적지 않았음을 확인합니다',
    kind: 'checkbox',
  },
] as const;

const FEEDBACK_CAPACITY = 20;

/** Owner-only M5-F consent, submission, and deletion view. */
export function createPlaytestFeedbackView(deps: PlaytestFeedbackDeps): ViewFactory {
  return (): View => ({
    mount(host, ctx) {
      const h = createHooks(ctx.bag);
      const snapshot = h.useStoreValue(
        deps.store,
        paths.gameSnapshot,
        (state) => state.game.snapshot,
      );
      const overview = h.useSignal<PlaytestFeedbackOverview | undefined>(undefined);
      const commandBusy = h.useSignal(false);
      const commandStatus = h.useSignal('');
      const overviewRequest = h.useAsync(async (signal) => {
        const value = await deps.api.overview(signal);
        overview.set(value);
        return value;
      });
      const requestStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const actionStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const notice = el('p');
      const policyEvidence = el('pre');
      const consentStatus = el('p');
      const analyticsStatus = el('p');
      const grant = el('button', { type: 'button' }, '피드백 제출에 동의');
      const withdraw = el('button', { type: 'button' }, '동의 철회 및 활성 피드백 삭제');
      const refresh = el('button', { type: 'button' }, '동의와 피드백 다시 조회');
      const feedbackSlots = Array.from({ length: FEEDBACK_CAPACITY }, createFeedbackSlot);
      const feedbackList = el('ol', {}, ...feedbackSlots.map((slot) => slot.element));
      const form = renderForm<PlaytestFeedbackDraft>(
        {
          fields: FEEDBACK_FIELDS,
          validator: asFormValidator(PlaytestFeedbackDraftSchema),
          submitLabel: '피드백 제출',
          idPrefix: 'playtest-feedback',
        },
        {
          initial: {
            category: 'bug',
            severity: 'minor',
            message: '',
            ...(snapshot.peek() === undefined ? {} : { runRevision: snapshot.peek()?.runRevision }),
            privacyConfirmed: false,
          },
          onSubmit: submitFeedback,
        },
      );
      ctx.bag.add(form);

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '개발 플레이테스트 피드백'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          requestStatus,
          notice,
          policyEvidence,
          analyticsStatus,
          consentStatus,
          grant,
          withdraw,
          refresh,
          actionStatus,
          el('section', {}, el('h2', {}, '새 피드백'), form.element),
          el('section', {}, el('h2', {}, '활성 피드백'), feedbackList),
        ),
      );

      h.bindText(requestStatus, () => overviewRequestText(overviewRequest.state.get()));
      h.bindText(actionStatus, () => commandStatus.get());
      h.bindText(notice, () => overview.get()?.policy.noticeText ?? '고지문을 불러오는 중입니다.');
      h.bindText(policyEvidence, () => policyEvidenceText(overview.get()));
      h.bindText(analyticsStatus, () =>
        overview.get()?.policy.analyticsCollection === 'disabled' ? '사용 분석: 수집하지 않음' : '',
      );
      h.bindText(consentStatus, () => {
        const current = overview.get()?.consent;
        return current === undefined
          ? '동의 상태: 조회 중'
          : `동의 상태: ${CONSENT_STATUS_LABEL[current.status]} · revision ${current.revision}`;
      });
      h.bindAttribute(grant, 'disabled', () => {
        const current = overview.get();
        return commandBusy.get() || current === undefined || current.consent.status === 'granted';
      });
      h.bindAttribute(withdraw, 'disabled', () => {
        const current = overview.get();
        return (
          commandBusy.get() ||
          current === undefined ||
          current.consent.status === 'notGranted' ||
          current.consent.status === 'withdrawn'
        );
      });
      h.bindAttribute(
        refresh,
        'disabled',
        () => commandBusy.get() || overviewRequest.state.get().status === 'loading',
      );
      h.bindAttribute(form.element, 'hidden', () => overview.get()?.consent.status !== 'granted');

      for (const [index, slot] of feedbackSlots.entries()) {
        h.bindAttribute(
          slot.element,
          'hidden',
          () => feedbackAt(overview.get(), index) === undefined,
        );
        h.bindText(slot.summary, () => feedbackSummary(feedbackAt(overview.get(), index)));
        h.bindText(slot.message, () => feedbackAt(overview.get(), index)?.message ?? '');
        h.bindText(slot.evidence, () => feedbackEvidence(feedbackAt(overview.get(), index)));
        h.bindAttribute(
          slot.remove,
          'disabled',
          () => commandBusy.get() || feedbackAt(overview.get(), index) === undefined,
        );
        h.useEventListener(slot.remove, 'click', () => {
          const feedback = feedbackAt(overview.peek(), index);
          if (feedback !== undefined) void deleteFeedback(feedback);
        });
      }

      h.useEventListener(grant, 'click', () => void changeConsent('grant'));
      h.useEventListener(withdraw, 'click', () => {
        if (
          globalThis.confirm(
            '동의를 철회하면 활성 피드백의 본문과 실행 hash가 즉시 삭제됩니다. 계속할까요?',
          )
        ) {
          void changeConsent('withdraw');
        }
      });
      h.useEventListener(refresh, 'click', () => overviewRequest.run());
      overviewRequest.run();

      async function changeConsent(action: 'grant' | 'withdraw'): Promise<void> {
        const current = overview.peek();
        if (!canChangeConsent(current, commandBusy.peek())) return;

        commandBusy.set(true);
        commandStatus.set(consentPendingText(action));
        try {
          const update = await deps.api.setConsent({
            policyVersionId: current.policy.id,
            expectedRevision: current.consent.revision,
            action,
          });
          overview.set(applyConsentUpdate(current, update, action));
          commandStatus.set(consentSuccessText(update, action));
          overviewRequest.run();
        } catch (error) {
          commandStatus.set(playtestDisplayError(error));
          overviewRequest.run();
        } finally {
          commandBusy.set(false);
        }
      }

      async function submitFeedback(draft: PlaytestFeedbackDraft): Promise<void> {
        const current = overview.peek();
        if (current?.consent.status !== 'granted' || commandBusy.peek()) {
          throw new Error('현재 고지에 동의한 뒤 제출해 주세요.');
        }

        commandBusy.set(true);
        commandStatus.set(
          '피드백을 한 번 제출하는 중입니다. 결과를 알 수 없으면 목록을 먼저 새로 고칩니다.',
        );
        try {
          await deps.api.submitFeedback({
            ...draft,
            expectedConsentRevision: current.consent.revision,
          });
          form.reset();
          commandStatus.set('피드백을 저장했습니다.');
          overviewRequest.run();
        } catch (error) {
          commandStatus.set(playtestDisplayError(error));
          overviewRequest.run();
          throw error;
        } finally {
          commandBusy.set(false);
        }
      }

      async function deleteFeedback(feedback: PlaytestFeedback): Promise<void> {
        if (
          commandBusy.peek() ||
          !globalThis.confirm('이 피드백의 본문과 실행 hash를 삭제할까요?')
        ) {
          return;
        }

        commandBusy.set(true);
        commandStatus.set('피드백 내용을 삭제하는 중입니다.');
        try {
          await deps.api.deleteFeedback(feedback.id);
          commandStatus.set('피드백 내용을 삭제했습니다.');
          overviewRequest.run();
        } catch (error) {
          commandStatus.set(playtestDisplayError(error));
          overviewRequest.run();
        } finally {
          commandBusy.set(false);
        }
      }
    },
    unmount() {},
  });
}

function canChangeConsent(
  overview: PlaytestFeedbackOverview | undefined,
  commandBusy: boolean,
): overview is PlaytestFeedbackOverview {
  return overview !== undefined && !commandBusy;
}

function consentPendingText(action: 'grant' | 'withdraw'): string {
  return action === 'grant' ? '동의를 저장하는 중입니다.' : '동의를 철회하는 중입니다.';
}

function applyConsentUpdate(
  overview: PlaytestFeedbackOverview,
  update: PlaytestConsentUpdate,
  action: 'grant' | 'withdraw',
): PlaytestFeedbackOverview {
  return {
    ...overview,
    consent: update.consent,
    feedback: action === 'withdraw' ? [] : overview.feedback,
  };
}

function consentSuccessText(update: PlaytestConsentUpdate, action: 'grant' | 'withdraw'): string {
  return action === 'grant'
    ? '현재 고지에 동의했습니다.'
    : `동의를 철회했고 활성 피드백 ${update.purgedFeedbackCount}건의 내용을 삭제했습니다.`;
}

function createFeedbackSlot(): FeedbackSlot {
  const summary = el('p');
  const message = el('p');
  const evidence = el('pre');
  const remove = el('button', { type: 'button' }, '이 피드백 삭제');
  return {
    element: el('li', { attrs: { hidden: '' } }, summary, message, evidence, remove),
    summary,
    message,
    evidence,
    remove,
  };
}

function overviewRequestText(state: AsyncState<PlaytestFeedbackOverview>): string {
  switch (state.status) {
    case 'idle':
      return '동의와 피드백을 아직 조회하지 않았습니다.';
    case 'loading':
      return '동의와 피드백을 조회하는 중입니다.';
    case 'success':
      return `활성 피드백 ${state.value.feedback.length}/${state.value.policy.maximumActiveFeedback}건`;
    case 'error':
      return playtestDisplayError(state.error);
  }
}

function policyEvidenceText(overview: PlaytestFeedbackOverview | undefined): string {
  if (overview === undefined) return '';
  return [
    `policy: ${overview.policy.policyKey} v${overview.policy.version}`,
    `policy sha256: ${overview.policy.canonicalSha256}`,
    `보존: 제출 후 최대 ${String(overview.policy.retentionMaximumDays)}일, 철회·삭제 시 즉시 내용 삭제`,
  ].join('\n');
}

function feedbackAt(
  overview: PlaytestFeedbackOverview | undefined,
  index: number,
): PlaytestFeedback | undefined {
  return overview?.feedback[index];
}

function feedbackSummary(feedback: PlaytestFeedback | undefined): string {
  if (feedback === undefined) return '';
  return `${CATEGORY_LABEL[feedback.category]} · ${SEVERITY_LABEL[feedback.severity]} · ${feedback.createdAt}`;
}

function feedbackEvidence(feedback: PlaytestFeedback | undefined): string {
  if (feedback === undefined || feedback.runRevision === null) return '실행 증거를 첨부하지 않음';
  return [
    `run revision: ${feedback.runRevision}`,
    `manifest sha256: ${feedback.runManifestSha256 ?? '없음'}`,
    `finalization sha256: ${feedback.finalizationSha256 ?? '아직 완료되지 않음'}`,
  ].join('\n');
}

function playtestDisplayError(error: unknown): string {
  if (error instanceof PlaytestRequestError) return error.message;
  if (error instanceof Error && error.name === 'AbortError') return '요청을 취소했습니다.';
  return '피드백 요청을 처리하지 못했습니다. 목록을 다시 조회해 주세요.';
}
