# M3 커리어·병역 상세 스펙

- 작성: 2026-07-26
- 상태: M3-A·M3-B·M3-C·M3-D 기능 구현 및 자동/실제 MySQL 검증 완료
- 상위 계획: [`development-plan.md` §4.2, §7, §8.1, §12](./development-plan.md)
- 선행 조건: [`m2-accounts-tax.md` §12](./m2-accounts-tax.md)의 M2 완료

## 1. 목표와 단계

M3의 완료 목표는 미필 캐릭터가 복무 형태를 선택해 복무와 장병내일준비적금을 마치고, 여섯 스펙
차원을 쌓아 공고에 지원하고, 면접·오퍼·취업계약을 거쳐 월급을 받은 뒤 연금저축·IRP 납입분을 실제
연말정산 세액공제로 확정하는 한 바퀴를 만드는 것이다.

구현은 다음 네 단계로 나눈다.

1. **M3-A 스펙과 산출물** — 여섯 스펙 차원, 성장 활동, 포트폴리오·이력서·LinkedIn 프로필의 불변 버전
2. **M3-B 채용과 근로계약** — 여섯 플랫폼, 여섯 업종, 결정론적 공고·지원·면접·오퍼·계약
3. **M3-C 급여와 연말정산** — 월 급여, 4대보험, 근로소득 원천징수, 연말정산, 연금 세원층 재분류
4. **M3-D 병역과 자산 형성** — 복무 상태기계, 형태별 급여·경력 효과, 장병내일준비적금, 전체 일일
   planner와 실제 MySQL 검증

M3-A는 migration 0017~0020, 서버 도메인·store·strict API, `/career` 기능 화면까지 구현됐다. complete
`dev-unranked-m3-v1` A/B/D bundle을 게시하고, 기존·새 런 bridge, focus, 활동 비용·일일 effort·취소,
세 산출물의 불변 version과 cursor page를 실제 MySQL 8과 HTTP에서 검증했다. CSS와 시각 다듬기는 기능
마일스톤을 모두 닫은 뒤 진행한다.

M3-B는 migration 0021, 결정론적 일일 공고, 직접·초대 지원, 서류·면접·오퍼 상태기계, 근로계약과
일일 경력 반영, strict API와 `/career` 기능 화면까지 구현됐다. fresh MySQL 8 전체 migration, 상태·확률·
날짜·원인 불변성의 부정 probe, HTTP 명령 replay와 입사 전날·당일·익일 경계를 검증했다.

M3-C는 migration 0022의 정책·급여 사실, 정수 급여·4대보험·원천징수 규칙,
settlement·원장·지갑·연간 누계·원티드 보상·다음 급여 예약, strict API와 `/career` 기능 화면까지 구현됐다.
1월 1일 annual coordinator, 연금 세원층 재분류, 2월 reconciliation과 M2 금융소득 combined 연결도
완료했다. 전체 자동 검증과 fresh MySQL 8의 public HTTP 흐름에서 부분월 gross, 지급일 귀속연도, net 입금,
balanced ledger, 2월 급여 선행·추가납부, 600만원 연금 allocation·환급, 금융소득 combined assessment와
5월 신고 연결을 각각 대조했다.

M3-D는 migration 0023의 durable 병역 상태·typed 복무 정책, 복무 시작/완료 action, 군 급여·근로소득,
장병내일준비적금 가입·납입·중도해지·만기·정부지원, strict API와 `/career` 기능 화면까지 구현됐다.
fresh MySQL 8과 public HTTP에서 546일 현역 복무, 군 급여 18회, 적금 납입 18회, 동일 기관 재가입 제한,
만기 원금·은행이자·정부지원 및 2026년 연말정산을 끝까지 대조했다. 다음 구현 경계는 M4-A 생활비·가구며
CSS와 시각 다듬기는 계속 보류한다.

M3는 커리어 콘텐츠를 대량으로 채우는 단계가 아니다. 각 업종·직무·활동의 최소 불변 카탈로그만 제공하고
추가 공고·자격증·교육·프로젝트는 M5 데이터 작업으로 남긴다. 생활비·실업급여·복지 판정·주거·부동산·
대출·퇴직금·이직·해고·휴직은 M4 이후 범위다. 따라서 M3 v1은 동시에 하나의 근로계약만 허용하고,
재직 중 일반 공고 지원을 막는다.

## 2. 버전과 결정론의 경계

### 2.1 정책과 콘텐츠를 분리한다

M3는 런에 다음 두 불변 키를 고정한다.

- `careerCatalogBundleKey` — 스펙 활동, 플랫폼, 업종·직무, 공고 템플릿, 연봉 밴드, 복무 형태,
  가상 고용주·금융기관을 묶은 콘텐츠 버전
- `employmentPolicySetKey` — 근로소득세, 4대보험, 연말정산, 병역 급여·복무기간,
  장병내일준비적금 자격·한도·지원 규칙을 묶은 제도 버전

게시된 bundle과 policy record는 update/delete하지 않는다. 새 규정은 새 key와 유효기간으로 추가하고,
이미 시작한 런의 과거 급여·지원·판정에는 소급하지 않는다. 법령상 지급일·귀속연도에 따라 새 규칙이
적용되어야 하는 항목은 런의 policy set 안에서 `effectiveFrom · effectiveTo`가 겹치지 않는 하위 버전을
게임 날짜로 선택한다. 벽시계의 현재 날짜나 외부 API 응답을 계산 중에 읽지 않는다.

ranked employment policy는 coverage의 모든 지급일과 귀속연도에 exact 하위 버전을 가져야 한다. 단,
`rankedEligible = false`이고 key가 `dev-unranked-*`인 개발 fixture는 장기 시뮬레이션이 정책 공백에서
멈추지 않도록 마지막 지급일 policy와 `taxYear <= targetTaxYear`인 최신 annual policy를 명시적으로
carry-forward할 수 있다. 이 fallback은 미래 법령을 검수했다는 뜻이 아니며 ranked run에는 절대 적용하지
않는다. 어떤 fallback을 썼는지도 pinned policy/source ID로 결과에 남긴다.

최초 `careerCatalogBundle`은 M3-A를 pin하기 전에 A의 spec·activity·artifact checklist, B의 플랫폼·업종·
직무군·공고·가상 고용주, D의 복무 option·가상 금융기관을 모두 포함한 **하나의 완전한 최소 bundle**로
seed하고 참조 무결성과 게시 검증을 한 번에 통과시킨다. M3-A/B/D 단계는 이 bundle을 읽는 table과 engine을
순서대로 활성화할 뿐, 이미 게시한 bundle에 뒤 단계의 content row를 추가하지 않는다. 콘텐츠를 보강하거나
수정할 때는 항상 새 bundle key를 게시한다.

캘리브레이션과 공식 policy fixture review가 끝나기 전 개발 seed는 key 자체가 `dev-unranked-*`처럼
비운영·비랭크임을 분명히 드러내고 ranked run에서 선택할 수 없게 한다. review를 통과한 값은 개발 key를
rename하지 않고 새 production key로 게시한다.

실제 2026년 숫자는 구현자가 기억으로 옮기지 않는다. 공식 원문을 대조한 typed seed data와 원문 URL,
시행일, 확인일, 원문 파일 checksum을 함께 저장한다. 특히 다음 값은 코드 상수가 아니다.

- 국민연금·건강보험·장기요양보험·고용보험·산재보험의 요율, 상·하한, 기준보수, 근로자·사용자 부담
- 근로소득 간이세액표, 근로소득공제·기본공제·세율·세액공제와 각 반올림 단위
- 복무 형태별 자격·기간·급여 단계, 장병내일준비적금 가입·납입·만기·매칭 규칙
- 업종·직무별 연봉 밴드, 플랫폼 경쟁도, 스펙 활동 비용·기간·점수

제도와 무관한 상태기계, 판정 순서, seed 입력, 정수 나눗셈과 tie-break는 이 문서에서 고정한다.

### 2.2 M2 런과의 연결

M3 마이그레이션 시 기존 런에는 그 환경과 mode에서 허용되고 §2.1의 complete 검증을 통과한 M3
bundle/policy key를 한 번만 pin한다. 이 pin은
시장 모델·일봉·M2 상품 bundle을 바꾸지 않으며, M3 행이 없는 기존 런의 `snapshot.career`는 마이그레이션
직후 §2.3의 bridge 초기 상태로 합성한다. 새 캐릭터 시작은 같은 key로 새 `run_revision`의 M3 상태를 만든다.

M3-B 계약이 이미 진행 중인 런에 M3-C 급여를 붙일 때는 과거 급여를 소급 지급하지 않는다. 계약에
`payrollBaselinePeriodNo`를 불변으로 pin하고, 마이그레이션 시 현재 `save.gameDay`보다 지급일이 엄격히 뒤인
첫 period를 baseline으로 선택해 그 회차부터 예약한다. 신규 계약의 baseline은 1이다. 첫 payroll row는
period 1이 아니라 그 계약의 baseline과 같아야 하고, 이후 period는 빈틈 없이 1씩 증가한다. 따라서 이미
지난 지급일을 pending으로 만들어 overdue 상태에 빠뜨리거나, 지급일만 현재 이후로 임의 변경하지 않는다.

M2의 `run_tax_profile`에 들어 있던 prior-year placeholder는 더 이상 근로소득 권위가 아니다. M3 이후에는
마감된 `employment_income_year`와 `year_end_tax_assessment`만 연금 공제율과 금융소득 종합과세의 다른
종합소득 입력을 제공한다. M3 이전 연도는 기존 값과 구분되는 `legacyProfile` source로 보존하고 새 급여와
섞지 않는다.

귀속연도에 M3 과세 급여나 `employment_income_year`가 하나라도 있으면 그 연도의 `run_tax_profile` 값은
읽지 않는다. `run_tax_profile`은 연도 열이 없으므로 그 legacy 값은 정확히 `worldStartDate.year - 1`에만
대응하며 다른 연도에 반복 사용하지 않는다. 그 한 연도에도 M3 근로소득과 legacy를 합산하는 경로는 없다.
M2 `save.policy_set_id`는 금융계좌·연금 납입과 금융소득세의 권위로 계속 유지한다. employment policy는
그 값을 복제하지 않고 게시 시 요구 M2 policy와의 compatibility를 명시하며, 런 pin transaction은 현재 두
policy 조합이 게시된 compatibility와 일치할 때만 성공한다.

단계 구현 중에는 첫 소비 시점에 맞춰 pin을 추가한다. M3-A의 `career_run`은 아직 소비하지 않는
`employmentPolicySetKey`를 임시 값으로 만들지 않고 `careerCatalogBundleKey · focusedJobFamilyKey ·
birthDate`만 저장한다. M3-C migration이 첫 급여·세금 계산 전에 published employment policy를 기존·새
런에 원자적으로 pin하고 `career_run`의 필수 FK로 추가한다. M3 최종 상태에서는 아래 §2.3의 두 key가
모두 존재한다.

M3-A migration 0017~0020이 게시한 첫 bundle에는 B 콘텐츠 자체는 모두 있지만, 서류·면접·역제안 확률과
합성 점수 weight처럼 채용 엔진의 calibration row가 포함되지 않았다. 게시된 bundle graph에 뒤늦게 row를
추가하지 않기 위해 M3-B는 별도 immutable `recruitmentRuleset`을 게시한다. 이 key는 세 번째 런 pin이
아니다. career bundle별 `newPosting` assignment와 명시적 bundle compatibility가 새 공유 공고에 적용할
ruleset을 고르고,
각 `job_posting`이 `careerCatalogBundleKey · recruitmentRulesetKey`를 함께 pin한다. 이미 materialize된
공고와 그 지원·오퍼·계약에는 새 assignment를 소급하지 않는다. 첫 개발 ruleset은
`dev-unranked-m3-recruitment-v1`이며 ranked run에 사용할 수 없다.

### 2.3 런 초기화, focus와 legacy bridge

`career_run`은 `(save_id, run_revision)`당 하나이며 `careerCatalogBundleKey · employmentPolicySetKey ·
focusedJobFamilyKey · birthDate`를 가진다. bundle은 자기 안의 직무군 하나를 `defaultFocusedJobFamilyKey`로
반드시 선언하고, 새 런과 기존 런 M3 pin은 그 값을 초기 `focusedJobFamilyKey`로 복사한다. focus는 표시용
선택일 뿐 evidence, 게시 산출물, 이미 제출한 지원서의 pinned 점수를 바꾸지 않는다.

생년월일 입력이 없던 기존 캐릭터와 새 캐릭터 모두 `birthDate = (worldStartDate의 연도 - startingAge)-01-01`
로 결정론적으로 파생한다. 이 값은 나이와 기간 검증의 단일 기준이며 이후 `run_revision` 안에서 바꾸지 않는다.

기존 `character` 시작 필드는 bundle의 typed bridge mapping으로 다음처럼 evidence를 만든다.

- `education`의 `highSchool · associate · bachelor · master · doctorate`는 서로 다른 다섯 bridge education
  entry에 일대일로 매핑하며 정확히 하나를 만든다.
- `certifications = N`은 bundle에 중복 없이 고정된 bridge certification order 50개 중 앞의 N개를 각각
  evidence로 만든다. 서버 권위 범위는 `0..=50`이고 50을 넘는 값은 자르지 않고 거절한다.
- `careerYears = N`은 `0..=30` 각각에 대응하는 31개 bridge experience entry 중 정확히 하나를 만든다.
  period는 `[worldStartDate - N calendar years, worldStartDate)`로 고정하며 N=0만 같은 시작·끝을 가진 빈
  period를 허용한다. 서버는 30을 넘는 값을 거절한다.

bridge order, 다섯 education mapping과 31개 experience mapping은 complete bundle 게시 검증 대상이다.
초기화는 캐릭터 생성 또는 기존 런 pin transaction 안에서 수행하고 재시도해도 같은 `evidenceKey`만 만든다.

## 3. 여섯 스펙 차원

### 3.1 고정 차원과 evidence

코드와 DB enum은 다음 여섯 값만 사용한다.

| enum | 사용자 표시 | evidence와 취득 경로 |
|------|-------------|----------------------|
| `education` | 학력 | 시작 학력 또는 학위 활동 완료. 가장 높은 유효 학위와 직무 관련 전공을 평가 |
| `certification` | 자격증 | 시작 자격증 또는 시험·과정 완료. 유효기간과 직무별 기여도를 가짐 |
| `language` | 어학 | 시험 점수 구간. 취득일·만료일이 지나면 공고 판정에서 제외 |
| `training` | 교육·연수 | 국비·자비 과정 완료. M3는 비용 부담만 구현하고 국비 복지는 M4로 미룸 |
| `experience` | 경력 | 근로계약과 인정되는 복무 형태에서 하루 단위로 누적 |
| `project` | 프로젝트 | 프로젝트 활동 완료. 포트폴리오에 공개할 수 있는 결과 evidence |

`spec_evidence`는 `(save_id, run_revision, evidence_key)`로 유일하고 kind, catalog entry, 취득 게임일,
nullable 만료 게임일, optional `periodStartDate · periodEndExclusiveDate`, 원천
activity/contract/service ID를 가진다. 기간 evidence는 `[periodStartDate, periodEndExclusiveDate)` 두 값을
함께 가지며 시작은 끝보다 앞서야 한다. §2.3의 0년 bridge experience만 같은 시작·끝의 빈 period를 허용하고,
기간이 없는 evidence는 두 값을 모두 비운다. 이미 취득한 evidence는 수정하지 않고 정정 event나 새 버전으로만
바꾼다. 시작 학력·경력·자격증도 캐릭터 생성 transaction에서 같은 형태로 만들어 이후 경로와 예외를 두지
않는다.

`experience` evidence는 실제 활동 기간과 별도로 nonnegative `creditedExperienceDays`를 반드시 보존한다.
일반 근로계약과 기존 period evidence는 period 일수, 0년 bridge는 0일이며, military evidence는 누적
`creditedExperienceDayPpm`을 1,000,000으로 나눈 몫이다. 이력서에는 실제 period를 표시하지만 공고의
`minimumExperienceDays`는 period 길이가 아니라 `creditedExperienceDays` 합을 사용해 부분 인정 경력을
과대계상하지 않는다. 다른 evidence kind의 이 필드는 null이다.

각 evidence catalog row는 직무군별 `contributionBp`를 가진다. 특정 공고에서 한 차원의 보유 점수는
적용 가능한, 만료되지 않은 evidence의 기여도를 모두 더한 뒤 `10,000bp`에서 자른다. 합산과 cap은 i64로
하고 음수 기여도는 허용하지 않는다. 같은 자격증·학위의 중복 evidence는 catalog의 `stackable`이 false면
최초 하나만 인정한다. 정렬 tie-break는 `acquiredGameDay, evidenceId` 오름차순이다.

직무군마다 contribution이 다르므로 여섯 점수에는 항상 평가 직무군이 붙는다. focus 화면과 snapshot의
`possessedScore`는 `career_run.focusedJobFamilyKey` 기준이고, 공고와 지원 응답의 점수는 각
`posting.jobFamilyKey` 기준이다. focus 점수를 공고 판정이나 지원서 pin에 재사용하지 않는다.

### 3.2 성장 활동

`spec_activity`는 `planned · active · completed · cancelled` 상태를 갖는다. 한 런은 최대 3개의 active
활동을 두고 priority `1..=3`을 중복 없이 지정한다. 시작 명령은 catalog 비용을 지갑에서 즉시 지급하며,
취소해도 환급하지 않는다. 완료 결과는 catalog에 선언된 evidence만 만든다.

활동 catalog는 `minimumCalendarDays · requiredEffortUnits · dailyEffortCapUnits · allowedLifeStatuses ·
costKrw · evidenceKey`를 가진다. `dailyEffortCapUnits`는 양의 정수이며 하루의 가용 effort는
`unemployed · employed · activeDuty · socialService · specialService · officerOrNco`별 bundle 값이다.
planner는 priority 오름차순으로 각 활동의 `dailyEffortCapUnits`까지 배분하고 남은
effort를 다음 활동에 넘긴다. `elapsedDays >= minimumCalendarDays`이고 누적 effort가 requirement에
도달한 날의 planner에서 완료한다. 소수 effort와 랜덤 성공은 없고, 시험 합격 여부가 필요한 카탈로그도
필요 effort에 포함해 결과는 결정론적으로 만든다.

### 3.3 점수의 두 관점

- `possessedScore` — 실제 보유한 유효 evidence 전체로 계산한다. 면접과 오퍼 판정에 사용한다.
- `visibleScore` — 지원서가 고정한 산출물 버전에 실린 evidence만 계산한다. 서류 판정에 사용한다.

공고의 각 차원은 `requiredScoreBp`와 합계가 정확히 `10,000`인 `weightBp`를 가진다. 차원별 적합도는
requirement가 0이면 10,000, 아니면
`min(10,000, floor(candidateScoreBp × 10,000 / requiredScoreBp))`이다. 전체 적합도는
`floor(sum(dimensionFitBp × weightBp) / 10,000)`이다. 모든 중간값은 i128, 나눗셈은 0 방향 내림이며
범위 초과·음수·가중치 합 오류는 공고 게시 오류다.

## 4. 포트폴리오·이력서·LinkedIn 프로필

### 4.1 불변 버전

산출물 kind는 `portfolio · resume · linkedinProfile`로 고정한다. 저장 명령 한 번이 곧 새 immutable
published version을 만든다. `(save_id, run_revision, kind, version_no)`가 유일하고 version은 kind별 1부터
빈틈없이 증가한다. 이전 버전을 수정·삭제하지 않고 화면에서 최신 버전과 지원서가 사용한 버전을 함께
볼 수 있다.

공통 필드는 `headline · summary · evidenceIds · createdGameDay`다. API는 headline과 summary의 양끝
Unicode whitespace를 먼저 trim하고 그 canonical 문자열만 저장한다. headline은 줄바꿈 없는
`1..=120` Unicode scalar, summary는 `0..=2,000` Unicode scalar다. NUL과 C0 control은 금지하되 summary의
LF와 tab만 예외로 허용한다. headline에는 이 예외가 없다. 두 필드는 화면·내보내기 어디서나 text node나
동등한 escaping API로만 렌더링하고 HTML로 해석하지 않는다. evidence는 현재 런 소유이며 해당 kind에
허용된 것만 참조한다.

- `portfolio` — `project · training · certification` evidence만, 최대 12개. project가 하나도 없으면
  만들 수 있지만 완성도에서 해당 항목은 0점이다.
- `resume` — 여섯 차원 모두, 최대 40개. period가 있는 education끼리 또는 experience끼리 같은 차원에서
  겹치면 거절한다. education과 experience의 교차 차원 overlap은 허용한다. 어떤 period도 §2.3에서 파생한
  15번째 생일보다 먼저 시작할 수 없으며, 끝이 현재 게임 날짜 뒤이거나 잘못된 `[start,end)`이면 거절한다.
- `linkedinProfile` — 여섯 차원 모두, 최대 30개. `openToWork`와 업종 최대 3개를 함께 고정한다.

### 4.2 완성도와 지원 pin

완성도는 bundle의 kind별 typed checklist를 충족한 `weightBp` 합으로 계산하며 `0..=10,000bp` 정수다.
허용 rule은 `headlinePresent · summaryPresent · minimumEvidenceCount(count) · containsDimension(dimension) ·
containsEvidenceKind(evidenceKind) · projectPresent · openToWork · industryCountAtLeast(count)`뿐이다.
각 artifact kind의 rule weight 합은 정확히 `10,000`이어야 하고, 음수 weight, 중복 rule identity,
kind에 적용할 수 없는 rule/parameter는 bundle 게시를 거절한다. present는 trim 뒤 비어 있지 않음을,
count rule은 중복 제거와 allowlist 검증을 끝낸 canonical evidence/industry 배열을 기준으로 판정한다.
사용자 문구의 의미를 분석하거나 LLM으로 품질을 판정하지 않는다.

지원서는 제출 시 필요한 산출물의 exact version ID와 공고 직무군으로 계산한 여섯 visible score·완성도를
저장한다. 이후 새 버전을 만들거나 focus를 바꿔도 진행 중 지원의 서류·면접·오퍼 결과는 바뀌지 않는다.

플랫폼별 필수 산출물은 다음과 같다.

| platform enum | 표시명 | 제출 또는 공개 산출물 |
|---------------|--------|------------------------|
| `sarangbang` | 사랑방 | 이력서 |
| `jobkorea` | 잡코리아 | 이력서 |
| `saramin` | 사람인 | 이력서 |
| `wanted` | 원티드 | 이력서 + 포트폴리오 |
| `linkedin` | LinkedIn | LinkedIn 프로필 |
| `work24` | 고용24 | 이력서 |

## 5. 여섯 플랫폼과 여섯 업종

### 5.1 초기 업종

M3 v1은 다음 여섯 업종 enum을 고정한다.

| enum | 사용자 표시 | 초기 직무군 성격 |
|------|-------------|------------------|
| `itSoftware` | IT·소프트웨어 | 개발·데이터, 프로젝트와 경력 비중이 큼 |
| `financeInsurance` | 금융·보험 | 자격증·학력 비중이 큼 |
| `manufacturing` | 제조·생산 | 자격증·경력 비중이 큼 |
| `constructionEngineering` | 건설·기술 | 자격증·경력 비중이 큼 |
| `retailService` | 유통·서비스 | 지역 접근성과 경력 비중이 큼 |
| `publicSocial` | 공공·사회서비스 | 학력·자격증·어학의 명시 요건 비중이 큼 |

표의 설명은 방향이고 실제 직무, 차원 weight, 연봉 밴드, 채용 지연은 bundle row다. M3에는 업종별 최소
2개 직무군과 플랫폼마다 하루 최소 한 슬롯을 제공할 만큼의 작은 카탈로그만 넣는다. M5가 row를 늘려도
enum과 판정기는 바꾸지 않는다.

### 5.2 플랫폼 차이

- 사랑방은 캐릭터 지역과 공고 지역이 같을 때만 노출하고 경쟁도는 낮으며 서류 결과가 빠르다.
- 잡코리아는 전 업종 슬롯이 많고 competition band가 높다.
- 사람인은 지원 외에 공개 최신 이력서 완성도로 결정론적 역제안을 만든다.
- 원티드는 `itSoftware`와 경력직 비중이 높고, 고용 유지 첫 급여 때 가상 채용보상금을 한 번 지급한다.
- LinkedIn은 `openToWork`인 최신 프로필의 완성도·어학·경력으로 인바운드 제안을 만든다.
- 고용24는 공공·사회서비스 공고와 training catalog 접근 창구다. 국비 금액·복지 자격 판정은 M4 전에는
  적용하지 않는다.

플랫폼별 슬롯 수, 업종 weight, competition band, 처리 일수, 보상금은 bundle 값이다. 실제 회사의 공고를
수집하거나 특정 기업명을 사용하지 않고 가상 고용주만 제공한다.

## 6. 결정론적 공고·지원·면접·오퍼

### 6.1 공고 생성

공고는 market cache처럼 player transaction 전에 준비하는 공유 불변 데이터다. `(worldModelVersion,
worldSeed, careerCatalogBundleKey, recruitmentRulesetKey, gameDay, platformKey, slotNo)`를
HMAC-SHA-256의 canonical byte input으로 쓰고 필요한 난수 단어를 counter로 확장한다. 여기서
`worldModelVersion`은 `market_calibration.version`이다. DB auto increment, save ID, HTTP 호출 순서,
프로세스 PID, 벽시계는 seed에 넣지 않는다.

바이트 계약은 다음과 같이 고정한다.

- HMAC key는 `worldSeed`의 unsigned 64-bit big-endian 8바이트다.
- message는 ASCII domain separator `lifeledger.recruitment.posting.v1\0` 뒤에 위 필드를 순서대로 붙인다.
  문자열은 UTF-8 byte length를 unsigned 32-bit big-endian으로 먼저 쓰고 bytes를 붙이며, `gameDay · slotNo ·
  counter`는 unsigned 32-bit big-endian이다. seed는 key로 이미 들어가므로 message에 다시 쓰지 않는다.
- counter 0의 32바이트 digest를 lowercase 64자리 hex `postingKey`로 쓴다. counter 1부터는 digest의 앞
  8바이트를 unsigned 64-bit big-endian word로 읽는다.
- 플랫폼에 속한 template을 `templateKey` byte 오름차순으로 정렬하고 각 template 업종의
  `platformIndustryWeightBp`를 weight로 쓴다. 선택값은 `floor(word × totalWeight / 2^64)`이며 누적합이
  선택값보다 처음 커지는 template을 고른다. weight 0은 선택되지 않고, 후보 없음·합계 0은 catalog
  오류다.

각 플랫폼의 `dailySlotCount`만큼 0-based 슬롯 `0 <= slotNo < dailySlotCount`를 materialize한다. 같은
template이 여러 슬롯에서 뽑혀도 slot이
다르므로 서로 다른 공고다. `(world, career bundle, postedGameDay, platform, slotNo)` unique가 먼저 만든
공고 하나에 수렴하고 ruleset assignment가 바뀌어도 같은 슬롯을 다시 만들지 않는다. API와 일일 전진은
현재 게임일 공고를 먼저 준비하며, M3-B 적용 전에 이미 진행된 런은 열린 공고를 잃지 않도록 현재일부터
bundle의 최대 `postingOpenDays - 1`만큼만 뒤로 materialize한다.

공고 key는 위 입력의 digest로 만들고 다음 값을 materialize한다.

- 플랫폼·가상 고용주·업종·직무·지역·고용형태
- 여섯 required score와 weight, hard requirement
- 연봉 band, 채용 단계별 예정 일수, 게시·마감 게임일
- competition band, 병역 요건, 산출물 요건, catalog/recruitment ruleset key

같은 세계·날짜의 공고는 모든 랭크 플레이어에게 같고, 자유 모드는 개인 world seed 때문에 달라진다.
다른 플레이어의 지원 수는 경쟁률이나 결과를 바꾸지 않는다.

### 6.2 지원 상태기계

`job_application`의 전이는 다음 한 방향뿐이다. 서류·면접 통과는 score·probability·roll 결과 필드로
보존하고 별도 순간 상태를 만들지 않는다.

- `submitted → documentRejected | interviewAwaitingConfirmation`
- `interviewAwaitingConfirmation → interviewConfirmed | withdrawn`
- `interviewConfirmed → interviewRejected | offered`
- `offered → accepted | declined | expired`
- 아직 끝나지 않은 다른 지원은 한 오퍼 수락이나 새 런 시작 때만 `closed`로 전이한다. 따라서 이 경우에만
  `submitted · interviewAwaitingConfirmation · interviewConfirmed · offered → closed`가 허용된다.

서류 통과 시 `interviewAwaitingConfirmation`과 확인 마감일을 함께 만든다. 플레이어가 마감 전 확인하면
예정일에 면접을 판정하고, 확인하지 않으면 `withdrawn`이다. 오퍼는 `expiresExclusiveGameDay` 전까지만
수락할 수 있다. 한 공고에는 한 번만 지원하고, active 지원은 최대 10개, 하루 신규 지원은 최대 3개다.
이 둘은 실제 제도가 아닌 M3 v1 게임 운영 상수다.

서류 판정일은 `submittedGameDay + platform.documentReviewDays`, 면접일은
`documentDecisionGameDay + template.interviewDelayDays`다. `confirmationDeadlineExclusiveGameDay`는
면접일과 같아 직전 게임일까지 확인해야 한다. 오퍼 만료일은
`offeredGameDay + template.offerExpiryDays`다. 확인 endpoint의 `decision: decline`은
`interviewAwaitingConfirmation`에서만 `withdrawn`으로 전이하고, 별도 withdraw는 `submitted ·
interviewAwaitingConfirmation · interviewConfirmed`에서 허용한다. offered 상태는 offer decline endpoint만
쓴다. 이 날짜들의 exclusive 경계에 도달한 뒤 들어온 명령은 각각 `interviewExpired · offerExpired`다.

사람인·LinkedIn 역제안은 `job_invitation`으로 만들며, 수락하면 이미 공개 프로필을 확인한 것으로 보아
서류를 통과하고 `interviewAwaitingConfirmation`에서 시작한다. 나머지 상태와 확률은 일반 지원과 같다.
역제안 roll은 `(worldSeed, postingKey, platformKey, invitationGameDay, "invitation")`만 사용하고 지원자의
점수는 probability threshold에만 반영한다. 자유 입력 headline·summary, artifact DB ID, version 번호를 seed에
넣지 않아 문구를 고쳐 roll을 다시 뽑을 수 없게 한다. 같은 점수의 플레이어는 같은 공고에서 같은 결과다.

초대 상태는 `open → accepted | declined | expired | closed`다. 만료일은 공고의
`closesExclusiveGameDay`와 같고 phase 60에서 만료를 먼저 적용한다. 런 전체 open invitation은 최대 5개,
플랫폼별 하루 최대 하나다. 각 invitation source 플랫폼에서 hard filter를 통과한 열린 공고를
`postingKey` byte 오름차순으로 보며 첫 성공 하나만 만든다. 사람인 점수는 최신 공개 resume의
`completenessBp`다. LinkedIn은 `openToWork = true`이고 공고 업종을 공개한 최신 profile만 후보이며 점수는
`floor((completenessBp × 5,000 + languageScoreBp × 2,500 + experienceScoreBp × 2,500) / 10,000)`이다.
두 차원 점수는 공고 직무군 기준으로 10,000에 cap한다. 초대는 그 artifact version과 evidence를 pin한다.
초대 수락은 active 10개 상한과 공고 중복을 검사하지만 직접 지원의 하루 3개 상한에는 포함하지 않는다.
오퍼 수락 시 남은 open invitation도 `closed`로 전이한다.

### 6.3 hard filter와 확률

지원 시 다음 hard filter를 먼저 검사한다.

1. 미취업이며 복무가 공고의 허용 상태인지
2. 공고가 열려 있고 지역·플랫폼 접근 조건을 충족하는지
3. 학위·필수 자격증·최소 경력·`completedOrExempt` 병역 요건을 충족하는지
4. 필요한 산출물 버전이 존재하고 현재 런 소유인지
5. 하루·active 지원 상한과 공고별 중복 금지

active 지원은 `submitted · interviewAwaitingConfirmation · interviewConfirmed · offered`다. `pendingStart ·
active` 근로계약 중 하나라도 있으면 취업 상태로 보아 새 지원과 초대 수락을 막는다. M3-D의 durable
병역 상태가 생기기 전 M3-B의 임시 권위는 기존 `character.military`다. `notServed → unserved`,
`serving · alternative → serving`, `completed → completed`, `exempted → exempt`로 정규화하며 serving은
지원할 수 없고 `completedOrExempt`는 completed/exempt만 통과한다. M3-D migration 뒤에는 같은 판정기가
durable service 상태를 입력받고 이 bridge를 더 이상 사용하지 않는다.

서류 score는 `visibleFit · artifactCompleteness · platformAffinity`를, 면접 score는
`possessedFit · experienceProjectFit · profileConsistency`를 bundle의 합계 10,000bp weight로 합친다.
`profileConsistency`는 지원 시 pinned evidence와 면접일의 유효 evidence가 같은지를 정수로 판정하며,
새 evidence가 생긴 것은 감점하지 않고 pinned evidence가 만료·정정된 경우만 catalog 값만큼 차감한다.

점수는 `competitionBand × scoreBand → passProbabilityPpm` lookup table로 확률에 바꾼다. stage roll은
`HMAC(worldSeed, postingKey, "document|interview|offerSalary", applicationOrdinal=1)`의 서로 다른 counter
word를 unsigned 정수로 읽고 `mod 1,000,000`한다. `roll < probabilityPpm`이면 통과다. 같은 공고와 같은
pinned 입력은 재시작·재시도와 무관하게 같은 결과다.

서류와 면접에 통과하면 연봉은 공고의 `[minimumAnnualSalaryKrw, maximumAnnualSalaryKrw]` 안에서
possessed score band와 별도 deterministic roll로 정한다. 원 단위 정수이며 catalog의 salary step에
내림한다. 성별은 hard filter·score·확률·연봉 입력에 절대 넣지 않는다. 병역은 공고에 명시된 자격과
실제 경력 evidence를 통해서만 영향을 준다.

### 6.4 M3-B 개발용 채용 ruleset

`dev-unranked-m3-recruitment-v1`은 엔진을 끝까지 검증하기 위한 비랭크 fixture다. production 승격 값이
아니며, 다음 값과 행의 완전성을 publish trigger가 검사한다.

- 서류 component weight: `visibleFit 6,000 · artifactCompleteness 2,500 · platformAffinity 1,500`
- 면접 component weight: `possessedFit 6,000 · experienceProjectFit 2,500 · profileConsistency 1,500`
- LinkedIn 초대 component weight: `artifactCompleteness 5,000 · languageScore 2,500 · experienceScore 2,500`
- score band: `low 0..=3,999 · medium 4,000..=6,999 · high 7,000..=10,000`
- 오퍼 응답창 종료 뒤 입사 지연 1일, 월 급여일 25일, active application 10개, 직접 지원 하루 3개,
  open invitation 5개

component 계산은 다음과 같다. 필요한 artifact가 둘 이상이면 `artifactCompleteness`는 exact 제출
version들의 completeness 산술평균을 내림한다. `platformAffinity`는 해당 공고 업종 weight를 그 플랫폼의
최대 업종 weight로 나눈 `floor(weight × 10,000 / maximumWeight)`다. `experienceProjectFit`은 possessed
dimension fit의 experience와 project를 산술평균해 내림한다. `profileConsistency`는 지원 때 pin한 distinct
evidence 중 면접일에도 유효한 수를 `floor(validCount × 10,000 / pinnedCount)`로 계산하고 pinned set이
비었으면 10,000이다. 지원 뒤 새 evidence는 이 값에 넣지 않는다. 각 stage 합성 score는
`floor(sum(component × weight) / 10,000)`이고 모든 입력은 `0..=10,000`이어야 한다.

pass probability ppm은 다음 exact table이다.

| stage | competition | low score | medium score | high score |
|-------|-------------|----------:|-------------:|-----------:|
| document | low | 400,000 | 700,000 | 900,000 |
| document | medium | 250,000 | 550,000 | 800,000 |
| document | high | 120,000 | 350,000 | 650,000 |
| interview | low | 350,000 | 650,000 | 880,000 |
| interview | medium | 220,000 | 500,000 | 760,000 |
| interview | high | 100,000 | 300,000 | 600,000 |
| invitation | low | 50,000 | 150,000 | 300,000 |
| invitation | medium | 35,000 | 120,000 | 250,000 |
| invitation | high | 20,000 | 80,000 | 200,000 |

application stage entropy는 HMAC key와 length-prefix 규칙을 §6.1과 같게 쓰되 domain separator를
`lifeledger.recruitment.stage.v1\0`로 바꾸고 `postingKey · stage · applicationOrdinal(=1) · counter`를
encode한다. document/interview는 counter 0의 앞 8바이트를 `mod 1,000,000`하고, salary는 stage
`offerSalary` counter 0의 word를 사용한다. invitation은 별도 domain separator
`lifeledger.recruitment.invitation.v1\0`와 `postingKey · platformKey · invitationGameDay · counter`를
encode하며 counter 0 word를 `mod 1,000,000`한다.

salary는 `possessedFit`의 score band로 공고의 salary step index를 세 구간으로 나눈다. 전체 step 수
`N = ((maximum - minimum) / step) + 1`은 compatibility 검사에서 3 이상이어야 한다. low/medium/high의
index 구간은 각각 `[floor(0×N/3), floor(1×N/3)) · [floor(1×N/3), floor(2×N/3)) ·
[floor(2×N/3), N)`이다. 선택 구간 길이를 `L`이라 할 때 offset은 `floor(word × L / 2^64)`이고,
`minimum + (startIndex + offset) × step`을 오퍼 연봉으로 고정한다.

## 7. 오퍼와 취업계약

오퍼는 연봉, 직무, 근무지역, 월급일, 입사 예정일, 수락 만료일, 원티드 보상 여부를 불변으로 가진다.
수락 transaction은 다른 accepted offer와 active employment가 없는지 확인하고
`employment_contract`와 입사 action을 만든다. 수락 뒤 다른 active 지원·오퍼는 `closed`로 전이한다.
M3-B 오퍼와 계약은 career catalog와 recruitment ruleset만 pin하고 employment policy를 임시로 만들지
않는다. 모든 수락 시점에서 같은 불변 조건을 유지하도록 입사 예정일은
`expiresExclusiveGameDay + ruleset.startDelayDays`, 월급일은 ruleset 값이다. M3-C가 첫
급여를 예약하기 전에 published employment policy를 계약과 런에 원자적으로 pin한다. 원티드 보상액은
catalog 값으로 오퍼에 보존하지만 실제 첫 급여 지급은 M3-C 범위다.

근로계약 상태는 `pendingStart · active · ended`다. M3에는 자발적 퇴사·해고·이직 명령이 없으므로
정상 플레이에서 `ended`는 새 런이나 향후 마일스톤 전이만 사용한다. 계약 기간은
`[startGameDay, endGameDay)`이고 M3 정규직의 end는 null이다. 새 런이 입사 전에 시작되면
`pendingStart` 계약은 `endGameDay = startGameDay`인 0일 구간으로 종료하고, 재직 중 시작되면 마지막으로
경력을 인정한 게임일의 다음 날을 exclusive end로 고정한다.

phase 10에서 pending 계약이 start day에 active가 된 뒤 그 날을 첫 재직일로 센다. 계약의
`creditedExperienceDays`를 active day마다 하나씩 늘리고, 같은 날 활동 planner는 `employed` capacity를
사용한다. M3에는 계약 종료 명령이 없으므로 이 진행 중 값으로 새 immutable experience evidence를 만들지
않는다. 향후 계약이 끝나는 마일스톤이 `[start,end)` period evidence를 한 번 확정한다.

계약의 연봉을 월급으로 나눌 때 `q = annualSalaryKrw / 12`, `r = annualSalaryKrw % 12`로 두고 계약
급여연도 1..r월은 `q + 1원`, 나머지는 `q원`으로 계산해 12개월 합계를 정확히 연봉과 맞춘다. M3 v1은
과세 기본급만 있고 상여·초과근무·비과세 수당·퇴직급여는 없다.

급여 period는 계약별 단조 증가 `periodNo`를 1부터 영구 보존하고,
`salaryMonthOrdinal = ((periodNo - 1) mod 12) + 1`로 연봉 분할 순번을 파생한다. 첫 입사월이 부분월이어도
`periodNo = 1`, ordinal 1이고 계약이 바뀌면 새 계약은 다시 1에서 시작한다. 따라서 2년 차 settlement도
첫해와 unique key가 충돌하지 않으며, 어느 달에 입사해도 연속한 12개 기본 월액의 합은 연봉과 같다.
첫 부분월에만 그 ordinal의 기본 월액을 아래 일수 비율로 줄인다.

급여 period는 직전 달력 월이고 catalog의 payday에 지급한다. payday가 없는 달은 말일로 당긴다.
입사 첫 달은 `[startDate, monthEnd]`의 재직일수/그 달의 달력일수로 gross를 내림한다. 이후 완전한 달은
월급 전액이다. 급여일마다 다음 급여 settlement를 한 번 예약하며 동일 contract/period를 unique로 막는다.

M3 v1의 `payroll_record.taxYear`와 `employment_income_year.taxYear`는 실제 `payday`의 달력연도다.
따라서 12월 근무분을 다음 1월에 지급하면 다음 귀속연도 누계에 들어가며, 1월 1일 coordinator는 직전
12월 31일까지 실제 지급이 끝난 payroll만 닫는다. 미지급 급여의 법정 의제지급·발생주의 조정은 별도
accrual 권위가 필요한 규칙이므로 M3 v1에서는 지원하지 않고, period 날짜는 일할 gross와 보험 부과
판정에만 쓴다.

원티드 채용보상은 고용주 급여가 아니라 플랫폼이 지급하는 `otherIncomeReward`다. 고용 유지 후 첫 급여일에
같은 planner transaction에서 지급하되 별도 `career_reward_payment`와 원장 transaction으로 기록하고,
근로소득·4대보험·연말정산 총급여에는 넣지 않는다. M3 v1은 gross의 소득세 20%와 지방소득세 2%를 각각
원천징수한 뒤 나머지를 지갑에 넣고 이 지급으로 과세를 종결한다. 이 분류·요율은 employment policy row와
근거 provenance를 가져야 하며, 공식 검수 전 fixture는 `dev-unranked-*`에서만 사용할 수 있다.

## 8. 월급, 4대보험과 근로소득 원천징수

### 8.1 typed policy 입력

급여 계산기는 지급일에 유효한 policy에서 다음 typed record를 읽는다.

- `nationalPension` — 기준소득월액 결정, 상·하한, 근로자·사용자 rate, 원 단위 처리
- `healthInsurance` — 보수월액, 근로자·사용자 rate와 절사 단위
- `longTermCare` — 건강보험료 또는 보수 기준 산식, 부담 분할, 절사 단위
- `employmentInsurance` — 근로자 rate와 사용자 규모별 rate
- `industrialAccident` — 업종별 사용자 rate. 근로자 월급에서는 공제하지 않음
- `employmentWithholdingTable` — 월 과세급여, 공제대상 가족 수, 자녀 수별 간이세액표
- `localIncomeWithholding` — 소득세 연동 산식과 절사 단위

M3는 간이세액 비율 선택을 제공하지 않고 100%로 고정한다. 부양가족 입력은 캐릭터의 현재 확정값만 쓰며
M4 전에는 장애·출산·주거·의료·교육비 같은 증빙을 합성하지 않는다. policy에 필요한 row가 없거나 기간이
겹치거나 rounding metadata가 없으면 급여일 전체를 `policyUnavailable`로 실패시키며 0원으로 대체하지 않는다.

간이세액표는 급여 귀속월이 아니라 실제 지급일에 유효한 version을 고른다. 2026-01-01부터
2026-02-28 지급분은 2024-02-29 개정 별표 2, 2026-03-01 지급분부터는 2026 개정 별표 2를 쓴다.
M3의 캐릭터에는 배우자·자녀의 나이와 관계 정보가 없으므로 `공제대상가족수 = 1 + dependents`로 고정하고
자녀 수는 0으로 전달한다. `dependents`의 서버 범위가 0..=6이므로 표 조회 가족 수는 1..=7이다. M4가 관계와
나이를 도입하기 전에는 임의로 자녀 공제나 배우자 공제를 추정하지 않는다.

첫 `dev-unranked-m3-employment-2026-v1` fixture는 다음 검수된 경계를 typed row로 저장한다.

- 국민연금은 2026년 근로자·사용자 각각 4.75%다. 최초 기준소득월액은 계약 월평균 과세급여의 1,000원
  미만을 버린 뒤 상·하한을 적용한다. 2026-01-01..06-30은 400,000..6,370,000원,
  2026-07-01..12-31은 410,000..6,590,000원이며 두 부담분을 각각 계산하고 10원 미만을 버린다.
- 직장 건강보험은 총 7.19%, 근로자·사용자 각각 3.595%다. 장기요양은 각자의 건강보험료에
  `0.9448 / 7.19`를 곱한다. 공개 원문에서 근로자 급여공제의 정확한 단수 예시까지 확인되지 않았으므로
  개발 fixture는 각 단계 10원 미만 절사를 명시하고 ranked policy 게시를 막는다. 검수된 단수 규칙은 기존
  row를 수정하지 않고 새 policy version으로 게시한다.
- 고용보험 실업급여분은 근로자·사용자 각각 0.9%다. 현재 가상 고용주는 모두 `under150`으로 mapping해
  사용자 고용안정·직업능력개발 0.25%를 더한다. 산재보험은 업종별 사용자 전액 부담이고 근로자 공제는 0이다.
  업종별 공식 고시 mapping이 끝나기 전에는 개발용 최소 rate만 unranked policy에 둔다.

부분월 보험료는 gross 급여 일할과 별도로 계산한다. 입사일이 그 달 1일이면 국민연금·건강보험·장기요양을
그 달부터 월액 전액 부과하고, 2일 이후면 다음 달부터 부과한다. 국민연금의 선택적 취득월 납부는 M3 v1에서
신청하지 않는다. 이 세 보험은 부과하는 달 안에서 재직일수로 다시 일할하지 않는다. 고용보험과 산재보험은
실제 해당 period gross를 보수로 사용하므로 부분월 gross만큼만 계산한다. 각 보험의 기준보수·부과 여부·rate·
rounding 결과는 `payroll_record`에 분리해 저장한다.

국민연금·건강보험·장기요양·고용보험의 근로자 부담은 net pay에서 공제한다. 산재보험과 각 사용자 부담은
`payroll_record`에 정보로 남기되 플레이어 지갑·순자산 원장에는 posting하지 않는다.

### 8.2 급여 원장

한 급여 transaction은 gross 급여를 `salaryIncome`, 지갑 순입금을 `wallet`, 근로자 보험료를
`employeeNationalPensionExpense · employeeHealthInsuranceExpense · employeeLongTermCareExpense ·
employeeEmploymentInsuranceExpense`, 세금 원천징수를 `employmentIncomeTaxWithholding ·
employmentLocalIncomeTaxWithholding`에 기록하고 합을 0으로 맞춘다.

`netPayKrw = grossPayKrw - employeeInsuranceTotalKrw - withheldIncomeTaxKrw -
withheldLocalIncomeTaxKrw`이며 음수면 정책 오류로 급여 전체를 롤백한다. payroll row, 근로소득 연간 누계,
원장, 지갑, settlement 전이는 같은 transaction이다. 0원 항목은 posting을 만들지 않으며 gross 자체가
0원이면 `noMovement`로 처리한다.

## 9. 연말정산과 연금 세액공제 확정

### 9.1 연간 마감 coordinator

1월 1일 일일 planner는 당일 소득·정산 전에 직전 귀속연도를 닫는다.

1. 직전 연도를 `taxYear`로 가진 payroll·군 복무 중 과세 근로소득·근로자 보험료·원천세 누계를 고정한다.
2. policy의 근로소득공제, 인적공제, 보험료 소득공제, 기본세율, 근로소득세액공제를 정수로 계산한다.
3. M2 `pension_contribution_year`의 납입과 예상 eligible을 읽되 아직 실제 공제로 간주하지 않는다.
4. 금융소득 종합과세 대상이 아니면 employment-only assessment가 definitive다.
5. 대상이면 M2 비교과세가 같은 employment taxable income과 공제·원천세를 읽어 combined definitive
   assessment를 만들고, employment-only 결과는 2월 회사 정산용 provisional로 보존한다.
6. definitive assessment가 실제 사용한 연금 세액공제와 세원층 재분류를 확정한다.
7. employment-only 원천세 차액은 다음 2월 급여일 settlement로, combined 추가세액·환급은 M2의 5월
   신고 settlement로 예약한다.

M2와 M3가 각자 연도를 닫지 않는다. 하나의 annual coordinator가 employment 입력을 먼저 고정한 뒤 기존
M2 비교과세 엔진을 호출하고 `employment_income_year · year_end_tax_assessment ·
financial_income_assessment`를 한 transaction에서 서로 연결한다. 같은 `(save, run, taxYear)`에 대한 재시도는
이미 확정된 coordinator receipt를 replay한다. M2의 연금저축 단독 6,000,000원, 연금저축+IRP 합산
9,000,000원과 계좌별 납입 권위는 계속 M2 pinned finance policy에서 읽고 employment policy에 복제하지 않는다.

coordinator가 2월 reconciliation을 예약하기 전 정산 anchor를 고정한다. 우선순위는 같은 런의 계속되는
민간 2월 payroll, 계속 복무 중인 2월 military pay, employment annual policy의 독립
`februaryReconciliationDayOfMonth` 순서다. 앞의 두 급여 anchor를 쓰면 해당 settlement를 먼저 ensure하고
그 뒤 reconciliation을 insert해 같은 due day에는 급여가 더 작은 settlement ID를 갖게 한다.
`dev-unranked-m3-employment-2026-v1`의 독립 정산일은 28일이며, 해당 일이 없는 달은 말일로 당긴다.
이 값은 급여 anchor가 없는 기능 fixture의 결정론적 fallback일 뿐이므로 ranked policy는 검수된 독립
정산일을 명시하지 않으면 게시할 수 없다. 과세
근로소득·보험료·원천세·추가세·환급이 모두 0이고 income event도 없는 연도는 reconciliation을 만들지
않으며 API의 `reconciliationGameDay`는 null이다. combined
assessment는 근로소득 과세표준·공제와 2월 정산 후 최종 employment income/local tax 선납액을 명시적으로
보존하고 M2 비교과세 입력으로 넘긴다. 기존 `otherComprehensiveIncome` 한 숫자로 이를 대신하지 않는다.

이 순서로 2월 환급 뒤 5월에 같은 원천세를 다시 공제하거나, 연금 공제를 두 번 쓰는 것을 막는다.
금융소득 신고 계산은 2월 정산 후 최종적으로 부담한 employment income/local tax를 원천납부액으로 보고
금융 원천세와 합산한다. 둘 다 1월 1일 assessment에서 이미 결정되므로 2월 현금 settlement 전에도 5월
금액을 결정론적으로 고정할 수 있다.

2월 employment reconciliation은 같은 날 기존 payroll보다 뒤에 생성된 settlement ID를 가지므로 정상
payroll이 먼저 지갑에 들어온다. 환급은 지갑에 더하고, 추가 원천세는 그 지갑에서 차감한 뒤 부족분을
M2와 같은 무이자 aggregate tax debt로 기록한다. 잔액 부족으로 assessment나 하루를 실패시키지 않으며
환급·추가액 0원은 `noMovement: zeroTaxDue`로 끝낸다.

M3 v1 연말정산이 인정하는 입력은 급여, 근로자 4대보험 중 법정 공제분, 기본 인적공제, M2 연금저축·IRP
납입뿐이다. 신용카드·의료비·교육비·주택·기부금·월세·중소기업 감면은 데이터가 없다는 이유로 0을
추정하지 않고 `notSupported`로 표시하며 M4 이후로 미룬다.

### 9.2 연금 세원층 재분류

M2 납입 시점의 `expectedCreditKrw`는 안내 값일 뿐 현금이나 확정 세액공제가 아니다. 연말정산 coordinator는
귀속연도의 실제 소득구간, 연금저축 단독 한도, 연금저축+IRP 합산 한도, 다른 비환급성 세액공제 후 남은
산출세액을 적용해 `actualPensionIncomeTaxCreditKrw · actualPensionLocalIncomeTaxEffectKrw`를 각각 확정하고
그 합만 `actualPensionCreditKrw`로 표시한다. 지방소득세 효과를 합산 rate 한 번으로 계산하지 않고 policy의
소득세 공제와 지방소득세 연동 산식·rounding을 순서대로 적용한다.

공제 대상 pool은 먼저 연금저축 납입을 `contributionGameDay, ledgerTransactionId, accountId` 오름차순으로
단독 한도까지 채운 뒤, 같은 순서의 IRP 납입으로 합산 한도의 나머지를 채운다. 실제 세액공제액이 산출세액에
막힌 경우, 해당 rate로 실제 credit을 만들 수 있는 최대 원 단위 납입액을 정수 이분 탐색해
`taxExcludedContribution`에서 `creditedContribution`으로 옮긴다. 판정 함수는 policy의 공식 rounding을
연간 합산액에 한 번 적용하며, 전체 이동액이 만드는 credit이 0원이면 이동하지 않는다. source 순서대로
누적 절사 차이를 귀속하므로 앞선 소액 source의 귀속 credit이 0원이어도 뒤 source까지 합친 전체 이동액의
credit이 양수일 수 있다. 이 경우 양수 이동 source는 모두 allocation과 세원층 원장에 남기고 assessment별
source credit 합은 연간 실제 credit과 같아야 한다. 계좌별 이동 합은 실제 eligible 납입을 넘지 않고 네
세원층 총합은 재분류 전후 같아야 한다.

같은 귀속연도 안에 연금 인출이 있었다면 납입 총액만으로 공제를 만들지 않는다. append-only
`pensionContribution`과 `pensionWithdrawal` event를 게임일·ledger ID 순서로 replay하고, M2 인출 순서대로
소진된 `taxExcludedContribution`을 가장 오래된 미확정 납입 source부터 차감한다. 12월 31일 종료 시점에
해당 source에 남아 있는 금액만 공제 대상 pool에 들어간다. 이미 인출된 원금을 뒤늦게
`creditedContribution`으로 재분류할 수 없고, 닫힌 계좌라도 남은 세원층이 있으면 같은 규칙으로 판정한다.

재분류는 돈을 움직이지 않지만 M2가 예약한 감사 경계를 지킨다. 양수 이동 source별로 해당 계좌의
`pensionTaxExcludedContribution`과 `pensionCreditedContribution` 사이에 같은 금액의 반대 posting을 가진
한 balanced ledger transaction을 만든다. 따라서 한 계좌의 여러 납입 source는 각 source identity를 가진
여러 balanced transaction으로 감사할 수 있다. append-only `pension_credit_allocation`과
`tax_account_value_event(cause: pensionCreditFinalized)`를 assessment와 같은 transaction에 기록한다.
재시도는 `(save_id, run_revision, tax_year, contribution_source_id)` unique key로 같은 allocation과 원장에
수렴한다. M3 v1은 수정신고를 지원하지 않아 definitive assessment와 allocation을 다시 열지 않는다.

M3-C migration은 `tax_account_value_event`의 cause CHECK와 insert trigger를 함께 확장한다. 이 자동 event는
command identity를 요구하지 않고 assessment/source allocation identity를 사용하며, 연말 전에 계좌가 닫혔어도
남아 있는 세원층의 최종 재분류 event를 허용한다. 기존 daily valuation/trade event의 active-account 제약과
delta 검증은 그대로 유지하고 `pensionCreditFinalized` branch에서만 총액 불변·두 contribution layer 간 이동을
검증한다.

## 10. 병역

### 10.0 M3-D 활성화와 기존 런 bridge

M3-D migration은 `career_run.militaryStatus`를 durable 권위로 추가한다. 새 런과 기존 런 모두
`character.military`를 한 번만 읽어 `notServed → unserved`, `completed → completed`,
`exempted → exempt`로 옮긴다. 이미 복무 중인 legacy 값은 경과일을 복원할 근거가 없으므로
`serving → activeDuty`, `alternative → socialService`의 pending service를 다음 게임일에 시작하고 전체
policy 기간을 미래에 다시 센다. 과거 급여·경력·적금 회차는 소급하지 않는다. 이 bridge service는 다른
복무 시작과 동일한 durable row/action을 사용하며, 시작 전에도 외부 status는 `serving`으로 유지해 채용
우회를 막는다.

0017~0022가 이미 게시한 `dev-unranked-m3-v1` bundle과
`dev-unranked-m3-employment-2026-v1` policy에는 D runtime이 요구하는 일부 typed child가 없다. 0023은
두 key가 비랭크이고 기존 child checksum이 예상값과 정확히 같을 때만 한 transaction에서 D 전용 child를
보강한 뒤 새 update/delete·publish-completeness trigger를 설치한다. 이 staged migration 예외는 해당 두
개발 key에만 허용하며 ranked/published production graph에는 절대 row를 덧붙이지 않는다.

0018의 개발 bundle에 남은 `military_option_version.minimumEducation`과 단일 자격증 FK는 M3-A 당시의
호환 projection이며 M3-D 자격 판정 권위가 아니다. 0023의 typed eligibility child가 모든 option에 대해
`minimumEducation · requiredCertificationCount · minimumExperienceDays`를 완전하게 명시하고 runtime은
그 child만 읽는다. 기존 published option row는 바꾸지 않는다. 특히 `industrial-technical-v1`은 typed
child에서 `minimumEducation = null · requiredCertificationCount = 1`로 고정하며, 기존 `associate` 값으로
학력을 추가 요구하지 않는다. 새 bundle은 typed child 없이는 게시할 수 없다.

### 10.1 상태와 형태

캐릭터의 외부 상태는 `unserved · serving · completed · exempt`다. `specialService`는 별도 외부 상태가
아니라 serving의 service type으로 정규화한다. M3 service type은 다음과 같다.

| enum | 표시명 | M3 효과 |
|------|--------|---------|
| `activeDuty` | 현역 | 계급 단계별 병 급여, 낮은 활동 effort, 민간 경력 없음 |
| `socialService` | 사회복무요원 | policy 보수, 출퇴근형 effort, 민간 경력 없음 |
| `industrialTechnical` | 산업기능요원 | 자격 조건, 지정 직무 급여, 해당 직무 경력 일수 인정 |
| `professionalResearch` | 전문연구요원 | 학력 조건, 연구 직무 급여, 해당 직무 경력 일수 인정 |
| `commissionedOfficer` | 장교 | 선발·학력 조건, 간부 급여, `publicSocial`/방산 직무 경력 인정 |
| `nonCommissionedOfficer` | 부사관 | 선발 조건, 간부 급여, `publicSocial`/기술 직무 경력 인정 |

면제는 캐릭터 생성 결과이므로 M3에서 새로 선택하는 명령이 없다. 복무 시작은 `unserved`에서만 가능하고,
나이·성별로 확률을 만들지 않는다. 캐릭터에 이미 확정된 대상 여부와 service option의 학력·자격증·경력
hard requirement만 검사한다. 성별은 시작 설정에서 확정된 법정 대상 여부 외에는 급여·경력·채용 확률에
쓰지 않는다.

복무 시작 명령은 현재 cursor에서 자격을 확정하고 `startGameDay = currentGameDay + 1`인 phase 10 action을
만든다. 별도 선발 확률은 만들지 않으며 장교·부사관의 “선발”은 M3 v1에서 option의 typed 최소 학력·
자격증 수·경력일 hard requirement를 모두 충족한 것으로 표현한다. 산업기능요원은 최소 자격증 수 1을,
전문연구요원은 석사 이상을 요구하고, 정확한 자격 종목을 추정하지 않는다.

`military_service`는 `[startGameDay, endGameDay)`와 immutable option/policy key를 가진다. end day 시작에
`completed`로 전이하므로 마지막 복무·급여·경력 일은 `endGameDay - 1`이다. 조기전역·복무중단·예비군·
민방위 세부 이벤트는 M3에서 구현하지 않는다.

### 10.2 급여·경력·활동 제한

복무 option은 보수 분류, 지급 주기, 단계별 급여, effort capacity, 인정 업종·직무와 하루 경력 비율을
typed catalog로 가진다. 실제 기간·급여표는 `employmentPolicySet` 하위 버전이고 코드에 숫자로 넣지 않는다.

기간의 월 계산은 `startDate`에 policy의 `serviceDurationMonths`를 달력 월로 더하고 없는 일자는 말일로
당긴 exclusive end date를 사용한다. pay stage의 `serviceMonth`는 start date부터 완전히 지난 달력 월 수다.
첫 `dev-unranked-m3-employment-2026-v1`의 기능 fixture는 다음처럼 고정한다.

| service type | 기간 | 월 급여 stage / 개발 가정 |
|---|---:|---|
| `activeDuty` | 18개월 | 복무월 `[0,2) 750,000 · [2,8) 900,000 · [8,14) 1,200,000 · [14,18) 1,500,000`원 |
| `socialService` | 21개월 | 복무월 `[0,3) 750,000 · [3,9) 900,000 · [9,15) 1,200,000 · [15,21) 1,500,000`원 |
| `industrialTechnical` | 34개월 | 현역 대상·자격증 1개·지정 가상고용주 경로. 2026 최저임금 월 환산 2,156,880원을 개발 계약 하한으로 사용 |
| `professionalResearch` | 36개월 | 석사 이상·지정 가상연구기관 경로. 2,156,880원을 개발 계약 하한으로 사용 |
| `commissionedOfficer` | 36개월 | 일반 단기복무·소위 1호봉 기본봉 2,150,400원만 쓰는 `basePayOnly` 개발 경로 |
| `nonCommissionedOfficer` | 48개월 | 일반 단기복무·하사 1호봉 기본봉 2,133,000원만 쓰는 `basePayOnly` 개발 경로 |

현역 stage는 일반 진급 최저복무기간을 고정 적용하는 `ordinaryMinimumPromotion` 게임 가정이다. 사회복무의
식비·교통비는 위치·실제 출근일 데이터가 없으므로 gross 0으로 섞지 않고
`reimbursementNotModeled`로 표시한다. 산업·전문요원의 실제 계약 급여와 장교·부사관의 초임호봉·수당은
경로별로 달라 위 숫자를 production/ranked policy로 게시하지 않는다. ranked option은 지정 고용/임용 경로와
보수표가 모두 typed row로 검수되기 전 `policyUnavailable`이다.

`dev-unranked-m3-employment-2026-v1`의 모든 option은 `paydayDayOfMonth = 10`인 monthly schedule을 쓴다.
복무 시작일 당일 또는 그 뒤의 첫 보정 지급일부터 `payday < endExclusiveDate`인 회차만 예약하며 없는
일자는 말일로 당긴다. M3 v1 개발 fixture는 부분월 일할 없이 지급일에 해당하는 pay stage의 월 gross
전액을 지급한다. 이 지급일과 무일할 규칙은 결정론적 기능 가정이므로 ranked policy는 실제 지급주기와
부분월 산식의 검수된 typed metadata가 없으면 게시할 수 없다.

- 현역·사회복무 급여는 `militaryPayIncome`으로 기록하고 policy가 정한 과세·사회보험 분류를 적용한다.
- 산업기능·전문연구·장교·부사관 급여는 option의 compensation rule에 따라 일반 payroll 계산기를
  재사용하되 `military_service_id`를 source로 고정한다.
- 경력 인정형은 매 복무일에 `experience` evidence progress를 누적하고, 복무 완료 때 기간 evidence를
  확정한다. period는 실제 service `[startDate,endExclusiveDate)`이고 `creditedExperienceDays`는 직무별
  누적 ppm의 정수 몫이다. dev-unranked bundle은 option·job family별 stackable military experience entry를
  typed mapping으로 두고, 기존 experience 기여 단위 300bp에 `experienceCreditPpm`을 곱해 내림한 양수
  contribution만 해당 직무군에 부여한다. evidence key는
  `militaryService:{serviceId}:{careerJobFamilyId}`이며 acquired day는 `endGameDay`다. 중간 조회는 progress와
  완료 예정 효과를 구분한다.
- 현역 중 자산 계좌 주문은 계속 허용한다. 활동 시작·진행 여부만 `allowedLifeStatuses`와 effort로 제한한다.

M3-D는 append-only `employment_income_event`를 급여 종류와 무관한 연간 누계 권위로 둔다. source는
`employmentPayroll · militaryPay`이고 source ID·occurrence, 지급 게임일/`taxYear`, gross, 근로자 보험료
각 항목과 원천세를 보존한다. 0023은 기존 `payroll_record`를 event로 한 번 backfill하고 이후 민간·군 급여
transaction은 payroll 사실과 income event를 함께 쓴다. `employment_income_year`는
`incomeEventCount · lastIncomeEventId`와 event 합으로 검증하며 payroll ID를 일반화된 권위로 사용하지
않는다. 이 구조가 군 급여만 있는 연도와 민간·군 급여가 섞인 연도를 같은 coordinator로 닫는다.

## 11. 장병내일준비적금

M3는 M2의 가상 기관 `life-bank-a · life-bank-b`에 장병내일준비적금 상품 버전을 한 개씩 둔다. 실제
은행명을 모사하지 않는다. 가입 대상, 최소 잔여복무기간, 기관별·개인 합산 월 한도, 계좌 수, 납입 단위,
비과세, 정부 매칭률, 만기·중도해지 자격은 공식 근거를 가진 policy data다. 은행 금리·우대 조건은 불변
product catalog다.

2026 policy는 `activeDuty · socialService`만 가입 가능하고 최소 잔여복무 1개월, 한 기관당 1계좌·전체
2계좌, 기관별 월 300,000원·개인 합산 월 550,000원, 한도 설정 50,000원 단위로 고정한다. 개발 상품의
실제 회차 납입은 최소 1,000원·1원 단위이며 가입 뒤 월 한도는 바꾸지 않는다. 가입일이
2026-12-31을 넘으면 새 policy version 없이는 `policyUnavailable`이다. 2024년 이후 납입원금의 정부
매칭률은 1,000,000ppm이고 이자에는 적용하지 않는다.

가상 두 은행의 최초 unranked product는 우대조건 없이 계약기간 1~11개월 4.00%, 12~14개월 4.50%,
15~24개월 5.00%의 가입일 고정 기본금리만 쓴다. 공개 약관에서 가상 상품 공통 day-count와 단수 규칙을
확정할 수 없으므로 `actual/365 · 원 미만 버림`은 dev-unranked 계산 가정으로 명시하고 ranked 상품에는
쓰지 않는다. 일반 중도해지 상품은 원금만 돌려주는 0% 개발 중도해지율을 명시하며 비과세와 정부지원을
주지 않는다.

`military_savings_contract`는 institution별 최대 하나이며 service와 product version을 pin한다. 납입일에는
지갑이 충분하면 원금을 잠긴 상품원금으로 옮기고 `paid`, 부족하면 `missed/noMovement`로 확정하며 같은
회차를 재시도하지 않는다. 모든 회차는 가입 때 service end까지 예약하고 `(contract_id, installment_no)`로
중복을 막는다.

가입 명령은 serving 상태에서만 받고 현재 날짜보다 엄격히 뒤인 첫 `debitDayOfMonth`를 1회차로 삼는다.
해당 일이 없는 달은 말일로 당기며, 이번 달 보정일이 이미 지났거나 명령 당일이면 다음 달로 넘긴다.
`dueGameDay < service.endGameDay`인 회차만 예약한다. 최소 잔여복무는 명령일에 policy의
`minimumRemainingServiceMonths`를 달력 월로 더하고 말일 보정한 날짜가 service end 이하여야 충족한다.
고정 30일로 환산하지 않는다. 따라서 명령과 같은 날 이미 끝난 financial phase에 납입을 끼워 넣지 않는다.
최초 dev-unranked 상품의 `contractTermMonths`는 가입 때 실제로 예약된 monthly installment 수이며, 해당
구간의 금리를 가입 시 pin한다. 단순 경과일 나눗셈으로 기간을 다시 계산하지 않는다. 이는 전역일 고정
상품을 위한 개발 fixture 규칙이므로 ranked 상품은 검수된 상품 약관의 계약기간 산식을 별도 typed
metadata로 제공해야 한다.

각 paid installment는 납입 게임일과 그 날의 matching policy version을 보존한다. 만기에는 installment별
보유일수로 은행 gross 이자를 정수 계산해 원금·은행 이자만 먼저 지갑에 넣고, 각 납입분의 정부 지원은
별도 `militarySavingsGovernmentMatch` settlement로 지급한다. 지원 지급일은 policy의
`nextMonthDayOfMonth`를 **계약 만기일의 다음 달**에 말일 보정하며 최초 unranked fixture는 그 달 25일이다.
settlement와 source identity는 paid installment별로 나누지만 같은 계약의 지원금은 모두 이 하나의 지급일을
쓴다. 납입일의 다음 달로 소급 예약하지 않는다. 이자 day-count,
rounding, 비과세와 지원 지급 시점은 product/policy 값이다. 중도해지는 원금과 policy가 허용한 이자만
지급하고 비과세·정부지원을 임의로 유지하지 않는다.

모든 회차가 `missed`라서 만기 원금·은행 이자·정부 지원이 모두 0원이면 계약과 만기 settlement는
정상적으로 `matured`·`settled/noMovement`로 확정하지만 ledger transaction은 만들지 않는다. 원장은 0원
posting을 허용하지 않으므로 이 경우 `maturityLedgerTransactionId`는 null이고, 실제 지급액이 양수일 때만
만기 ledger와 그 참조를 요구한다.

중도해지는 command transaction에서 즉시 지급하며 예약 settlement를 만들지 않는다. 원장 source kind는
`militarySavingsEarlyClose`, source ID는 contract ID, occurrence는 1로 고정해
`militarySavingsMaturity`와 구분한다.

만기와 복무 완료가 같은 날이면 먼저 시작-of-day 복무 완료 자격을 확정한 뒤 financial settlement에서
만기를 실행한다. 계약 원금, 은행 이자, 정부 지원금은 별도 ledger account와 API 필드로 보여 주며 서로
합쳐 `interestKrw`로 부르지 않는다.

## 12. 일일 planner, 잠금과 멱등성

### 12.1 하루의 고정 순서

시장 캐시·공고 materialization은 player transaction 전에 멱등하게 준비할 수 있다. 플레이어 하루는
M2 planner를 다음처럼 확장한 하나의 MySQL transaction이다.

1. save의 네 부분 cursor, M2 policy/product bundle, M3 career/employment bundle을 잠근다.
2. due `career_scheduled_action`과 `scheduled_settlement` payload를 모두 strict tagged union으로 해석하고
   잠금 ID를 수집한다. 알 수 없는 kind·version·필드가 하나라도 있으면 쓰기 전에 실패한다.
3. §12.2 순서로 현재 런 행을 잠그고 후보 집합과 payload를 다시 비교한다.
4. 1월 1일이면 M2 연금 opening value와 M3 직전 근로소득을 당일 변화 전에 pin하고 §9 coordinator로
   employment-only/combined assessment와 연금 allocation을 확정한다.
5. `[start, end)` 기준의 입사·복무 시작/완료 action을 적용한다.
6. 그 날 재직·복무 경력 1일과 활동 effort를 누적하고 완료 evidence를 만든다.
7. 서류·확인마감·면접·오퍼만료·역제안 action을 `(phase_rank, due_game_day, id)` 순으로 판정한다.
8. M2-D 순서대로 시장 평가, 연금 시가손익, LLX 권리와 금융 due plan을 만든다.
9. 급여·세금조정·군 급여·장병적금 납입/만기를 기존 M2 kind와 함께 전역
   `(due_game_day, settlement_id)` 순서의 shadow plan으로 계산한다. 앞 정산의 가상 지갑·계좌·세금 효과를
   뒤 정산에 전달한다.
10. 상태·누계·원장·후속 예약·조건부 전이를 기록하고 cursor를 한 번 올려 commit한 뒤 완성 snapshot
    하나만 SSE로 보낸다.

financial settlement의 동일 날짜 순서는 기존 M2처럼 ID가 권위다. 가입·계약 명령에서 생성하는 최초
예약과 각 후속 예약의 삽입 순서를 고정하고 integration test로 보호한다. 플레이어가 서로 다른 날 만든
의무의 ID 순서 차이는 실제 선택 시점의 결과이며 replay가 이를 바꾸지 않는다.

`career_scheduled_action.phase_rank`는 `employmentOrServiceLifecycle: 10 · documentReview: 20 ·
confirmationExpiry: 30 · interviewDecision: 40 · offerExpiry: 50 · invitationGeneration: 60`으로 고정한다.
알 수 없는 rank/kind 조합은 parser 오류이고, bundle이 이 순서를 바꾸지 못한다.

action payload는 모두 `version: 1`과 잠글 ID만 가진다. lifecycle은 `employmentContractId` 또는
`militaryServiceId`, 서류·확인·면접·오퍼 action은 `applicationId`, 역제안은 `platformKey · gameDay`를
가진 exact tagged union이다. financial settlement도 급여/군 급여는 `contractOrServiceId · payrollPeriod`,
세금조정은 `taxYear · assessmentId`, 장병적금은 `contractId · installmentNo?`만 저장한다. 급여액·세율·
점수·확률·지원금은 payload에 복사하지 않고 잠근 계약과 pinned policy/catalog에서 읽는다.

### 12.2 전역 SQL 잠금 순서

M2 §11.1의 순서 앞뒤에 M3 행을 다음처럼 추가한다.

`save → spec_activity(id) → spec_evidence(id) → profile_artifact_version(id) →
job_application(id) → job_invitation(id) → job_offer(id) → employment_contract(id) →
military_service(id) → military_savings_contract(id) → military_savings_installment(contract_id, no) →
financial_account(id) → M2 account/position/product rows in §11.1 order →
career_scheduled_action(phase_rank, due_game_day, id) → scheduled_settlement(due_game_day, id) →
employment_income_event(id) → employment_income_year(tax_year) → year_end_tax_assessment(tax_year) →
pension_credit_allocation(tax_year, source_id) → M2 financial-income tax rows`

없는 단계는 건너뛰고 각 단계는 표시한 key 오름차순으로 `FOR UPDATE`한다. job posting과 게시된 bundle,
policy, product는 불변이므로 잠그지 않는다. 모든 write는 save lock 뒤 command identity를 먼저 검사한다.

### 12.3 command identity

모든 M3 상태 변경은 M2와 같은 canonical UUID `commandId`와
`expectedRunRevision · expectedStateRevision · expectedGameDay`를 받는다. canonical payload hash와 명령
종류가 같으면 최초 결과를 replay하고, 다르면 `idempotencyConflict`다. 성공 상태·원장·예약·receipt는
같은 transaction에서 commit한다. application/contract/계약 ID를 command ID로 대신하지 않는다.

planner가 만드는 자동 판정은 `(save_id, run_revision, source_kind, source_id, occurrence)` unique key와
조건부 상태 전이로 한 번만 적용한다. 확률 roll은 DB ID가 아니라 §6의 stable key를 쓰므로 unique race와
재시도가 결과를 바꾸지 않는다.

## 13. DB와 strict API 계약

### 13.1 스키마

M3 migration은 최소 다음 표를 역할별로 둔다.

- 불변 데이터: `career_catalog_bundle · spec_catalog_entry · activity_catalog_entry · platform_catalog ·
  job_template · artifact_checklist_rule · recruitment_ruleset · recruitment_stage_component_weight ·
  recruitment_score_band · recruitment_pass_probability · employment_policy_set · employment_policy_source ·
  national_pension_policy · health_insurance_policy · long_term_care_policy · employment_insurance_policy ·
  industrial_accident_policy · employment_withholding_table_version · employment_withholding_table_row ·
  local_income_withholding_policy · employment_annual_tax_policy · other_income_reward_policy ·
  military_option_version · military_option_experience_evidence_mapping · military_savings_product_version`
- 버전 선택: `career_recruitment_compatibility · recruitment_ruleset_assignment ·
  employment_policy_assignment · employment_finance_compatibility`
- 런 기준: `career_run`
- 스펙·문서: `spec_activity · spec_evidence · profile_artifact_version · profile_artifact_evidence`
- 채용: `job_posting · job_application · job_invitation · job_offer`
- 고용·세금: `employment_contract · payroll_record · career_reward_payment · employment_income_event ·
  employment_income_year · year_end_tax_assessment · pension_credit_allocation`
- 병역: `military_service · military_service_progress · military_savings_contract ·
  military_savings_installment`
- 예약: `career_scheduled_action`, 기존 `scheduled_settlement`의 새 strict kind

M3-D의 `career_scheduled_action`은 군 lifecycle branch에서 recruitment ruleset FK를 요구하지 않는다.
employment/recruitment action은 기존 non-null ruleset을 유지하고 military action만 option/policy와
`militaryServiceId`로 소유권을 검증한다. 새 settlement kind는 `militaryPay ·
militarySavingsInstallment · militarySavingsMaturity · militarySavingsGovernmentMatch`이며 모두 exact
version 1 payload를 쓴다.

현재 런의 가변·append-only 행은 모두 `(save_id, run_revision, id)` 복합 FK로 소유권을 강제한다. 돈은
`BIGINT` 원, rate는 `INTEGER ppm/bp`, 날짜는 게임 epoch에 매핑된 `DATE`, cursor는 game day 정수다.
MySQL `ENUM`이나 느슨한 parameters JSON 대신 `VARCHAR`+Rust enum과 kind별 typed 열/CHECK를 사용한다.
strict action/settlement payload JSON만 `version: 1` tagged object로 허용한다.

employment policy set은 draft에서만 typed 자식 row를 받고 publish 뒤 update/delete하지 않는다. publish는
필수 effective range가 빈틈·중첩 없이 닫히는지, 간이세액표 family/pay band가 완전한지, 모든 rate와 money가
정수 안전범위인지, provenance URL·확인일·원문 checksum이 있는지와 요구 M2 finance policy compatibility를
검증한다. 새 settlement kind는 `employmentPayroll · employmentReconciliation`이며 원티드 보상은 최초
`employmentPayroll`과 같은 날 별도 source identity로 실행한다.

`payroll_record`는 `(save_id, run_revision, employment_contract_id, period_no)`를 unique로 두고
`period_no`는 계약의 `payrollBaselinePeriodNo`부터 빈틈 없이 증가한다. `financial_income_assessment`에는 combined 계산이 사용한 employment
taxable income, deductions, final prepaid income tax와 final prepaid local income tax를 추가해 1월에 pin하고
게시된 assessment에서는 바꾸지 않는다.

M3 최종 `career_run`은 pinned 두 key, bundle의 기본값으로 시작하는 `focused_job_family_key`, §2.3의
`birth_date`를 보존한다. M3-A의 단계적 예외와 M3-C pin 순서는 §2.2를 따른다. `spec_evidence`의 nullable
period 두 열은 둘 다 null이거나 둘 다 값이어야 하며,
일반 period는 start < end, 0년 bridge experience만 start = end다. bundle 게시 transaction은 A/B/D의 모든
필수 row, 기본 focus FK, bridge mapping·order, checklist rule의 kind별 weight 합 10,000을 검증한 뒤에만
published 상태로 바꾼다.

### 13.2 API

조회는 다음 경로를 제공한다.

- `GET /api/career/specs?before=&limit=`
- `GET /api/career/activities?before=&limit=`
- `GET /api/career/artifacts?kind=&before=&limit=`
- `GET /api/career/jobs?platform=&industry=&before=&limit=`
- `GET /api/career/applications?before=&limit=`
- `GET /api/career/employment`
- `GET /api/career/payroll?before=&limit=`
- `GET /api/career/tax-years/{year}`
- `GET /api/military/options`
- `GET /api/military/service`
- `GET /api/military/savings-products`
- `GET /api/military/savings?before=&limit=`

명령은 다음 경로를 제공한다.

- `POST /api/career/focus`
- `POST /api/career/activities` / `POST /api/career/activities/{id}/cancel`
- `POST /api/career/artifacts`
- `POST /api/career/applications`
- `POST /api/career/applications/{id}/interview-confirmation`
- `POST /api/career/applications/{id}/withdraw`
- `POST /api/career/invitations/{id}/accept` / `POST /api/career/invitations/{id}/decline`
- `POST /api/career/offers/{id}/accept` / `POST /api/career/offers/{id}/decline`
- `POST /api/military/service`
- `POST /api/military/savings`
- `POST /api/military/savings/{id}/close`

모든 command body는 다음 공통 exact object를 확장한다.

`{ commandId, expectedRunRevision, expectedStateRevision, expectedGameDay, ...domainFields }`

- focus 변경은 pinned bundle에 속한 `focusedJobFamilyKey`만 받는다. 다른 bundle의 key는 거절한다.
- 활동 시작은 `activityCatalogEntryId · priority`, 취소는 추가 필드 없음
- artifact는 kind별 tagged union이다. 공통 `kind · headline · summary · evidenceIds`에
  `linkedinProfile`만 `openToWork · industries`를 추가하고 다른 kind에서는 이 두 필드를 금지한다.
- 지원은 `postingKey · resumeVersionId? · portfolioVersionId? · linkedinProfileVersionId?`이고 플랫폼이
  요구하지 않는 version 필드는 금지한다.
- 면접 확인은 `decision: confirm|decline`, 지원 철회·초대 거절·오퍼 수락/거절은 추가 필드 없음
- 초대 수락은 `invitationId`를 path에서만 받고 초대에 pinned된 공개 artifact를 그대로 사용한다.
- 복무 시작은 `militaryOptionVersionId`, 장병적금 가입은
  `productVersionId · monthlyContributionKrw · debitDayOfMonth`, 중도해지는 추가 필드 없음

성공 응답은 `{ result, replayed, snapshot }` exact object이고 result는 명령별 resource ID와 상태만 반환한다.
focus 성공 result는 선택된 `focusedJobFamilyKey`를 돌려준다. 공통 cursor를 모두 검사한 뒤에만
`career_run`을 갱신하므로 stale focus 명령도 다른 상태 변경과 같이 `settlementConflict`로 실패하거나 같은
command를 replay한다.

specs·activities와 위의 모든 history/list 조회는 `before · limit`를 함께 지원한다. limit 기본은 50,
허용 범위는 `1..=200`이고 page는 `{ items, nextBefore }`다. `/specs`는 이 page와 별도로
`focusedJobFamilyKey · possessedScores`를, `/activities`는 history page와 별도로 `active · catalog`를
반환한다. active는 최대 3개, catalog는 최대 200개이며 stable key 순서로 전부 반환한다. page limit은 이
두 bounded 배열에 적용하지 않는다. 공고 응답은 공고 직무군 기준 여섯 `possessedScores`를, 지원 응답은
같은 직무군 기준의 pinned `visibleScores`와 해당 판정에 쓰는 `possessedScores`를 포함한다. 지원 history의
offer는 terminal 뒤에도 불변 조건과 결과를 보존하며 전용 상태 `offered · accepted · declined · expired ·
closed`를 쓴다. DB의 `pending`만 API의 `offered`로 변환한다.

`/career/payroll` item은 exact object
`{ id, contractId, periodNo, salaryMonthOrdinal, periodStartDate, periodEndExclusiveDate, paidGameDay,
grossPayKrw, employeeNationalPensionKrw, employerNationalPensionKrw,
employeeHealthInsuranceKrw, employerHealthInsuranceKrw, employeeLongTermCareKrw,
employerLongTermCareKrw, employeeEmploymentInsuranceKrw, employerEmploymentInsuranceKrw,
employerIndustrialAccidentKrw, withheldIncomeTaxKrw, withheldLocalIncomeTaxKrw, netPayKrw,
reward? }`다. `reward`는
`{ paymentId, grossRewardKrw, withheldIncomeTaxKrw, withheldLocalIncomeTaxKrw, netRewardKrw }`이며
원티드 첫 지급에만 존재한다. 근로자 부담과 사용자 부담을 합친 모호한 `insuranceTotal`만 보내지 않는다.

`/career/tax-years/{year}`는 exact object
`{ taxYear, status, source, grossEmploymentIncomeKrw, employeeInsuranceDeductionKrw,
earnedIncomeDeductionKrw, personalDeductionKrw, taxableIncomeKrw, calculatedIncomeTaxKrw,
earnedIncomeTaxCreditKrw, pensionCreditEligibleContributionKrw,
actualPensionIncomeTaxCreditKrw, actualPensionLocalIncomeTaxEffectKrw,
withheldIncomeTaxKrw, withheldLocalIncomeTaxKrw, assessedIncomeTaxKrw,
assessedLocalIncomeTaxKrw, additionalTaxKrw, refundKrw, reconciliationGameDay? }`다.
`source`는 `employmentOnly · combined · legacyProfile`이고 `status`는 `open · provisional · definitive`다.
아직 닫히지 않은 연도는 확정 계산 필드를 null로 보내며 누락하지 않는다. `legacyProfile` definitive는 기존
profile이 실제로 보존한 총급여·과세표준만 숫자로 보내고, 원천세·세부 공제·산출세액·연금 공제·정산처럼
원자료가 없는 필드는 0으로 추정하지 않고 null로 보낸다. M3 `employmentOnly · combined` definitive는 모든
확정 계산 필드가 숫자여야 한다.

리소스 ID와 일반 history cursor는 M2처럼 canonical decimal string이다. 예외적으로 공고 목록의 `before`는
`postingKey` 자체를 쓰며 lowercase 64자리 SHA-256 hex 문자열이다. 공고의 별도 DB ID를 API 식별자나
cursor로 노출하지 않는다. KRW는 기존 client
계약과 같이 JSON safe integer인 원 단위 정수로 주고받고, catalog/명령 상한에서 이를 강제한다. 내부 합산은 i128, DB는
BIGINT이며 API 안전 범위를 넘는 결과는 조용히 반올림하지 않고 `limitExceeded`로 거절한다.

알 수 없는 필드, 잘못된 enum, 중복 evidence, 배열 상한 초과, 서로 모순되는 tagged union은 client/server
양쪽 boundary에서 거절한다.

공통 M3 실패 code는 `invalidCommand · characterRequired · policyUnavailable · catalogUnavailable ·
notEligible · activityLimit · artifactRequired · postingClosed · applicationLimit · alreadyApplied ·
interviewExpired · offerExpired · alreadyEmployed · militaryStateConflict · insufficientWalletCash ·
limitExceeded · idempotencyConflict · settlementConflict · busy`로 고정한다. 한국어 `message`는 표시용이고
분기 계약이 아니다. 내부 확률 roll, policy JSON, SQL, 다른 사용자의 ID를 오류에 노출하지 않는다.

### 13.3 bounded snapshot

`GameSnapshot.career`는 다음 요약만 가진다.

캐릭터를 아직 만들지 않은 save는 durable `career_run`을 만들지 않는다. 이때 snapshot은 현재 `newRun`
assignment가 가리키는 published bundle의 기본 직무군 key와 0점·빈 배열을 합성한다. 따라서 클라이언트는
별도 sentinel key나 nullable 계약 없이 같은 exact shape를 유지하고, 실제 런의 pin으로 오해할 durable
상태도 생기지 않는다.

- `focusedJobFamilyKey`와 그 직무군으로 계산한 여섯 `possessedScore`, active activity 최대 3개,
  최신 artifact version 3개
- open application 최대 10개, open invitation 최대 5개와 confirmation/면접/오퍼의 다음 날짜
- nullable active employment와 가장 최근 payroll 하나
- current employment tax year 요약과 가장 최근 finalized assessment 하나
- military status, nullable active service, active military savings contract 최대 2개
- M3 관련 다음 scheduled action/settlement를 합쳐 최대 20개

군 요약은 `militaryStatus`와 nullable `activeMilitaryService`를 가진다. active service는 exact
`{ id, optionVersionId, serviceType, displayName, status, startGameDay, endGameDay,
creditedServiceDays, totalServiceDays, effortLifeStatus, grantsCareerExperience, nextPayGameDay }`이며
`status`는 `pendingStart · serving`, `nextPayGameDay`는 nullable이다. active savings item은 exact
`{ id, productVersionId, institutionKey, status, monthlyContributionKrw, debitDayOfMonth,
principalKrw, paidInstallmentCount, missedInstallmentCount, nextInstallmentGameDay, maturityGameDay }`이고
다음 회차가 없으면 `nextInstallmentGameDay`만 null이다. 종료 service와 납입별 paid/missed, 만기·중도해지
원금/이자/정부지원은 `/api/military/service`와 `/api/military/savings` history에서 조회한다.
M3-D 이전 `character.military=completed` bridge는 존재하지 않았던 복무 기간·option을 소급 합성하지 않으므로
`/api/military/service`가 `{ militaryStatus: completed, service: null }`을 반환할 수 있다. M3-D command로
완료한 경우에는 completed service history가 반드시 함께 온다. exempt도 service가 null이다.

다음 M3 일정은 `pendingCareerSchedule` exact 배열로 합친다. item은
`{ sourceKind: careerAction, id, dueGameDay, kind }` 또는
`{ sourceKind: settlement, id, dueGameDay, kind }`의 tagged union이다. action kind는
`employmentStart · militaryServiceStart · militaryServiceCompletion · documentReview ·
confirmationExpiry · interviewDecision · offerExpiry · invitationGeneration`, settlement kind는
`employmentPayroll · employmentReconciliation · militaryPay · militarySavingsInstallment ·
militarySavingsMaturity · militarySavingsGovernmentMatch`만 허용한다. 배열은 `dueGameDay` 오름차순,
같은 날은 action을 settlement보다 먼저 두고 action은 `phaseRank, id`, settlement는 `id` 오름차순으로
정렬해 가장 가까운 20개만 싣는다. 중도해지는 즉시 command이므로 이 배열에 들어가지 않는다.

과거 artifact, 종료 지원, 전체 급여, 과거 세금연도, 적금 회차는 cursor 조회로 분리한다. snapshot 배열은
항상 안정된 key 순서이며 null을 0원·면제·미필로 해석하지 않는다. snapshot의 focus 점수는 화면 요약일
뿐이고 job/application 응답의 posting-specific 점수를 대신하지 않는다.

## 14. 기능 중심 화면

M3 화면은 사용자 정의 CSS 없이 다음 조작을 끝까지 제공한다.

- 여섯 스펙 점수와 evidence, 활동 catalog·시작·취소·진행률
- 기본 직무군 focus 표시와 focus 변경, focus 점수와 공고별 점수의 구분
- 포트폴리오·이력서·LinkedIn 프로필 새 버전 작성과 버전별 evidence 비교
- 여섯 플랫폼/업종 필터, 공고 요구 점수·내 점수·병역 요건·연봉 band
- 지원 현황, 면접 확인, 결과, 오퍼 수락/거절과 근로계약
- 월 gross·4대보험·소득세·지방소득세·net pay 명세
- 연말정산의 총급여·과세표준·산출세액·원천세·연금 예상/실제 공제·환급/추가납부
- 복무 option 자격·기간·급여·경력 효과, 복무 진행과 장병내일준비적금 납입·만기 예상액

DOM은 mount에서 한 번 만들고 hooks로 텍스트·행·disabled 상태만 갱신한다. 화면은 확률을 재계산하거나
월급·세금을 추정하지 않고 서버 응답을 표시한다. 상세 구현 전에 `client-foundation` skill을 읽는다.

## 15. 테스트와 실제 MySQL 검증

### 15.1 단위·protocol 테스트

테스트는 순수 규칙, parser, 순수 service orchestration에만 둔다. 프로젝트의 BDD/DCI 구조와 한국어
context/title 규칙을 따른다.

- 여섯 evidence 합산·cap·만료·비중 0·requirement 0·i128 overflow와 visible/possessed 차이, bundle 기본
  focus·focus 변경 뒤 snapshot 점수와 posting-specific 점수의 분리
- 활동 effort priority, 생활상태별 capacity, `dailyEffortCapUnits`, 최소 달력일, 취소, 같은 날 완료
  tie-break
- artifact version 증가, 이전 버전 불변, evidence allowlist, headline/summary trim·Unicode scalar·control
  검증과 text-only rendering 경계, typed checklist 10,000 합, application pin 후 새 버전·focus 변경 무영향
- evidence period pair와 `[start,end)`, 1월 1일 파생 생일, 15세 경계, resume의 education/experience
  same-dimension overlap 거절과 cross-dimension overlap 허용
- 세계 seed 고정 공고·stage roll·salary roll이 재시작·조회 순서·DB ID와 무관한 golden vector인지
- 여섯 플랫폼 hard filter, 역제안, active/day 상한, 서류→확인→면접→오퍼 전이와 경계 게임일
- 연봉 12분할 remainder, 월 말 payday, 입사월 일할, 4대보험 상·하한·절사, 간이세액표 경계
- 총급여·근로소득공제·인적공제·근로소득세액공제·원천세 차액과 2월/5월 이중 공제 방지
- 연금 한도·산출세액 부족·계좌별 allocation·0원 credit·세원층 합 불변·동일 연도 replay
- 복무 `[start,end)`, 자격, 단계별 급여, 경력 인정/비인정, effort 제한, 완료 후 병역 요건
- 장병적금 기관/합산 한도, 부족 회차, 납입별 policy, 만기·중도해지·정부지원·원장 합
- strict tagged payload, unknown field/version, decimal ID, specs/activities/history pagination, catalog 200·
  active 3 상한, focus를 포함한 cursor 경쟁, 중간 실패 전체 rollback

DOM·라우팅·실제 네트워크 왕복·snapshot test는 만들지 않는다.

### 15.2 실제 MySQL 8 스모크

PII가 없는 격리 MySQL 8에서 다음을 검증한다.

- 빈 DB와 M2 완료 DB의 forward migration, 기존 world/market/account/ledger byte-for-byte 보존
- M3-A pin 전에 complete A/B/D bundle이 원자적으로 게시되고 불완전 bundle·ranked의 `dev-unranked-*` key가
  거절되는지
- 기존 런 M3 pin과 §2.3의 5개 education mapping, certification 앞 N개(0·50 경계), 31개 experience
  mapping·고정 period, 새 런 초기 evidence, run revision 격리
- 게시 bundle/policy/posting/artifact/payroll/assessment/allocation의 update/delete trigger
- 다른 사용자의 artifact/application/offer/contract/service/savings ID를 복합 FK와 소유권 join이 거절
- 동시 지원·오퍼수락·급여·적금납입·연금납입·advance가 §12.2 잠금 순서에 수렴
- 같은 날 두 번째 settlement와 연말 coordinator를 강제 실패시켰을 때 급여·원장·세금·세원층 전체 rollback
- command replay와 자동 action replay가 한 지원, 한 오퍼, 한 payroll, 한 적금 회차, 한 assessment만 생성
- 1월 1일 pin → 2월 연말정산 → 5월 금융소득 신고를 실제 날짜 경계로 재대조

운영 DB에는 격리 복제와 백업·rollback 이미지 검증 전 migration을 적용하지 않는다.

## 16. 공식 자료와 seed data 검수

M3-A migration을 실행하기 전에 §2.1의 complete minimal `careerCatalogBundle` seed를 먼저 만든다. seed
review는 기본 focus, 다섯 education bridge, 순서가 고정된 certification bridge 50개, experience bridge
31개와 period 규칙, activity의 daily cap, artifact checklist, B/D의 모든 FK가 같은 bundle 안에서 닫히는지
확인한다. 단계별 migration은 이 published content에 row를 덧붙이지 않는다.

최초 `employmentPolicySet` seed를 만들 때 최소 다음 공식 자료를 원문 기준으로 다시 검수한다.

- [국세청 근로소득·간이세액표 안내](https://j.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=7875&mi=6596)
- [2026-02-28까지 적용하는 소득세법 시행령 별표 2 원문](https://www.law.go.kr/LSW/flDownload.do?flSeq=163197407)
- [2026-03-01부터 적용하는 소득세법 시행령 별표 2 원문](https://www.law.go.kr/LSW/flDownload.do?flSeq=163116877)
- [국민연금공단 2026년 사업장 실무안내 원문](https://m.nps.or.kr/fileDown.do?atchFileId=FL26000090&atchFileSn=1)
- [국민연금공단 2026년 7월 기준소득월액 상·하한 안내](https://www.nps.or.kr/pnsgdnc/newgdnc/getOHAE0001M1.do?hmpgBbsCd=BS20240137&hmpgCd=01&menuId=MN24000897&pageIndex=1&pstId=ZZ202600000000000147&searchGbu=&searchText=&sortSe=FR)
- [국민건강보험공단 2026년 건강·장기요양 보험료율 안내](https://edi.nhis.or.kr/portal/images/popup/20251204_pop01longdesc.html)
- [고용노동부 고용보험료 부담비율 안내](https://www.moel.go.kr/info/astmgmt/employ/employList.do)
- [고용노동부 산재보험료율 안내](https://www.moel.go.kr/news/enews/report/enewsView.do?news_seq=18810)
- [고용노동부 산재보험 근로자 부담 원칙](https://www.moel.go.kr/info/astmgmt/employ/sanjaeList.do)
- [병무청 육군 복무기간 안내](https://www.mma.go.kr/minwon/contents.do?mc=mma0000728)
- [병무청 사회복무 소집제도](https://www.mma.go.kr/contents.do?mc=mma0000744)
- [병무청 산업·전문요원 복무기간](https://www.mma.go.kr/minwon/contents.do?mc=mma0000760)
- [병무청 산업·전문요원 편입요건](https://www.mma.go.kr/seoul/contents.do?mc=mma0000764)
- [2026 병 봉급 공무원보수규정 별표 13](https://www.law.go.kr/flDownload.do?flSeq=160436483)
- [2026 군인 봉급표](https://www.mpm.go.kr/mpm/info/resultPay/bizSalary/2026/)
- [2026 최저임금 고용노동부 고시](https://www.moel.go.kr/news/enews/report/enewsView.do?news_seq=18144)
- [국방부 장병내일준비적금 안내](https://www.mnd.go.kr/mnd/288/subview.do)
- [병역법 시행령 제158조의2](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=171035)
- [2026 장병내일준비적금 특약](https://img2.kbstar.com/obj/ocommon/260213military_full.pdf)

employment 자료 확인일은 `2026-07-26`, military·장병적금 자료 확인일은 `2026-07-27`이다. 링크의
요약문을 seed로 쓰지 않고 시행 중인 법령·고시·공식 표 원문과
교차 검증한다. 원문이 서로 다르거나 2026 귀속 표가 확정되지 않은 항목은 `policyUnavailable` 상태로
남겨 계산을 막고, 추정 숫자를 production/ranked key에 넣지 않는다. 엔진 개발에 필요한 임시 fixture는
`dev-unranked-*` key와 ranked-disabled metadata로만 제공하고 calibration과 공식 원문 review 뒤 새
production key를 게시한다.

## 17. M3 완료 조건

1. complete A/B/D bundle과 legacy bridge로 런이 초기화되고, 기본/변경 focus의 snapshot 점수와 공고별
   점수가 분리된 채 여섯 스펙의 취득·만료·period·점수와 세 산출물의 immutable version/application pin이
   재현된다.
2. 여섯 플랫폼·여섯 업종 공고가 같은 seed에서 같고, 지원·면접·오퍼가 replay에도 중복되지 않는다.
3. 오퍼 수락으로 한 근로계약이 생기고 월 gross에서 4대보험·원천세를 뺀 net이 원장과 지갑에 일치한다.
4. 1월 1일 연말정산이 2월 현금 정산과 M2 5월 금융소득 신고를 중복 없이 연결한다.
5. 연금저축·IRP 예상 공제가 실제 세액 한도만큼만 `creditedContribution`으로 재분류된다.
6. 모든 복무 형태의 기간·급여·활동 capacity·경력 효과와 병역 요건이 policy/catalog대로 적용된다.
7. 장병내일준비적금의 납입·미납·만기·중도해지·정부지원이 원장과 policy version으로 설명된다.
8. 활동 → 문서 → 지원 → 면접 → 취업 → 급여 → 연금 납입 → 연말정산을 기능 화면에서 조작할 수 있다.
9. headline/summary와 typed checklist, resume 기간 규칙, activity daily cap, specs/activities/history
   pagination 및 catalog/active 상한이 client/server exact 계약에서 일치한다.
10. 서버 test/clippy/fmt, 클라이언트 test/typecheck/lint/build, 실제 MySQL 8 격리 스모크가 통과한다.

## 18. 구현 전 남은 데이터 작업

상태기계·계산 순서·enum·seed·반올림의 저장 위치는 이 문서로 확정됐다. M3-A pin에 필요한 complete
minimal A/B/D content와 bridge/checklist row는 seed하지 않고 미루는 작업이 아니라 구현 선행조건이다.
남은 것은 제품 설계 미결정이 아니라 개발용 `dev-unranked-*` seed를 production/ranked 새 key로 승격해도
되는지 판단하기 위한 다음 versioned data의 검증·캘리브레이션이다.

1. 2026 귀속 간이세액표·4대보험 상하한·복무 급여표·장병적금 규칙을 공식 원문에서 typed fixture로
   전사하고 checksum review를 통과시키는 일
2. 여섯 업종의 초기 연봉 band와 직무별 여섯 차원 weight를 어떤 공개 통계 기준으로 calibration할지
3. 플랫폼별 슬롯·competition/pass table과 활동 effort/cost가 재미있는지를 정하는 플레이테스트 수치

2와 3은 bundle key를 바꾸는 밸런스 데이터이며 엔진 계약을 바꾸지 않는다. 검증 전에도 complete 개발
bundle/policy fixture는 존재하지만 `dev-unranked-*`로만 게시한다. review가 끝나면 그 record를 수정하거나
`2026-v1`로 rename하지 않고, 검수 provenance와 ranked 허용 metadata를 갖춘 새 production key를 게시한다.
