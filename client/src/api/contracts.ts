import { z } from 'zod';

/**
 * 서버 계약. 지금은 손으로 쓰지만, 서버가 OpenAPI 를 내보내면
 * 이 파일을 코드젠 산출물로 교체한다 (그때도 아래 타입 이름은 유지한다).
 */

export const HealthSchema = z.object({
  status: z.literal('ok'),
  version: z.string(),
});

export const GameSnapshotSchema = z.object({
  gameDay: z.number().int().nonnegative(),
  startDate: z.string(),
  cashKrw: z.number().int(),
  netWorthKrw: z.number().int(),
});

export const AdvanceRequestSchema = z.object({
  days: z.number().int().min(1).max(3650),
});

export type Health = z.infer<typeof HealthSchema>;
export type GameSnapshot = z.infer<typeof GameSnapshotSchema>;
export type AdvanceRequest = z.infer<typeof AdvanceRequestSchema>;

/** 진행 단위. 서버가 아니라 UI 어휘라서 클라이언트에 둔다. */
export const STEP_DAYS = { day: 1, week: 7, month: 30 } as const;
export type StepUnit = keyof typeof STEP_DAYS;
