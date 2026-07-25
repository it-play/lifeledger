import { z } from 'zod';

/**
 * The server contract. Hand-written for now; once the server emits OpenAPI this file is
 * replaced by generated code, keeping the type names below.
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

// -- Character creation (§3) ---------------------------------------------

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
 * Form input validation, covering the shape of each field only. Contradictory
 * combinations (§3.5) are the server's sole authority and are not reimplemented here.
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

/** The 422 body for a failed combination check; `field` matches the form field name. */
export const ValidationFailureSchema = z.object({
  errors: z.array(z.object({ field: z.string(), message: z.string() })),
});

// -- Login (§4.5) --------------------------------------------------------

export const ProviderKindSchema = z.enum(['datagsm', 'google']);

/**
 * A login provider the server enabled. One without credentials never reaches this list,
 * so the client simply draws what it receives.
 */
export const AuthProviderSchema = z.object({
  id: ProviderKindSchema,
  label: z.string(),
});

export const AuthProviderListSchema = z.array(AuthProviderSchema);

export const MeSchema = z.object({
  provider: ProviderKindSchema,
  email: z.string().nullable(),
  displayName: z.string().nullable(),
});

export type Health = z.infer<typeof HealthSchema>;
export type GameSnapshot = z.infer<typeof GameSnapshotSchema>;
export type AdvanceRequest = z.infer<typeof AdvanceRequestSchema>;
export type CharacterDraft = z.infer<typeof CharacterDraftSchema>;
export type Preset = z.infer<typeof PresetSchema>;
export type ValidationFailure = z.infer<typeof ValidationFailureSchema>;
export type ProviderKind = z.infer<typeof ProviderKindSchema>;
export type AuthProvider = z.infer<typeof AuthProviderSchema>;
export type Me = z.infer<typeof MeSchema>;

/** Step units. UI vocabulary rather than server contract, so they live here. */
export const STEP_DAYS = { day: 1, week: 7, month: 30 } as const;
export type StepUnit = keyof typeof STEP_DAYS;
