import type {
  GameSnapshot,
  InsolvencyCaseDetailResponse,
  InsolvencyCaseStatus,
  InsolvencyClaimPageResponse,
  InsolvencyEligibilityReason,
  InsolvencyEligibilityStatus,
  InsolvencyLiquidationPageResponse,
  InsolvencyOverviewResponse,
} from '../../api/contracts.js';
import {
  type InsolvencyApi,
  InsolvencyCommandError,
  InsolvencyQueryError,
} from '../../api/insolvency-api.js';
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import { createInsolvencyRetryPolicy } from '../insolvency-retry/index.js';
import { type AppState, paths } from '../state.js';

export interface RecoveryDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: InsolvencyApi;
  readonly createCommandId: () => string;
}

interface RecoveryData {
  readonly overview: InsolvencyOverviewResponse;
  readonly detail?: InsolvencyCaseDetailResponse;
  readonly claims?: InsolvencyClaimPageResponse;
  readonly liquidations?: InsolvencyLiquidationPageResponse;
}

interface ClaimRow {
  readonly element: HTMLTableRowElement;
  readonly loan: HTMLTableCellElement;
  readonly allowed: HTMLTableCellElement;
  readonly distributed: HTMLTableCellElement;
  readonly discharged: HTMLTableCellElement;
}

interface DistributionRow {
  readonly element: HTMLTableRowElement;
  readonly claim: HTMLTableCellElement;
  readonly amount: HTMLTableCellElement;
  readonly day: HTMLTableCellElement;
}

const MAX_TRANSITIONS = 16;
const PAGE_SIZE = 20;

const ELIGIBILITY_LABEL: Record<InsolvencyEligibilityStatus, string> = {
  eligible: '신청 자격 있음',
  ineligible: '신청 자격 없음',
  compositionUnsupported: '현재 자산·채무 구성을 안전하게 처리할 수 없음',
  unavailable: '현재 월드에서 이용할 수 없음',
};

const REASON_LABEL: Record<InsolvencyEligibilityReason, string> = {
  policyUnavailable: '적용 가능한 도산 정책이 없습니다.',
  componentUnavailable: '도산 절차 구성 요소를 사용할 수 없습니다.',
  invalidWalletCash: '지갑 현금 상태를 판정할 수 없습니다.',
  noSupportedDefaultedDebt: '지원되는 연체 채무가 없습니다.',
  debtNotGreaterThanCash: '대상 채무가 보유 현금보다 많지 않습니다.',
  unsupportedLoanComposition: '지원하지 않는 대출이 포함되어 있습니다.',
  unsupportedAssetComposition: '지원하지 않는 자산이 포함되어 있습니다.',
  unsupportedNonLoanObligation: '지원하지 않는 비대출 의무가 포함되어 있습니다.',
  existingNonTerminalCase: '이미 진행 중인 회복 사건이 있습니다.',
};

const CASE_STATUS_LABEL: Record<InsolvencyCaseStatus, string> = {
  prepared: '제출 전 준비',
  filed: '제출됨',
  liquidation: '청산 중',
  discharged: '잔여 채무 면책',
  rebuilding: '신용 회복 중',
  withdrawn: '철회됨',
  recovered: '회복 완료',
};

/** M4-E1 server-authoritative cash-only insolvency and credit-recovery view. */
export function createRecoveryView(deps: RecoveryDeps): ViewFactory {
  const retries = createInsolvencyRetryPolicy({ createCommandId: deps.createCommandId });

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
      const data = h.useSignal<RecoveryData | undefined>(undefined);
      const commandBusy = h.useSignal(false);
      const commandFeedback = h.useSignal('');

      const refreshRequest = h.useAsync(async (signal): Promise<RecoveryData> => {
        const overview = await deps.api.overview(signal);
        const currentCase = overview.currentCase;
        if (currentCase === null) return { overview };
        const [detail, claims, liquidations] = await Promise.all([
          deps.api.detail(currentCase.id, signal),
          deps.api.claims(currentCase.id, undefined, signal),
          deps.api.liquidations(currentCase.id, undefined, signal),
        ]);
        return { overview, detail, claims, liquidations };
      });
      const nextClaimsRequest = h.useAsync(async (signal) => {
        const current = data.peek();
        const caseId = current?.overview.currentCase?.id;
        const cursor = current?.claims?.nextCursor;
        if (caseId === undefined || cursor === null || cursor === undefined) {
          throw new Error('조회할 다음 채권 페이지가 없습니다.');
        }
        return deps.api.claims(caseId, { cursor }, signal);
      });
      const nextLiquidationsRequest = h.useAsync(async (signal) => {
        const current = data.peek();
        const caseId = current?.overview.currentCase?.id;
        const cursor = current?.liquidations?.nextCursor;
        if (caseId === undefined || cursor === null || cursor === undefined) {
          throw new Error('조회할 다음 청산 페이지가 없습니다.');
        }
        return deps.api.liquidations(caseId, { cursor }, signal);
      });

      const requestStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const commandStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const refresh = el('button', { type: 'button' }, '회복 상태 다시 조회');
      const availability = el('dd');
      const eligibility = el('dd');
      const reasons = el('dd');
      const caseStatus = el('dd');
      const protectedCash = el('dd');
      const liquidatableCash = el('dd');
      const totalClaim = el('dd');
      const distributed = el('dd');
      const discharged = el('dd');
      const recoveryDays = el('dd');
      const prepare = el('button', { type: 'button' }, '현금 청산 사건 준비');
      const submit = el('button', { type: 'button' }, '준비된 사건 제출');
      const withdraw = el('button', { type: 'button' }, '준비된 사건 철회');
      const detailSection = el('section');
      const provenance = el('p');
      const transitionSlots = Array.from({ length: MAX_TRANSITIONS }, () => el('li'));
      const claimRows = Array.from({ length: PAGE_SIZE }, createClaimRow);
      const nextClaims = el('button', { type: 'button' }, '다음 채권 페이지');
      const walletAsset = el('p');
      const distributionRows = Array.from({ length: PAGE_SIZE }, createDistributionRow);
      const nextLiquidations = el('button', { type: 'button' }, '다음 청산 페이지');

      detailSection.append(
        el('h2', {}, '사건 상세'),
        provenance,
        el('h3', {}, '상태 전이'),
        el('ol', {}, ...transitionSlots),
        el('h3', {}, '채권'),
        el(
          'table',
          {},
          el(
            'thead',
            {},
            el(
              'tr',
              {},
              el('th', { attrs: { scope: 'col' } }, '대출'),
              el('th', { attrs: { scope: 'col' } }, '인정액'),
              el('th', { attrs: { scope: 'col' } }, '배분액'),
              el('th', { attrs: { scope: 'col' } }, '면책액'),
            ),
          ),
          el('tbody', {}, ...claimRows.map((row) => row.element)),
        ),
        nextClaims,
        el('h3', {}, '현금 청산'),
        walletAsset,
        el(
          'table',
          {},
          el(
            'thead',
            {},
            el(
              'tr',
              {},
              el('th', { attrs: { scope: 'col' } }, '채권'),
              el('th', { attrs: { scope: 'col' } }, '배분액'),
              el('th', { attrs: { scope: 'col' } }, '적용일'),
            ),
          ),
          el('tbody', {}, ...distributionRows.map((row) => row.element)),
        ),
        nextLiquidations,
      );

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '채무 청산과 신용 회복'),
          el(
            'p',
            {},
            '현재 절차는 지갑 현금과 지원되는 무담보 연체 대출만 처리합니다. 보호액·청산액·배분액은 서버가 확정하며, 제출 후에는 신용 회복 기간 동안 신규 대출이 제한됩니다.',
          ),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          requestStatus,
          commandStatus,
          refresh,
          el(
            'dl',
            {},
            el('dt', {}, '기능 상태'),
            availability,
            el('dt', {}, '신청 자격'),
            eligibility,
            el('dt', {}, '판정 사유'),
            reasons,
            el('dt', {}, '사건 상태'),
            caseStatus,
            el('dt', {}, '보호 현금'),
            protectedCash,
            el('dt', {}, '청산 가능 현금'),
            liquidatableCash,
            el('dt', {}, '총 인정 채권'),
            totalClaim,
            el('dt', {}, '배분 완료'),
            distributed,
            el('dt', {}, '면책 완료'),
            discharged,
            el('dt', {}, '남은 신용 회복 기간'),
            recoveryDays,
          ),
          el('p', {}, prepare, ' ', submit, ' ', withdraw),
          detailSection,
        ),
      );

      const canIssueCommand = (): boolean => {
        const current = snapshot.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          !commandBusy.get()
        );
      };

      h.bindText(requestStatus, () => requestStatusText(refreshRequest.state.get(), data.get()));
      h.bindText(commandStatus, () => commandFeedback.get());
      h.bindText(availability, () =>
        data.get()?.overview.availability === 'cashOnlyLiquidation'
          ? '현금 전용 청산 이용 가능'
          : '이용 불가',
      );
      h.bindText(eligibility, () => {
        const status = data.get()?.overview.eligibility;
        return status === undefined ? '확인 전' : ELIGIBILITY_LABEL[status];
      });
      h.bindText(reasons, () => reasonText(data.get()?.overview));
      h.bindText(caseStatus, () => caseStatusText(data.get()?.overview));
      h.bindText(protectedCash, () =>
        moneyText(data.get()?.overview.currentCase?.protectedCashKrw),
      );
      h.bindText(liquidatableCash, () => moneyText(data.get()?.detail?.liquidatableKrw));
      h.bindText(totalClaim, () => moneyText(data.get()?.detail?.totalClaimKrw));
      h.bindText(distributed, () => moneyText(data.get()?.overview.currentCase?.distributedKrw));
      h.bindText(discharged, () => moneyText(data.get()?.overview.currentCase?.dischargedKrw));
      h.bindText(recoveryDays, () => recoveryDaysText(snapshot.get(), data.get()?.overview));
      h.bindText(provenance, () => provenanceText(data.get()?.detail));
      h.bindText(walletAsset, () => walletText(data.get()?.liquidations));
      h.bindAttribute(refresh, 'disabled', () => refreshRequest.state.get().status === 'loading');
      h.bindAttribute(detailSection, 'hidden', () => data.get()?.detail === undefined);
      h.bindAttribute(prepare, 'disabled', () => {
        const overview = data.get()?.overview;
        return (
          !canIssueCommand() ||
          overview?.availability !== 'cashOnlyLiquidation' ||
          overview.eligibility !== 'eligible' ||
          overview.currentCase !== null
        );
      });
      h.bindAttribute(submit, 'disabled', () => {
        return !canIssueCommand() || data.get()?.overview.currentCase?.status !== 'prepared';
      });
      h.bindAttribute(withdraw, 'disabled', () => {
        return !canIssueCommand() || data.get()?.overview.currentCase?.status !== 'prepared';
      });
      h.bindAttribute(nextClaims, 'disabled', () => {
        return (
          data.get()?.claims?.nextCursor === null ||
          data.get()?.claims?.nextCursor === undefined ||
          nextClaimsRequest.state.get().status === 'loading'
        );
      });
      h.bindAttribute(nextLiquidations, 'disabled', () => {
        return (
          data.get()?.liquidations?.nextCursor === null ||
          data.get()?.liquidations?.nextCursor === undefined ||
          nextLiquidationsRequest.state.get().status === 'loading'
        );
      });

      for (const [index, slot] of transitionSlots.entries()) {
        h.bindAttribute(slot, 'hidden', () => data.get()?.detail?.transitions[index] === undefined);
        h.bindText(slot, () => transitionText(data.get()?.detail?.transitions[index]));
      }
      for (const [index, row] of claimRows.entries()) bindClaimRow(h, row, data, index);
      for (const [index, row] of distributionRows.entries()) {
        bindDistributionRow(h, row, data, index);
      }

      h.useEffect(() => {
        const state = refreshRequest.state.get();
        if (state.status === 'success') data.set(state.value);
      });
      h.useEffect(() => {
        const state = nextClaimsRequest.state.get();
        const current = data.peek();
        if (state.status === 'success' && current !== undefined) {
          data.set({ ...current, claims: state.value });
        }
      });
      h.useEffect(() => {
        const state = nextLiquidationsRequest.state.get();
        const current = data.peek();
        if (state.status === 'success' && current !== undefined) {
          data.set({ ...current, liquidations: state.value });
        }
      });
      h.useWatch(snapshot, (next, previous) => {
        if (next === undefined || next.characterName === null) {
          refreshRequest.cancel();
          data.set(undefined);
          return;
        }
        if (
          previous === undefined ||
          next.runRevision !== previous.runRevision ||
          next.stateRevision !== previous.stateRevision
        ) {
          refreshRequest.run();
        }
      });
      h.useEventListener(refresh, 'click', () => refreshRequest.run());
      h.useEventListener(nextClaims, 'click', () => nextClaimsRequest.run());
      h.useEventListener(nextLiquidations, 'click', () => nextLiquidationsRequest.run());
      h.useEventListener(prepare, 'click', () => void issuePrepare());
      h.useEventListener(submit, 'click', () => void issueAction('submit'));
      h.useEventListener(withdraw, 'click', () => void issueAction('withdraw'));

      if (snapshot.peek()?.characterName !== null && snapshot.peek() !== undefined) {
        refreshRequest.run();
      }

      async function issuePrepare(): Promise<void> {
        const current = commandSnapshot(deps);
        const command = retries.prepare(current);
        await issue(
          command,
          () => deps.api.prepare(command.request),
          '청산 사건을 준비했습니다. 세부 내역을 확인한 뒤 제출할 수 있습니다.',
        );
      }

      async function issueAction(action: 'submit' | 'withdraw'): Promise<void> {
        const current = commandSnapshot(deps);
        const caseId = data.peek()?.overview.currentCase?.id;
        if (caseId === undefined) return;
        const command = retries.act(current, caseId, action);
        await issue(
          command,
          () => deps.api.act(caseId, command.request),
          action === 'submit'
            ? '청산과 면책을 처리하고 신용 회복 기간을 시작했습니다.'
            : '준비된 청산 사건을 철회했습니다.',
        );
      }

      async function issue(
        command: ReturnType<typeof retries.prepare> | ReturnType<typeof retries.act>,
        request: () => ReturnType<InsolvencyApi['prepare']>,
        successMessage: string,
      ): Promise<void> {
        if (!canIssueCommand()) return;
        commandBusy.set(true);
        commandFeedback.set('명령을 처리하는 중입니다.');
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await request();
          retries.complete(command);
          deps.snapshots.apply(response.snapshot);
          commandFeedback.set(
            response.replayed ? `${successMessage} 이전 결과를 재확인했습니다.` : successMessage,
          );
        } catch (error) {
          retries.fail(command, error);
          commandFeedback.set(displayError(error));
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }
    },
    unmount() {},
  });
}

function createClaimRow(): ClaimRow {
  const loan = el('td');
  const allowed = el('td');
  const distributed = el('td');
  const discharged = el('td');
  return {
    element: el('tr', {}, loan, allowed, distributed, discharged),
    loan,
    allowed,
    distributed,
    discharged,
  };
}

function createDistributionRow(): DistributionRow {
  const claim = el('td');
  const amount = el('td');
  const day = el('td');
  return { element: el('tr', {}, claim, amount, day), claim, amount, day };
}

function bindClaimRow(
  h: ReturnType<typeof createHooks>,
  row: ClaimRow,
  data: { get(): RecoveryData | undefined },
  index: number,
): void {
  h.bindAttribute(row.element, 'hidden', () => data.get()?.claims?.claims[index] === undefined);
  h.bindText(row.loan, () => data.get()?.claims?.claims[index]?.loanContractId ?? '');
  h.bindText(row.allowed, () => moneyText(data.get()?.claims?.claims[index]?.allowedKrw));
  h.bindText(row.distributed, () => moneyText(data.get()?.claims?.claims[index]?.distributedKrw));
  h.bindText(row.discharged, () => moneyText(data.get()?.claims?.claims[index]?.dischargedKrw));
}

function bindDistributionRow(
  h: ReturnType<typeof createHooks>,
  row: DistributionRow,
  data: { get(): RecoveryData | undefined },
  index: number,
): void {
  h.bindAttribute(
    row.element,
    'hidden',
    () => data.get()?.liquidations?.distributions[index] === undefined,
  );
  h.bindText(row.claim, () => data.get()?.liquidations?.distributions[index]?.claimId ?? '');
  h.bindText(row.amount, () =>
    moneyText(data.get()?.liquidations?.distributions[index]?.amountKrw),
  );
  h.bindText(
    row.day,
    () => data.get()?.liquidations?.distributions[index]?.appliedGameDay.toString() ?? '',
  );
}

function requestStatusText(
  state: { readonly status: string; readonly error?: unknown },
  data: RecoveryData | undefined,
): string {
  if (state.status === 'loading') return '회복 상태를 조회하는 중입니다.';
  if (state.status === 'error') return displayError(state.error);
  return data === undefined ? '회복 상태를 조회해 주세요.' : '서버의 최신 회복 상태입니다.';
}

function reasonText(overview: InsolvencyOverviewResponse | undefined): string {
  if (overview === undefined) return '확인 전';
  return overview.reasons.length === 0
    ? '현재 제한 사유가 없습니다.'
    : overview.reasons.map((reason) => REASON_LABEL[reason]).join(' ');
}

function caseStatusText(overview: InsolvencyOverviewResponse | undefined): string {
  const current = overview?.currentCase;
  return current === null || current === undefined
    ? '현재 사건 없음'
    : CASE_STATUS_LABEL[current.status];
}

function moneyText(value: number | undefined): string {
  return value === undefined ? '—' : formatWon(value);
}

function recoveryDaysText(
  snapshot: GameSnapshot | undefined,
  overview: InsolvencyOverviewResponse | undefined,
): string {
  const end = overview?.currentCase?.creditRestrictionEndExclusive;
  if (snapshot === undefined || end === null || end === undefined) return '—';
  return `${Math.max(0, end - snapshot.gameDay).toLocaleString('ko-KR')} 게임일`;
}

function provenanceText(detail: InsolvencyCaseDetailResponse | undefined): string {
  return detail === undefined
    ? ''
    : `정책 ${detail.policySetId} · 생활 카탈로그 ${detail.lifeCatalogSetId} · 도산 구성 ${detail.insolvencyComponentVersionId} · 구성 해시 ${detail.compositionSha256}`;
}

function transitionText(
  transition: InsolvencyCaseDetailResponse['transitions'][number] | undefined,
): string {
  if (transition === undefined) return '';
  const from = transition.fromStatus === null ? '시작' : CASE_STATUS_LABEL[transition.fromStatus];
  return `${transition.sequence}. ${from} → ${CASE_STATUS_LABEL[transition.toStatus]} (${transition.gameDay}일차)`;
}

function walletText(page: InsolvencyLiquidationPageResponse | undefined): string {
  const wallet = page?.walletAsset;
  if (wallet === null || wallet === undefined) return '기록된 지갑 현금 청산이 없습니다.';
  return `원금 ${formatWon(wallet.originalAmountKrw)} · 보호 ${formatWon(wallet.protectedAmountKrw)} · 청산 가능 ${formatWon(wallet.liquidatableKrw)} · 배분 ${formatWon(wallet.distributedKrw)}`;
}

function commandSnapshot(deps: RecoveryDeps): GameSnapshot {
  const snapshot = deps.store.getState().game.snapshot;
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error('현재 게임 상태가 없습니다.');
  }
  return snapshot;
}

function displayError(error: unknown): string {
  if (error instanceof InsolvencyCommandError || error instanceof InsolvencyQueryError) {
    const labels: Partial<Record<typeof error.code, string>> = {
      insolvencyCompositionUnsupported: '현재 자산·채무 구성은 안전하게 처리할 수 없습니다.',
      insolvencyCompositionChanged:
        '준비 후 자산·채무 구성이 바뀌었습니다. 사건을 다시 준비해 주세요.',
      insolvencyStateConflict: '회복 사건 상태가 바뀌었습니다. 최신 상태를 다시 조회해 주세요.',
      insolvencyResourceNotFound: '현재 실행에서 회복 사건을 찾을 수 없습니다.',
      ineligible: '현재는 청산 신청 자격이 없습니다.',
      busy: '서버가 다른 명령을 처리 중입니다. 잠시 후 다시 시도해 주세요.',
    };
    return labels[error.code] ?? error.message;
  }
  return error instanceof Error
    ? `요청 결과를 확인하지 못했습니다. 같은 버튼으로 다시 시도해 주세요. (${error.message})`
    : '요청 결과를 확인하지 못했습니다. 같은 버튼으로 다시 시도해 주세요.';
}
