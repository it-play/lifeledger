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
  debtKrw: z.number().int(),
  netWorthKrw: z.number().int(),
  characterName: z.string().nullable(),
});

export const AdvanceRequestSchema = z.object({
  days: z.number().int().min(1).max(3650),
});

// ── 캐릭터 생성 (계획 문서 §3) ────────────────────────────────────────────

export const GenderSchema = z.enum(['male', 'female', 'other']);
export const MilitaryStatusSchema = z.enum([
  'notServed',
  'serving',
  'completed',
  'exempted',
  'alternative',
]);
export const EducationSchema = z.enum([
  'highSchool',
  'associate',
  'bachelor',
  'master',
  'doctorate',
]);
export const RegionSchema = z.enum(['capitalArea', 'metropolitan', 'smallCity', 'rural']);
export const FamilyBackgroundSchema = z.enum(['supportive', 'independent', 'dependent']);
export const HealthLevelSchema = z.enum(['good', 'normal', 'poor']);

/**
 * 폼 입력 검증. 여기서는 **각 필드의 형태만** 본다.
 * 조합 모순(§3.5)은 서버가 유일한 권위이므로 클라이언트에서 재구현하지 않는다.
 */
export const CharacterDraftSchema = z.object({
  name: z.string().trim().min(1, '이름을 입력하세요').max(20, '이름이 너무 깁니다'),
  age: z.number().int().min(19, '19세 이상이어야 합니다').max(50, '50세 이하여야 합니다'),
  gender: GenderSchema,
  military: MilitaryStatusSchema,
  region: RegionSchema,
  background: FamilyBackgroundSchema,
  education: EducationSchema,
  careerYears: z.number().int().min(0).max(30, '경력은 30년을 넘을 수 없습니다'),
  certifications: z.number().int().min(0).max(50),
  startingCashKrw: z.number().int().min(0, '시작 자금은 0원 이상이어야 합니다'),
  studentLoanKrw: z.number().int().min(0),
  creditLoanKrw: z.number().int().min(0),
  health: HealthLevelSchema,
  dependents: z.number().int().min(0).max(6, '부양가족은 6명을 넘을 수 없습니다'),
});

export const PresetSchema = z.object({
  id: z.string(),
  label: z.string(),
  summary: z.string(),
  age: z.number().int(),
  military: MilitaryStatusSchema,
  education: EducationSchema,
  region: RegionSchema,
  background: FamilyBackgroundSchema,
  careerYears: z.number().int(),
  certifications: z.number().int(),
  startingCashKrw: z.number().int(),
  studentLoanKrw: z.number().int(),
  creditLoanKrw: z.number().int(),
  health: HealthLevelSchema,
  dependents: z.number().int(),
});

export const PresetListSchema = z.array(PresetSchema);

/** 서버가 422 로 돌려주는 조합 검증 실패. field 는 폼 필드 이름과 같다. */
export const ValidationFailureSchema = z.object({
  errors: z.array(z.object({ field: z.string(), message: z.string() })),
});

export type Health = z.infer<typeof HealthSchema>;
export type GameSnapshot = z.infer<typeof GameSnapshotSchema>;
export type AdvanceRequest = z.infer<typeof AdvanceRequestSchema>;
export type CharacterDraft = z.infer<typeof CharacterDraftSchema>;
export type Preset = z.infer<typeof PresetSchema>;
export type ValidationFailure = z.infer<typeof ValidationFailureSchema>;

/** 진행 단위. 서버가 아니라 UI 어휘라서 클라이언트에 둔다. */
export const STEP_DAYS = { day: 1, week: 7, month: 30 } as const;
export type StepUnit = keyof typeof STEP_DAYS;
