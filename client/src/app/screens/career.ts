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
  type CareerPlatform,
  type EvidenceKind,
  type GameSnapshot,
  type LifeStatus,
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

        let specsGeneration = 0;
        let activitiesGeneration = 0;
        let artifactsGeneration = 0;
        let jobsGeneration = 0;
        let applicationsGeneration = 0;
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
        const catalogAvailable = h.useComputed(() => {
          return activityCatalog.get().length > 0;
        });
        const activeActivities = h.useComputed<readonly CareerActivitySummary[]>(() => {
          return queriedActiveActivities.get() ?? career.get()?.activeActivities ?? [];
        });

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
        ctx.bag.add(focusForm);
        ctx.bag.add(activityStartForm);
        ctx.bag.add(activityCancelForm);
        ctx.bag.add(artifactForm);
        ctx.bag.add(applicationForm);
        ctx.bag.add(interviewForm);
        ctx.bag.add(pathActionForm);

        const focusSubmit = submitButtonOf(focusForm.element);
        const activityStartSubmit = submitButtonOf(activityStartForm.element);
        const activityCancelSubmit = submitButtonOf(activityCancelForm.element);
        const artifactSubmit = submitButtonOf(artifactForm.element);
        const applicationSubmit = submitButtonOf(applicationForm.element);
        const interviewSubmit = submitButtonOf(interviewForm.element);
        const pathActionSubmit = submitButtonOf(pathActionForm.element);
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
        const employmentStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });

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
          el('section', {}, el('h2', {}, '근로계약'), employmentStatus),
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
        const invalidateCareerQueries = (): void => {
          invalidateSpecs();
          invalidateActivities();
          invalidateArtifacts();
          invalidateJobs();
          invalidateApplications();
        };
        const runCareerQueries = (): void => {
          if (!gameReady.peek()) return;
          specsRequest.run();
          activitiesRequest.run();
          artifactsRequest.run();
          jobsRequest.run();
          applicationsRequest.run();
          employmentRequest.run();
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
        h.useEventListener(specsLoadOlder, 'click', loadOlderSpecs);
        h.useEventListener(activitiesLoadOlder, 'click', loadOlderActivities);
        h.useEventListener(artifactsLoadOlder, 'click', loadOlderArtifacts);
        h.useEventListener(jobsLoadOlder, 'click', loadOlderJobs);
        h.useEventListener(applicationsLoadOlder, 'click', loadOlderApplications);
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
  return `[${job.platform}/${INDUSTRY_LABEL[job.industry]}] ${job.employerName} · ${job.jobFamilyKey} · ${job.region} · 연봉 ${formatWon(job.minimumAnnualSalaryKrw)}~${formatWon(job.maximumAnnualSalaryKrw)} · 요구 산출물 ${requiredArtifacts} · 키 ${job.postingKey}`;
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

function employmentText(contract: CareerEmploymentContract | null): string {
  if (contract === null) return '현재 근로계약이 없습니다.';
  return `${contract.employerName} · ${contract.jobFamilyKey} · ${contract.status} · 연봉 ${formatWon(contract.annualSalaryKrw)} · 입사 예정/시작 ${contract.startGameDay}일 · 인정 경력 ${contract.creditedExperienceDays}일`;
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
  return `#${evidence.id} ${evidence.displayName} · ${EVIDENCE_KIND_LABEL[evidence.kind]} · ${evidence.acquiredGameDay}일차 취득 · ${expiration} · ${period}`;
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
