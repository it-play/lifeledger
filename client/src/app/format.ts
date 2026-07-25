const WON = new Intl.NumberFormat('ko-KR', { style: 'currency', currency: 'KRW' });
const DATE = new Intl.DateTimeFormat('ko-KR', { dateStyle: 'medium' });

export const formatWon = (amount: number): string => WON.format(amount);

/**
 * 게임일 → 표시용 날짜.
 * 서버가 시작일만 주고 날짜 계산은 클라이언트가 한다 (결정론적이라 권위 문제가 없다).
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
