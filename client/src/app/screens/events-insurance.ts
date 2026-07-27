import type {
  GameSnapshot,
  InsuranceClaimHistoryItem,
  InsuranceContract,
  InsuranceContractsResponse,
  InsuranceEligibilityReason,
  InsuranceProduct,
  LifeEventChoice,
  LifeEventChoiceResponse,
  LifeEventHistoryItem,
  LifeEventsResponse,
  PendingInsuranceClaim,
  PendingLifeEvent,
} from '../../api/contracts.js';
import {
  type InsuranceApi,
  InsuranceCommandError,
  InsuranceQueryError,
} from '../../api/insurance-api.js';
import {
  type LifeEventApi,
  LifeEventCommandError,
  LifeEventQueryError,
} from '../../api/life-event-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks, type Hooks } from '../../lib/hooks/index.js';
import type { Signal } from '../../lib/reactive/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import { createInsuranceRetryPolicy } from '../insurance-retry/index.js';
import { createLifeEventChoiceRetryPolicy } from '../life-event-retry/index.js';
import { type AppState, paths } from '../state.js';

export interface EventsInsuranceDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly eventApi: LifeEventApi;
  readonly insuranceApi: InsuranceApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface ChoiceSlot {
  readonly button: HTMLButtonElement;
}

interface PendingEventSlot {
  readonly element: HTMLLIElement;
  readonly name: HTMLHeadingElement;
  readonly period: HTMLParagraphElement;
  readonly choices: readonly ChoiceSlot[];
}

interface EventHistorySlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
}

interface InsuranceProductSlot {
  readonly element: HTMLLIElement;
  readonly name: HTMLHeadingElement;
  readonly coverage: HTMLParagraphElement;
  readonly terms: HTMLParagraphElement;
  readonly eligibility: HTMLParagraphElement;
  readonly enroll: HTMLButtonElement;
}

interface InsuranceContractSlot {
  readonly element: HTMLLIElement;
  readonly name: HTMLHeadingElement;
  readonly coverage: HTMLParagraphElement;
  readonly premium: HTMLParagraphElement;
  readonly benefit: HTMLParagraphElement;
  readonly cancel: HTMLButtonElement;
}

interface InsuranceClaimSlot {
  readonly element: HTMLLIElement;
  readonly name: HTMLHeadingElement;
  readonly detail: HTMLParagraphElement;
  readonly allocations: HTMLParagraphElement;
  readonly file: HTMLButtonElement;
}

interface InsuranceHistorySlot {
  readonly element: HTMLLIElement;
  readonly text: HTMLSpanElement;
}

const MAX_PENDING_EVENTS = 8;
const MAX_EVENT_CHOICES = 8;
const MAX_EVENT_HISTORY = 20;
const MAX_INSURANCE_PRODUCTS = 16;
const MAX_INSURANCE_CONTRACTS = 20;
const MAX_PENDING_CLAIMS = 8;
const MAX_INSURANCE_HISTORY = 20;

/** M4-D3 deterministic event choices and server-authoritative insurance commands. */
export function createEventsInsuranceView(deps: EventsInsuranceDeps): ViewFactory {
  const eventRetries = createLifeEventChoiceRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const insuranceRetries = createInsuranceRetryPolicy({
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
      const events = h.useSignal<LifeEventsResponse | undefined>(undefined);
      const insurance = h.useSignal<InsuranceContractsResponse | undefined>(undefined);
      const insuranceCursor = h.useSignal<string | undefined>(undefined);
      const busyEventId = h.useSignal<string | undefined>(undefined);
      const busyInsuranceKey = h.useSignal<string | undefined>(undefined);
      const commandFeedback = h.useSignal('');
      const eventRequest = h.useAsync((signal) => deps.eventApi.list(undefined, signal));
      const insuranceRequest = h.useAsync((signal) => {
        const cursor = insuranceCursor.peek();
        return deps.insuranceApi.list(cursor === undefined ? undefined : { cursor }, signal);
      });
      const gameReady = h.useComputed(() => {
        const current = snapshot.get();
        return current !== undefined && current.characterName !== null;
      });
      const canChooseEvent = h.useComputed(() => {
        const current = snapshot.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          eventRequest.state.get().status === 'success'
        );
      });
      const canIssueInsuranceCommand = h.useComputed(() => {
        const current = snapshot.get();
        const response = insurance.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          busyInsuranceKey.get() === undefined &&
          insuranceRequest.state.get().status === 'success' &&
          response?.insuranceCapability === 'contractsAndClaims'
        );
      });

      const eventRequestStatus = el('p', {
        attrs: { role: 'status', 'aria-live': 'polite' },
      });
      const insuranceRequestStatus = el('p', {
        attrs: { role: 'status', 'aria-live': 'polite' },
      });
      const commandStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const refresh = el('button', { type: 'button' }, '전체 다시 조회');
      const eventCapability = el('dd');
      const insuranceCapability = el('dd');
      const pendingEventSlots = Array.from({ length: MAX_PENDING_EVENTS }, createPendingEventSlot);
      const eventHistorySlots = Array.from({ length: MAX_EVENT_HISTORY }, createEventHistorySlot);
      const productSlots = Array.from(
        { length: MAX_INSURANCE_PRODUCTS },
        createInsuranceProductSlot,
      );
      const contractSlots = Array.from(
        { length: MAX_INSURANCE_CONTRACTS },
        createInsuranceContractSlot,
      );
      const claimSlots = Array.from({ length: MAX_PENDING_CLAIMS }, createInsuranceClaimSlot);
      const insuranceHistorySlots = Array.from(
        { length: MAX_INSURANCE_HISTORY },
        createInsuranceHistorySlot,
      );
      const nextInsurancePage = el('button', { type: 'button' }, '다음 보험 기록');

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '생애 사건과 보험'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          eventRequestStatus,
          insuranceRequestStatus,
          commandStatus,
          refresh,
          el(
            'section',
            {},
            el('h2', {}, '생애 사건'),
            el('dl', {}, el('dt', {}, '현재 기능'), eventCapability),
            el('h3', {}, '선택 대기 사건'),
            el('ol', {}, ...pendingEventSlots.map((slot) => slot.element)),
            el('h3', {}, '최근 해결 기록'),
            el('ol', {}, ...eventHistorySlots.map((slot) => slot.element)),
          ),
          el(
            'section',
            {},
            el('h2', {}, '보험'),
            el('dl', {}, el('dt', {}, '현재 기능'), insuranceCapability),
            el('h3', {}, '가입 가능 상품'),
            el('ol', {}, ...productSlots.map((slot) => slot.element)),
            el('h3', {}, '보험 계약'),
            el('ol', {}, ...contractSlots.map((slot) => slot.element)),
            el('h3', {}, '청구 대기'),
            el('ol', {}, ...claimSlots.map((slot) => slot.element)),
            el('h3', {}, '보험금 청구 기록'),
            el('ol', {}, ...insuranceHistorySlots.map((slot) => slot.element)),
            nextInsurancePage,
          ),
        ),
      );

      h.bindText(eventRequestStatus, () =>
        eventRequestText(eventRequest.state.get(), gameReady.get()),
      );
      h.bindText(insuranceRequestStatus, () =>
        insuranceRequestText(insuranceRequest.state.get(), gameReady.get()),
      );
      h.bindText(commandStatus, () => commandFeedback.get());
      h.bindText(eventCapability, () => eventCapabilityText(events.get()));
      h.bindText(insuranceCapability, () => insuranceCapabilityText(insurance.get()));
      h.bindAttribute(
        refresh,
        'disabled',
        () =>
          !gameReady.get() ||
          eventRequest.state.get().status === 'loading' ||
          insuranceRequest.state.get().status === 'loading' ||
          busyInsuranceKey.get() !== undefined ||
          busyEventId.get() !== undefined,
      );
      h.bindAttribute(
        nextInsurancePage,
        'hidden',
        () => insurance.get()?.nextCursor === null || insurance.get() === undefined,
      );
      h.bindAttribute(
        nextInsurancePage,
        'disabled',
        () =>
          insurance.get()?.nextCursor === null ||
          insuranceRequest.state.get().status === 'loading' ||
          !gameReady.get() ||
          busyInsuranceKey.get() !== undefined,
      );

      bindPendingEventSlots({
        h,
        slots: pendingEventSlots,
        events,
        canChooseEvent,
        busyEventId,
        busyInsuranceKey,
        onChoose(event, choice) {
          void submitEventChoice(event, choice).catch((error: unknown) => {
            commandFeedback.set(lifeEventDisplayError(error));
          });
        },
      });
      bindEventHistorySlots(h, eventHistorySlots, events);
      bindInsuranceProductSlots({
        h,
        slots: productSlots,
        insurance,
        snapshot,
        canIssueInsuranceCommand,
        busyEventId,
        onEnroll(product) {
          void enrollInsurance(product).catch((error: unknown) => {
            commandFeedback.set(insuranceDisplayError(error, '가입'));
          });
        },
      });
      bindInsuranceContractSlots({
        h,
        slots: contractSlots,
        insurance,
        canIssueInsuranceCommand,
        busyEventId,
        onCancel(contract) {
          void cancelInsurance(contract).catch((error: unknown) => {
            commandFeedback.set(insuranceDisplayError(error, '취소'));
          });
        },
      });
      bindInsuranceClaimSlots({
        h,
        slots: claimSlots,
        insurance,
        canIssueInsuranceCommand,
        busyEventId,
        onFile(claim) {
          void fileInsuranceClaim(claim).catch((error: unknown) => {
            commandFeedback.set(insuranceDisplayError(error, '청구'));
          });
        },
      });
      bindInsuranceHistorySlots(h, insuranceHistorySlots, insurance);

      h.useEffect(() => {
        const state = eventRequest.state.get();
        if (state.status === 'success') events.set(state.value);
      });
      h.useEffect(() => {
        const state = insuranceRequest.state.get();
        if (state.status === 'success') insurance.set(state.value);
      });
      h.useWatch(snapshot, (next, previous) => {
        if (next === undefined || next.characterName === null) {
          eventRequest.cancel();
          insuranceRequest.cancel();
          events.set(undefined);
          insurance.set(undefined);
          insuranceCursor.set(undefined);
          return;
        }
        if (
          previous === undefined ||
          next.runRevision !== previous.runRevision ||
          next.stateRevision !== previous.stateRevision
        ) {
          insuranceCursor.set(undefined);
          eventRequest.run();
          insuranceRequest.run();
        }
      });
      h.useEventListener(refresh, 'click', () => {
        if (!gameReady.peek()) return;
        insuranceCursor.set(undefined);
        eventRequest.run();
        insuranceRequest.run();
      });
      h.useEventListener(nextInsurancePage, 'click', () => {
        const nextCursor = insurance.peek()?.nextCursor;
        if (nextCursor === undefined || nextCursor === null) return;
        insuranceCursor.set(nextCursor);
        insuranceRequest.run();
      });
      if (gameReady.peek()) {
        eventRequest.run();
        insuranceRequest.run();
      }

      async function submitEventChoice(
        event: PendingLifeEvent,
        choice: LifeEventChoice,
      ): Promise<void> {
        if (!canChooseEvent.peek() || busyEventId.peek() === event.id) return;
        const current = commandSnapshot(deps);
        const command = eventRetries.select(current, event.id, choice.id);

        busyEventId.set(event.id);
        commandFeedback.set(`${event.displayName} 선택을 처리하는 중입니다.`);
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.eventApi.choose(command.eventId, command.request);
          handleEventChoiceSuccess(event, command, response);
        } catch (error) {
          eventRetries.fail(command, error);
          commandFeedback.set(lifeEventDisplayError(error));
          if (error instanceof LifeEventCommandError) eventRequest.run();
        } finally {
          deps.store.set(paths.gameOrdering, false);
          busyEventId.set(undefined);
        }
      }

      function handleEventChoiceSuccess(
        event: PendingLifeEvent,
        command: ReturnType<typeof eventRetries.select>,
        response: LifeEventChoiceResponse,
      ): void {
        eventRetries.complete(command);
        deps.snapshots.apply(response.snapshot);
        const message = response.replayed
          ? `${event.displayName}의 기존 선택 결과를 확인했습니다.`
          : `${event.displayName} 선택을 반영했습니다.`;
        commandFeedback.set(message);
        deps.toasts.show(message, { tone: 'success' });
      }

      async function enrollInsurance(product: InsuranceProduct): Promise<void> {
        if (!canIssueInsuranceCommand.peek() || hasActiveContract(snapshot.peek(), product.id)) {
          return;
        }
        const command = insuranceRetries.enroll(commandSnapshot(deps), product.id);
        await runInsuranceCommand(
          `enrollment:${product.id}`,
          `${product.displayName} 가입`,
          command,
          () => deps.insuranceApi.enroll(command.request),
        );
      }

      async function cancelInsurance(contract: InsuranceContract): Promise<void> {
        if (!canIssueInsuranceCommand.peek() || contract.status !== 'active') return;
        const command = insuranceRetries.cancel(commandSnapshot(deps), contract.id);
        await runInsuranceCommand(
          `cancellation:${contract.id}`,
          `${contract.displayName} 취소`,
          command,
          () => deps.insuranceApi.cancel(command.contractId, command.request),
        );
      }

      async function fileInsuranceClaim(claim: PendingInsuranceClaim): Promise<void> {
        if (!canIssueInsuranceCommand.peek() || claim.status !== 'ready') return;
        const command = insuranceRetries.claim(commandSnapshot(deps), claim.id);
        await runInsuranceCommand(
          `claim:${claim.id}`,
          `${claim.eventDisplayName} 보험금 청구`,
          command,
          () => deps.insuranceApi.fileClaim(command.request),
        );
      }

      async function runInsuranceCommand<T>(
        key: string,
        label: string,
        command: Parameters<typeof insuranceRetries.complete>[0],
        request: () => Promise<T & { readonly replayed: boolean; readonly snapshot: GameSnapshot }>,
      ): Promise<void> {
        busyInsuranceKey.set(key);
        commandFeedback.set(`${label} 처리 중입니다.`);
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await request();
          insuranceRetries.complete(command);
          deps.snapshots.apply(response.snapshot);
          insuranceCursor.set(undefined);
          insuranceRequest.run();
          const message = insuranceSuccessMessage(label, response.replayed);
          commandFeedback.set(message);
          deps.toasts.show(message, { tone: 'success' });
        } catch (error) {
          handleInsuranceFailure(command, error, label);
        } finally {
          deps.store.set(paths.gameOrdering, false);
          busyInsuranceKey.set(undefined);
        }
      }

      function handleInsuranceFailure(
        command: Parameters<typeof insuranceRetries.fail>[0],
        error: unknown,
        label: string,
      ): void {
        insuranceRetries.fail(command, error);
        commandFeedback.set(insuranceDisplayError(error, label));
        if (error instanceof InsuranceCommandError) insuranceRequest.run();
      }
    },
    unmount() {},
  });
}

function bindPendingEventSlots(options: {
  readonly h: Hooks;
  readonly slots: readonly PendingEventSlot[];
  readonly events: Signal<LifeEventsResponse | undefined>;
  readonly canChooseEvent: Signal<boolean>;
  readonly busyEventId: Signal<string | undefined>;
  readonly busyInsuranceKey: Signal<string | undefined>;
  readonly onChoose: (event: PendingLifeEvent, choice: LifeEventChoice) => void;
}): void {
  const { h, slots, events, canChooseEvent, busyEventId, busyInsuranceKey, onChoose } = options;
  for (const [eventIndex, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => pendingEventAt(events.get(), eventIndex) === undefined,
    );
    h.bindText(slot.name, () => pendingEventAt(events.get(), eventIndex)?.displayName ?? '');
    h.bindText(slot.period, () => pendingEventPeriodText(pendingEventAt(events.get(), eventIndex)));
    for (const [choiceIndex, choiceSlot] of slot.choices.entries()) {
      h.bindAttribute(
        choiceSlot.button,
        'hidden',
        () => eventChoiceAt(events.get(), eventIndex, choiceIndex) === undefined,
      );
      h.bindText(choiceSlot.button, () =>
        eventChoiceText(eventChoiceAt(events.get(), eventIndex, choiceIndex)),
      );
      h.bindAttribute(choiceSlot.button, 'disabled', () => {
        const event = pendingEventAt(events.get(), eventIndex);
        return (
          event === undefined ||
          eventChoiceAt(events.get(), eventIndex, choiceIndex) === undefined ||
          !canChooseEvent.get() ||
          busyEventId.get() === event.id ||
          busyInsuranceKey.get() !== undefined
        );
      });
      h.useEventListener(choiceSlot.button, 'click', () => {
        const event = pendingEventAt(events.peek(), eventIndex);
        const choice = eventChoiceAt(events.peek(), eventIndex, choiceIndex);
        if (event !== undefined && choice !== undefined) onChoose(event, choice);
      });
    }
  }
}

function bindEventHistorySlots(
  h: Hooks,
  slots: readonly EventHistorySlot[],
  events: Signal<LifeEventsResponse | undefined>,
): void {
  for (const [index, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => eventHistoryAt(events.get(), index) === undefined,
    );
    h.bindText(slot.text, () => eventHistoryText(eventHistoryAt(events.get(), index)));
  }
}

function bindInsuranceProductSlots(options: {
  readonly h: Hooks;
  readonly slots: readonly InsuranceProductSlot[];
  readonly insurance: Signal<InsuranceContractsResponse | undefined>;
  readonly snapshot: Signal<GameSnapshot | undefined>;
  readonly canIssueInsuranceCommand: Signal<boolean>;
  readonly busyEventId: Signal<string | undefined>;
  readonly onEnroll: (product: InsuranceProduct) => void;
}): void {
  const { h, slots, insurance, snapshot, canIssueInsuranceCommand, busyEventId, onEnroll } =
    options;
  for (const [index, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => insuranceProductAt(insurance.get(), index) === undefined,
    );
    h.bindText(slot.name, () => insuranceProductName(insuranceProductAt(insurance.get(), index)));
    h.bindText(slot.coverage, () =>
      insuranceProductCoverageText(insuranceProductAt(insurance.get(), index)),
    );
    h.bindText(slot.terms, () =>
      insuranceProductTermsText(insuranceProductAt(insurance.get(), index)),
    );
    h.bindText(slot.eligibility, () =>
      insuranceProductEligibilityText(insuranceProductAt(insurance.get(), index)),
    );
    h.bindAttribute(slot.enroll, 'disabled', () => {
      const product = insuranceProductAt(insurance.get(), index);
      return (
        product === undefined ||
        product.eligibilityStatus !== 'eligible' ||
        hasActiveContract(snapshot.get(), product.id) ||
        !canIssueInsuranceCommand.get() ||
        busyEventId.get() !== undefined
      );
    });
    h.useEventListener(slot.enroll, 'click', () => {
      const product = insuranceProductAt(insurance.peek(), index);
      if (product !== undefined) onEnroll(product);
    });
  }
}

function bindInsuranceContractSlots(options: {
  readonly h: Hooks;
  readonly slots: readonly InsuranceContractSlot[];
  readonly insurance: Signal<InsuranceContractsResponse | undefined>;
  readonly canIssueInsuranceCommand: Signal<boolean>;
  readonly busyEventId: Signal<string | undefined>;
  readonly onCancel: (contract: InsuranceContract) => void;
}): void {
  const { h, slots, insurance, canIssueInsuranceCommand, busyEventId, onCancel } = options;
  for (const [index, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => insuranceContractAt(insurance.get(), index) === undefined,
    );
    h.bindText(slot.name, () => insuranceContractName(insuranceContractAt(insurance.get(), index)));
    h.bindText(slot.coverage, () =>
      insuranceContractCoverageText(insuranceContractAt(insurance.get(), index)),
    );
    h.bindText(slot.premium, () =>
      insuranceContractPremiumText(insuranceContractAt(insurance.get(), index)),
    );
    h.bindText(slot.benefit, () =>
      insuranceContractBenefitText(insuranceContractAt(insurance.get(), index)),
    );
    h.bindAttribute(slot.cancel, 'disabled', () => {
      const contract = insuranceContractAt(insurance.get(), index);
      return (
        contract === undefined ||
        contract.status !== 'active' ||
        !canIssueInsuranceCommand.get() ||
        busyEventId.get() !== undefined
      );
    });
    h.useEventListener(slot.cancel, 'click', () => {
      const contract = insuranceContractAt(insurance.peek(), index);
      if (contract !== undefined) onCancel(contract);
    });
  }
}

function bindInsuranceClaimSlots(options: {
  readonly h: Hooks;
  readonly slots: readonly InsuranceClaimSlot[];
  readonly insurance: Signal<InsuranceContractsResponse | undefined>;
  readonly canIssueInsuranceCommand: Signal<boolean>;
  readonly busyEventId: Signal<string | undefined>;
  readonly onFile: (claim: PendingInsuranceClaim) => void;
}): void {
  const { h, slots, insurance, canIssueInsuranceCommand, busyEventId, onFile } = options;
  for (const [index, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => pendingClaimAt(insurance.get(), index) === undefined,
    );
    h.bindText(slot.name, () => pendingClaimName(pendingClaimAt(insurance.get(), index)));
    h.bindText(slot.detail, () => pendingClaimDetail(pendingClaimAt(insurance.get(), index)));
    h.bindText(slot.allocations, () =>
      pendingClaimAllocations(pendingClaimAt(insurance.get(), index)),
    );
    h.bindAttribute(
      slot.file,
      'hidden',
      () => pendingClaimAt(insurance.get(), index)?.status !== 'ready',
    );
    h.bindAttribute(slot.file, 'disabled', () => {
      const claim = pendingClaimAt(insurance.get(), index);
      return (
        claim?.status !== 'ready' ||
        !canIssueInsuranceCommand.get() ||
        busyEventId.get() !== undefined
      );
    });
    h.useEventListener(slot.file, 'click', () => {
      const claim = pendingClaimAt(insurance.peek(), index);
      if (claim?.status === 'ready') onFile(claim);
    });
  }
}

function bindInsuranceHistorySlots(
  h: Hooks,
  slots: readonly InsuranceHistorySlot[],
  insurance: Signal<InsuranceContractsResponse | undefined>,
): void {
  for (const [index, slot] of slots.entries()) {
    h.bindAttribute(
      slot.element,
      'hidden',
      () => insuranceHistoryAt(insurance.get(), index) === undefined,
    );
    h.bindText(slot.text, () => insuranceHistoryText(insuranceHistoryAt(insurance.get(), index)));
  }
}

function createPendingEventSlot(): PendingEventSlot {
  const name = el('h4');
  const period = el('p');
  const choices = Array.from({ length: MAX_EVENT_CHOICES }, () => ({
    button: el('button', { type: 'button' }),
  }));
  const element = el(
    'li',
    {},
    el('article', {}, name, period, el('div', {}, ...choices.map((choice) => choice.button))),
  );
  element.hidden = true;
  for (const choice of choices) choice.button.hidden = true;
  return { element, name, period, choices };
}

function createEventHistorySlot(): EventHistorySlot {
  const text = el('span');
  const element = el('li', {}, text);
  element.hidden = true;
  return { element, text };
}

function createInsuranceProductSlot(): InsuranceProductSlot {
  const name = el('h4');
  const coverage = el('p');
  const terms = el('p');
  const eligibility = el('p');
  const enroll = el('button', { type: 'button' }, '가입');
  const element = el('li', {}, el('article', {}, name, coverage, terms, eligibility, enroll));
  element.hidden = true;
  return { element, name, coverage, terms, eligibility, enroll };
}

function createInsuranceContractSlot(): InsuranceContractSlot {
  const name = el('h4');
  const coverage = el('p');
  const premium = el('p');
  const benefit = el('p');
  const cancel = el('button', { type: 'button' }, '중도 취소');
  const element = el('li', {}, el('article', {}, name, coverage, premium, benefit, cancel));
  element.hidden = true;
  return { element, name, coverage, premium, benefit, cancel };
}

function createInsuranceClaimSlot(): InsuranceClaimSlot {
  const name = el('h4');
  const detail = el('p');
  const allocations = el('p');
  const file = el('button', { type: 'button' }, '보험금 청구');
  const element = el('li', {}, el('article', {}, name, detail, allocations, file));
  element.hidden = true;
  file.hidden = true;
  return { element, name, detail, allocations, file };
}

function createInsuranceHistorySlot(): InsuranceHistorySlot {
  const text = el('span');
  const element = el('li', {}, text);
  element.hidden = true;
  return { element, text };
}

function pendingEventAt(
  response: LifeEventsResponse | undefined,
  index: number,
): PendingLifeEvent | undefined {
  return response?.pendingEvents[index];
}

function eventChoiceAt(
  response: LifeEventsResponse | undefined,
  eventIndex: number,
  choiceIndex: number,
): LifeEventChoice | undefined {
  return pendingEventAt(response, eventIndex)?.choices[choiceIndex];
}

function eventHistoryAt(
  response: LifeEventsResponse | undefined,
  index: number,
): LifeEventHistoryItem | undefined {
  return response?.history[index];
}

function insuranceProductAt(
  response: InsuranceContractsResponse | undefined,
  index: number,
): InsuranceProduct | undefined {
  return response?.products[index];
}

function insuranceContractAt(
  response: InsuranceContractsResponse | undefined,
  index: number,
): InsuranceContract | undefined {
  return response?.contracts[index];
}

function pendingClaimAt(
  response: InsuranceContractsResponse | undefined,
  index: number,
): PendingInsuranceClaim | undefined {
  return response?.pendingClaims[index];
}

function insuranceHistoryAt(
  response: InsuranceContractsResponse | undefined,
  index: number,
): InsuranceClaimHistoryItem | undefined {
  return response?.history[index];
}

function eventCapabilityText(response: LifeEventsResponse | undefined): string {
  if (response === undefined) return '';
  return response.lifeEventCapability === 'deterministicChoices'
    ? '결정론적 선택 사건 사용 가능'
    : '이 실행에서는 생애 사건을 이용할 수 없음';
}

function insuranceCapabilityText(response: InsuranceContractsResponse | undefined): string {
  if (response === undefined) return '';
  return response.insuranceCapability === 'contractsAndClaims'
    ? '보험 가입·취소·보험금 청구 사용 가능'
    : '이 실행에서는 보험을 이용할 수 없음';
}

function pendingEventPeriodText(event: PendingLifeEvent | undefined): string {
  return event === undefined
    ? ''
    : `${event.offeredGameDay}일차에 제안 · ${event.expiresGameDay}일차 시작 전에 선택`;
}

function eventChoiceText(choice: LifeEventChoice | undefined): string {
  if (choice === undefined) return '';
  const effect =
    choice.effectSummary.kind === 'noEffect'
      ? '금전 효과 없음'
      : `지갑에서 ${formatWon(choice.effectSummary.amountKrw)} 지출`;
  return `${choice.displayName} · ${effect}`;
}

function eventHistoryText(event: LifeEventHistoryItem | undefined): string {
  if (event === undefined) return '';
  const resolution =
    event.resolutionKind === 'accepted'
      ? '수락'
      : event.resolutionKind === 'declined'
        ? '거절'
        : '기한 만료';
  return `${event.displayName} · ${resolution} · ${event.choice.displayName} · ${event.resolvedGameDay}일차`;
}

function insuranceProductName(product: InsuranceProduct | undefined): string {
  return product === undefined ? '' : `#${product.id} ${product.displayName}`;
}

function insuranceProductCoverageText(product: InsuranceProduct | undefined): string {
  return product === undefined ? '' : `보장: ${product.coveredEventDisplayName}`;
}

function insuranceProductTermsText(product: InsuranceProduct | undefined): string {
  if (product === undefined) return '';
  return `보험료 ${formatWon(product.premiumKrw)} / ${product.premiumIntervalGameDays}일 · 기간 ${product.termGameDays}일 · 대기 ${product.waitingPeriodGameDays}일 · 공제 ${formatWon(product.deductibleKrw)} · 1회 ${formatWon(product.occurrenceLimitKrw)} · 총 ${formatWon(product.termLimitKrw)} · 청구 ${product.claimWindowGameDays}일`;
}

function insuranceProductEligibilityText(product: InsuranceProduct | undefined): string {
  if (product === undefined) return '';
  const status = {
    eligible: '가입 가능',
    ineligible: '가입 불가',
    indeterminate: '자격 확인 불가',
  }[product.eligibilityStatus];
  const reasons = product.reasons.map(insuranceEligibilityReasonText).join(', ');
  return reasons.length === 0 ? status : `${status}: ${reasons}`;
}

function insuranceEligibilityReasonText(reason: InsuranceEligibilityReason): string {
  const labels: Record<InsuranceEligibilityReason, string> = {
    ageOutsideRange: '가입 연령 범위 밖',
    dependentRequired: '부양가족 필요',
    residenceRequired: '현재 거주지 필요',
    militaryServing: '복무 중 가입 불가',
    authorityUnavailable: '판정 정보 확인 불가',
  };
  return labels[reason];
}

function insuranceContractName(contract: InsuranceContract | undefined): string {
  return contract === undefined
    ? ''
    : `#${contract.id} ${contract.displayName} · ${insuranceContractStatusText(contract.status)}`;
}

function insuranceContractStatusText(status: InsuranceContract['status']): string {
  const labels: Record<InsuranceContract['status'], string> = {
    active: '유지 중',
    lapsed: '해지',
    expired: '만기',
    cancelled: '중도 취소',
  };
  return labels[status];
}

function insuranceContractCoverageText(contract: InsuranceContract | undefined): string {
  if (contract === undefined) return '';
  return `보장 ${contract.coverageStartGameDay}일차부터 ${contract.coverageEndExclusive}일차 전까지 · 대기 종료 ${contract.waitingEndsGameDay}일차`;
}

function insuranceContractPremiumText(contract: InsuranceContract | undefined): string {
  if (contract === undefined) return '';
  return contract.nextPremiumDueGameDay === null
    ? `보험료 ${formatWon(contract.premiumKrw)} · 다음 납부 없음`
    : `보험료 ${formatWon(contract.premiumKrw)} · 다음 납부 ${contract.nextPremiumDueGameDay}일차`;
}

function insuranceContractBenefitText(contract: InsuranceContract | undefined): string {
  if (contract === undefined) return '';
  return `지급 ${formatWon(contract.paidBenefitKrw)} · 예약 ${formatWon(contract.reservedBenefitKrw)} · 남은 한도 ${formatWon(contract.remainingBenefitKrw)}`;
}

function pendingClaimName(claim: PendingInsuranceClaim | undefined): string {
  if (claim === undefined) return '';
  return `#${claim.id} ${claim.eventDisplayName} · ${claim.status === 'candidate' ? '선택 대기' : '청구 가능'}`;
}

function pendingClaimDetail(claim: PendingInsuranceClaim | undefined): string {
  if (claim === undefined) return '';
  if (claim.status === 'candidate') {
    return `${claim.offeredGameDay}일차 발생 · 손해액과 지급액 확정 전`;
  }
  return `${claim.offeredGameDay}일차 발생 · 손해 ${formatWon(claim.grossCostKrw)} · 지급 ${formatWon(claim.payoutKrw)} · ${claim.filingDeadlineGameDay}일차 시작 전에 청구`;
}

function pendingClaimAllocations(claim: PendingInsuranceClaim | undefined): string {
  if (claim?.status !== 'ready') return '';
  return claim.contractAllocations
    .map(
      (allocation) =>
        `계약 #${allocation.contractId} 공제 ${formatWon(allocation.deductibleKrw)} · 배분 ${formatWon(allocation.payoutKrw)}`,
    )
    .join(' / ');
}

function insuranceHistoryText(claim: InsuranceClaimHistoryItem | undefined): string {
  if (claim === undefined) return '';
  const identity = `#${claim.id} ${claim.eventDisplayName} · ${claim.resolvedGameDay}일차 해결`;
  switch (claim.status) {
    case 'notApplicable':
      return `${identity} · 금전 효과 없음`;
    case 'notCovered':
      return `${identity} · 보장 없음 · 손해 ${formatWon(claim.grossCostKrw)}`;
    case 'paid':
      return `${identity} · ${claim.paidGameDay}일차 ${formatWon(claim.payoutKrw)} 지급`;
    case 'expired':
      return `${identity} · 청구 기한 만료 · 예정액 ${formatWon(claim.payoutKrw)}`;
  }
}

function eventRequestText(state: AsyncState<LifeEventsResponse>, gameReady: boolean): string {
  if (!gameReady) return '캐릭터를 만든 뒤 생애 사건을 조회할 수 있습니다.';
  switch (state.status) {
    case 'idle':
      return '생애 사건을 조회할 수 있습니다.';
    case 'loading':
      return '생애 사건을 조회하는 중입니다.';
    case 'success':
      return state.value.lifeEventCapability === 'unavailable'
        ? '현재 실행에서는 생애 사건을 이용할 수 없습니다.'
        : `선택 대기 ${state.value.pendingEvents.length}건, 최근 기록 ${state.value.history.length}건입니다.`;
    case 'error':
      return lifeEventQueryErrorText(state.error);
  }
}

function insuranceRequestText(
  state: AsyncState<InsuranceContractsResponse>,
  gameReady: boolean,
): string {
  if (!gameReady) return '캐릭터를 만든 뒤 보험을 조회할 수 있습니다.';
  switch (state.status) {
    case 'idle':
      return '보험 계약과 청구를 조회할 수 있습니다.';
    case 'loading':
      return '보험 계약과 청구를 조회하는 중입니다.';
    case 'success':
      return state.value.insuranceCapability === 'unavailable'
        ? '현재 실행에서는 보험을 이용할 수 없습니다.'
        : `상품 ${state.value.products.length}건, 계약 ${state.value.contracts.length}건, 청구 대기 ${state.value.pendingClaims.length}건, 기록 ${state.value.history.length}건입니다.`;
    case 'error':
      return insuranceQueryErrorText(state.error);
  }
}

function hasActiveContract(snapshot: GameSnapshot | undefined, productVersionId: string): boolean {
  return (
    snapshot?.life.activeInsuranceContracts.some(
      (contract) => contract.productVersionId === productVersionId,
    ) ?? false
  );
}

function commandSnapshot(deps: EventsInsuranceDeps): GameSnapshot {
  const snapshot = deps.store.getState().game.snapshot;
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error('명령을 처리하려면 먼저 캐릭터를 만들어야 합니다.');
  }
  if (snapshot.autoSpeed !== null) {
    throw new Error('자동 진행을 멈춘 뒤 명령을 실행해 주세요.');
  }
  return snapshot;
}

function lifeEventQueryErrorText(error: unknown): string {
  if (error instanceof LifeEventQueryError) return error.message;
  return error instanceof Error
    ? `생애 사건을 조회하지 못했습니다: ${error.message}`
    : '생애 사건을 조회하지 못했습니다.';
}

function insuranceQueryErrorText(error: unknown): string {
  if (error instanceof InsuranceQueryError) return error.message;
  return error instanceof Error
    ? `보험 계약을 조회하지 못했습니다: ${error.message}`
    : '보험 계약을 조회하지 못했습니다.';
}

function lifeEventDisplayError(error: unknown): string {
  if (error instanceof LifeEventCommandError) return error.message;
  return '선택 결과를 확인하지 못했습니다. 같은 사건의 선택 버튼을 눌러 다시 확인해 주세요.';
}

function insuranceDisplayError(error: unknown, action: string): string {
  if (error instanceof InsuranceCommandError) return error.message;
  return `${action} 결과를 확인하지 못했습니다. 같은 버튼을 눌러 최초 명령을 다시 확인해 주세요.`;
}

function insuranceSuccessMessage(action: string, replayed: boolean): string {
  return replayed ? `${action} 기존 결과를 확인했습니다.` : `${action} 결과를 반영했습니다.`;
}
