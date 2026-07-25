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

export const CONNECTION_LABEL: Record<string, string> = {
  idle: '대기',
  connecting: '연결 중',
  open: '연결됨',
  reconnecting: '재연결 중',
  closed: '끊김',
};

/** Login failure reasons the server reports via `?login_error=` (§4.5). */
export const LOGIN_ERROR_LABEL: Record<string, string> = {
  cancelled: '로그인을 취소했습니다.',
  expired: '로그인 시간이 지났습니다. 다시 시도해 주세요.',
  state_mismatch: '로그인 요청을 확인하지 못했습니다. 다시 시도해 주세요.',
  invalid_response: '로그인 응답이 올바르지 않습니다. 다시 시도해 주세요.',
};
