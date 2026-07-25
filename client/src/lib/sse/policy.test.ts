import { describe, expect, it } from '@jest/globals';
import { createDefaultRetryDecider, createExponentialBackoff } from './policy.js';

/**
 * 대상: 재연결 정책 (핵심 로직)
 * 구조: Data(끊긴 이유·시도 횟수) — Context(어떤 상황인가) — Interaction(얼마나 기다리고 다시 붙는가)
 */

describe('지수 백오프', () => {
  describe('맥락: 재연결이 연달아 실패하는 경우', () => {
    it('given 시도 1~5회, when 지터가 없으면, then 지수로 늘고 상한에서 멈춘다', () => {
      const backoff = createExponentialBackoff({
        baseMs: 1000,
        factor: 2,
        maxMs: 8000,
        jitterRatio: 0,
      });

      const delays = [1, 2, 3, 4, 5].map((attempt) => backoff.delayMs(attempt, undefined));

      expect(delays).toEqual([1000, 2000, 4000, 8000, 8000]);
    });
  });

  describe('맥락: 서버가 retry 필드로 지연을 지정한 경우', () => {
    it('given retry 250ms, when 지연을 계산하면, then 그 값이 기준이 된다', () => {
      const backoff = createExponentialBackoff({ baseMs: 1000, jitterRatio: 0 });

      expect(backoff.delayMs(1, 250)).toBe(250);
      expect(backoff.delayMs(2, 250)).toBe(500);
    });
  });

  describe('맥락: 여러 클라이언트가 동시에 재연결하는 경우', () => {
    it('given 지터 비율 0.5, when 난수가 최소·최대면, then 지정 범위 안에서만 흔들린다', () => {
      const lower = createExponentialBackoff({ baseMs: 1000, jitterRatio: 0.5, random: () => 0 });
      const upper = createExponentialBackoff({ baseMs: 1000, jitterRatio: 0.5, random: () => 1 });

      expect(lower.delayMs(1, undefined)).toBe(500);
      expect(upper.delayMs(1, undefined)).toBe(1500);
    });
  });
});

describe('재시도 판단', () => {
  const decider = createDefaultRetryDecider();

  describe('맥락: 일시적인 문제로 끊긴 경우', () => {
    it('given 네트워크 오류나 정상 종료, when 판단하면, then 재시도한다', () => {
      expect(decider.shouldRetry({ kind: 'network', error: new Error('x') })).toBe(true);
      expect(decider.shouldRetry({ kind: 'stream-ended' })).toBe(true);
    });

    it('given 5xx 또는 429, when 판단하면, then 재시도한다', () => {
      expect(decider.shouldRetry({ kind: 'http', status: 500 })).toBe(true);
      expect(decider.shouldRetry({ kind: 'http', status: 429 })).toBe(true);
    });
  });

  describe('맥락: 다시 시도해도 결과가 같은 경우', () => {
    it('given 인증 실패나 없는 경로, when 판단하면, then 재시도하지 않는다', () => {
      expect(decider.shouldRetry({ kind: 'http', status: 401 })).toBe(false);
      expect(decider.shouldRetry({ kind: 'http', status: 404 })).toBe(false);
    });

    it('given 204 (보낼 것 없음), when 판단하면, then 재시도하지 않는다', () => {
      expect(decider.shouldRetry({ kind: 'http', status: 204 })).toBe(false);
    });

    it('given Content-Type 불일치나 호출자 종료, when 판단하면, then 재시도하지 않는다', () => {
      expect(decider.shouldRetry({ kind: 'bad-content-type', contentType: 'text/html' })).toBe(
        false,
      );
      expect(decider.shouldRetry({ kind: 'closed-by-caller' })).toBe(false);
    });
  });
});
