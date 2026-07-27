import { type CareerApi, CareerCommandError } from '../../api/career-api.js';
import {
  type CareerActivityCatalogEntry,
  type CareerActivityHistoryItem,
  type CareerActivityStartDraft,
  CareerActivityStartDraftSchema,
  type CareerActivitySummary,
  type CareerApplication,
  type CareerApplicationDraft,
  CareerApplicationDraftSchema,
  type CareerArtifact,
  type CareerArtifactDraft,
  CareerArtifactDraftSchema,
  type CareerArtifactKind,
  type CareerArtifactSummary,
  type CareerEmploymentContract,
  type CareerEvidence,
  type CareerFocusDraft,
  CareerFocusDraftSchema,
  type CareerIndustry,
  type CareerInvitation,
  type CareerJob,
  type CareerPayrollItem,
  type CareerPendingScheduleItem,
  type CareerPlatform,
  type CareerTaxYearState,
  type EvidenceKind,
  type GameSnapshot,
  type LifeStatus,
  type MilitaryOption,
  type MilitarySavingsEnrollmentDraft,
  MilitarySavingsEnrollmentDraftSchema,
  type MilitarySavingsHistoryItem,
  type MilitarySavingsProduct,
  type MilitaryServiceHistory,
  type MilitaryServiceResponse,
  type MilitaryServiceStartDraft,
  MilitaryServiceStartDraftSchema,
  type MilitaryServiceType,
  TaxYearSchema,
} from '../../api/contracts.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { bindText, el } from '../../lib/dom/index.js';
import { type FieldSpec, type FormValidator, renderForm } from '../../lib/form/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import {
  type CareerPathCommand,
  createCareerActivityCancelRetryPolicy,
  createCareerActivityStartRetryPolicy,
  createCareerApplicationRetryPolicy,
  createCareerArtifactRetryPolicy,
  createCareerFocusRetryPolicy,
  createCareerInterviewRetryPolicy,
  createCareerPathRetryPolicy,
  createMilitarySavingsCloseRetryPolicy,
  createMilitarySavingsEnrollmentRetryPolicy,
  createMilitaryServiceStartRetryPolicy,
} from '../career-retry/index.js';
import { formatBasisPoints, formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import { type AppState, paths } from '../state.js';

export interface CareerDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: CareerApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface CareerCancelFormDraft {
  readonly activityId: string;
}

interface CareerInterviewFormDraft {
  readonly applicationId: string;
  readonly decision: 'confirm' | 'decline';
}

type CareerPathFormAction =
  | 'withdrawApplication'
  | 'acceptInvitation'
  | 'declineInvitation'
  | 'acceptOffer'
  | 'declineOffer';

interface CareerPathFormDraft {
  readonly action: CareerPathFormAction;
  readonly resourceId: string;
}

interface MilitarySavingsCloseDraft {
  readonly contractId: string;
}

interface FixedList<T> {
  readonly element: HTMLUListElement;
  setItems(items: readonly T[]): void;
}

interface FixedListRow<T> {
  readonly element: HTMLLIElement;
  setItem(item: T | undefined): void;
}

type PageLoadRequest =
  | {
      readonly kind: 'initial';
      readonly generation: number;
      readonly limit: number;
    }
  | {
      readonly kind: 'older';
      readonly generation: number;
      readonly before: string;
      readonly limit: number;
    };

const PAGE_SIZE = 50;
const HISTORY_CAPACITY = 200;
const CATALOG_CAPACITY = 200;
const ACTIVE_ACTIVITY_CAPACITY = 3;
const LATEST_ARTIFACT_CAPACITY = 3;
const JOB_CAPACITY = 200;
const OPEN_INVITATION_CAPACITY = 5;
const MILITARY_OPTION_CAPACITY = 6;
const MILITARY_SAVINGS_PRODUCT_CAPACITY = 20;
const ACTIVE_MILITARY_SAVINGS_CAPACITY = 2;
const PENDING_CAREER_SCHEDULE_CAPACITY = 20;

const FOCUS_FIELDS: readonly FieldSpec[] = [
  {
    name: 'focusedJobFamilyKey',
    label: '기본 직무군 키',
    kind: 'text',
    help: '현재 월드의 직무군 키를 입력하세요.',
  },
];

const ACTIVITY_START_FIELDS: readonly FieldSpec[] = [
  {
    name: 'activityCatalogEntryId',
    label: '활동',
    kind: 'select',
    options: fixedSelectOptions(CATALOG_CAPACITY, '활동을 선택하세요'),
  },
  {
    name: 'priority',
    label: '우선순위',
    kind: 'select',
    options: [
      { value: '1', label: '1순위' },
      { value: '2', label: '2순위' },
      { value: '3', label: '3순위' },
    ],
  },
];

const ACTIVITY_CANCEL_FIELDS: readonly FieldSpec[] = [
  {
    name: 'activityId',
    label: '취소할 활동',
    kind: 'select',
    options: fixedSelectOptions(ACTIVE_ACTIVITY_CAPACITY, '활동을 선택하세요'),
  },
];

const ARTIFACT_FIELDS: readonly FieldSpec[] = [
  {
    name: 'kind',
    label: '산출물 종류',
    kind: 'select',
    options: [
      { value: 'portfolio', label: '포트폴리오' },
      { value: 'resume', label: '이력서' },
      { value: 'linkedinProfile', label: 'LinkedIn 프로필' },
    ],
  },
  { name: 'headline', label: '제목', kind: 'text' },
  { name: 'summary', label: '요약', kind: 'textarea', help: '최대 2,000자' },
  {
    name: 'evidenceIds',
    label: '포함할 증빙 ID',
    kind: 'text',
    help: '증빙 ID를 쉼표로 구분해 최대 40개까지 입력하세요. 비워 둘 수 있습니다.',
  },
  {
    name: 'openToWork',
    label: '구직 중 공개',
    kind: 'checkbox',
    help: 'LinkedIn 프로필에서만 사용합니다.',
  },
  {
    name: 'industries',
    label: '공개 업종',
    kind: 'text',
    help: 'LinkedIn 전용. itSoftware, financeInsurance, manufacturing, constructionEngineering, retailService, publicSocial 중 최대 3개를 쉼표로 구분하세요.',
  },
];

const APPLICATION_FIELDS: readonly FieldSpec[] = [
  {
    name: 'postingKey',
    label: '공고 키',
    kind: 'text',
    help: '공고 목록의 64자리 키를 입력하세요.',
  },
  {
    name: 'resumeVersionId',
    label: '이력서 버전 ID',
    kind: 'text',
    help: '필요한 경우에만 입력하세요.',
  },
  {
    name: 'portfolioVersionId',
    label: '포트폴리오 버전 ID',
    kind: 'text',
    help: '필요한 경우에만 입력하세요.',
  },
  {
    name: 'linkedinProfileVersionId',
    label: 'LinkedIn 버전 ID',
    kind: 'text',
    help: '필요한 경우에만 입력하세요.',
  },
];

const INTERVIEW_FIELDS: readonly FieldSpec[] = [
  { name: 'applicationId', label: '지원 ID', kind: 'text' },
  {
    name: 'decision',
    label: '면접 확인',
    kind: 'select',
    options: [
      { value: 'confirm', label: '면접 확인' },
      { value: 'decline', label: '면접 거절' },
    ],
  },
];

const PATH_ACTION_FIELDS: readonly FieldSpec[] = [
  {
    name: 'action',
    label: '처리할 명령',
    kind: 'select',
    options: [
      { value: 'withdrawApplication', label: '지원 철회' },
      { value: 'acceptInvitation', label: '역제안 수락' },
      { value: 'declineInvitation', label: '역제안 거절' },
      { value: 'acceptOffer', label: '오퍼 수락' },
      { value: 'declineOffer', label: '오퍼 거절' },
    ],
  },
  { name: 'resourceId', label: '대상 ID', kind: 'text' },
];

const MILITARY_SERVICE_START_FIELDS: readonly FieldSpec[] = [
  {
    name: 'militaryOptionVersionId',
    label: '복무 option',
    kind: 'select',
    options: fixedSelectOptions(MILITARY_OPTION_CAPACITY, '복무 option을 선택하세요'),
  },
];

const MILITARY_SAVINGS_ENROLLMENT_FIELDS: readonly FieldSpec[] = [
  {
    name: 'productVersionId',
    label: '장병적금 상품',
    kind: 'select',
    options: fixedSelectOptions(MILITARY_SAVINGS_PRODUCT_CAPACITY, '상품을 선택하세요'),
  },
  {
    name: 'monthlyContributionKrw',
    label: '월 납입액 (원)',
    kind: 'number',
    help: '상품 목록의 서버 한도와 설정 단위를 확인하세요.',
  },
  {
    name: 'debitDayOfMonth',
    label: '매월 납입일',
    kind: 'number',
    help: '1~31일. 없는 날짜는 서버가 말일로 보정합니다.',
  },
];

const MILITARY_SAVINGS_CLOSE_FIELDS: readonly FieldSpec[] = [
  {
    name: 'contractId',
    label: '중도해지할 계약',
    kind: 'select',
    options: fixedSelectOptions(ACTIVE_MILITARY_SAVINGS_CAPACITY, '계약을 선택하세요'),
  },
];

const FOCUS_VALIDATOR = withKoreanErrors(asFormValidator(CareerFocusDraftSchema), {
  focusedJobFamilyKey: '기본 직무군 키를 1~64자로 입력하세요.',
});

const ACTIVITY_START_SCHEMA_VALIDATOR = asFormValidator(CareerActivityStartDraftSchema);
const ACTIVITY_START_VALIDATOR: FormValidator<CareerActivityStartDraft> = {
  validate(raw) {
    return localizeValidation(
      ACTIVITY_START_SCHEMA_VALIDATOR.validate({
        activityCatalogEntryId: raw.activityCatalogEntryId,
        priority: typeof raw.priority === 'string' ? Number(raw.priority) : raw.priority,
      }),
      {
        activityCatalogEntryId: '시작할 활동을 선택하세요.',
        priority: '우선순위를 1~3에서 선택하세요.',
      },
    );
  },
};

const ACTIVITY_CANCEL_SCHEMA_VALIDATOR = asFormValidator(
  CareerActivityStartDraftSchema.pick({ activityCatalogEntryId: true }),
);
const ACTIVITY_CANCEL_VALIDATOR: FormValidator<CareerCancelFormDraft> = {
  validate(raw) {
    const result = ACTIVITY_CANCEL_SCHEMA_VALIDATOR.validate({
      activityCatalogEntryId: raw.activityId,
    });
    if (!result.ok) {
      return { ok: false, errors: { activityId: '취소할 활동을 선택하세요.' } };
    }
    return { ok: true, value: { activityId: result.value.activityCatalogEntryId } };
  },
};

const ARTIFACT_SCHEMA_VALIDATOR = asFormValidator(CareerArtifactDraftSchema);
const ARTIFACT_VALIDATOR: FormValidator<CareerArtifactDraft> = {
  validate(raw) {
    const common: Readonly<Record<string, unknown>> = {
      kind: raw.kind,
      headline: raw.headline,
      summary: raw.summary,
      evidenceIds: commaSeparatedValues(raw.evidenceIds),
    };
    const candidate =
      raw.kind === 'linkedinProfile'
        ? {
            ...common,
            openToWork: raw.openToWork,
            industries: commaSeparatedValues(raw.industries),
          }
        : common;
    return localizeValidation(ARTIFACT_SCHEMA_VALIDATOR.validate(candidate), {
      kind: '산출물 종류를 선택하세요.',
      headline: '제목을 1~120자로 입력하고 제어 문자는 제외하세요.',
      summary: '요약은 2,000자 이하로 입력하고 제어 문자는 제외하세요.',
      evidenceIds: '증빙 ID는 중복 없이 쉼표로 구분해 입력하세요.',
      openToWork: '구직 중 공개 여부를 확인하세요.',
      industries: '업종 키는 중복 없이 최대 3개까지 입력하세요.',
    });
  },
};

const APPLICATION_SCHEMA_VALIDATOR = asFormValidator(CareerApplicationDraftSchema);
const APPLICATION_VALIDATOR: FormValidator<CareerApplicationDraft> = {
  validate(raw) {
    const candidate = {
      postingKey: raw.postingKey,
      resumeVersionId: optionalText(raw.resumeVersionId),
      portfolioVersionId: optionalText(raw.portfolioVersionId),
      linkedinProfileVersionId: optionalText(raw.linkedinProfileVersionId),
    };
    return localizeValidation(APPLICATION_SCHEMA_VALIDATOR.validate(candidate), {
      postingKey: '목록에서 표시한 64자리 공고 키를 입력하세요.',
      resumeVersionId: '필요한 산출물 버전 ID를 하나 이상 입력하고 중복하지 마세요.',
    });
  },
};

const INTERVIEW_VALIDATOR: FormValidator<CareerInterviewFormDraft> = {
  validate(raw) {
    const applicationId = optionalText(raw.applicationId);
    const decision = raw.decision;
    if (applicationId === undefined || (decision !== 'confirm' && decision !== 'decline')) {
      return { ok: false, errors: { applicationId: '지원 ID와 면접 결정을 확인하세요.' } };
    }
    return { ok: true, value: { applicationId, decision } };
  },
};

const PATH_ACTION_VALIDATOR: FormValidator<CareerPathFormDraft> = {
  validate(raw) {
    const resourceId = optionalText(raw.resourceId);
    const action = raw.action;
    if (
      resourceId === undefined ||
      (action !== 'withdrawApplication' &&
        action !== 'acceptInvitation' &&
        action !== 'declineInvitation' &&
        action !== 'acceptOffer' &&
        action !== 'declineOffer')
    ) {
      return { ok: false, errors: { resourceId: '처리할 명령과 대상 ID를 확인하세요.' } };
    }
    return { ok: true, value: { action, resourceId } };
  },
};

const MILITARY_SERVICE_START_VALIDATOR = withKoreanErrors(
  asFormValidator(MilitaryServiceStartDraftSchema),
  { militaryOptionVersionId: '시작할 복무 option을 선택하세요.' },
);

const MILITARY_SAVINGS_ENROLLMENT_SCHEMA_VALIDATOR = asFormValidator(
  MilitarySavingsEnrollmentDraftSchema,
);
const MILITARY_SAVINGS_ENROLLMENT_VALIDATOR: FormValidator<MilitarySavingsEnrollmentDraft> = {
  validate(raw) {
    return localizeValidation(
      MILITARY_SAVINGS_ENROLLMENT_SCHEMA_VALIDATOR.validate({
        productVersionId: raw.productVersionId,
        monthlyContributionKrw:
          typeof raw.monthlyContributionKrw === 'string'
            ? Number(raw.monthlyContributionKrw)
            : raw.monthlyContributionKrw,
        debitDayOfMonth:
          typeof raw.debitDayOfMonth === 'string'
            ? Number(raw.debitDayOfMonth)
            : raw.debitDayOfMonth,
      }),
      {
        productVersionId: '가입할 장병적금 상품을 선택하세요.',
        monthlyContributionKrw: '월 납입액을 원 단위 양의 정수로 입력하세요.',
        debitDayOfMonth: '납입일을 1~31 사이 정수로 입력하세요.',
      },
    );
  },
};

const MILITARY_SAVINGS_CLOSE_SCHEMA_VALIDATOR = asFormValidator(
  MilitaryServiceStartDraftSchema.pick({ militaryOptionVersionId: true }),
);
const MILITARY_SAVINGS_CLOSE_VALIDATOR: FormValidator<MilitarySavingsCloseDraft> = {
  validate(raw) {
    const result = MILITARY_SAVINGS_CLOSE_SCHEMA_VALIDATOR.validate({
      militaryOptionVersionId: raw.contractId,
    });
    return result.ok
      ? { ok: true, value: { contractId: result.value.militaryOptionVersionId } }
      : { ok: false, errors: { contractId: '중도해지할 활성 계약을 선택하세요.' } };
  },
};

const EVIDENCE_KIND_LABEL: Record<EvidenceKind, string> = {
  education: '학력',
  certification: '자격증',
  language: '어학',
  training: '교육',
  experience: '경력',
  project: '프로젝트',
};

const LIFE_STATUS_LABEL: Record<LifeStatus, string> = {
  unemployed: '미취업',
  employed: '재직',
  activeDuty: '현역 복무',
  socialService: '사회복무',
  specialService: '대체복무',
  officerOrNco: '간부 복무',
};

const ARTIFACT_KIND_LABEL: Record<CareerArtifactKind, string> = {
  portfolio: '포트폴리오',
  resume: '이력서',
  linkedinProfile: 'LinkedIn 프로필',
};

const INDUSTRY_LABEL: Record<CareerIndustry, string> = {
  itSoftware: 'IT·소프트웨어',
  financeInsurance: '금융·보험',
  manufacturing: '제조',
  constructionEngineering: '건설·엔지니어링',
  retailService: '유통·서비스',
  publicSocial: '공공·사회',
};

const REGION_LABEL: Record<CareerJob['region'], string> = {
  capitalArea: '수도권',
  metropolitan: '광역시',
  smallCity: '중소도시',
  rural: '농어촌',
};

const EMPLOYMENT_TYPE_LABEL: Record<CareerJob['employmentType'], string> = {
  regular: '정규직',
};

const MILITARY_REQUIREMENT_LABEL: Record<CareerJob['militaryRequirement'], string> = {
  any: '제한 없음',
  completedOrExempt: '군필 또는 면제',
};

const MILITARY_STATUS_LABEL = {
  unserved: '미필',
  serving: '복무 중',
  completed: '복무 완료',
  exempt: '면제',
} as const;

const MILITARY_SERVICE_TYPE_LABEL: Record<MilitaryServiceType, string> = {
  activeDuty: '현역',
  socialService: '사회복무요원',
  industrialTechnical: '산업기능요원',
  professionalResearch: '전문연구요원',
  commissionedOfficer: '장교',
  nonCommissionedOfficer: '부사관',
};

const MILITARY_SERVICE_STATUS_LABEL: Record<MilitaryServiceHistory['status'], string> = {
  pendingStart: '시작 대기',
  serving: '복무 중',
  completed: '복무 완료',
};

const MILITARY_SERVICE_SOURCE_LABEL: Record<MilitaryServiceHistory['sourceKind'], string> = {
  userCommand: '사용자 명령',
  legacyBridge: '기존 런 연결',
};

const MILITARY_SAVINGS_STATUS_LABEL: Record<MilitarySavingsHistoryItem['status'], string> = {
  active: '유지 중',
  matured: '만기',
  closed: '중도해지',
};

const MILITARY_SAVINGS_INSTALLMENT_STATUS_LABEL: Record<
  MilitarySavingsHistoryItem['installments'][number]['status'],
  string
> = {
  scheduled: '예정',
  paid: '납입',
  missed: '미납',
};

const CAREER_ACTION_SCHEDULE_LABEL: Record<
  Extract<CareerPendingScheduleItem, { sourceKind: 'careerAction' }>['kind'],
  string
> = {
  employmentStart: '근로계약 시작',
  militaryServiceStart: '복무 시작',
  militaryServiceCompletion: '복무 완료',
  documentReview: '서류 심사',
  confirmationExpiry: '면접 확인 만료',
  interviewDecision: '면접 결과',
  offerExpiry: '오퍼 만료',
  invitationGeneration: '역제안 생성',
};

const CAREER_SETTLEMENT_SCHEDULE_LABEL: Record<
  Extract<CareerPendingScheduleItem, { sourceKind: 'settlement' }>['kind'],
  string
> = {
  employmentPayroll: '급여 지급',
  employmentReconciliation: '연말정산',
  militaryPay: '군 급여 지급',
  militarySavingsInstallment: '장병적금 납입',
  militarySavingsMaturity: '장병적금 만기',
  militarySavingsGovernmentMatch: '장병적금 정부지원금',
};

const EDUCATION_LEVEL_LABEL = {
  highSchool: '고등학교',
  associate: '전문학사',
  bachelor: '학사',
  master: '석사',
  doctorate: '박사',
} as const;

const MILITARY_COMPENSATION_LABEL: Record<MilitaryOption['compensationKind'], string> = {
  militaryPay: '군 급여',
  employmentPayroll: '일반 급여 계산',
};

const JOB_SCORE_DIMENSIONS = [
  ['education', '학력'],
  ['certification', '자격증'],
  ['language', '어학'],
  ['training', '교육'],
  ['experience', '경력'],
  ['project', '프로젝트'],
] as const satisfies readonly (readonly [keyof CareerJob['requiredScores'], string])[];

/** M3-A career controls and bounded history views. */
export function createCareerView(deps: CareerDeps): ViewFactory {
  const focusRetries = createCareerFocusRetryPolicy({ createCommandId: deps.createCommandId });
  const activityStartRetries = createCareerActivityStartRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const activityCancelRetries = createCareerActivityCancelRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const artifactRetries = createCareerArtifactRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const applicationRetries = createCareerApplicationRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const interviewRetries = createCareerInterviewRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const pathRetries = createCareerPathRetryPolicy({ createCommandId: deps.createCommandId });
  const militaryServiceStartRetries = createMilitaryServiceStartRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const militarySavingsEnrollmentRetries = createMilitarySavingsEnrollmentRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const militarySavingsCloseRetries = createMilitarySavingsCloseRetryPolicy({
    createCommandId: deps.createCommandId,
  });

  const submitFocus = async (draft: CareerFocusDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '기본 직무군을 변경');
    const request = focusRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.focus(request);
      focusRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 기본 직무군 변경 결과를 확인했습니다.'
          : `기본 직무군 변경: ${response.result.focusedJobFamilyKey}`,
        { tone: 'success' },
      );
    } catch (error) {
      focusRetries.fail(request, error);
      throw careerDisplayError(error, '기본 직무군 변경');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitActivityStart = async (draft: CareerActivityStartDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '활동을 시작');
    const request = activityStartRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.startActivity(request);
      activityStartRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 활동 시작 결과를 확인했습니다.'
          : `활동 #${response.result.activityId}을 시작했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      activityStartRetries.fail(request, error);
      throw careerDisplayError(error, '활동 시작');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitActivityCancel = async (draft: CareerCancelFormDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '활동을 취소');
    const command = activityCancelRetries.select(snapshot, {
      activityId: draft.activityId,
    });
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.cancelActivity(command.activityId, command.request);
      activityCancelRetries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 활동 취소 결과를 확인했습니다.'
          : `활동 #${response.result.activityId}을 취소했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      activityCancelRetries.fail(command, error);
      throw careerDisplayError(error, '활동 취소');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitArtifact = async (draft: CareerArtifactDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '산출물을 게시');
    const request = artifactRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.publishArtifact(request);
      artifactRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 산출물 게시 결과를 확인했습니다.'
          : `${ARTIFACT_KIND_LABEL[response.result.kind]} ${response.result.versionNo}버전을 게시했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      artifactRetries.fail(request, error);
      throw careerDisplayError(error, '산출물 게시');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitApplication = async (draft: CareerApplicationDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '공고에 지원');
    const request = applicationRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.apply(request);
      applicationRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 지원 결과를 확인했습니다.'
          : `지원 #${response.result.applicationId}을 제출했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      applicationRetries.fail(request, error);
      throw careerDisplayError(error, '공고 지원');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitInterview = async (draft: CareerInterviewFormDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '면접 확인을 처리');
    const command = interviewRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.confirmInterview(command.applicationId, command.request);
      interviewRetries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 면접 확인 결과를 확인했습니다.'
          : '면접 확인 결과를 저장했습니다.',
        { tone: 'success' },
      );
    } catch (error) {
      interviewRetries.fail(command, error);
      throw careerDisplayError(error, '면접 확인');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitPathAction = async (draft: CareerPathFormDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '채용 명령을 처리');
    const command = pathRetries.select(snapshot, draft.action, draft.resourceId);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await runCareerPathAction(deps.api, command);
      pathRetries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 채용 명령 결과를 확인했습니다.'
          : '채용 명령을 처리했습니다.',
        {
          tone: 'success',
        },
      );
    } catch (error) {
      pathRetries.fail(command, error);
      throw careerDisplayError(error, '채용 명령');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitMilitaryServiceStart = async (draft: MilitaryServiceStartDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '복무를 시작');
    const request = militaryServiceStartRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.startMilitaryService(request);
      militaryServiceStartRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 복무 시작 결과를 확인했습니다.'
          : `복무 #${response.result.militaryServiceId}의 시작을 예약했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      militaryServiceStartRetries.fail(request, error);
      throw careerDisplayError(error, '복무 시작');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitMilitarySavingsEnrollment = async (
    draft: MilitarySavingsEnrollmentDraft,
  ): Promise<void> => {
    const snapshot = commandSnapshot(deps, '장병적금에 가입');
    const request = militarySavingsEnrollmentRetries.select(snapshot, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.enrollMilitarySavings(request);
      militarySavingsEnrollmentRetries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 장병적금 가입 결과를 확인했습니다.'
          : `장병적금 #${response.result.militarySavingsContractId}에 가입했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      militarySavingsEnrollmentRetries.fail(request, error);
      throw careerDisplayError(error, '장병적금 가입');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  const submitMilitarySavingsClose = async (draft: MilitarySavingsCloseDraft): Promise<void> => {
    const snapshot = commandSnapshot(deps, '장병적금을 중도해지');
    const command = militarySavingsCloseRetries.select(snapshot, draft.contractId);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.closeMilitarySavings(command.contractId, command.request);
      militarySavingsCloseRetries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(
        response.replayed
          ? '이미 처리된 장병적금 중도해지 결과를 확인했습니다.'
          : `장병적금 #${response.result.militarySavingsContractId}을 중도해지했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      militarySavingsCloseRetries.fail(command, error);
      throw careerDisplayError(error, '장병적금 중도해지');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };

  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host, ctx) {
        const h = createHooks(ctx.bag);
        const snapshot = h.useStoreValue(
          deps.store,
          paths.gameSnapshot,
          (state) => state.game.snapshot,
        );
        const career = h.useStoreValue(
          deps.store,
          paths.gameCareer,
          (state) => state.game.snapshot?.career,
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
        const gameReady = h.useComputed(() => {
          const current = snapshot.get();
          return current !== undefined && current.characterName !== null;
        });
        const canMutate = h.useComputed(() => {
          const current = snapshot.get();
          return (
            current !== undefined &&
            current.characterName !== null &&
            current.autoSpeed === null &&
            !advancing.get() &&
            !ordering.get()
          );
        });

        const evidenceItems = h.useSignal<readonly CareerEvidence[]>([]);
        const evidenceNextBefore = h.useSignal<string | null>(null);
        const activityCatalog = h.useSignal<readonly CareerActivityCatalogEntry[]>([]);
        const queriedActiveActivities = h.useSignal<readonly CareerActivitySummary[] | undefined>(
          undefined,
        );
        const activityHistoryItems = h.useSignal<readonly CareerActivityHistoryItem[]>([]);
        const activityNextBefore = h.useSignal<string | null>(null);
        const artifactItems = h.useSignal<readonly CareerArtifact[]>([]);
        const artifactNextBefore = h.useSignal<string | null>(null);
        const jobItems = h.useSignal<readonly CareerJob[]>([]);
        const jobNextBefore = h.useSignal<string | null>(null);
        const applicationItems = h.useSignal<readonly CareerApplication[]>([]);
        const applicationNextBefore = h.useSignal<string | null>(null);
        const invitationItems = h.useSignal<readonly CareerInvitation[]>([]);
        const employment = h.useSignal<CareerEmploymentContract | null>(null);
        const payrollItems = h.useSignal<readonly CareerPayrollItem[]>([]);
        const payrollNextBefore = h.useSignal<string | null>(null);
        const selectedTaxYear = h.useSignal<number | null>(
          career.peek()?.currentEmploymentTaxYear.taxYear ?? null,
        );
        const taxYearInputError = h.useSignal<string | null>(null);
        const militaryOptions = h.useSignal<readonly MilitaryOption[]>([]);
        const militaryService = h.useSignal<MilitaryServiceResponse | undefined>(undefined);
        const militarySavingsProducts = h.useSignal<readonly MilitarySavingsProduct[]>([]);
        const militarySavingsItems = h.useSignal<readonly MilitarySavingsHistoryItem[]>([]);
        const militarySavingsNextBefore = h.useSignal<string | null>(null);

        let specsGeneration = 0;
        let activitiesGeneration = 0;
        let artifactsGeneration = 0;
        let jobsGeneration = 0;
        let applicationsGeneration = 0;
        let payrollGeneration = 0;
        let militarySavingsGeneration = 0;
        let specsPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: specsGeneration,
          limit: PAGE_SIZE,
        };
        let activitiesPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: activitiesGeneration,
          limit: PAGE_SIZE,
        };
        let artifactsPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: artifactsGeneration,
          limit: PAGE_SIZE,
        };
        let jobsPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: jobsGeneration,
          limit: PAGE_SIZE,
        };
        let applicationsPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: applicationsGeneration,
          limit: PAGE_SIZE,
        };
        let payrollPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: payrollGeneration,
          limit: PAGE_SIZE,
        };
        let militarySavingsPageRequest: PageLoadRequest = {
          kind: 'initial',
          generation: militarySavingsGeneration,
          limit: PAGE_SIZE,
        };
        let jobPlatformFilter: CareerPlatform | undefined;
        let jobIndustryFilter: CareerIndustry | undefined;

        const specsRequest = h.useAsync(async (signal) => {
          const request = specsPageRequest;
          const response = await deps.api.getSpecs(pageQuery(request), signal);
          return { request, response };
        });
        const activitiesRequest = h.useAsync(async (signal) => {
          const request = activitiesPageRequest;
          const response = await deps.api.getActivities(pageQuery(request), signal);
          return { request, response };
        });
        const artifactsRequest = h.useAsync(async (signal) => {
          const request = artifactsPageRequest;
          const response = await deps.api.getArtifacts(pageQuery(request), signal);
          return { request, response };
        });
        const jobsRequest = h.useAsync(async (signal) => {
          const request = jobsPageRequest;
          const response = await deps.api.getJobs(
            {
              ...pageQuery(request),
              ...(jobPlatformFilter === undefined ? {} : { platform: jobPlatformFilter }),
              ...(jobIndustryFilter === undefined ? {} : { industry: jobIndustryFilter }),
            },
            signal,
          );
          return { request, response };
        });
        const applicationsRequest = h.useAsync(async (signal) => {
          const request = applicationsPageRequest;
          const response = await deps.api.getApplications(pageQuery(request), signal);
          return { request, response };
        });
        const employmentRequest = h.useAsync((signal) => deps.api.getEmployment(signal));
        const payrollRequest = h.useAsync(async (signal) => {
          const request = payrollPageRequest;
          const response = await deps.api.getPayroll(pageQuery(request), signal);
          return { request, response };
        });
        const taxYearRequest = h.useAsync(async (signal) => {
          const year = selectedTaxYear.peek();
          if (year === null) throw new Error('조회할 귀속연도가 없습니다.');
          return deps.api.getTaxYear(year, signal);
        });
        const militaryOptionsRequest = h.useAsync((signal) => deps.api.getMilitaryOptions(signal));
        const militaryServiceRequest = h.useAsync((signal) => deps.api.getMilitaryService(signal));
        const militarySavingsProductsRequest = h.useAsync((signal) =>
          deps.api.getMilitarySavingsProducts(signal),
        );
        const militarySavingsRequest = h.useAsync(async (signal) => {
          const request = militarySavingsPageRequest;
          const response = await deps.api.getMilitarySavings(pageQuery(request), signal);
          return { request, response };
        });
        const taxYearResult = h.useComputed<CareerTaxYearState | null>(() => {
          if (!gameReady.get()) return null;
          const state = taxYearRequest.state.get();
          return state.status === 'success' ? state.value : null;
        });
        const catalogAvailable = h.useComputed(() => {
          return activityCatalog.get().length > 0;
        });
        const activeActivities = h.useComputed<readonly CareerActivitySummary[]>(() => {
          return queriedActiveActivities.get() ?? career.get()?.activeActivities ?? [];
        });
        const eligibleMilitaryOptions = h.useComputed(() =>
          militaryOptions.get().filter((option) => option.eligible),
        );
        const eligibleMilitarySavingsProducts = h.useComputed(() =>
          militarySavingsProducts.get().filter((product) => product.eligible),
        );

        const focusForm = renderForm(
          {
            fields: FOCUS_FIELDS,
            validator: FOCUS_VALIDATOR,
            submitLabel: '기본 직무군 변경',
            idPrefix: 'career-focus',
          },
          {
            initial: { focusedJobFamilyKey: career.peek()?.focusedJobFamilyKey ?? '' },
            onSubmit: submitFocus,
          },
        );
        const activityStartForm = renderForm(
          {
            fields: ACTIVITY_START_FIELDS,
            validator: ACTIVITY_START_VALIDATOR,
            submitLabel: '활동 시작',
            idPrefix: 'career-activity-start',
          },
          { initial: { priority: 1 }, onSubmit: submitActivityStart },
        );
        const activityCancelForm = renderForm(
          {
            fields: ACTIVITY_CANCEL_FIELDS,
            validator: ACTIVITY_CANCEL_VALIDATOR,
            submitLabel: '활동 취소',
            idPrefix: 'career-activity-cancel',
          },
          { onSubmit: submitActivityCancel },
        );
        const artifactForm = renderForm(
          {
            fields: ARTIFACT_FIELDS,
            validator: ARTIFACT_VALIDATOR,
            submitLabel: '새 버전 게시',
            idPrefix: 'career-artifact',
          },
          {
            initial: {
              kind: 'portfolio',
              headline: '',
              summary: '',
              evidenceIds: '',
              openToWork: false,
              industries: '',
            },
            onSubmit: submitArtifact,
          },
        );
        const applicationForm = renderForm(
          {
            fields: APPLICATION_FIELDS,
            validator: APPLICATION_VALIDATOR,
            submitLabel: '공고 지원',
            idPrefix: 'career-application',
          },
          { onSubmit: submitApplication },
        );
        const interviewForm = renderForm(
          {
            fields: INTERVIEW_FIELDS,
            validator: INTERVIEW_VALIDATOR,
            submitLabel: '면접 확인 처리',
            idPrefix: 'career-interview',
          },
          { initial: { decision: 'confirm' }, onSubmit: submitInterview },
        );
        const pathActionForm = renderForm(
          {
            fields: PATH_ACTION_FIELDS,
            validator: PATH_ACTION_VALIDATOR,
            submitLabel: '선택한 채용 명령 처리',
            idPrefix: 'career-path-action',
          },
          { initial: { action: 'withdrawApplication' }, onSubmit: submitPathAction },
        );
        const militaryServiceStartForm = renderForm(
          {
            fields: MILITARY_SERVICE_START_FIELDS,
            validator: MILITARY_SERVICE_START_VALIDATOR,
            submitLabel: '복무 시작 예약',
            idPrefix: 'military-service-start',
          },
          { onSubmit: submitMilitaryServiceStart },
        );
        const militarySavingsEnrollmentForm = renderForm(
          {
            fields: MILITARY_SAVINGS_ENROLLMENT_FIELDS,
            validator: MILITARY_SAVINGS_ENROLLMENT_VALIDATOR,
            submitLabel: '장병적금 가입',
            idPrefix: 'military-savings-enrollment',
          },
          {
            initial: { monthlyContributionKrw: 250_000, debitDayOfMonth: 25 },
            onSubmit: submitMilitarySavingsEnrollment,
          },
        );
        const militarySavingsCloseForm = renderForm(
          {
            fields: MILITARY_SAVINGS_CLOSE_FIELDS,
            validator: MILITARY_SAVINGS_CLOSE_VALIDATOR,
            submitLabel: '장병적금 중도해지',
            idPrefix: 'military-savings-close',
          },
          { onSubmit: submitMilitarySavingsClose },
        );
        ctx.bag.add(focusForm);
        ctx.bag.add(activityStartForm);
        ctx.bag.add(activityCancelForm);
        ctx.bag.add(artifactForm);
        ctx.bag.add(applicationForm);
        ctx.bag.add(interviewForm);
        ctx.bag.add(pathActionForm);
        ctx.bag.add(militaryServiceStartForm);
        ctx.bag.add(militarySavingsEnrollmentForm);
        ctx.bag.add(militarySavingsCloseForm);

        const focusSubmit = submitButtonOf(focusForm.element);
        const activityStartSubmit = submitButtonOf(activityStartForm.element);
        const activityCancelSubmit = submitButtonOf(activityCancelForm.element);
        const artifactSubmit = submitButtonOf(artifactForm.element);
        const applicationSubmit = submitButtonOf(applicationForm.element);
        const interviewSubmit = submitButtonOf(interviewForm.element);
        const pathActionSubmit = submitButtonOf(pathActionForm.element);
        const militaryServiceStartSubmit = submitButtonOf(militaryServiceStartForm.element);
        const militarySavingsEnrollmentSubmit = submitButtonOf(
          militarySavingsEnrollmentForm.element,
        );
        const militarySavingsCloseSubmit = submitButtonOf(militarySavingsCloseForm.element);
        const artifactKind = selectFieldOf(artifactForm.element, 'kind');
        const openToWorkRow = fieldRowOf(artifactForm.element, 'openToWork');
        const industriesRow = fieldRowOf(artifactForm.element, 'industries');

        const currentFocusValue = el('strong');
        const commandStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const educationScore = el('dd');
        const certificationScore = el('dd');
        const languageScore = el('dd');
        const trainingScore = el('dd');
        const experienceScore = el('dd');
        const projectScore = el('dd');

        const specsStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const specsRefresh = el('button', { type: 'button' }, '스펙 다시 조회');
        const specsLoadOlder = el('button', { type: 'button' }, '이전 증빙 더 보기');
        const evidenceList = createFixedList(HISTORY_CAPACITY, evidenceText);

        const activitiesStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const activitiesRefresh = el('button', { type: 'button' }, '활동 다시 조회');
        const activitiesLoadOlder = el('button', { type: 'button' }, '이전 활동 이력 더 보기');
        const activeList = createFixedList(ACTIVE_ACTIVITY_CAPACITY, activityText);
        const catalogList = createFixedList(CATALOG_CAPACITY, catalogEntryText);
        const activityHistoryList = createFixedList(HISTORY_CAPACITY, activityHistoryText);

        const artifactsStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const artifactsRefresh = el('button', { type: 'button' }, '산출물 다시 조회');
        const artifactsLoadOlder = el('button', { type: 'button' }, '이전 버전 더 보기');
        const latestArtifactList = createFixedList(LATEST_ARTIFACT_CAPACITY, artifactSummaryText);
        const artifactList = createFixedList(HISTORY_CAPACITY, artifactText);
        const jobsStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
        const jobsRefresh = el('button', { type: 'button' }, '공고 다시 조회');
        const jobsLoadOlder = el('button', { type: 'button' }, '이전 공고 더 보기');
        const platformFilter = selectWithOptions([
          { value: '', label: '전체 플랫폼' },
          { value: 'sarangbang', label: '사랑방' },
          { value: 'jobkorea', label: '잡코리아' },
          { value: 'saramin', label: '사람인' },
          { value: 'wanted', label: '원티드' },
          { value: 'linkedin', label: 'LinkedIn' },
          { value: 'work24', label: '고용24' },
        ]);
        const industryFilter = selectWithOptions([
          { value: '', label: '전체 업종' },
          ...Object.entries(INDUSTRY_LABEL).map(([value, label]) => ({ value, label })),
        ]);
        const jobList = createFixedList(JOB_CAPACITY, jobText);
        const applicationsStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
        const applicationsRefresh = el('button', { type: 'button' }, '지원 현황 다시 조회');
        const applicationsLoadOlder = el('button', { type: 'button' }, '이전 지원 더 보기');
        const applicationList = createFixedList(HISTORY_CAPACITY, applicationText);
        const invitationList = createFixedList(OPEN_INVITATION_CAPACITY, invitationText);
        const pendingCareerScheduleList = createFixedList(
          PENDING_CAREER_SCHEDULE_CAPACITY,
          pendingCareerScheduleText,
        );
        const employmentStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
        const payrollStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
        const payrollRefresh = el('button', { type: 'button' }, '급여 다시 조회');
        const payrollLoadOlder = el('button', { type: 'button' }, '이전 급여 더 보기');
        const payrollList = createFixedList(HISTORY_CAPACITY, payrollText);
        const taxYearRequestStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const taxYearInput = el('input', {
          type: 'number',
          attrs: { min: '1', max: '9999', step: '1', 'aria-label': '조회할 귀속연도' },
        });
        const initialTaxYear = selectedTaxYear.peek();
        if (initialTaxYear !== null) taxYearInput.value = initialTaxYear.toString();
        const taxYearRefresh = el('button', { type: 'button' }, '연말정산 조회');
        const taxYearValues = {
          year: el('dd'),
          status: el('dd'),
          source: el('dd'),
          grossEmploymentIncome: el('dd'),
          employeeInsuranceDeduction: el('dd'),
          earnedIncomeDeduction: el('dd'),
          personalDeduction: el('dd'),
          taxableIncome: el('dd'),
          calculatedIncomeTax: el('dd'),
          earnedIncomeTaxCredit: el('dd'),
          withheldIncomeTax: el('dd'),
          withheldLocalIncomeTax: el('dd'),
          currentExpectedPensionCredit: el('dd'),
          pensionCreditEligibleContribution: el('dd'),
          actualPensionIncomeTaxCredit: el('dd'),
          actualPensionLocalIncomeTaxEffect: el('dd'),
          assessedIncomeTax: el('dd'),
          assessedLocalIncomeTax: el('dd'),
          refund: el('dd'),
          additionalTax: el('dd'),
          reconciliationGameDay: el('dd'),
        };
        const militaryStatusValue = el('dd');
        const activeMilitaryServiceValue = el('dd');
        const militaryServiceHistoryValue = el('dd');
        const militaryOptionsStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const militaryOptionsRefresh = el('button', { type: 'button' }, '복무 option 다시 조회');
        const militaryOptionList = createFixedList(MILITARY_OPTION_CAPACITY, militaryOptionText);
        const militaryServiceStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const militaryServiceRefresh = el('button', { type: 'button' }, '복무 이력 다시 조회');
        const militarySavingsProductsStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const militarySavingsProductsRefresh = el(
          'button',
          { type: 'button' },
          '장병적금 상품 다시 조회',
        );
        const militarySavingsProductList = createFixedList(
          MILITARY_SAVINGS_PRODUCT_CAPACITY,
          militarySavingsProductText,
        );
        const activeMilitarySavingsList = createFixedList(
          ACTIVE_MILITARY_SAVINGS_CAPACITY,
          activeMilitarySavingsText,
        );
        const militarySavingsHistoryStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const militarySavingsHistoryRefresh = el(
          'button',
          { type: 'button' },
          '장병적금 이력 다시 조회',
        );
        const militarySavingsHistoryLoadOlder = el(
          'button',
          { type: 'button' },
          '이전 장병적금 더 보기',
        );
        const militarySavingsHistoryList = createFixedList(
          HISTORY_CAPACITY,
          militarySavingsHistoryText,
        );

        root = el(
          'main',
          { class: 'career' },
          el('h1', {}, '커리어'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          commandStatus,
          el(
            'section',
            {},
            el('h2', {}, '스펙과 기본 직무군'),
            el('p', {}, '현재 기본 직무군: ', currentFocusValue),
            el(
              'dl',
              {},
              el('dt', {}, '학력'),
              educationScore,
              el('dt', {}, '자격증'),
              certificationScore,
              el('dt', {}, '어학'),
              languageScore,
              el('dt', {}, '교육'),
              trainingScore,
              el('dt', {}, '경력'),
              experienceScore,
              el('dt', {}, '프로젝트'),
              projectScore,
            ),
            el(
              'p',
              {},
              '위 점수는 기본 직무군 기준 보유 점수입니다. 지원서에 고정되는 공개 점수와는 다릅니다.',
            ),
            el('fieldset', {}, el('legend', {}, '기본 직무군 변경'), focusForm.element),
            el('h3', {}, '취득 증빙'),
            specsStatus,
            specsRefresh,
            evidenceList.element,
            specsLoadOlder,
          ),
          el(
            'section',
            {},
            el('h2', {}, '성장 활동'),
            activitiesStatus,
            activitiesRefresh,
            el('h3', {}, '진행 중인 활동'),
            activeList.element,
            el('fieldset', {}, el('legend', {}, '활동 시작'), activityStartForm.element),
            el('fieldset', {}, el('legend', {}, '활동 취소'), activityCancelForm.element),
            el('h3', {}, '활동 카탈로그'),
            catalogList.element,
            el('h3', {}, '활동 이력'),
            activityHistoryList.element,
            activitiesLoadOlder,
          ),
          el(
            'section',
            {},
            el('h2', {}, '산출물'),
            artifactsStatus,
            artifactsRefresh,
            el('h3', {}, '스냅샷의 최신 버전'),
            latestArtifactList.element,
            el('h3', {}, '새 불변 버전 작성'),
            artifactForm.element,
            el('h3', {}, '버전 이력'),
            artifactList.element,
            artifactsLoadOlder,
          ),
          el(
            'section',
            {},
            el('h2', {}, '채용 공고'),
            el('label', {}, '플랫폼 ', platformFilter),
            el('label', {}, '업종 ', industryFilter),
            jobsStatus,
            jobsRefresh,
            jobList.element,
            jobsLoadOlder,
            el('fieldset', {}, el('legend', {}, '공고 지원'), applicationForm.element),
          ),
          el(
            'section',
            {},
            el('h2', {}, '지원·역제안·오퍼'),
            applicationsStatus,
            applicationsRefresh,
            el('h3', {}, '지원 현황'),
            applicationList.element,
            applicationsLoadOlder,
            el('h3', {}, '열린 역제안'),
            invitationList.element,
            el('fieldset', {}, el('legend', {}, '면접 확인'), interviewForm.element),
            el('fieldset', {}, el('legend', {}, '지원·역제안·오퍼 처리'), pathActionForm.element),
          ),
          el('section', {}, el('h2', {}, '예정된 커리어 일정'), pendingCareerScheduleList.element),
          el(
            'section',
            {},
            el('h2', {}, '근로계약과 급여'),
            employmentStatus,
            payrollStatus,
            payrollRefresh,
            payrollList.element,
            payrollLoadOlder,
          ),
          el(
            'section',
            {},
            el('h2', {}, '연말정산'),
            el('label', {}, '귀속연도 ', taxYearInput),
            taxYearRefresh,
            taxYearRequestStatus,
            el(
              'dl',
              {},
              el('dt', {}, '조회 귀속연도'),
              taxYearValues.year,
              el('dt', {}, '정산 상태'),
              taxYearValues.status,
              el('dt', {}, '정산 출처'),
              taxYearValues.source,
              el('dt', {}, '총급여'),
              taxYearValues.grossEmploymentIncome,
              el('dt', {}, '보험료 소득공제'),
              taxYearValues.employeeInsuranceDeduction,
              el('dt', {}, '근로소득공제'),
              taxYearValues.earnedIncomeDeduction,
              el('dt', {}, '기본 인적공제'),
              taxYearValues.personalDeduction,
              el('dt', {}, '과세표준'),
              taxYearValues.taxableIncome,
              el('dt', {}, '산출세액'),
              taxYearValues.calculatedIncomeTax,
              el('dt', {}, '근로소득세액공제'),
              taxYearValues.earnedIncomeTaxCredit,
              el('dt', {}, '원천 소득세'),
              taxYearValues.withheldIncomeTax,
              el('dt', {}, '원천 지방소득세'),
              taxYearValues.withheldLocalIncomeTax,
              el('dt', {}, '현재 연금 예상 공제 합계'),
              taxYearValues.currentExpectedPensionCredit,
              el('dt', {}, '연금 공제 대상 납입액'),
              taxYearValues.pensionCreditEligibleContribution,
              el('dt', {}, '실제 연금 소득세공제'),
              taxYearValues.actualPensionIncomeTaxCredit,
              el('dt', {}, '실제 연금 지방소득세 효과'),
              taxYearValues.actualPensionLocalIncomeTaxEffect,
              el('dt', {}, '확정 소득세'),
              taxYearValues.assessedIncomeTax,
              el('dt', {}, '확정 지방소득세'),
              taxYearValues.assessedLocalIncomeTax,
              el('dt', {}, '환급'),
              taxYearValues.refund,
              el('dt', {}, '추가 납부'),
              taxYearValues.additionalTax,
              el('dt', {}, '회사 정산 게임일'),
              taxYearValues.reconciliationGameDay,
            ),
            el(
              'p',
              {},
              '현재 연금 예상 공제 합계는 현재 스냅샷 기준이며, 선택한 과거 귀속연도의 예상치가 아닙니다.',
            ),
          ),
          el(
            'section',
            {},
            el('h2', {}, '병역과 장병내일준비적금'),
            el(
              'dl',
              {},
              el('dt', {}, '병역 상태'),
              militaryStatusValue,
              el('dt', {}, '스냅샷 복무 진행'),
              activeMilitaryServiceValue,
              el('dt', {}, '복무 전체 이력'),
              militaryServiceHistoryValue,
            ),
            militaryServiceStatus,
            militaryServiceRefresh,
            el('h3', {}, '복무 option과 자격'),
            militaryOptionsStatus,
            militaryOptionsRefresh,
            militaryOptionList.element,
            el('fieldset', {}, el('legend', {}, '복무 시작'), militaryServiceStartForm.element),
            el('h3', {}, '장병적금 상품'),
            militarySavingsProductsStatus,
            militarySavingsProductsRefresh,
            militarySavingsProductList.element,
            el(
              'fieldset',
              {},
              el('legend', {}, '장병적금 가입'),
              militarySavingsEnrollmentForm.element,
            ),
            el('h3', {}, '스냅샷의 활성 장병적금'),
            activeMilitarySavingsList.element,
            el(
              'fieldset',
              {},
              el('legend', {}, '장병적금 중도해지'),
              militarySavingsCloseForm.element,
            ),
            el('h3', {}, '장병적금 납입·만기 이력'),
            militarySavingsHistoryStatus,
            militarySavingsHistoryRefresh,
            militarySavingsHistoryList.element,
            militarySavingsHistoryLoadOlder,
            el(
              'p',
              {},
              '금리·급여·만기 예상액은 서버가 보낸 값만 표시하며, 화면에서는 다시 계산하지 않습니다.',
            ),
          ),
        );
        host.replaceChildren(root);

        h.bindText(currentFocusValue, () => career.get()?.focusedJobFamilyKey ?? '—');
        h.bindText(commandStatus, () =>
          careerCommandStatus(snapshot.get(), advancing.get(), ordering.get()),
        );
        h.bindText(educationScore, () => scoreText(career.get()?.possessedScores.education));
        h.bindText(certificationScore, () =>
          scoreText(career.get()?.possessedScores.certification),
        );
        h.bindText(languageScore, () => scoreText(career.get()?.possessedScores.language));
        h.bindText(trainingScore, () => scoreText(career.get()?.possessedScores.training));
        h.bindText(experienceScore, () => scoreText(career.get()?.possessedScores.experience));
        h.bindText(projectScore, () => scoreText(career.get()?.possessedScores.project));
        h.bindText(specsStatus, () =>
          requestStatusText(
            specsRequest.state.get(),
            gameReady.get(),
            '증빙',
            evidenceItems.get().length,
            evidenceNextBefore.get(),
          ),
        );
        h.bindText(activitiesStatus, () =>
          requestStatusText(
            activitiesRequest.state.get(),
            gameReady.get(),
            '활동 이력',
            activityHistoryItems.get().length,
            activityNextBefore.get(),
          ),
        );
        h.bindText(artifactsStatus, () =>
          requestStatusText(
            artifactsRequest.state.get(),
            gameReady.get(),
            '산출물 버전',
            artifactItems.get().length,
            artifactNextBefore.get(),
          ),
        );
        h.bindText(jobsStatus, () =>
          requestStatusText(
            jobsRequest.state.get(),
            gameReady.get(),
            '공고',
            jobItems.get().length,
            jobNextBefore.get(),
          ),
        );
        h.bindText(applicationsStatus, () =>
          requestStatusText(
            applicationsRequest.state.get(),
            gameReady.get(),
            '지원',
            applicationItems.get().length,
            applicationNextBefore.get(),
          ),
        );
        h.bindText(employmentStatus, () => employmentText(employment.get()));
        h.bindText(payrollStatus, () =>
          requestStatusText(
            payrollRequest.state.get(),
            gameReady.get(),
            '급여',
            payrollItems.get().length,
            payrollNextBefore.get(),
          ),
        );
        h.bindText(taxYearRequestStatus, () =>
          taxYearRequestStatusText(
            taxYearRequest.state.get(),
            gameReady.get(),
            selectedTaxYear.get(),
            taxYearInputError.get(),
          ),
        );
        h.bindText(taxYearValues.year, () => taxYearText(taxYearResult.get()));
        h.bindText(taxYearValues.status, () => taxYearStatusText(taxYearResult.get()));
        h.bindText(taxYearValues.source, () => taxYearSourceText(taxYearResult.get()));
        h.bindText(taxYearValues.grossEmploymentIncome, () =>
          employmentTaxMoneyText(taxYearResult.get()?.grossEmploymentIncomeKrw),
        );
        h.bindText(taxYearValues.employeeInsuranceDeduction, () =>
          employmentTaxMoneyText(taxYearResult.get()?.employeeInsuranceDeductionKrw),
        );
        h.bindText(taxYearValues.earnedIncomeDeduction, () =>
          employmentTaxMoneyText(taxYearResult.get()?.earnedIncomeDeductionKrw),
        );
        h.bindText(taxYearValues.personalDeduction, () =>
          employmentTaxMoneyText(taxYearResult.get()?.personalDeductionKrw),
        );
        h.bindText(taxYearValues.taxableIncome, () =>
          employmentTaxMoneyText(taxYearResult.get()?.taxableIncomeKrw),
        );
        h.bindText(taxYearValues.calculatedIncomeTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.calculatedIncomeTaxKrw),
        );
        h.bindText(taxYearValues.earnedIncomeTaxCredit, () =>
          employmentTaxMoneyText(taxYearResult.get()?.earnedIncomeTaxCreditKrw),
        );
        h.bindText(taxYearValues.withheldIncomeTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.withheldIncomeTaxKrw),
        );
        h.bindText(taxYearValues.withheldLocalIncomeTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.withheldLocalIncomeTaxKrw),
        );
        h.bindText(taxYearValues.currentExpectedPensionCredit, () =>
          currentExpectedPensionCreditText(snapshot.get()),
        );
        h.bindText(taxYearValues.pensionCreditEligibleContribution, () =>
          employmentTaxMoneyText(taxYearResult.get()?.pensionCreditEligibleContributionKrw),
        );
        h.bindText(taxYearValues.actualPensionIncomeTaxCredit, () =>
          employmentTaxMoneyText(taxYearResult.get()?.actualPensionIncomeTaxCreditKrw),
        );
        h.bindText(taxYearValues.actualPensionLocalIncomeTaxEffect, () =>
          employmentTaxMoneyText(taxYearResult.get()?.actualPensionLocalIncomeTaxEffectKrw),
        );
        h.bindText(taxYearValues.assessedIncomeTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.assessedIncomeTaxKrw),
        );
        h.bindText(taxYearValues.assessedLocalIncomeTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.assessedLocalIncomeTaxKrw),
        );
        h.bindText(taxYearValues.refund, () =>
          employmentTaxMoneyText(taxYearResult.get()?.refundKrw),
        );
        h.bindText(taxYearValues.additionalTax, () =>
          employmentTaxMoneyText(taxYearResult.get()?.additionalTaxKrw),
        );
        h.bindText(taxYearValues.reconciliationGameDay, () =>
          employmentTaxReconciliationText(taxYearResult.get()?.reconciliationGameDay),
        );
        h.bindText(militaryStatusValue, () => {
          const status = career.get()?.militaryStatus;
          return status === undefined ? '—' : MILITARY_STATUS_LABEL[status];
        });
        h.bindText(activeMilitaryServiceValue, () =>
          activeMilitaryServiceText(career.get()?.activeMilitaryService),
        );
        h.bindText(militaryServiceHistoryValue, () =>
          militaryServiceHistoryText(militaryService.get()?.service),
        );
        h.bindText(militaryServiceStatus, () =>
          militaryServiceRequestStatusText(militaryServiceRequest.state.get(), gameReady.get()),
        );
        h.bindText(militaryOptionsStatus, () =>
          requestStatusText(
            militaryOptionsRequest.state.get(),
            gameReady.get(),
            '복무 option',
            militaryOptions.get().length,
            null,
          ),
        );
        h.bindText(militarySavingsProductsStatus, () =>
          requestStatusText(
            militarySavingsProductsRequest.state.get(),
            gameReady.get(),
            '장병적금 상품',
            militarySavingsProducts.get().length,
            null,
          ),
        );
        h.bindText(militarySavingsHistoryStatus, () =>
          requestStatusText(
            militarySavingsRequest.state.get(),
            gameReady.get(),
            '장병적금 이력',
            militarySavingsItems.get().length,
            militarySavingsNextBefore.get(),
          ),
        );

        h.bindAttribute(focusSubmit, 'disabled', () => !canMutate.get());
        h.bindAttribute(
          activityStartSubmit,
          'disabled',
          () => !canMutate.get() || !catalogAvailable.get(),
        );
        h.bindAttribute(
          activityCancelSubmit,
          'disabled',
          () => !canMutate.get() || activeActivities.get().length === 0,
        );
        h.bindAttribute(artifactSubmit, 'disabled', () => !canMutate.get());
        h.bindAttribute(applicationSubmit, 'disabled', () => !canMutate.get());
        h.bindAttribute(interviewSubmit, 'disabled', () => !canMutate.get());
        h.bindAttribute(pathActionSubmit, 'disabled', () => !canMutate.get());
        h.bindAttribute(
          militaryServiceStartSubmit,
          'disabled',
          () => !canMutate.get() || eligibleMilitaryOptions.get().length === 0,
        );
        h.bindAttribute(
          militarySavingsEnrollmentSubmit,
          'disabled',
          () => !canMutate.get() || eligibleMilitarySavingsProducts.get().length === 0,
        );
        h.bindAttribute(
          militarySavingsCloseSubmit,
          'disabled',
          () => !canMutate.get() || (career.get()?.activeMilitarySavings.length ?? 0) === 0,
        );
        h.bindAttribute(
          militaryOptionsRefresh,
          'disabled',
          () => !gameReady.get() || militaryOptionsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          militaryServiceRefresh,
          'disabled',
          () => !gameReady.get() || militaryServiceRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          militarySavingsProductsRefresh,
          'disabled',
          () => !gameReady.get() || militarySavingsProductsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          militarySavingsHistoryRefresh,
          'disabled',
          () => !gameReady.get() || militarySavingsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(militarySavingsHistoryLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            militarySavingsRequest.state.get(),
            militarySavingsItems.get().length,
            militarySavingsNextBefore.get(),
          ),
        );
        h.bindAttribute(
          payrollRefresh,
          'disabled',
          () => !gameReady.get() || payrollRequest.state.get().status === 'loading',
        );
        h.bindAttribute(taxYearInput, 'disabled', () => !gameReady.get());
        h.bindAttribute(
          taxYearRefresh,
          'disabled',
          () => !gameReady.get() || taxYearRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          specsRefresh,
          'disabled',
          () => !gameReady.get() || specsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          activitiesRefresh,
          'disabled',
          () => !gameReady.get() || activitiesRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          artifactsRefresh,
          'disabled',
          () => !gameReady.get() || artifactsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(specsLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            specsRequest.state.get(),
            evidenceItems.get().length,
            evidenceNextBefore.get(),
          ),
        );
        h.bindAttribute(activitiesLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            activitiesRequest.state.get(),
            activityHistoryItems.get().length,
            activityNextBefore.get(),
          ),
        );
        h.bindAttribute(artifactsLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            artifactsRequest.state.get(),
            artifactItems.get().length,
            artifactNextBefore.get(),
          ),
        );
        h.bindAttribute(
          jobsRefresh,
          'disabled',
          () => !gameReady.get() || jobsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          applicationsRefresh,
          'disabled',
          () => !gameReady.get() || applicationsRequest.state.get().status === 'loading',
        );
        h.bindAttribute(jobsLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            jobsRequest.state.get(),
            jobItems.get().length,
            jobNextBefore.get(),
          ),
        );
        h.bindAttribute(applicationsLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            applicationsRequest.state.get(),
            applicationItems.get().length,
            applicationNextBefore.get(),
          ),
        );
        h.bindAttribute(payrollLoadOlder, 'disabled', () =>
          cannotLoadOlder(
            gameReady.get(),
            payrollRequest.state.get(),
            payrollItems.get().length,
            payrollNextBefore.get(),
          ),
        );

        h.useEffect(() => {
          const state = specsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, specsGeneration, evidenceNextBefore.peek())) return;
          evidenceItems.set(resolvePageItems(request, evidenceItems.peek(), response.items));
          evidenceNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          evidenceList.setItems(gameReady.get() ? evidenceItems.get() : []);
        });
        h.useEffect(() => {
          activeList.setItems(gameReady.get() ? activeActivities.get() : []);
          updateFixedSelectOptions(
            activityCancelForm.element,
            'activityId',
            activeActivities.get().map((item) => ({
              value: item.id,
              label: `#${item.id} ${item.displayName}`,
            })),
            undefined,
          );
        });
        h.useEffect(() => {
          const state = activitiesRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, activitiesGeneration, activityNextBefore.peek())) return;
          activityCatalog.set(response.catalog);
          queriedActiveActivities.set(response.active);
          activityHistoryItems.set(
            resolvePageItems(request, activityHistoryItems.peek(), response.items),
          );
          activityNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          const catalog = gameReady.get() ? activityCatalog.get() : [];
          catalogList.setItems(catalog);
          activityHistoryList.setItems(gameReady.get() ? activityHistoryItems.get() : []);
          updateFixedSelectOptions(
            activityStartForm.element,
            'activityCatalogEntryId',
            catalog.map((item) => ({
              value: item.id,
              label: `#${item.id} ${item.displayName}`,
            })),
            undefined,
          );
        });
        h.useEffect(() => {
          latestArtifactList.setItems(gameReady.get() ? (career.get()?.latestArtifacts ?? []) : []);
        });
        h.useEffect(() => {
          const state = artifactsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, artifactsGeneration, artifactNextBefore.peek())) return;
          artifactItems.set(resolvePageItems(request, artifactItems.peek(), response.items));
          artifactNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          artifactList.setItems(gameReady.get() ? artifactItems.get() : []);
        });
        h.useEffect(() => {
          const state = jobsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, jobsGeneration, jobNextBefore.peek())) return;
          jobItems.set(resolveJobPageItems(request, jobItems.peek(), response.items));
          jobNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          jobList.setItems(gameReady.get() ? jobItems.get() : []);
        });
        h.useEffect(() => {
          const state = applicationsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, applicationsGeneration, applicationNextBefore.peek())) return;
          applicationItems.set(resolvePageItems(request, applicationItems.peek(), response.items));
          applicationNextBefore.set(response.nextBefore);
          invitationItems.set(response.openInvitations);
        });
        h.useEffect(() => {
          applicationList.setItems(gameReady.get() ? applicationItems.get() : []);
          invitationList.setItems(gameReady.get() ? invitationItems.get() : []);
        });
        h.useEffect(() => {
          const state = employmentRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          employment.set(state.value.contract);
        });
        h.useEffect(() => {
          const state = payrollRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, payrollGeneration, payrollNextBefore.peek())) return;
          payrollItems.set(resolvePageItems(request, payrollItems.peek(), response.items));
          payrollNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          payrollList.setItems(gameReady.get() ? payrollItems.get() : []);
        });
        h.useEffect(() => {
          const state = militaryOptionsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          militaryOptions.set(state.value.items);
        });
        h.useEffect(() => {
          const options = gameReady.get() ? militaryOptions.get() : [];
          militaryOptionList.setItems(options);
          updateFixedSelectOptions(
            militaryServiceStartForm.element,
            'militaryOptionVersionId',
            options
              .filter((option) => option.eligible)
              .map((option) => ({
                value: option.id,
                label: `#${option.id} ${option.displayName}`,
              })),
            undefined,
          );
        });
        h.useEffect(() => {
          const state = militaryServiceRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          militaryService.set(state.value);
        });
        h.useEffect(() => {
          const state = militarySavingsProductsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          militarySavingsProducts.set(state.value.items);
        });
        h.useEffect(() => {
          const products = gameReady.get() ? militarySavingsProducts.get() : [];
          militarySavingsProductList.setItems(products);
          updateFixedSelectOptions(
            militarySavingsEnrollmentForm.element,
            'productVersionId',
            products
              .filter((product) => product.eligible)
              .map((product) => ({
                value: product.id,
                label: `#${product.id} ${product.institutionDisplayName}`,
              })),
            undefined,
          );
        });
        h.useEffect(() => {
          const items = gameReady.get() ? (career.get()?.pendingCareerSchedule ?? []) : [];
          pendingCareerScheduleList.setItems(items);
        });
        h.useEffect(() => {
          const active = gameReady.get() ? (career.get()?.activeMilitarySavings ?? []) : [];
          activeMilitarySavingsList.setItems(active);
          updateFixedSelectOptions(
            militarySavingsCloseForm.element,
            'contractId',
            active.map((contract) => ({
              value: contract.id,
              label: `#${contract.id} ${contract.institutionKey}`,
            })),
            undefined,
          );
        });
        h.useEffect(() => {
          const state = militarySavingsRequest.state.get();
          if (!gameReady.get() || state.status !== 'success') return;
          const { request, response } = state.value;
          if (!canApplyPage(request, militarySavingsGeneration, militarySavingsNextBefore.peek())) {
            return;
          }
          militarySavingsItems.set(
            resolvePageItems(request, militarySavingsItems.peek(), response.items),
          );
          militarySavingsNextBefore.set(response.nextBefore);
        });
        h.useEffect(() => {
          militarySavingsHistoryList.setItems(gameReady.get() ? militarySavingsItems.get() : []);
        });

        let lastAppliedFocus: string | undefined;
        h.useEffect(() => {
          const focusedJobFamilyKey = career.get()?.focusedJobFamilyKey;
          if (focusedJobFamilyKey === undefined) return;
          const field = textFieldOf(focusForm.element, 'focusedJobFamilyKey');
          if (field.value === '' || field.value === lastAppliedFocus) {
            focusForm.setValues({ focusedJobFamilyKey });
          }
          lastAppliedFocus = focusedJobFamilyKey;
        });

        const syncArtifactFields = (): void => {
          const linkedin = artifactKind.value === 'linkedinProfile';
          if (openToWorkRow.hidden !== !linkedin) openToWorkRow.hidden = !linkedin;
          if (industriesRow.hidden !== !linkedin) industriesRow.hidden = !linkedin;
        };
        h.useEventListener(artifactKind, 'change', syncArtifactFields);
        syncArtifactFields();

        const invalidateSpecs = (): void => {
          specsGeneration += 1;
          specsRequest.cancel();
          evidenceItems.set([]);
          evidenceNextBefore.set(null);
          specsPageRequest = {
            kind: 'initial',
            generation: specsGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshSpecs = (): void => {
          invalidateSpecs();
          if (gameReady.peek()) specsRequest.run();
        };
        const invalidateActivities = (): void => {
          activitiesGeneration += 1;
          activitiesRequest.cancel();
          activityCatalog.set([]);
          queriedActiveActivities.set(undefined);
          activityHistoryItems.set([]);
          activityNextBefore.set(null);
          activitiesPageRequest = {
            kind: 'initial',
            generation: activitiesGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshActivities = (): void => {
          invalidateActivities();
          if (gameReady.peek()) activitiesRequest.run();
        };
        const invalidateArtifacts = (): void => {
          artifactsGeneration += 1;
          artifactsRequest.cancel();
          artifactItems.set([]);
          artifactNextBefore.set(null);
          artifactsPageRequest = {
            kind: 'initial',
            generation: artifactsGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshArtifacts = (): void => {
          invalidateArtifacts();
          if (gameReady.peek()) artifactsRequest.run();
        };
        const invalidateJobs = (): void => {
          jobsGeneration += 1;
          jobsRequest.cancel();
          jobItems.set([]);
          jobNextBefore.set(null);
          jobsPageRequest = { kind: 'initial', generation: jobsGeneration, limit: PAGE_SIZE };
        };
        const refreshJobs = (): void => {
          invalidateJobs();
          if (gameReady.peek()) jobsRequest.run();
        };
        const invalidateApplications = (): void => {
          applicationsGeneration += 1;
          applicationsRequest.cancel();
          applicationItems.set([]);
          applicationNextBefore.set(null);
          invitationItems.set([]);
          applicationsPageRequest = {
            kind: 'initial',
            generation: applicationsGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshApplications = (): void => {
          invalidateApplications();
          if (gameReady.peek()) {
            applicationsRequest.run();
            employmentRequest.run();
          }
        };
        const invalidatePayroll = (): void => {
          payrollGeneration += 1;
          payrollRequest.cancel();
          payrollItems.set([]);
          payrollNextBefore.set(null);
          payrollPageRequest = {
            kind: 'initial',
            generation: payrollGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshPayroll = (): void => {
          invalidatePayroll();
          if (gameReady.peek()) payrollRequest.run();
        };
        const invalidateTaxYear = (): void => {
          taxYearRequest.cancel();
        };
        const ensureSelectedTaxYear = (): boolean => {
          if (selectedTaxYear.peek() !== null) return true;
          const currentYear = career.peek()?.currentEmploymentTaxYear.taxYear;
          if (currentYear === undefined) return false;
          selectedTaxYear.set(currentYear);
          taxYearInput.value = currentYear.toString();
          return true;
        };
        const refreshTaxYear = (): void => {
          const parsedYear = TaxYearSchema.safeParse(Number(taxYearInput.value));
          if (!parsedYear.success) {
            taxYearInputError.set('귀속연도를 1~9999 사이의 정수로 입력하세요.');
            return;
          }
          taxYearInputError.set(null);
          selectedTaxYear.set(parsedYear.data);
          invalidateTaxYear();
          if (gameReady.peek()) taxYearRequest.run();
        };
        const invalidateMilitaryOptions = (): void => {
          militaryOptionsRequest.cancel();
          militaryOptions.set([]);
        };
        const refreshMilitaryOptions = (): void => {
          invalidateMilitaryOptions();
          if (gameReady.peek()) militaryOptionsRequest.run();
        };
        const invalidateMilitaryService = (): void => {
          militaryServiceRequest.cancel();
          militaryService.set(undefined);
        };
        const refreshMilitaryService = (): void => {
          invalidateMilitaryService();
          if (gameReady.peek()) militaryServiceRequest.run();
        };
        const invalidateMilitarySavingsProducts = (): void => {
          militarySavingsProductsRequest.cancel();
          militarySavingsProducts.set([]);
        };
        const refreshMilitarySavingsProducts = (): void => {
          invalidateMilitarySavingsProducts();
          if (gameReady.peek()) militarySavingsProductsRequest.run();
        };
        const invalidateMilitarySavings = (): void => {
          militarySavingsGeneration += 1;
          militarySavingsRequest.cancel();
          militarySavingsItems.set([]);
          militarySavingsNextBefore.set(null);
          militarySavingsPageRequest = {
            kind: 'initial',
            generation: militarySavingsGeneration,
            limit: PAGE_SIZE,
          };
        };
        const refreshMilitarySavings = (): void => {
          invalidateMilitarySavings();
          if (gameReady.peek()) militarySavingsRequest.run();
        };
        const invalidateCareerQueries = (): void => {
          invalidateSpecs();
          invalidateActivities();
          invalidateArtifacts();
          invalidateJobs();
          invalidateApplications();
          invalidatePayroll();
          invalidateTaxYear();
          invalidateMilitaryOptions();
          invalidateMilitaryService();
          invalidateMilitarySavingsProducts();
          invalidateMilitarySavings();
        };
        const runCareerQueries = (): void => {
          if (!gameReady.peek()) return;
          specsRequest.run();
          activitiesRequest.run();
          artifactsRequest.run();
          jobsRequest.run();
          applicationsRequest.run();
          employmentRequest.run();
          payrollRequest.run();
          if (ensureSelectedTaxYear()) taxYearRequest.run();
          militaryOptionsRequest.run();
          militaryServiceRequest.run();
          militarySavingsProductsRequest.run();
          militarySavingsRequest.run();
        };
        const loadOlderSpecs = (): void => {
          const before = evidenceNextBefore.peek();
          const remaining = HISTORY_CAPACITY - evidenceItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            specsRequest.state.peek().status === 'loading'
          ) {
            return;
          }
          specsPageRequest = {
            kind: 'older',
            generation: specsGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          specsRequest.run();
        };
        const loadOlderActivities = (): void => {
          const before = activityNextBefore.peek();
          const remaining = HISTORY_CAPACITY - activityHistoryItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            activitiesRequest.state.peek().status === 'loading'
          ) {
            return;
          }
          activitiesPageRequest = {
            kind: 'older',
            generation: activitiesGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          activitiesRequest.run();
        };
        const loadOlderArtifacts = (): void => {
          const before = artifactNextBefore.peek();
          const remaining = HISTORY_CAPACITY - artifactItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            artifactsRequest.state.peek().status === 'loading'
          ) {
            return;
          }
          artifactsPageRequest = {
            kind: 'older',
            generation: artifactsGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          artifactsRequest.run();
        };
        const loadOlderJobs = (): void => {
          const before = jobNextBefore.peek();
          const remaining = JOB_CAPACITY - jobItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            jobsRequest.state.peek().status === 'loading'
          )
            return;
          jobsPageRequest = {
            kind: 'older',
            generation: jobsGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          jobsRequest.run();
        };
        const loadOlderApplications = (): void => {
          const before = applicationNextBefore.peek();
          const remaining = HISTORY_CAPACITY - applicationItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            applicationsRequest.state.peek().status === 'loading'
          )
            return;
          applicationsPageRequest = {
            kind: 'older',
            generation: applicationsGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          applicationsRequest.run();
        };
        const loadOlderPayroll = (): void => {
          const before = payrollNextBefore.peek();
          const remaining = HISTORY_CAPACITY - payrollItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            payrollRequest.state.peek().status === 'loading'
          )
            return;
          payrollPageRequest = {
            kind: 'older',
            generation: payrollGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          payrollRequest.run();
        };
        const loadOlderMilitarySavings = (): void => {
          const before = militarySavingsNextBefore.peek();
          const remaining = HISTORY_CAPACITY - militarySavingsItems.peek().length;
          if (
            !gameReady.peek() ||
            before === null ||
            remaining <= 0 ||
            militarySavingsRequest.state.peek().status === 'loading'
          ) {
            return;
          }
          militarySavingsPageRequest = {
            kind: 'older',
            generation: militarySavingsGeneration,
            before,
            limit: Math.min(PAGE_SIZE, remaining),
          };
          militarySavingsRequest.run();
        };
        const throttledRunCareerQueries = h.useThrottled(runCareerQueries, 250);
        const snapshotCursor = h.useComputed(() => {
          const current = snapshot.get();
          if (current === undefined || current.characterName === null) return 'no-character';
          return `${current.runRevision}:${current.stateRevision}:${current.gameDay}`;
        });
        h.useWatch(snapshotCursor, () => {
          invalidateCareerQueries();
          throttledRunCareerQueries();
        });
        h.useEventListener(specsRefresh, 'click', () => {
          if (gameReady.peek()) refreshSpecs();
        });
        h.useEventListener(activitiesRefresh, 'click', () => {
          if (gameReady.peek()) refreshActivities();
        });
        h.useEventListener(artifactsRefresh, 'click', () => {
          if (gameReady.peek()) refreshArtifacts();
        });
        h.useEventListener(jobsRefresh, 'click', () => {
          if (gameReady.peek()) refreshJobs();
        });
        h.useEventListener(applicationsRefresh, 'click', () => {
          if (gameReady.peek()) refreshApplications();
        });
        h.useEventListener(payrollRefresh, 'click', () => {
          if (gameReady.peek()) refreshPayroll();
        });
        h.useEventListener(taxYearRefresh, 'click', () => {
          if (gameReady.peek()) refreshTaxYear();
        });
        h.useEventListener(taxYearInput, 'input', () => {
          if (taxYearInputError.peek() !== null) taxYearInputError.set(null);
        });
        h.useEventListener(specsLoadOlder, 'click', loadOlderSpecs);
        h.useEventListener(activitiesLoadOlder, 'click', loadOlderActivities);
        h.useEventListener(artifactsLoadOlder, 'click', loadOlderArtifacts);
        h.useEventListener(jobsLoadOlder, 'click', loadOlderJobs);
        h.useEventListener(applicationsLoadOlder, 'click', loadOlderApplications);
        h.useEventListener(payrollLoadOlder, 'click', loadOlderPayroll);
        h.useEventListener(militaryOptionsRefresh, 'click', () => {
          if (gameReady.peek()) refreshMilitaryOptions();
        });
        h.useEventListener(militaryServiceRefresh, 'click', () => {
          if (gameReady.peek()) refreshMilitaryService();
        });
        h.useEventListener(militarySavingsProductsRefresh, 'click', () => {
          if (gameReady.peek()) refreshMilitarySavingsProducts();
        });
        h.useEventListener(militarySavingsHistoryRefresh, 'click', () => {
          if (gameReady.peek()) refreshMilitarySavings();
        });
        h.useEventListener(militarySavingsHistoryLoadOlder, 'click', loadOlderMilitarySavings);
        h.useEventListener(platformFilter, 'change', () => {
          jobPlatformFilter =
            platformFilter.value === '' ? undefined : (platformFilter.value as CareerPlatform);
          refreshJobs();
        });
        h.useEventListener(industryFilter, 'change', () => {
          jobIndustryFilter =
            industryFilter.value === '' ? undefined : (industryFilter.value as CareerIndustry);
          refreshJobs();
        });
        invalidateCareerQueries();
        runCareerQueries();
      },

      unmount() {
        root?.remove();
        root = undefined;
      },
    };
  };
}

function pageQuery(
  request: PageLoadRequest,
): { readonly limit: number } | { readonly before: string; readonly limit: number } {
  return request.kind === 'older'
    ? { before: request.before, limit: request.limit }
    : { limit: request.limit };
}

function canApplyPage(
  request: PageLoadRequest,
  currentGeneration: number,
  currentBefore: string | null,
): boolean {
  return (
    request.generation === currentGeneration &&
    (request.kind === 'initial' || request.before === currentBefore)
  );
}

function resolvePageItems<T extends { readonly id: string }>(
  request: PageLoadRequest,
  current: readonly T[],
  incoming: readonly T[],
): readonly T[] {
  return request.kind === 'initial' ? incoming : mergeCareerItems(current, incoming);
}

function mergeCareerItems<T extends { readonly id: string }>(
  current: readonly T[],
  incoming: readonly T[],
): readonly T[] {
  const byId = new Map(current.map((item) => [item.id, item]));
  for (const item of incoming) byId.set(item.id, item);
  return [...byId.values()]
    .sort((left, right) => {
      const leftId = BigInt(left.id);
      const rightId = BigInt(right.id);
      if (leftId === rightId) return 0;
      return leftId > rightId ? -1 : 1;
    })
    .slice(0, HISTORY_CAPACITY);
}

function resolveJobPageItems(
  request: PageLoadRequest,
  current: readonly CareerJob[],
  incoming: readonly CareerJob[],
): readonly CareerJob[] {
  if (request.kind === 'initial') return incoming;
  const byKey = new Map(current.map((item) => [item.postingKey, item]));
  for (const item of incoming) byKey.set(item.postingKey, item);
  return [...byKey.values()]
    .sort((left, right) => right.postingKey.localeCompare(left.postingKey))
    .slice(0, JOB_CAPACITY);
}

async function runCareerPathAction(deps: CareerApi, command: CareerPathCommand) {
  switch (command.action) {
    case 'withdrawApplication':
      return deps.withdrawApplication(command.resourceId, command.request);
    case 'acceptInvitation':
      return deps.acceptInvitation(command.resourceId, command.request);
    case 'declineInvitation':
      return deps.declineInvitation(command.resourceId, command.request);
    case 'acceptOffer':
      return deps.acceptOffer(command.resourceId, command.request);
    case 'declineOffer':
      return deps.declineOffer(command.resourceId, command.request);
  }
}

function cannotLoadOlder<T>(
  gameReady: boolean,
  state: AsyncState<T>,
  loadedCount: number,
  nextBefore: string | null,
): boolean {
  return (
    !gameReady ||
    state.status === 'loading' ||
    nextBefore === null ||
    loadedCount >= HISTORY_CAPACITY
  );
}

function commandSnapshot(deps: CareerDeps, action: string): GameSnapshot {
  const state = deps.store.getState();
  const snapshot = state.game.snapshot;
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error(`캐릭터를 만든 뒤 ${action}할 수 있습니다.`);
  }
  if (snapshot.autoSpeed !== null) {
    throw new Error(`자동 진행을 멈춘 뒤 ${action}할 수 있습니다.`);
  }
  if (state.game.advancing || state.game.ordering) {
    throw new Error(`다른 게임 명령이 끝난 뒤 ${action}해 주세요.`);
  }
  return snapshot;
}

function careerDisplayError(error: unknown, action: string): Error {
  if (error instanceof CareerCommandError) return new Error(error.message);
  return new Error(`${action} 결과를 확인하지 못했습니다. 같은 입력으로 다시 시도해 주세요.`);
}

function careerCommandStatus(
  snapshot: GameSnapshot | undefined,
  advancing: boolean,
  ordering: boolean,
): string {
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (snapshot.characterName === null) return '캐릭터를 만든 뒤 커리어를 관리할 수 있습니다.';
  if (snapshot.autoSpeed !== null) return '커리어 명령을 보내려면 자동 진행을 멈추세요.';
  if (advancing || ordering) return '다른 게임 명령을 처리하는 중입니다.';
  return '커리어 명령을 보낼 수 있습니다.';
}

function requestStatusText<T>(
  state: AsyncState<T>,
  gameReady: boolean,
  subject: string,
  loadedCount: number,
  nextBefore: string | null,
): string {
  if (!gameReady) return '캐릭터를 만든 뒤 조회할 수 있습니다.';
  if (state.status === 'idle') return `${subject} 조회를 기다리는 중입니다.`;
  if (state.status === 'loading') {
    return loadedCount === 0
      ? `${subject} 목록을 불러오는 중…`
      : `${subject} ${loadedCount.toLocaleString('ko-KR')}개 표시 중 · 이전 기록을 불러오는 중…`;
  }
  if (state.status === 'error') {
    return loadedCount === 0
      ? `${subject} 목록을 불러오지 못했습니다. 다시 시도해 주세요.`
      : `${subject} ${loadedCount.toLocaleString('ko-KR')}개 표시 중 · 이전 기록을 불러오지 못했습니다.`;
  }
  const capacityNotice =
    loadedCount >= HISTORY_CAPACITY && nextBefore !== null
      ? ` 화면 표시 한도 ${HISTORY_CAPACITY.toLocaleString('ko-KR')}개에 도달했습니다.`
      : '';
  return `${subject} ${loadedCount.toLocaleString('ko-KR')}개를 불러왔습니다.${capacityNotice}`;
}

const EMPLOYMENT_TAX_STATUS_LABEL: Record<CareerTaxYearState['status'], string> = {
  open: '집계 중',
  provisional: '회사 정산 잠정 확정',
  definitive: '최종 확정',
};

const EMPLOYMENT_TAX_SOURCE_LABEL: Record<CareerTaxYearState['source'], string> = {
  employmentOnly: '근로소득 단독',
  combined: '금융소득 합산',
  legacyProfile: '기존 시작 프로필',
};

function taxYearRequestStatusText(
  state: AsyncState<CareerTaxYearState>,
  gameReady: boolean,
  selectedYear: number | null,
  inputError: string | null,
): string {
  if (inputError !== null) return inputError;
  if (!gameReady) return '캐릭터를 만든 뒤 연말정산을 조회할 수 있습니다.';
  if (state.status === 'idle') return '조회할 귀속연도를 입력하세요.';
  const subject = selectedYear === null ? '연말정산' : `${selectedYear}년 연말정산`;
  if (state.status === 'loading') return `${subject}을 불러오는 중…`;
  if (state.status === 'error') return `${subject}을 불러오지 못했습니다. 다시 시도해 주세요.`;
  return `${state.value.taxYear}년 연말정산을 불러왔습니다.`;
}

function taxYearText(year: CareerTaxYearState | null): string {
  return year === null ? '—' : `${year.taxYear}년`;
}

function taxYearStatusText(year: CareerTaxYearState | null): string {
  return year === null ? '—' : EMPLOYMENT_TAX_STATUS_LABEL[year.status];
}

function taxYearSourceText(year: CareerTaxYearState | null): string {
  return year === null ? '—' : EMPLOYMENT_TAX_SOURCE_LABEL[year.source];
}

function employmentTaxMoneyText(value: number | null | undefined): string {
  if (value === undefined) return '—';
  return value === null ? '미확정' : formatWon(value);
}

function employmentTaxReconciliationText(value: number | null | undefined): string {
  if (value === undefined) return '—';
  return value === null ? '미확정' : `${value.toLocaleString('ko-KR')}일차`;
}

function currentExpectedPensionCreditText(snapshot: GameSnapshot | undefined): string {
  if (snapshot === undefined) return '—';
  const expectedCreditKrw = snapshot.finance.pensionAccounts.reduce(
    (total, account) => total + BigInt(account.expectedCreditKrw),
    0n,
  );
  return formatWon(expectedCreditKrw);
}

function withKoreanErrors<T>(
  validator: FormValidator<T>,
  messages: Readonly<Record<string, string>>,
): FormValidator<T> {
  return {
    validate(raw) {
      return localizeValidation(validator.validate(raw), messages);
    },
  };
}

function localizeValidation<T>(
  validation: ReturnType<FormValidator<T>['validate']>,
  messages: Readonly<Record<string, string>>,
): ReturnType<FormValidator<T>['validate']> {
  if (validation.ok) return validation;
  const errors: Record<string, string> = {};
  for (const key of Object.keys(validation.errors)) {
    const field = key.split('.')[0];
    const target = field === undefined || field === '' ? 'form' : field;
    errors[target] = messages[target] ?? '입력값을 확인하세요.';
  }
  return { ok: false, errors };
}

function commaSeparatedValues(raw: unknown): readonly string[] {
  if (typeof raw !== 'string') return [];
  return raw
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

function optionalText(raw: unknown): string | undefined {
  if (typeof raw !== 'string') return undefined;
  const value = raw.trim();
  return value === '' ? undefined : value;
}

function selectWithOptions(
  options: readonly { readonly value: string; readonly label: string }[],
): HTMLSelectElement {
  return el(
    'select',
    {},
    ...options.map((option) => el('option', { value: option.value }, option.label)),
  );
}

function jobText(job: CareerJob): string {
  const requiredArtifacts =
    job.requiredArtifacts.map((kind) => ARTIFACT_KIND_LABEL[kind]).join(', ') || '없음';
  const scores = JOB_SCORE_DIMENSIONS.map(
    ([dimension, label]) =>
      `${label} 요구 ${scoreText(job.requiredScores[dimension])}/보유 ${scoreText(job.possessedScores[dimension])}`,
  ).join(', ');
  return `[${job.platform}/${INDUSTRY_LABEL[job.industry]}] ${job.employerName} · ${job.jobFamilyKey} · ${REGION_LABEL[job.region]} · ${EMPLOYMENT_TYPE_LABEL[job.employmentType]} · 점수 ${scores} · 병역 ${MILITARY_REQUIREMENT_LABEL[job.militaryRequirement]} · 연봉 밴드 ${formatWon(job.minimumAnnualSalaryKrw)}~${formatWon(job.maximumAnnualSalaryKrw)} (${formatWon(job.salaryStepKrw)} 단위) · 요구 산출물 ${requiredArtifacts} · 키 ${job.postingKey}`;
}

function applicationText(application: CareerApplication): string {
  const offer =
    application.offer === null
      ? ''
      : ` · 오퍼 #${application.offer.id} ${formatWon(application.offer.annualSalaryKrw)} (마감 ${application.offer.expiresExclusiveGameDay}일)`;
  return `#${application.id} [${application.status}] ${application.employerName} · ${application.jobFamilyKey} · 서류 ${scoreText(application.documentScoreBp)} · 면접 ${scoreText(application.interviewScoreBp)}${offer}`;
}

function invitationText(invitation: CareerInvitation): string {
  return `#${invitation.id} [${invitation.platform}] ${invitation.employerName} · ${invitation.jobFamilyKey} · 산출물 #${invitation.artifactVersionId} · ${invitation.expiresExclusiveGameDay}일 전까지 응답`;
}

function pendingCareerScheduleText(item: CareerPendingScheduleItem): string {
  const label =
    item.sourceKind === 'careerAction'
      ? CAREER_ACTION_SCHEDULE_LABEL[item.kind]
      : CAREER_SETTLEMENT_SCHEDULE_LABEL[item.kind];
  return `#${item.id} · ${item.dueGameDay}일차 · ${label}`;
}

function employmentText(contract: CareerEmploymentContract | null): string {
  if (contract === null) return '현재 근로계약이 없습니다.';
  return `${contract.employerName} · ${contract.jobFamilyKey} · ${contract.status} · 연봉 ${formatWon(contract.annualSalaryKrw)} · 입사 예정/시작 ${contract.startGameDay}일 · 인정 경력 ${contract.creditedExperienceDays}일`;
}

function payrollText(payroll: CareerPayrollItem): string {
  const employeeInsuranceKrw =
    payroll.employeeNationalPensionKrw +
    payroll.employeeHealthInsuranceKrw +
    payroll.employeeLongTermCareKrw +
    payroll.employeeEmploymentInsuranceKrw;
  const employerInsuranceKrw =
    payroll.employerNationalPensionKrw +
    payroll.employerHealthInsuranceKrw +
    payroll.employerLongTermCareKrw +
    payroll.employerEmploymentInsuranceKrw +
    payroll.employerIndustrialAccidentKrw;
  const reward =
    payroll.reward === undefined
      ? ''
      : ` · 채용보상 총 ${formatWon(payroll.reward.grossRewardKrw)}/실수령 ${formatWon(payroll.reward.netRewardKrw)}`;
  return `#${payroll.id} 계약 #${payroll.contractId} ${payroll.periodNo}회차(${payroll.salaryMonthOrdinal}월차) · ${payroll.periodStartDate}~${payroll.periodEndExclusiveDate} · 총급여 ${formatWon(payroll.grossPayKrw)} · 근로자 보험 ${formatWon(employeeInsuranceKrw)} · 사용자 보험 ${formatWon(employerInsuranceKrw)} · 원천세 ${formatWon(payroll.withheldIncomeTaxKrw + payroll.withheldLocalIncomeTaxKrw)} · 실수령 ${formatWon(payroll.netPayKrw)} · 지급 게임일 ${payroll.paidGameDay}${reward}`;
}

function militaryServiceRequestStatusText(
  state: AsyncState<MilitaryServiceResponse>,
  gameReady: boolean,
): string {
  if (!gameReady) return '캐릭터를 만든 뒤 복무 이력을 조회할 수 있습니다.';
  if (state.status === 'idle') return '복무 이력 조회를 기다리는 중입니다.';
  if (state.status === 'loading') return '복무 이력을 불러오는 중…';
  if (state.status === 'error') return '복무 이력을 불러오지 못했습니다. 다시 시도해 주세요.';
  return state.value.service === null
    ? '복무 이력이 없습니다.'
    : `복무 #${state.value.service.id} 이력을 불러왔습니다.`;
}

function activeMilitaryServiceText(
  service: GameSnapshot['career']['activeMilitaryService'] | null | undefined,
): string {
  if (service === undefined) return '—';
  if (service === null) return '현재 진행 중인 복무가 없습니다.';
  const nextPay =
    service.nextPayGameDay === null ? '다음 급여 없음' : `다음 급여 ${service.nextPayGameDay}일차`;
  return `#${service.id} ${service.displayName} · ${MILITARY_SERVICE_STATUS_LABEL[service.status]} · ${service.creditedServiceDays}/${service.totalServiceDays}일 인정 · ${service.startGameDay}일차 이상 ${service.endGameDay}일차 미만 · ${nextPay}`;
}

function militaryServiceHistoryText(service: MilitaryServiceHistory | null | undefined): string {
  if (service === undefined) return '—';
  if (service === null) return '복무 이력이 없습니다.';
  const completion =
    service.completedGameDay === null ? '미완료' : `${service.completedGameDay}일차 완료`;
  return `#${service.id} ${MILITARY_SERVICE_TYPE_LABEL[service.serviceType]} · ${MILITARY_SERVICE_STATUS_LABEL[service.status]} · ${MILITARY_SERVICE_SOURCE_LABEL[service.sourceKind]} · ${service.startDate} 이상 ${service.endExclusiveDate} 미만 · 인정 ${service.creditedServiceDays}/${service.totalServiceDays}일 · ${completion}`;
}

function militaryOptionText(option: MilitaryOption): string {
  const eligibility = option.eligible
    ? '복무 자격 충족'
    : `자격 미충족: ${option.ineligibilityReasons.map(militaryOptionIneligibilityText).join(', ')}`;
  const minimumEducation =
    option.hardRequirements.minimumEducation === null
      ? '제한 없음'
      : EDUCATION_LEVEL_LABEL[option.hardRequirements.minimumEducation];
  const stages = option.payStages
    .map(
      (stage) =>
        `${stage.startServiceMonth}~${stage.endExclusiveServiceMonth - 1}개월 ${formatWon(stage.grossMonthlyPayKrw)}`,
    )
    .join(' / ');
  const experience =
    option.experienceCredits.length === 0
      ? '민간 경력 없음'
      : option.experienceCredits
          .map((credit) => `${credit.jobFamilyKey} ${ratePpmText(credit.dailyCreditPpm)}`)
          .join(', ');
  return `#${option.id} ${option.displayName} · ${eligibility} · ${option.serviceDurationMonths}개월 · 요구 학력 ${minimumEducation}, 자격증 ${option.hardRequirements.requiredCertificationCount}개, 경력 ${option.hardRequirements.minimumExperienceDays}일 · ${MILITARY_COMPENSATION_LABEL[option.compensationKind]}/월 지급 · 급여 ${stages} · 하루 활동량 ${option.dailyEffortCapacityUnits.toLocaleString('ko-KR')} · ${experience}`;
}

function militarySavingsProductText(product: MilitarySavingsProduct): string {
  const eligibility = product.eligible
    ? '가입 가능'
    : `가입 불가: ${product.ineligibilityReasons.map(militarySavingsIneligibilityText).join(', ')}`;
  const serviceTypes = product.eligibleServiceTypes
    .map((serviceType) => MILITARY_SERVICE_TYPE_LABEL[serviceType])
    .join(', ');
  const tiers = product.interestTiers
    .map(
      (tier) =>
        `${tier.minimumTermMonths}~${tier.maximumTermMonthsInclusive}개월 ${ratePpmText(tier.annualInterestRatePpm)}`,
    )
    .join(' / ');
  return `#${product.id} ${product.institutionDisplayName} · ${eligibility} · 대상 ${serviceTypes} · 가입 ${product.joinStartDate}~${product.joinEndDate} · 잔여복무 최소 ${product.minimumRemainingServiceMonths}개월 · 월 ${formatWon(product.minimumMonthlyContributionKrw)}~${formatWon(product.maximumInstitutionMonthlyContributionKrw)} (설정 단위 ${formatWon(product.limitSettingUnitKrw)}, 전체 ${formatWon(product.maximumTotalMonthlyContributionKrw)}) · 금리 ${tiers} · 실제일수/365·원 미만 버림 · 중도해지 ${ratePpmText(product.earlyCloseAnnualInterestRatePpm)} · 정부지원 ${ratePpmText(product.governmentMatchingRatePpm)} (다음 달 ${product.governmentMatchPaymentDayOfMonth}일) · ${product.maturityTaxExempt ? '만기 비과세' : '과세'}`;
}

function activeMilitarySavingsText(
  contract: GameSnapshot['career']['activeMilitarySavings'][number],
): string {
  const next =
    contract.nextInstallmentGameDay === null
      ? '다음 납입 없음'
      : `다음 납입 ${contract.nextInstallmentGameDay}일차`;
  return `#${contract.id} ${contract.institutionKey} · 월 ${formatWon(contract.monthlyContributionKrw)} (${contract.debitDayOfMonth}일) · 원금 ${formatWon(contract.principalKrw)} · 납입 ${contract.paidInstallmentCount}회/미납 ${contract.missedInstallmentCount}회 · ${next} · 만기 ${contract.maturityGameDay}일차`;
}

function militarySavingsHistoryText(contract: MilitarySavingsHistoryItem): string {
  const projection =
    contract.projectedMaturity === null
      ? ''
      : ` · 서버 예상 원금 ${formatWon(contract.projectedMaturity.principalKrw)}, 은행이자 ${formatWon(contract.projectedMaturity.grossBankInterestKrw)}, 정부지원 ${formatWon(contract.projectedMaturity.governmentMatchKrw)}, 총혜택 ${formatWon(contract.projectedMaturity.totalBenefitKrw)}`;
  const actual =
    contract.status === 'active'
      ? ''
      : ` · 정산 원금 ${formatWon(contract.settledPrincipalKrw)}, 은행이자 ${formatWon(contract.grossBankInterestKrw)}, 정부지원 ${formatWon(contract.governmentMatchKrw)}, 은행지급 ${formatWon(contract.bankPayoutKrw)}`;
  const installments = contract.installments
    .map(
      (installment) =>
        `${installment.installmentNo}회 ${MILITARY_SAVINGS_INSTALLMENT_STATUS_LABEL[installment.status]}`,
    )
    .join(', ');
  return `#${contract.id} ${contract.institutionDisplayName} · ${MILITARY_SAVINGS_STATUS_LABEL[contract.status]} · 월 ${formatWon(contract.monthlyContributionKrw)} · 약정 ${contract.contractTermMonths}개월/${ratePpmText(contract.annualInterestRatePpm)} · 현재 원금 ${formatWon(contract.principalKrw)} · 만기 ${contract.maturityGameDay}일차${projection}${actual} · 회차 ${installments || '없음'}`;
}

function militaryOptionIneligibilityText(
  reason: MilitaryOption['ineligibilityReasons'][number],
): string {
  const labels: Record<MilitaryOption['ineligibilityReasons'][number], string> = {
    militarySubjectRequired: '복무 대상 아님',
    militaryStateConflict: '현재 병역 상태',
    minimumEducation: '최소 학력',
    minimumCertificationCount: '최소 자격증 수',
    minimumExperienceDays: '최소 경력',
    policyUnavailable: '정책 없음',
  };
  return labels[reason];
}

function militarySavingsIneligibilityText(
  reason: MilitarySavingsProduct['ineligibilityReasons'][number],
): string {
  const labels: Record<MilitarySavingsProduct['ineligibilityReasons'][number], string> = {
    militaryStateConflict: '복무 상태',
    serviceTypeNotEligible: '가입 대상 복무가 아님',
    minimumRemainingService: '잔여복무기간 부족',
    activeContractLimit: '전체 계좌 한도',
    institutionLimit: '기관별 계좌 한도',
    joinWindowClosed: '가입 기간 종료',
    policyUnavailable: '정책 없음',
  };
  return labels[reason];
}

function ratePpmText(ratePpm: number): string {
  const whole = Math.floor(ratePpm / 10_000);
  const fraction = (ratePpm % 10_000).toString().padStart(4, '0').replace(/0+$/, '');
  return `${whole}${fraction.length === 0 ? '' : `.${fraction}`}%`;
}

function fixedSelectOptions(
  capacity: number,
  placeholder: string,
): readonly { readonly value: string; readonly label: string }[] {
  return [
    { value: '', label: placeholder },
    ...Array.from({ length: capacity }, (_, index) => ({
      value: `__slot-${index}`,
      label: '—',
    })),
  ];
}

function updateFixedSelectOptions(
  form: HTMLFormElement,
  fieldName: string,
  choices: readonly { readonly value: string; readonly label: string }[],
  preferredValue: string | undefined,
): void {
  const field = form.elements.namedItem(fieldName);
  if (!(field instanceof HTMLSelectElement)) {
    throw new Error(`고정 선택 필드를 찾을 수 없습니다: ${fieldName}`);
  }
  const capacity = field.options.length - 1;
  if (choices.length > capacity) {
    throw new Error(`고정 선택 필드 용량을 초과했습니다: ${fieldName}`);
  }

  const previousValue = field.value;
  for (let index = 0; index < capacity; index += 1) {
    const option = field.options.item(index + 1);
    if (option === null) continue;
    const choice = choices[index];
    option.value = choice?.value ?? `__slot-${index}`;
    option.textContent = choice?.label ?? '—';
    option.hidden = choice === undefined;
    option.disabled = choice === undefined;
  }

  const hasPrevious = choices.some((choice) => choice.value === previousValue);
  const hasPreferred = choices.some((choice) => choice.value === preferredValue);
  field.value = hasPrevious
    ? previousValue
    : hasPreferred
      ? (preferredValue ?? '')
      : (choices[0]?.value ?? '');
}

function createFixedList<T>(capacity: number, format: (item: T) => string): FixedList<T> {
  const element = el('ul');
  const rows: FixedListRow<T>[] = [];
  for (let index = 0; index < capacity; index += 1) {
    const row = createFixedListRow(format);
    rows.push(row);
    element.appendChild(row.element);
  }
  return {
    element,
    setItems(items) {
      for (let index = 0; index < rows.length; index += 1) {
        rows[index]?.setItem(items[index]);
      }
    },
  };
}

function createFixedListRow<T>(format: (item: T) => string): FixedListRow<T> {
  const element = el('li');
  const update = bindText(element);
  element.hidden = true;
  return {
    element,
    setItem(item) {
      if (element.hidden !== (item === undefined)) element.hidden = item === undefined;
      if (item !== undefined) update(format(item));
    },
  };
}

function submitButtonOf(form: HTMLFormElement): HTMLButtonElement {
  const button = form.querySelector('button[type="submit"]');
  if (!(button instanceof HTMLButtonElement)) throw new Error('제출 버튼을 찾을 수 없습니다.');
  return button;
}

function selectFieldOf(form: HTMLFormElement, name: string): HTMLSelectElement {
  const field = form.elements.namedItem(name);
  if (!(field instanceof HTMLSelectElement))
    throw new Error(`선택 필드를 찾을 수 없습니다: ${name}`);
  return field;
}

function textFieldOf(form: HTMLFormElement, name: string): HTMLInputElement {
  const field = form.elements.namedItem(name);
  if (!(field instanceof HTMLInputElement))
    throw new Error(`입력 필드를 찾을 수 없습니다: ${name}`);
  return field;
}

function fieldRowOf(form: HTMLFormElement, name: string): HTMLElement {
  const field = form.elements.namedItem(name);
  if (!(field instanceof HTMLElement)) throw new Error(`입력 필드를 찾을 수 없습니다: ${name}`);
  const row = field.closest('.field');
  if (!(row instanceof HTMLElement)) throw new Error(`입력 행을 찾을 수 없습니다: ${name}`);
  return row;
}

function scoreText(score: number | null | undefined): string {
  return score === null || score === undefined ? '—' : formatBasisPoints(score);
}

function evidenceText(evidence: CareerEvidence): string {
  const expiration =
    evidence.expiresOnGameDay === null ? '만료 없음' : `${evidence.expiresOnGameDay}일차 만료`;
  const period =
    evidence.periodStartDate === null || evidence.periodEndExclusiveDate === null
      ? '기간 없음'
      : `${evidence.periodStartDate} 이상 ${evidence.periodEndExclusiveDate} 미만`;
  const creditedExperience =
    evidence.creditedExperienceDays === null
      ? ''
      : ` · 인정 경력 ${evidence.creditedExperienceDays}일`;
  return `#${evidence.id} ${evidence.displayName} · ${EVIDENCE_KIND_LABEL[evidence.kind]} · ${evidence.acquiredGameDay}일차 취득 · ${expiration} · ${period}${creditedExperience}`;
}

function activityText(activity: CareerActivitySummary): string {
  const priority = activity.priority === null ? '우선순위 없음' : `${activity.priority}순위`;
  return `#${activity.id} ${activity.displayName} · ${priority} · 노력 ${activity.accumulatedEffortUnits.toLocaleString('ko-KR')}/${activity.requiredEffortUnits.toLocaleString('ko-KR')} · 달력 ${activity.elapsedCalendarDays}/${activity.minimumCalendarDays}일`;
}

function catalogEntryText(entry: CareerActivityCatalogEntry): string {
  const statuses = entry.allowedLifeStatuses.map((status) => LIFE_STATUS_LABEL[status]).join(', ');
  return `#${entry.id} ${entry.displayName} · ${EVIDENCE_KIND_LABEL[entry.outputKind]} · 최소 ${entry.minimumCalendarDays}일 · 필요 노력 ${entry.requiredEffortUnits.toLocaleString('ko-KR')} (하루 최대 ${entry.dailyEffortCapUnits.toLocaleString('ko-KR')}) · 비용 ${formatWon(entry.costKrw)} · 가능 상태 ${statuses}`;
}

function activityHistoryText(activity: CareerActivityHistoryItem): string {
  const completed =
    activity.completedGameDay === null ? '' : ` · ${activity.completedGameDay}일차 완료`;
  const cancelled =
    activity.cancelledGameDay === null ? '' : ` · ${activity.cancelledGameDay}일차 취소`;
  return `#${activity.id} ${activity.displayName} · ${activityStatusLabel(activity.status)} · 노력 ${activity.accumulatedEffortUnits.toLocaleString('ko-KR')}/${activity.requiredEffortUnits.toLocaleString('ko-KR')}${completed}${cancelled}`;
}

function activityStatusLabel(status: CareerActivitySummary['status']): string {
  const labels: Record<CareerActivitySummary['status'], string> = {
    planned: '계획',
    active: '진행 중',
    completed: '완료',
    cancelled: '취소',
  };
  return labels[status];
}

function artifactSummaryText(artifact: CareerArtifactSummary): string {
  return `#${artifact.id} ${ARTIFACT_KIND_LABEL[artifact.kind]} ${artifact.versionNo}버전 · 완성도 ${formatBasisPoints(artifact.completenessBp)} · ${artifact.createdGameDay}일차 게시`;
}

function artifactText(artifact: CareerArtifact): string {
  const evidence =
    artifact.evidenceIds.length === 0
      ? '증빙 없음'
      : `증빙 ${artifact.evidenceIds.map((id) => `#${id}`).join(', ')}`;
  const linkedin =
    artifact.kind === 'linkedinProfile'
      ? ` · ${artifact.openToWork ? '구직 공개' : '구직 비공개'} · 업종 ${artifact.industries.map((industry) => INDUSTRY_LABEL[industry]).join(', ') || '없음'}`
      : '';
  return `#${artifact.id} ${ARTIFACT_KIND_LABEL[artifact.kind]} ${artifact.versionNo}버전 · 완성도 ${formatBasisPoints(artifact.completenessBp)} · ${artifact.createdGameDay}일차 · ${artifact.headline} · ${evidence}${linkedin} · 요약: ${artifact.summary || '없음'}`;
}
