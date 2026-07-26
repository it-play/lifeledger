import type { LedgerSourceKind, MarketRegime } from '../api/contracts.js';

const WON = new Intl.NumberFormat('ko-KR', { style: 'currency', currency: 'KRW' });
const DATE = new Intl.DateTimeFormat('ko-KR', { dateStyle: 'medium' });

export const formatWon = (amount: number): string => WON.format(amount);

/**
 * Game day to display date. The server sends only the start date; the arithmetic is
 * deterministic, so doing it here costs no authority.
 */
export function gameDateOf(startDate: string, gameDay: number): Date {
  const start = new Date(`${startDate}T00:00:00Z`);
  start.setUTCDate(start.getUTCDate() + gameDay);
  return start;
}

export const formatGameDate = (startDate: string, gameDay: number): string =>
  DATE.format(gameDateOf(startDate, gameDay));

export function formatReturnPpm(returnPpm: number): string {
  const absolute = Math.abs(returnPpm);
  const whole = Math.floor(absolute / 10_000);
  const fraction = (absolute % 10_000).toString().padStart(4, '0').replace(/0+$/, '');
  const sign = returnPpm > 0 ? '+' : returnPpm < 0 ? '-' : '';

  return `${sign}${whole}${fraction.length > 0 ? `.${fraction}` : ''}%`;
}

export function formatBasisPoints(basisPoints: number): string {
  const absolute = Math.abs(basisPoints);
  const whole = Math.floor(absolute / 100);
  const fraction = (absolute % 100).toString().padStart(2, '0');
  const sign = basisPoints < 0 ? '-' : '';

  return `${sign}${whole}.${fraction}%`;
}

export const MARKET_REGIME_LABEL: Record<MarketRegime, string> = {
  expansion: '경기 확장',
  slowdown: '경기 둔화',
  recession: '경기 침체',
  recovery: '경기 회복',
};

export const CONNECTION_LABEL: Record<string, string> = {
  idle: '대기',
  connecting: '연결 중',
  open: '연결됨',
  reconnecting: '재연결 중',
  closed: '끊김',
};

export const LEDGER_SOURCE_LABEL: Record<LedgerSourceKind, string> = {
  m2OpeningBalance: '기초 잔액',
  transfer: '이체',
  trade: '매매',
  cashProductEnrollment: '현금상품 가입',
  cashProductClose: '현금상품 해지',
  interestAccrual: '이자 발생',
  scheduledSettlement: '예약 정산',
  isaClose: 'ISA 해지',
  pensionWithdrawal: '연금 인출',
  specActivity: '스펙 활동',
  correction: '정정',
};

/** Login failure reasons the server reports via `?login_error=` (§4.5). */
export const LOGIN_ERROR_LABEL: Record<string, string> = {
  cancelled: '로그인을 취소했습니다.',
  expired: '로그인 시간이 지났습니다. 다시 시도해 주세요.',
  state_mismatch: '로그인 요청을 확인하지 못했습니다. 다시 시도해 주세요.',
  invalid_response: '로그인 응답이 올바르지 않습니다. 다시 시도해 주세요.',
};
