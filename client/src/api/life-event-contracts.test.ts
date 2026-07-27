import { describe, expect, it } from '@jest/globals';
import {
  LifeEventChoiceRequestSchema,
  LifeEventChoiceResultSchema,
  LifeEventsResponseSchema,
  PendingLifeEventSchema,
} from './contracts.js';

describe('생애 사건 공개 계약', () => {
  describe('맥락: 결정론적 사건을 조회한 경우', () => {
    it('given canonical pending과 history, when 검증하면, then strict 응답을 받는다', () => {
      const response = LifeEventsResponseSchema.parse({
        lifeEventCapability: 'deterministicChoices',
        insuranceCapability: 'unavailable',
        pendingEvents: [givenPendingEvent()],
        history: [
          {
            id: '40',
            eventKey: 'fictionalDependentCareRequest',
            displayName: '가족 돌봄 요청',
            offeredGameDay: 20,
            resolvedGameDay: 21,
            resolutionKind: 'declined',
            choice: givenDeclineChoice(),
          },
        ],
        nextCursor: 'opaque-cursor',
      });

      expect(response.pendingEvents[0]?.defaultChoiceId).toBe('82');
      expect(response.history[0]?.resolutionKind).toBe('declined');
    });

    it('given D3 active insurance pin, when 검증하면, then 기존 5개 필드에 contractsAndClaims를 공개한다', () => {
      const response = LifeEventsResponseSchema.parse({
        lifeEventCapability: 'deterministicChoices',
        insuranceCapability: 'contractsAndClaims',
        pendingEvents: [givenPendingEvent()],
        history: [],
        nextCursor: null,
      });

      expect(Object.keys(response)).toEqual([
        'lifeEventCapability',
        'insuranceCapability',
        'pendingEvents',
        'history',
        'nextCursor',
      ]);
      expect(response.insuranceCapability).toBe('contractsAndClaims');
    });
  });

  describe('맥락: 사건 기능이 비활성화된 기존 실행인 경우', () => {
    it('given unavailable인데 pending이 있음, when 검증하면, then 호환 응답을 거절한다', () => {
      const result = LifeEventsResponseSchema.safeParse({
        lifeEventCapability: 'unavailable',
        insuranceCapability: 'unavailable',
        pendingEvents: [givenPendingEvent()],
        history: [],
        nextCursor: null,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 공개 선택지가 카탈로그와 불일치한 경우', () => {
    it('given default choice가 배열에 없음, when 검증하면, then 사건을 거절한다', () => {
      const result = PendingLifeEventSchema.safeParse({
        ...givenPendingEvent(),
        defaultChoiceId: '99',
      });

      expect(result.success).toBe(false);
    });

    it('given 비용 선택지가 default, when 검증하면, then 자동 만료 비용을 거절한다', () => {
      const result = PendingLifeEventSchema.safeParse({
        ...givenPendingEvent(),
        defaultChoiceId: '81',
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 기한 만료 기록이 비용 선택지를 가리키는 경우', () => {
    it('given expired인데 wallet expense 선택, when 검증하면, then 공개 기록을 거절한다', () => {
      const result = LifeEventsResponseSchema.safeParse({
        lifeEventCapability: 'deterministicChoices',
        insuranceCapability: 'unavailable',
        pendingEvents: [],
        history: [
          {
            id: '40',
            eventKey: 'fictionalDependentCareRequest',
            displayName: '가족 돌봄 요청',
            offeredGameDay: 20,
            resolvedGameDay: 27,
            resolutionKind: 'expired',
            choice: givenSupportChoice(),
          },
        ],
        nextCursor: null,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 사건 선택 명령을 만든 경우', () => {
    it('given unknown field, when 검증하면, then strict body가 거절한다', () => {
      const result = LifeEventChoiceRequestSchema.safeParse({
        ...givenChoiceRequest(),
        amountKrw: 120_000,
      });

      expect(result.success).toBe(false);
    });

    it('given canonical cursor와 choice ID, when 검증하면, then 금액 없이 명령을 받는다', () => {
      const request = LifeEventChoiceRequestSchema.parse(givenChoiceRequest());

      expect(request.choiceId).toBe('81');
    });
  });

  describe('맥락: 선택 결과를 공개한 경우', () => {
    it('given 지갑 차감이 양수, when 검증하면, then 방향이 잘못된 결과를 거절한다', () => {
      const result = LifeEventChoiceResultSchema.safeParse({
        eventId: '71',
        choiceId: '81',
        resolutionKind: 'accepted',
        resolvedGameDay: 17,
        walletDeltaKrw: 120_000,
      });

      expect(result.success).toBe(false);
    });
  });
});

function givenPendingEvent(): Record<string, unknown> {
  return {
    id: '71',
    eventKey: 'fictionalDependentCareRequest',
    displayName: '가족 돌봄 요청',
    offeredGameDay: 17,
    expiresGameDay: 24,
    defaultChoiceId: '82',
    choices: [givenSupportChoice(), givenDeclineChoice()],
  };
}

function givenSupportChoice(): Record<string, unknown> {
  return {
    id: '81',
    displayName: '지금 돕는다',
    decisionKind: 'accepted',
    effectSummary: { kind: 'walletExpense', amountKrw: 120_000 },
  };
}

function givenDeclineChoice(): Record<string, unknown> {
  return {
    id: '82',
    displayName: '이번에는 돕지 않는다',
    decisionKind: 'declined',
    effectSummary: { kind: 'noEffect' },
  };
}

function givenChoiceRequest(): Record<string, unknown> {
  return {
    commandId: '00000000-0000-4000-8000-000000000001',
    expectedRunRevision: 3,
    expectedStateRevision: 42,
    expectedGameDay: 17,
    choiceId: '81',
  };
}
