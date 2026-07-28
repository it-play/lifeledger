# M4 생애 상세 스펙

- 작성: 2026-07-26
- 상태: M4-E1 production 주 경로 인수 완료, 전세 overlay·D+1,825 경계 인수 대기, 시각 스타일링 보류
- 상위 계획: [`development-plan.md` §3, §4.2, §6, §8, §9, §12](./development-plan.md)
- 선행 마일스톤: M0 게임 루프, M1 시장 코어, M2 계좌·세제, **M3 커리어 전체**

## 1. 목표와 단계

M4는 M3까지의 금융·고용 루프에 생활을 유지하는 비용, 주거, 신용, 위기와 재기 수단을 붙인다.
완료 시 플레이어는 소득과 자산만 늘리는 것이 아니라 **매달 의무를 감당하고, 주거와 부채를 선택하고,
위기를 보험·복지·법적 절차로 통과하는 30년 플레이**를 끝낼 수 있어야 한다.

구현은 다음 수직 슬라이스로 나눈다.

1. **M4-A 생활비·가구** — 지역·가구 구성·CPI 기반 예산, 월별 청구와 부족액 처리
2. **M4-B 신용·대출** — 시작 부채 계약화, 상환·중도상환·연체, 신용 상태, DSR/LTV
3. **M4-C 주거·부동산** — 매매, 임대차, 담보대출, 보유·양도 관련 세금
4. **M4-D 복지·생애 사건·보험** — 데이터 조건식 판정, 결정론적 사건, 보장 청구
5. **M4-E 위기·재기·단순 법인** — 파산·회생·면책 상태기계와 최소 법인 손익
6. **M4-F 기능 화면·장기 검증** — 스타일 없는 조작 화면, 30년 회귀와 실제 MySQL 8 검증

### 1.1 현재 재개 지점 (2026-07-29)

현재 권위 checkpoint는 `main`의 `0de740f`이며 development production DB는 migration `45/45`, 실패 0이다.
M4-E1의 구현·배포·주 경로 인수 결과는 §13.16~§13.17에 기록했다. 별도 MySQL, 격리 schema, recovery dump는
만들지 않았고 `main`의 `server/**` push → 원격 image build → 새 server의 `sqlx::migrate!()` → health
순서로 production DB를 전진시켰다. 접속은 `ssh snowykte0426@59.28.34.117`, service host port는 `10105`,
public base는 `https://kimtaeeun.site/lifeledger`다. 비밀번호·session token은 문서나 repository에 남기지 않는다.

production 인수 fixture는 user 4, save 118, run revision 1이다. case 2는 day 31/state revision 35의
`rebuilding`이며 현금 18,087,371원, 채무 0원, 배분 5,225,344원, 면책 47,315,384원,
`creditRestrictionEndExclusive=1856`이다. loan 3·4는 `discharged/0`이고 인수 session은 삭제했다. 이 fixture는
D+1,825 재개 전까지 append-only provenance로 보존한다.

M4-E1을 아직 전체 완료로 올리지 않는 이유는 두 가지뿐이다.

1. 무담보와 주담대 quote는 `creditRestricted/insolvencyRebuilding`을 반환했지만, 현재 jeonse listing 두 건의
   전세대출 quote는 overlay 판정 전에 `contractConflict`로 거절됐다. listing·현재 월·product 13의 exact
   join을 진단하고 전세 quote와 저장 quote 실행 재평가까지 확인해야 한다.
2. 순수 core의 D+1,825 exclusive 경계 테스트는 통과했지만 production 1일 pipeline 경계는 아직 재생하지
   않았다. 30일 advance가 reverse proxy 504를 냈어도 같은 command 재개로 정확히 day 30까지 한 번만
   전진했으므로 멱등성은 확인했다. 1,825일을 그대로 기다리는 대신 large-step 병목을 먼저 계측·개선하거나
   재현 가능한 bounded 운영 fixture를 설계한 뒤 day 1855 제한·day 1856 회복을 검증한다. published case의
   종료일을 임시 SQL로 바꿔 성공으로 기록하지 않는다.

다음 재개 순서는 고정한다.

1. 이 문서 §8.5, §8.8, §13.17과 [`development-plan.md` §12](./development-plan.md)를 먼저 읽는다.
2. save 118의 새 인수 session을 임시로 만들되 token을 출력·문서화하지 않고, 전세 listing
   `contractConflict` 원인부터 고친다. 관련 코드는 `server/src/store/loans.rs`의 전세 quote와
   `credit_restricted_in_tx` 호출부다.
3. 변경 범위의 표적 test/check만 통과시킨 뒤 `git-commit` 규칙으로 commit·push하고 production CD에서
   전세 quote·실행 overlay를 확인한다.
4. large-step 성능을 계측해 D+1,825 production recovery와 재시작 hash를 끝낸 뒤 M4-E1을 완료로 바꾼다.
   그 다음 기능 단계는 §9의 M4-E2 단순 법인이며, 시각 스타일링은 M4-F 기능·30년 검증 뒤로 계속 미룬다.

재개 전에 이 문서의 §2, §8.1~§8.9, §10~§13.17과 상위
[`development-plan.md` §12](./development-plan.md)를 읽는다. 작업 규칙은 [`AGENTS.md`](../AGENTS.md), schema와
migration은 [database-schema](../.agents/skills/database-schema/SKILL.md)·
[migration-guide](../.agents/skills/migration-guide/SKILL.md), API는
[api-design](../.agents/skills/api-design/SKILL.md), 화면은
[client-foundation](../.agents/skills/client-foundation/SKILL.md), 검증은
[test](../.agents/skills/test/SKILL.md)·[security-checklist](../.agents/skills/security-checklist/SKILL.md)를
따른다. 운영 배포는 [deploy workflow](../.github/workflows/deploy-server.yml),
[deployspec](../server/deploy/deployspec.yml), [Dockerfile](../server/Dockerfile)을 함께 확인한다.

M4는 M3의 다음 권위를 **소비만** 한다.

- 캐릭터의 학력·경력·병역·고용 상태와 근로 가능 상태
- 급여·사업 외 근로소득·4대보험·원천징수·연말정산 원장
- 게임일별 시간 예산과 M3가 예약한 정산

가구·부양관계·거주지는 M4가 권위를 가진다. 새 런에서는 캐릭터의 초기 부양가족 draft를 M4 가구 행으로
계약화하고 이후 변경은 M4 사건·명령으로만 일어난다. M4는 급여나 채용 결과를 다시 계산하지 않는다.
M3 계약의 이름이 달라져도 위 의미를 제공하는 조회 인터페이스를 경계로 연결한다. M3가 완료되고
마이그레이션·API·일일 planner 계약이 고정되기 전에는 M4 구현을 시작하지 않는다.

시각적 스타일링, 실제 신용평가사 점수의 복제, 법률 자문 수준의 도산 절차, 실제 주소·개별 매물,
건강 진단 시뮬레이션은 범위 밖이다.

## 2. 불변 버전과 재현성 경계

실제 법령 수치와 게임 밸런스 수치를 코드 상수 하나에 섞지 않는다. 새 런은 시작 transaction에서 다음
불변 버전을 `run_rule_bundle`로 고정한다.

| 버전 | 담는 값 | 변경 방법 |
|------|---------|-----------|
| `policy_set_id` | 세금, 복지, DSR/LTV, 도산 절차상 요건처럼 실제 제도를 모델링한 값 | 원문 검증 뒤 새 게시 버전 |
| `life_catalog_set_id` | 생활비 기준액, 지역 계수, 사건·보험·법인 템플릿 | 플레이테스트 뒤 새 게시 버전 |
| `credit_model_version_id` | 신용 상태 전이와 상품 가격 조정 규칙 | 검증 뒤 새 게시 버전 |
| `real_estate_model_version_id` | 지역 지수, 매물 생성, 유동성·임대료 모델 | 캘리브레이션 뒤 새 게시 버전 |

게시된 세트는 update/delete하지 않고, 기존 런에 새 행을 끼워 넣지 않는다. 새 버전은 `draft → sealed`만
허용하고 `sealed_at`과 canonical JSON SHA-256을 기록한다. 모든 참조가 게시 상태인지 확인한 뒤 M2의
시장·policy assignment와 함께 새 런에 할당한다. 각 기존 assignment의 revision 숫자가 우연히 같아야 하는
것은 아니다. `run_rule_bundle_assignment(newRun)`이 자기 `assignmentRevision`을 하나 가지며, 선택한 네
버전 ID와 선택 당시 market·policy·career·employment assignment revision을 각각 복제한다. 게임 시작은 이
합성 assignment 한 행을 잠가 전체 조합을 원자적으로 pin한다.

`life_catalog_set`은 생활비, 복지, 사건, 보험, 법인 component version ID를 담는 aggregate manifest다.
아직 구현되지 않은 component도 행동 가능한 row가 없는 명시적 sealed `disabled` version을 참조하며 null이나
코드 기본값으로 대신하지 않는다. 다음 slice는 기존 aggregate에 child를 추가하지 않고 새 component version과
새 aggregate를 게시해 `newRun` assignment만 바꾼다. 중간 slice에서 만든 개발 런은 당시 기능 집합을 그대로
유지한다. M4의 배포 migration들은 한 rollout 단위로 모두 적용하고, 외부 요청을 받기 전 마지막 migration이
기존 pre-M4 런과 새 런 assignment를 최종 aggregate에 한 번 bridge한다. 출시 중간 migration 상태에서 만든
런은 지원하지 않는다.

M4-E1에서 insolvency component를 처음 추가할 때는 기존 schema v1 aggregate에 존재하지 않던 열을 disabled
값으로 사후 backfill하지 않는다. sealed v1 행과 canonical hash를 그대로 두고 null을 구조적 부재로 읽으며,
insolvency ID를 canonical manifest에 포함하는 schema v2 aggregate부터 non-null을 강제한다. 이 호환 예외는
기존에 이미 선언된 component를 null로 생략해도 된다는 뜻이 아니다.

M4 첫 migration은 기존 `policy_set`에 `canonical_sha256`을 추가하고, sealed set은 `policy_set` header와
`policy_rule`을 `(domain, ruleKey, effectiveFrom, id)` 순으로 canonicalize한 hash를 한 번 backfill한다.
새 M4 policy rule은 `policy_source_document(sourceKey, sourceUrl, checkedOn, originalSha256)`와
`policy_rule_source` 연결이 하나 이상 없으면 set을 seal할 수 없다. M2에서 이미 게시돼 개별 원문 연결을
보존하지 못한 rule은 `legacyProvenance`로 명시해 기존 런만 계속 읽을 수 있지만, 새 M4 법정 요건의 근거로
재사용하거나 ranked policy를 게시하는 데 사용할 수 없다.

이 문서에서 특정 법정 한도·세율·기간·신용점수를 임의로 확정하지 않는다. 그런 값은 출처와 기준일을 가진
`policy_rule` 또는 게시된 카탈로그 행에 들어가며, 빠졌을 때 코드 기본값으로 보충하지 않고 명령이나 하루를
실패시킨다. 반면 다음 **게임 실행 의미**는 엔진 버전의 고정 계약이다.

- 돈은 원 단위 `BIGINT`, 금리·비율은 bp 또는 ppm, 중간 계산은 i128이다.
- 달리 적지 않은 양의 금액 비율 계산은 0원 방향으로 내리고, 각 규칙이 요구하는 독립 세액도 각각 내린다.
- 원 미만을 다음 기간으로 넘겨야 하는 이자·CPI 비용은 계약별 signed remainder를 저장한다.
- 같은 날의 명령·정산·사건 순서, 상태 전이, entropy key와 tie-break는 아래 절대로 고정한다.
- 필수 사용자 선택이 기한까지 없으면 카탈로그의 `defaultChoiceId`를 적용한다. 기본 선택은 보이지 않는
  난수나 서버 배포 설정으로 정하지 않는다.

## 3. 가구·지역·CPI 생활비

### 3.1 생활 단위와 기준액

`household`는 한 런에 하나이고 캐릭터를 기준 구성원으로 가진다. `household_member`는 M4가 소유하며
`dependent · partner · child · parent` 역할과 합류·이탈 게임일을 보존한다. 새 런의 초기 행은 캐릭터
draft에서 만들고, 이후 생애 사건은 기존 행을 덮어쓰지 않고 유효기간을 닫고 새 관계를 추가한다. M4 v1의
기존 draft는 관계 없이 `dependents` 수만 가지므로 1부터 N까지 안정된 ordinal의 `dependent` 행으로 bridge한다.
이 행의 시작 나이는 life catalog의 typed `legacyDependentAgeYears`를 사용해 M3 §2.3과 같은 1월 1일
생년월일로 파생한다. 누락된 bridge 값은 초기화를 실패시키며 임의 성인/자녀로 추정하지 않는다. partner,
child, parent는 이후 typed 사건이 관계와 생년월일을 명시할 때만 생긴다. 플레이어의 나이는
`career_run.birthDate`를 그대로 단일 권위로 읽고 snapshot에 증가값을 중복 저장하지 않는다.

`character.region`은 불변 출신지다. M4 초기화는 life catalog의 typed region bridge로 이 값을 최초
`residence.regionKey`에 복사하지만 이후 이사는 residence만 바꾸며 출신지를 덮어쓰지 않는다.

`cost_of_living_profile`은 다음 typed 필드를 가진 불변 카탈로그다.

- 기준 CPI index와 기준 월의 `housing · food · transport · communication · utilities · healthcare ·
  education · dependentCare · discretionary` 원 금액
- 지역 key별 항목 계수 ppm
- 가구원 역할·연령 band별 한계비용 계수 ppm
- 소유·전세·월세·무상거주별 주거 항목 대체 규칙
- 각 항목의 필수 여부와 미납 시 부족분 처리 방식

항목별 `householdFactorPpm`은 플레이어의 1,000,000ppm에 그 항목에 적용되는 active 가구원별
`marginalFactorPpm(role, ageBand, category)`을 모두 더한 값이다. `housing`의 유효 기준액은 먼저
`기준액 × tenureReplacementFactorPpm / 1,000,000`으로 정한다. 게시 가능한 catalog는 이 나눗셈이 정확히
나누어떨어져야 하며 런타임도 그 불변식을 확인해 원 미만을 조용히 버리지 않는다. 다른 항목의 유효 기준액은
기준액과 같다. 월 `m`의 항목별 청구는
`유효 기준액 × 현재 CPI index × 지역계수ppm × householdFactorPpm × budgetBandFactorPpm`을 i128에서 한 번
곱하고 `기준 CPI index × 1,000,000³`로 한 번 나눈다. 이전 달 remainder numerator는 나눗셈 전에 더한다.
몫은 0원 방향으로 내리고 새 remainder는 `numerator - quotient × denominator`다. DB는 이 signed i128 값을
손실 없이 담는 `DECIMAL(39,0)` numerator와 적용 profile ID를 보존한다. 계산 순서가 바뀌어 원 단위가
달라지지 않으며 `(household, category)`별 remainder를 다음 달에 넘긴다.

중도 시작 월과 완전한 달 사이에도 remainder 단위를 바꾸지 않기 위해 calendar-day 분수는 고정
`prorationScale = lcm(28,29,30,31) = 377,580`으로 정규화한다. 완전한 달은 위 numerator와 denominator에
각각 377,580을 곱하고, 중도 월은 numerator에
`remainingCalendarDays × (377,580 / daysInMonth)`, denominator에 377,580을 곱한다. 따라서 모든 달의
remainder가 같은 denominator를 가지며 다음 달에 재스케일하거나 원 미만을 버리지 않고 한 번의 정수
나눗셈으로 이어진다.
CPI는 런에 고정된 M2 시장 월드의 해당 게임일 값을 읽는다. 미래 CPI나 서버 현재 날짜는 읽지 않는다.
조회 계약은 항목별 원 기준액·기준 CPI·현재 CPI·지역·가구·budget·tenure replacement 계수와 월별
`prorationScale · prorationUnits · prorationDays · daysInMonth`를 함께 반환한다. 화면은 이 고정 입력을 표시할
뿐 금액을 다시 계산하지 않는다.

지역은 캐릭터의 출신지가 아니라 현재 `residence.regionKey`다. 이사 완료 전에는 기존 지역, 완료 transaction
뒤 다음 생활비 산정부터 새 지역을 쓴다. 가구 변경도 같은 원칙으로 해당 변경이 commit된 다음 청구부터
반영한다.

### 3.2 예산과 월 청구

플레이어는 카탈로그의 허용 band 중 `frugal · standard · generous` 같은 소비 수준을 항목별로 선택한다.
표시명과 계수는 카탈로그 데이터이며 엔진은 임의의 문자열을 받지 않는다. 선택하지 않은 새 항목은
카탈로그의 명시적 `defaultBandId`를 쓴다. 필수 항목은 0으로 낮출 수 없다.

생활비는 각 게임 월의 첫날에 그 달 값을 확정하고 월 말 정산을 예약한다. 게임 시작일이 월 중간이면 위
분자에 `remainingCalendarDays`, 분모에 `daysInMonth`를 추가해 한 번에 일할 계산하고 remainder를 보존한다.
이후 가구원이 달 중간에 바뀌면
다음 달부터 반영하며, 사건이 만든 즉시 의료비·돌봄비는 별도 정산으로 처리한다. 이 단순화는 월 청구를
과거 날짜에 재산정하지 않기 위한 게임 규칙이다.

같은 날에는 §10.1의 기존 M2·M3 정산 다음에 `대출 정기상환 → 주거 계약 월세 → 당월 필수 생활비 →
기존 essentialArrear → 당월 선택 생활비` 순서로 현금을 배분한다. 월세는 별도 settlement와
`leaseArrear`를 만들고, 임차 residence의 생활비 `housing` 항목은
카탈로그 replacement가 월세를 중복 포함하지 않는다. 같은 그룹 안에서는 고정 category enum 순서이며
기존 essentialArrear는 `dueYearMonth, category, id` 순서다. 필수 생활비가 부족하면 가능한 금액을 먼저
납부하고 나머지는 `essentialArrear`
무이자 의무로 만들며, 선택 항목은 현금 범위에서 축소하고 부채를 만들지 않는다. 실제 생계비 채무를
모사한 것이 아니라 게임이 음수 현금을 만들지 않기 위한 고정 기본값이다. 미납 필수액은 복지·도산 판정의
입력이지만 대출 신용 연체와 같은 것으로 세지 않는다. 월말에는 위 순서로 자동 상환하고, 플레이어는
`POST /api/life/arrears/{id}/payments`에서 같은 command/cursor 계약으로 즉시 전액 또는 일부 상환할 수 있다.

`GameSnapshot.life`와 `GET /api/life/budget`의 `activeArrears`는 위 우선순위로 정렬한 최대 20건의 **상환
window**다. `hasMoreActiveArrears`는 뒤에 남은 행이 있는지를 알리고 `totalEssentialArrearKrw`는 window가
아니라 전체 active 잔액의 합이다. window 안의 행을 전액 상환하면 다음 행이 최신 snapshot에 들어오므로 모든
체납을 순서대로 조작할 수 있다. `hasMoreActiveArrears=false`일 때만 배열 합이 전체 합과 정확히 같아야 하고,
true이면 전체 합이 배열 합보다 커야 한다.

`living_cost_month`는 입력 CPI·지역·가구 fingerprint, 항목별 gross·paid·arrear와 remainder를 보존한다.
같은 `(household_id, year_month)`는 한 번만 확정하며 버전 변경으로 과거를 다시 계산하지 않는다.

### 3.3 M4-A 개발 카탈로그와 활성화 경계

첫 기능 fixture는 `dev-unranked-m4-life-2026-v1`이며 ranked에서 사용할 수 없는 플레이테스트 값이다.
기준 CPI는 1,000,000이고 1인 표준 월 기준액은 다음과 같다.

| category | 기준액(원) | 필수 | legacy dependent 한계계수(ppm) |
|---|---:|:---:|---:|
| `housing` | 450,000 | 예 | 200,000 |
| `food` | 350,000 | 예 | 350,000 |
| `transport` | 120,000 | 예 | 150,000 |
| `communication` | 60,000 | 예 | 100,000 |
| `utilities` | 100,000 | 예 | 150,000 |
| `healthcare` | 70,000 | 예 | 150,000 |
| `education` | 50,000 | 아니오 | 350,000 |
| `dependentCare` | 120,000 | 예 | 300,000 |
| `discretionary` | 180,000 | 아니오 | 150,000 |

초기 bridge dependent의 나이는 12세다. M4-A에는 관계 변경 사건이 없으므로 현재 fixture는
`dependent`의 전 연령 band만 게시한다. partner·child·parent와 세분 age band는 사건 component가 그 관계를
실제로 만들 때 새 cost profile과 aggregate로 게시하며 기존 profile에 행을 추가하지 않는다.

소비 band는 `frugal 850,000ppm · standard 1,000,000ppm · generous 1,250,000ppm`이고 모든 항목의 기본은
`standard`다. M4-A는 필수 항목을 삭제하거나 0원 band로 바꾸는 경로를 제공하지 않는다. 지역 계수는
카테고리별로 다음 순서(`housing, food, transport, communication, utilities, healthcare, education,
dependentCare, discretionary`)의 ppm을 사용한다.

| region | category 순서별 계수(ppm) |
|---|---|
| `capitalArea` | 1,300,000 · 1,080,000 · 1,100,000 · 1,000,000 · 1,050,000 · 1,050,000 · 1,150,000 · 1,120,000 · 1,120,000 |
| `metropolitan` | 1,000,000 · 1,020,000 · 1,050,000 · 1,000,000 · 1,000,000 · 1,000,000 · 1,000,000 · 1,000,000 · 1,000,000 |
| `smallCity` | 820,000 · 960,000 · 950,000 · 1,000,000 · 980,000 · 950,000 · 900,000 · 900,000 · 930,000 |
| `rural` | 680,000 · 940,000 · 1,100,000 · 1,030,000 · 1,080,000 · 920,000 · 850,000 · 850,000 · 880,000 |

M4-C 전의 residence bridge는 명시적 `rentFree`다. `rentFree`와 미래 `monthlyRent`는 생활비의 housing
대체계수를 0ppm으로 두고, `owner`는 350,000ppm, `jeonse`는 200,000ppm을 쓴다. 월세는 M4-C의 별도
settlement가 생기기 전까지 합성하지 않는다.

새 런은 day 0 시작 transaction에서 household·residence와 첫 달을 초기화한다. 기존 M3 런은 migration이
household·member·residence와 pinned bundle만 만들고, 다음 일일 transaction의 target market date를
`activationDate`로 삼아 아직 없는 해당 월을 한 번 초기화한다. activationDate가 월 중간이면 그 날을 포함한
남은 달력일로 일할한다. 이 ensure는 `(household, yearMonth)` unique key로 멱등하고 과거 월을 소급 청구하지
않는다. M4-A 동안 구현되지 않은 welfare·event·insurance·corporation component와 credit·real-estate model은
행동 가능한 child가 없는 sealed `disabled` version으로 pin한다.

CPI가 없는 보존 world v1~v3 런은 household·residence bridge는 만들되 생활비 component가 disabled인
compatibility aggregate를 영구 pin한다. 미래 CPI를 합성하거나 v4로 갈아끼우지 않으며 budget/arrear mutation은
`rateUnavailable`로 거절한다. CPI가 있는 v4 기존 런과 `newRun`만 active M4-A aggregate를 pin한다.
`GET /api/life/budget`은 compatibility 런에서도 성공한다. 이때 bridge된 household·residence는 반환하고
`rateStatus=rateUnavailable`, `allowedBands=[]`, `selections=[]`, `currentMonth=null`, `activeArrears=[]`,
`hasMoreActiveArrears=false`, `totalEssentialArrearKrw=0`으로 고정한다. active 런만 하나 이상의 band와 canonical
아홉 selection을 반환한다.
`run_rule_bundle`은 `(saveId, runRevision)`마다 선택한 네 M4 version ID와 선택 당시 market·finance·career·
employment assignment revision을 복제한 불변 이력이다. 조회 시 전역 `newRun` pointer를 다시 읽지 않는다.

M3의 급여·연말정산 호환 조회는 더 이상 `character.dependents`를 직접 읽지 않고 M4가 제공하는
`taxDependentEligible` active member 수를 읽는다. legacy dependent bridge는 모두 true로 만들어 기존 계산을
보존하고 player는 false다. 이후 partner·child·parent 사건은 관계와 나이만으로 세법상 부양가족을 추정하지
않고 적용 policy가 판정한 값을 member effective period에 함께 기록한다.

M4-A의 고정 protocol 이름은 settlement `livingCostMonth`, ledger source
`livingCostMonth · essentialArrearPayment`, account `livingCostExpense · essentialArrearLiability`다.
월 정산은 실제 소비된 선택비와 필수비 전액을 expense로, 현금 지급액을 wallet 감소로, 미지급 필수액을
arrear liability 증가로 기록한다. arrear 상환은 wallet 감소와 liability 감소를 같은 balanced transaction에
기록한다. M4-B가 기존 시작 부채를 계약화하기 전에는 `save.debtKrw`의 pre-M4 잔액에 active
essential arrear 잔액만 더한 transitional projection을 유지한다. M4-B bridge는 이 arrear를 먼저 빼고 남은
pre-M4 잔액만 `legacyDebt`로 계약화한다.

`PUT /api/life/budget`은 공통 command/cursor와 `selections` 배열을 받으며 아홉 category를 각각 정확히 한 번
포함해야 한다. 각 원소는 `{ category, bandId }`뿐이고 서버는 catalog 허용 여부를 다시 검증한다. 응답 result는
적용 게임일과 canonical category 순서의 전체 선택을 반환한다. 이미 확정된 당월 청구는 바꾸지 않고 다음에
아직 생성되지 않은 월부터 적용한다. arrear payment는 `{ amountKrw }`를 받아 1원 이상 현재 잔액 이하만
허용하고 result에 지급액과 남은 잔액을 반환한다.
같은 command ID·종류·payload·최초 cursor의 재전송은 그 뒤 게임 상태가 전진했더라도 저장된 result를
`replayed=true`로 반환하고 응답 snapshot만 현재 최신 상태를 담는다. 현재 cursor 검증은 새 명령에만 적용한다.

## 4. 대출·상환·연체와 신용

### 4.1 시작 부채의 실제 계약화

M4 이후 `save.debt_krw`는 조회용 합계일 뿐 권위가 아니다. 부채 권위는
`loan_contract`의 남은 원금·발생 이자·비용, `essential_arrear`, `lease_arrear`, `tax_obligation`, 임대인이
받은 보증금 반환 의무의 합이다. insolvency claim은 이 의무의 collection authority를 복제한 view이므로
별도 부채로 두 번 더하지 않는다. DSR은 이 전체 합이 아니라 policy가 선언한 loan source만 사용한다.
새 캐릭터 draft의 시작 대출은 금액만 받지 않고 다음 strict variant다. M4-B가 활성화하는 시작 variant는
`studentLoan`과 `unsecuredLoan`뿐이다.

- `studentLoan { productVersionId, principalKrw }`
- `unsecuredLoan { productVersionId, principalKrw }`
- M3 이전 호환 draft의 `studentLoanKrw · creditLoanKrw`는 pinned active credit catalog에 게시된 명시적
  legacy mapping이 있을 때만 위 두 variant로 변환

`leaseDepositLoan { productVersionId, principalKrw, initialLeaseTemplateId }`와 `mortgage` type은 engine
contract에 예약하지만 임대차·담보가치·실행 transaction은 M4-C에서 활성화한다. M4-B API는 두 kind의
실행 가능한 상품을 반환하지 않고 담보 심사는 `valuationUnavailable`로 끝낸다.

캐릭터 시작 transaction은 카탈로그 자격을 검증하고 계약, 상환 스케줄, opening 원장, 필요한 임대차를
한 번에 만든다. 어느 하나라도 유효하지 않으면 런 전체를 만들지 않는다. 기존 런의 aggregate 부채는
마이그레이션 시 출처별 매핑 가능한 행만 계약화하고, pre-M4 `save.debt_krw`에서 출처를 복원할 수 없는
잔액은 숨겨진 기본 상품이 아니라 표시 가능한 `legacyDebt` 계약 하나로 한 번 고정한다. M4 활성화 뒤
세금 부족액은 opaque aggregate를 직접 늘리지 않고 `tax_obligation`을 먼저 만든 뒤 projection을 갱신한다.
매 transaction은 권위 의무 합과 `save.debt_krw`가 같은지 검증한다.

### 4.2 계약과 상환 계산

대출 상품 버전은 `kind · rateType · referenceRate · spreadBp · dayCount · repaymentMethod · term ·
paymentCalendar · graceRule · prepaymentRule · delinquencyRule · collateralRule`의 typed 열 또는 strict
tagged 구조를 가진다. 계약은 가입 시 적용한 스프레드·기간·지급일·상환법을 복제하고 상품 버전을 참조한다.

M4가 지원하는 상환법은 다음 셋이다.

- `equalPrincipal` — 회차별 예정 원금을 총 회차로 나누고 잔여 원금은 마지막 회차가 모두 가져간다.
- `levelPayment` — 정수 시뮬레이션으로 만기에 잔액이 0 이하가 되는 가장 작은 원 단위 납입액을 이분 탐색한다.
- `bullet` — 회차에는 이자, 만기에는 원금 전부를 낸다.

각 회차 이자는 직전 지급일 다음 날부터 현재 지급일까지의 일수에 대해
`principal × annualRateBp × elapsedDays + interestRemainder`를 `dayCount × 10,000`으로 나눈 몫이다.
첫 회차는 계약 활성 game day 다음 날부터 첫 지급일까지를 포함해 세며, 활성 game day의 개시
스냅샷 자체에는 이자를 붙이지 않는다. 따라서 1월 1일 game day 0에 활성한 월말 납입 계약의
첫 구간은 game day 1(1월 2일)부터 1월 31일까지 30일이다.
나머지는 다음 회차로 넘기고, 마지막 종료에는 remainder를 버리며 숨은 1원 청구를 만들지 않는다.
변동금리는 상품에 적힌 reset 게임일의 확정 시장금리와 spread를 합쳐 그 다음 이자 구간에만 적용한다.
매월 1일 reset에서는 그 1일이 바로 다음 이자 구간의 첫날이며, 그날 시장값을 확정한 transaction이
`observationGameDay = effectiveFromGameDay`로 rate reset을 기록한다. 전날 금리나 reset 다음 날부터 적용하는
숨은 경로는 없다.
마이너스 금리 처리와 금리 상·하한은 반드시 상품 버전에 있어야 한다.

변동금리 `levelPayment`는 각 reset에서 남은 원금과 남은 회차를 대상으로 같은 정수 이분 탐색을 다시
실행해 이후 납입액을 고정한다. `equalPrincipal`의 예정 원금과 `bullet`의 만기 원금은 reset으로 바꾸지
않고 이후 이자만 새 금리를 쓴다. 재산정 전 납입액을 계속 쓰는 숨은 기본 경로는 없다.

상환 정산 순서는 `기한 지난 비용 → 기한 지난 이자 → 기한 지난 원금 → 당일 비용 → 당일 이자 → 당일 원금`이다.
지갑 부족 시 각 bucket을 가능한 만큼 납부하고 잔액을 연체 bucket으로 옮긴다. 조기상환 명령은 연체가
없을 때만 받고, 카탈로그 비용을 계산한 뒤 원금에 적용한다. 담보대출 일부 조기상환 후 정기 납입액을
다시 계산할지 만기를 줄일지는 계약의 immutable `prepaymentEffect`가 정한다.

### 4.3 연체와 신용 상태

계약 상태는 다음 전이만 허용한다.

`pending → active → paidOff`

`active ↔ delinquent → defaulted → {restructured | discharged | chargedOff}`

`pending → cancelled`는 돈이 움직이기 전만 가능하다. `paidOff · discharged · chargedOff · cancelled`는
종료 상태다. 하루가 끝났을 때 미납 bucket이 있으면 `delinquent`, 모두 해소되면 `active`로 돌아간다.
`defaulted` 전이의 연체 일수·금액 조건과 기한이익 상실은 `credit_model_version` 데이터다.

외부 기관의 실제 점수를 가장하지 않기 위해 API는 `creditScore`가 아니라
`creditBand: prime|standard|limited|distressed|insolvent`를 주 권위로 노출한다. 내부 `credit_units`는
결정론적 정수이며 다음 고정 순서로 하루 한 번 갱신한다.

1. 새 연체·default·법적 절차의 고정 event penalty 적용
2. 현재 연체일에 따른 daily penalty 적용
3. 연체가 전혀 없는 날의 recovery 적용
4. model의 min/max 범위로 clamp하고 band 경계를 다시 판정

penalty·recovery·경계값은 게시된 credit model에 있다. 같은 날 여러 계약은 계약 ID 순으로 event를
기록하되 합계 후 한 번 clamp한다. 신용 이력은 append-only이며 계약 삭제로 사라지지 않는다.

### 4.4 DSR·LTV와 대출 심사

대출 quote는 실행과 분리한다. quote는 표시용이고 만료되며, 실행 transaction은 최신 상태를 잠근 뒤
같은 심사를 다시 계산한다.

- DSR 분자는 policy가 포함하도록 선언한 모든 계약의 다음 평가기간 예정 원리금이다.
- DSR 분모는 policy가 인정하는 M3 소득 source의 확정 연소득 또는 정해진 환산소득이다.
- LTV 분자는 기존 선순위 담보 잔액, 실행할 신규 원금과 policy가 포함하는 비용의 합이다.
- LTV 분모는 해당 게임일의 정책상 인정 담보가액이다.

두 비율은 `floor(numerator × 1,000,000 / denominator)` ppm이다. 분모가 0이면 무한대로 대체하지 않고
심사를 `incomeUnavailable` 또는 `valuationUnavailable`로 거절한다. 정책은 상품·지역·주택수·신용 band별
포함 source와 상한을 versioned condition으로 제공한다. 클라이언트가 ratio나 통과 여부를 보내지 않는다.

### 4.5 M4-B 구현 경계와 개발 fixture

M4-B는 세 수직 슬라이스로 구현한다.

1. **M4-B1 catalog·schema·순수 규칙** — active credit model, typed 상품, 계약·회차·credit·tax obligation
   schema와 상환·DSR 계산
2. **M4-B2 시작·bridge·일일 상환** — 새 시작 대출 계약화, 기존 opaque debt의 read-only legacy 계약,
   정기 상환·연체 bucket·credit 일일 전이와 권위 부채 projection
3. **M4-B3 심사·명령·화면** — quote·신규 무담보 대출·조기상환, strict API, `/loans` 기능 화면과 실제
   MySQL·public HTTP 검증

M4-A 런의 immutable `run_rule_bundle`은 active credit model로 다시 pin하지 않는다. migration은 그 런의
`household.legacyDebtKrwAtActivation`을 명시적 bridge-only `legacyDebt` 계약 한 건으로 옮기되 scheduled
payment와 mutation을 만들지 않는다. 조회에는 표시하지만 `rateStatus=rateUnavailable`이고 신규 quote,
상환·조기상환은 거절한다. M4-B 배포 뒤 시작한 새 런만 active credit model을 pin한다. bridge-only 상품은
sealed credit graph의 행동 가능한 child가 아니며 public 상품 목록에 나오지 않는다.

`loan_product_version`은 active `credit_model_version`의 typed child다. active model의 strict parameters
manifest는 credit 전이값과 product 전체 terms를 함께 담고, typed child가 manifest와 정확히 일치할 때만
seal한다. 상품이나 model을 바꾸면 새 version과 새 `run_rule_bundle_assignment` revision을 게시하며 기존
런의 terms를 갱신하지 않는다.

첫 개발 model `dev-unranked-m4b-credit-2026-v1`은 실제 신용평점이 아닌 게임용 `creditUnits`를 쓴다.
범위는 `0..1000`, 시작값은 `700`이고 band는 `prime 850..1000 · standard 650..849 · limited 450..649 ·
distressed 1..449 · insolvent 0`이다. `active→delinquent` 최초 event는 `-80`, `delinquent→defaulted`는
`-300`, M4-E 법적 절차가 활성화되기 전 `legalProcedure` event penalty는 명시적 `0`이다. 연체 또는 default
계약이 하나라도 있는 날의 daily penalty는 계약 수와 무관하게 `-5`, 둘 다 없는 날의 recovery는 `+1`이다.
같은 날 event는 contract ID 순으로 기록하고 penalty 합계와 daily 변화까지 적용한
뒤 한 번 clamp한다. 가장 오래된 미납 bucket이 90일 이상이거나, 미납 합이 1,000,000원 이상이면서 가장
오래된 bucket이 30일 이상이면 `defaulted`다. 이 값은 모두 `GAME_BALANCE` provenance이며 법정·외부기관
점수로 표시하지 않는다.

첫 active 상품 terms도 법정 수치가 아닌 `GAME_BALANCE`다.

| product key | 고정 terms |
|---|---|
| `dev-student-fixed-equal-principal-2026-v1` | `studentLoan`, 시작 전용, 은행권, 고정 170bp, actual/365, `equalPrincipal`, 120개월, 매월 말일, 거치 없음, 1원~50,000,000원, 조기상환 비용 0, `reduceTerm`, DSR 포함 |
| `dev-unsecured-variable-level-payment-2026-v1` | `unsecuredLoan`, 시작·quote 실행, 은행권, treasury3m+400bp, 300~1500bp floor/cap, 매월 1일 reset 후 다음 이자구간부터 적용, actual/365, `levelPayment`, 60개월, 매월 말일, 거치 없음, 1원~200,000,000원, 조기상환 원금의 10,000ppm, `recalculatePayment`, DSR 포함 |
| `compat-legacy-debt-zero-bullet-v1` | `legacyDebt`, migration bridge 전용, 고정 0bp, `bullet`, schedule 없음, read-only, quote·상환·조기상환 금지 |

개발 model의 DSR policy는 실제 제도값과 게임 상품값을 분리한다. 2026-07-27 기준 신규 실행 뒤 일반
가계대출 총잔액이 **100,000,000원을 초과**할 때 차주단위 DSR을 적용하고, 은행권은 400,000ppm,
비은행권은 500,000ppm 이하여야 한다. `studentLoan`도 기타대출로 포함해 다음 12개월의 실제 예정 원리금을
합산한다. 분할상환 신용대출도 다음 12개월 schedule을 합산하고, bullet 신용대출만 원금 5년 환산액과
연이자를 쓴다. 인정 연소득은 M3 adapter가 반환한 양의 `verifiedAnnualIncomeKrw`와 source를 사용하며 0원·
누락이면 `incomeUnavailable`이다.

2026년 하반기 신용·기타대출 stress rate는 150bp이고 계약금리가 아니라 한도 심사용 rate에만 더한다.
기존+신규 신용대출 잔액이 100,000,000원을 초과할 때, 5년 이상 고정은 0ppm, 3년 이상 5년 미만 고정은
600,000ppm, 그 밖의 고정·변동은 1,000,000ppm을 150bp에 곱한다. DSR product inclusion과 stress profile은
policy row가 소유한다. LTV, 담보가치, 주택·지역 규칙과 주담대 stress profile은 M4-C provider가 공급하며
M4-B는 인터페이스와 `valuationUnavailable`만 고정한다.

quote와 실행은 월드의 현재 simulation date에 유효한 policy row만 고른다. 따라서
`unsecuredStressDsr2026H2`의 `effectiveFrom=2026-07-01` 전에는 기본 DSR 한도와 포함 규칙은 적용하되 stress
gate와 가산금리는 적용하지 않아 `stressRateBp=0`이다. 시행일부터는 같은 pinned policy set의 stress row를
적용하며, 같은 key의 유효 row가 둘 이상이면 fail-closed한다.

실제 credit policy row는 다음 원문을 `policy_source_document`에 기준일과 원문 SHA-256으로 연결한 별도
sealed unranked credit-only policy set에 게시하고 active `credit_model_version.creditPolicySetId`가 pin한다.
기존 finance `policy_set_assignment(newRun)`, `save.policySetId`와 M2·M3 세금 rule graph는 교체하거나 복제하지
않는다. 기존 legacy provenance policy에 credit row를 추가하지도 않는다.

- 금융위원회, [차주단위 DSR 1억원 초과·은행 40%·비은행 50%](https://www.fsc.go.kr/po040200/78428),
  2022-07-01 시행
- 금융위원회, [DSR 제외·신용대출 산정 FAQ](https://www.fsc.go.kr/po020201/76750), 2021-10-26 발표
- 금융위원회, [스트레스 DSR 도입방안](https://www.fsc.go.kr/no040101?cnId=2035), 2023-12-27 발표
- 금융위원회, [3단계 스트레스 DSR](https://www.fsc.go.kr/no010101/84617), 2025-07-01 시행
- 국가법령정보센터, [현행 은행업감독규정](https://www.law.go.kr/admRulLsInfoP.do?admRulSeq=2100000276094),
  2026-04-01 시행본

CharacterStart v2는 `startingLoans`를 최대 두 건 받고 `studentLoan → unsecuredLoan` canonical 순서, kind
중복 없음, pinned active model의 exact product ID와 kind 일치를 요구한다. fingerprint는
`lifeledger.game.start.v2`이고 product ID와 principal을 포함한다. 기존 v1 request/receipt는 v1 fingerprint를
그대로 재생하며, 새 v1 request는 게시된 legacy mapping이 있는 active model에서만 두 amount를 v2 의미로
변환한다. client preset은 금액만 제안하고 product ID를 하드코딩하지 않으며 `/api/loans/products` 결과에서
해당 kind의 starting 기본 상품을 고른다.

`GET /api/loans/products`는 캐릭터가 없으면 현재 `newRun` assignment의 world game day 0과 credit model을,
캐릭터가 있으면 그 run에 pin된 world·game day·credit model을 사용한다. 응답은
`creditModelVersionId|null · products(max 16)`이고 bridge-only·unsealed·다른 model 상품은 반환하지 않는다.
각 상품은 `id · key · displayName · kind · lenderSector · rateStatus · rateType · currentAnnualRateBp|null ·
referenceRateKey|null · spreadBp|null · minimumAnnualRateBp · maximumAnnualRateBp · rateResetRule · dayCountRule ·
repaymentMethod · termMonths · paymentCalendar · graceMonths · minimumPrincipalKrw · maximumPrincipalKrw ·
prepaymentFeePpm · prepaymentEffect · startingEligible · quoteEligible · executionEligible · prepaymentAllowed ·
dsrIncluded · provenance`을 반환한다. 동적 기준금리가 없으면 해당 상품만 `rateUnavailable`과 null 금리를
반환하며 product ID를 바꾸거나 숨기지 않는다. 상품을 읽은 뒤 assignment가 바뀌면 실행 transaction의
재검증이 그 ID를 거절하므로 client는 목록을 다시 읽는다.
M4-B active model은 `studentLoan`과 `unsecuredLoan` 각각 `startingEligible=true` 상품을 정확히 한 건
게시한다. 둘 중 하나가 없거나 복수이면 server/client 모두 임의의 첫 상품을 고르지 않고 catalog invariant
오류로 fail-closed한다. 향후 시작 상품을 복수 선택지로 열 때는 별도 `startingDefault` 권위 필드를 먼저
설계한다.

`GET /api/credit`는 별도 계산 없이 같은 transaction에서 snapshot projection을 재사용해
`creditBand|null · creditReasons(max 8) · activeLoans(max 8) · nextLoanInstallment|null ·
totalLoanBalanceKrw`를 반환한다. 캐릭터가 없거나 active model이 없는 run은 `creditBand=null`과
`modelUnavailable` reason을 반환한다. raw `creditUnits`와 내부 penalty 수치는 공개하지 않는다.

`POST /api/loans/quotes` request는 공통 `commandId · expectedRunRevision · expectedStateRevision ·
expectedGameDay`에 `productVersionId · principalKrw`만 더한 strict object다. M4-B에서는 현재 run의 pinned
active model에 속한 sealed `unsecuredLoan` 상품 중 `quoteEligible · executionEligible=true`인 exact ID만
받고 게시된 원금 범위를 다시 검증한다. fingerprint는 `lifeledger.life.loanQuote.v1`이며 cursor, product ID,
principal을 모두 포함한다. quote는 표시용 durable command라 `stateRevision`을 올리지 않지만
`command_identity · loan_quote · command_receipt`를 한 transaction에 기록한다. 같은 payload 재전송은 저장된
quote와 최신 snapshot을 반환하고 `replayed=true`로 표시하며, 같은 ID의 다른 payload는
`idempotencyConflict`다.

응답은 `{ result, replayed, snapshot }`이고 result는 `quoteId · productVersionId · requestedPrincipalKrw ·
createdGameDay · expiresGameDay · decisionCode · decisionReasons(max 8) · verifiedAnnualIncomeKrw|null ·
verifiedIncomeSource|null · existingLoanBalanceKrw · postExecutionBalanceKrw · dsrApplied · dsr|null ·
stressRateBp · quotedTerms`다. `dsr`이 있으면 `numeratorKrw · denominatorKrw · ratioPpm · limitPpm`을 모두
가지며 ratio는 원 단위 정수에서 내림 계산한 값이다. `quotedTerms`는 `annualRateBp · repaymentMethod ·
termMonths · firstInstallment{dueGameDay, feeKrw, principalKrw, interestKrw, totalKrw}`다. 변동금리 상품의
미래 기준금리를 고정된 사실처럼 보이지 않도록 전체 기간 예상 이자·납입 합계는 저장하거나 공개하지 않는다.
`existingLoanBalanceKrw`와 `postExecutionBalanceKrw`는 DSR 일반대출에 포함되는 원금 잔액 기준이고 후자는
전자와 요청 원금의 정확한 합이다. quote는 생성 game day까지만 유효해
`expiresGameDay=createdGameDay`다.

`decisionCode`는 `eligible · debtServiceLimit · incomeUnavailable · creditRestricted ·
valuationUnavailable`이다. M4-B 무담보 quote는 valuation 결정을 만들지 않는다. reason은
`activeDefault → activeDelinquency → activeRestructuring → creditBandRestricted → activeLoanLimit →
incomeUnavailable → debtServiceLimit → eligible` 고정 우선순위의 distinct code다. 신용 제한 결정은 해당
앞 다섯 reason을 모두 canonical 순서로 반환하고 DSR보다 먼저 끝낸다. 개발 model의 무담보 신규대출은
`prime · standard` band만 허용하고, `delinquent · defaulted · restructured` 계약이 있거나 활성 계약이
8건이면 `creditRestricted`다. 이 값은
`credit_model_version.parameters.loanEligibility`가 소유하는 `GAME_BALANCE` 규칙이며 실제 금융기관 기준으로
표시하지 않는다.

M3 소득 adapter는 현재 run의 `status=active` 근로계약 한 건이 있을 때만 그 immutable
`annualSalaryKrw`를 `activeEmploymentContract` source로 반환한다. `pendingStart`, 종료 계약,
지급된 현재연도 YTD, 과거 assessment와 군급여는 별도 환산·우선순위 policy가 생기기 전까지 인정하지 않고
null로 fail-closed한다. DSR gate가 적용되지 않으면 소득이 없어도 quote할 수 있고 `dsrApplied=false ·
dsr=null`이다. gate가 적용됐는데 인정 소득이 없으면 `incomeUnavailable`, `dsrApplied=true · dsr=null`이다.
소득이 있어 계산한 ratio가 한도를 넘으면 `debtServiceLimit`과 완전한 `dsr` 근거를 반환한다.

`POST /api/loans` request는 공통 command/cursor에 `quoteId`만 더한 strict object다. client가 상품 ID,
원금, 금리, 상환방식이나 심사 결과를 다시 보내지 않는다. fingerprint는
`lifeledger.life.executeLoan.v1`이고 cursor와 quote ID를 포함한다. 실행 transaction은 견적이 인증 사용자의
현재 run 소유이고 `eligible`, 생성 게임일과 현재 게임일이 같으며 아직 어떤 계약에서도 사용되지 않았는지
확인한다. 없는 견적, 다른 사용자·run의 견적, 만료·비적격 견적과 이미 사용된 견적은 존재 여부를 구분하지
않고 `contractConflict`로 거절한다. `loan_quote`는 immutable로 유지하고 소비 여부는
`loan_contract.loanQuoteId`의 유일성으로 판정한다.

유효한 견적도 저장된 심사 결과나 견적 금리를 그대로 실행하지 않는다. save·household·활성 계약을 잠근
최신 상태에서 pinned model의 상품 자격, 현재 credit band와 계약 상태, 활성 계약 상한, 인정 소득, 현재
simulation date의 DSR/stress policy, 현재 기준금리와 원금 범위를 모두 다시 계산한다. 새 결정이
`creditRestricted · incomeUnavailable · debtServiceLimit`이면 같은 이름의 stable failure code로 거절하고,
현재 금리를 해석할 수 없으면 `rateUnavailable`로 거절한다. 성공하면 현재 금리로 계약과 전체 상환표,
`loanInstallment` settlement를 만들고 원장에 `wallet +principal`과
`loanPrincipalLiability -principal`을 같은 `loanOrigination` transaction으로 기록한다. save의 현금과 권위
부채를 각각 원금만큼 올리고 debt projection을 검증한 뒤 `stateRevision`을 정확히 1 올린다. 계약·상환표·
settlement·원장·save·command identity·receipt 중 하나라도 실패하면 전부 rollback한다.

실행 응답은 `{ result, replayed, snapshot }`이고 result는 `loanId · quoteId · productVersionId ·
principalKrw · activatedGameDay · maturityGameDay · annualRateBp · repaymentMethod · termMonths ·
firstInstallment{dueGameDay, feeKrw, principalKrw, interestKrw, totalKrw}`다. 같은 command ID와 payload의
응답 유실 재시도는 저장된 result와 최신 snapshot을 반환하고 `replayed=true`이며 계약·원장·settlement와
state revision을 다시 만들지 않는다. 같은 견적을 다른 command ID로 실행하면 `contractConflict`다.
최초 성공만 snapshot을 broadcast한다.

`POST /api/loans/{loanId}/prepayments` request는 공통 command/cursor에 `principalKrw`만 더한 strict
object다. `principalKrw`는 지갑에서 꺼낼 총액이 아니라 줄일 원금이며, path의 loan ID와 함께 fingerprint
`lifeledger.life.prepayLoan.v1`에 포함한다. 현재 run의 본인 계약을 찾지 못한 경우와 다른 run·가구의 계약,
read-only·비활성·조기상환 금지 계약, 원금이 `1..remainingPrincipalKrw` 밖인 경우는 존재 여부나 현재
잔액을 구분하지 않고 `contractConflict`로 거절한다. 명령은 계약이 `active`이고 미납
`loan_obligation_bucket`과 계약의 accrued fee·interest가 모두 0일 때만 받는다. 따라서 정기 납입을
건너뛰거나 연체 상환 순서를 우회하는 경로가 없다.

조기상환 수수료는 계약에 복제된 `prepaymentFeePpm`으로
`floor(principalKrw × prepaymentFeePpm / 1,000,000)`을 i128 중간값에서 계산한다. 지갑 차감액은
`principalKrw + feeKrw`이며 현금이 부족하면 `insufficientWalletCash`다. 성공 transaction은
`manualPrepayment` payment와 0이 아닌 `prepaymentFee → prepaymentPrincipal` allocation을 순서대로
기록하고, 원장에 `wallet -(principal+fee) · loanPrincipalLiability +principal`과 수수료가 있을 때
`loanFeeExpense +fee`를 같은 `loanPrepayment` transaction으로 기록한다. save의 현금은 총 차감액만큼,
권위 부채와 계약 잔여 원금은 요청 원금만큼 줄고 debt projection을 검증한 뒤 `stateRevision`을 정확히
1 올린다.

M4-B의 이자는 지급일에 bucket으로 확정되기 전까지 일별 채무로 물질화하지 않는 schedule projection이다.
조기상환은 미납 bucket이 없는 현재 snapshot에서 아직 `pending`인 회차만 새 잔액으로 다시 계산하며,
교체된 예정 이자를 별도 상환액으로 받지 않는다. `recalculatePayment`는 기존 지급일과 남은 회차 수를
유지한 채 새 잔액·현재 계약 금리·계약 remainder로 전체 pending schedule을 다시 만들고 각 row의
`scheduleRevision`을 1 올린다. 원 단위 최소 납입 때문에 마지막 회차 전에 원금이 0이 되어 양수 opening
principal을 가진 기존 회차 수를 유지할 수 없는 일부 상환은 `contractConflict`로 거절하며, 이 경우 사용자는
잔액 전액을 상환할 수 있다. `reduceTerm`은 기존 pending 회차의 예정 원금을 회차별 상한으로 유지해
새 잔액을 앞 회차부터 배분하고, 원금이 0이 된 뒤의 연속 suffix 회차와 settlement를 `loanPrepayment`
reason으로 취소한다. 유지한 회차는 같은 지급일에 새 opening principal·이자·remainder를 기록하고
`scheduleRevision`을 1 올린다. 계약의 원래 `maturityGameDay`는 감사용 immutable 조건으로 보존하며 실제
남은 마지막 지급일은 non-cancelled schedule에서 읽는다. 전액상환은 모든 pending 회차와 settlement를
취소하고 계약을 `paidOff`, `nextInstallmentNo=null`, remainder 0으로 만든다.

조기상환 응답은 `{ result, replayed, snapshot }`이고 result는 `loanId · paymentId · principalKrw · feeKrw ·
totalDebitedKrw · appliedGameDay · remainingPrincipalKrw · status(active|paidOff) · prepaymentEffect ·
remainingInstallments · nextInstallment|null · finalInstallmentDueGameDay|null`이다. `nextInstallment`는
`installmentNo · dueGameDay · feeKrw · principalKrw · interestKrw · totalKrw`를 가진다. 같은 command ID와
payload의 재시도는 저장된 result와 최신 snapshot을 반환하고 payment·allocation·원장·schedule·state
revision을 다시 만들지 않으며, 최초 성공만 snapshot을 broadcast한다.

HTTP request는 shape로 version을 구분하는 strict union이다. v1은 기존 wrapper와 `character` 안의
`studentLoanKrw · creditLoanKrw`를 그대로 유지한다. v2는 같은 command/cursor wrapper에 top-level
`startingLoans`를 필수로 두고, `character`에서는 위 두 legacy 금액 필드를 허용하지 않는다. 각 원소는
`{ kind: studentLoan|unsecuredLoan, productVersionId, principalKrw }`뿐이다. 따라서 한 요청이 v1과 v2
필드를 섞거나 금액을 두 곳에 중복 선언할 수 없다. response/receipt shape는 두 version이 같다.

고정 protocol 이름은 settlement `loanInstallment`, settlement source `loanContract`, ledger source
`loanOrigination · loanInstallment · loanPrepayment · debtAuthorityBridge`, ledger account
`loanPrincipalLiability · loanInterestExpense · loanInterestLiability · loanFeeExpense ·
taxObligationLiability`다. 공개 경로는 §11 표를 따르고 quote·실행·조기상환은 command/cursor, immutable
fingerprint, stored result + latest snapshot replay를 사용한다. quote는 생성 게임일에만 유효하고 실행은 최신
상태에서 DSR을 다시 계산한다. `GameSnapshot.life`에는 credit band와 이유 최대 8개, 활성 loan summary 최대
8개, 다음 납입 1건, 전체 loan balance만 둔다. 전체 schedule·payment는 `before + limit(1..50)` cursor 조회다.
요약의 `creditBand`는 active model에서만 값을 갖고 bridge-only 런에서는 `null`이다. `creditReasons`는
`modelUnavailable → activeDefault → activeDelinquency → cleanHistory` 고정 우선순위의 서로 다른 code다. loan
summary는 `id · productVersionId · productKind · displayName · rateStatus · currentAnnualRateBp · status ·
remainingPrincipalKrw · overdueKrw · readOnly`를 두고, 다음 납입은 `loanId · installmentNo · dueGameDay ·
feeKrw · interestKrw · principalKrw · remainingDueKrw`를 둔다. 전체 loan balance는 남은 원금·미납
이자·미납 비용의 합이며 `save.debtKrw`에 더해지는 기타 의무는 포함하지 않는다.

`GET /api/loans/{loanId}`는 현재 인증 사용자의 현재 run에 속한 exact 계약만 조회한다. 유효한 ID지만
계약이 없거나 다른 사용자·이전 run 소유인 경우와 현재 캐릭터가 없는 경우는 모두 같은 HTTP 404
`{ code: loanNotFound, message }`로 답하고 전역 ID 선조회나 존재 여부별 메시지를 만들지 않는다. 종료 계약과
read-only legacy 계약도 소유한 현재 run의 이력이므로 조회할 수 있다. 응답은
`id · productVersionId · productKind · displayName · rateStatus · currentAnnualRateBp|null · status · readOnly ·
originalPrincipalKrw · remainingPrincipalKrw · accruedInterestKrw · accruedFeeKrw · overdueKrw · repaymentMethod ·
termMonths|null · totalInstallments|null · activatedGameDay · maturityGameDay|null ·
finalInstallmentDueGameDay|null · nextInstallmentNo|null · oldestUnpaidDueGameDay|null · prepaymentAllowed ·
prepaymentFeePpm|null · prepaymentEffect|null · dsrIncluded`의 strict object다. `forbidden`은 공개
`prepaymentEffect=null`로 정규화하며 내부 household/model/quote/command ID, interest remainder와 DB timestamp는
노출하지 않는다. `overdueKrw`는 아직 남은 delinquent bucket의 합이고 `finalInstallmentDueGameDay`는 cancelled가
아닌 회차의 최대 지급일이며 scheduled history가 없는 legacy 계약은 null이다. `prepaymentAllowed`는 현재
상환 가능 판정이 아니라 계약에 복제된 immutable capability라 status·overdue·readOnly와 함께 해석한다.

`GET /api/loans/{loanId}/installments`는 query `before? · limit?`만 받는다. `limit` 기본값과 최댓값은 50이며
범위는 `1..50`이다. schedule과 payment는 직접 부모-자식 관계가 아니고 manual prepayment에는 installment
bucket이 없으므로 한 배열에 억지로 중첩하지 않는다. 응답은 `{ loanId, installments(max 50), payments(max
50), hasMoreInstallments, hasMorePayments, nextBefore|null }`의 dual window이고 limit은 두 배열에 각각
적용한다. installment는 `installmentNo DESC`, payment는 `paymentNo DESC`이며 각각 `limit+1`을 읽어 다음
페이지 여부를 판정한다. 최초 요청 이후 새 payment가 생겨도 exclusive high-water mark 아래만 읽어 중복하지
않는다.

`before`는 client가 해석하지 않는 canonical ASCII token
`v1.l{loanId}.i{installmentBefore}.p{paymentBefore}`다. 모든 수는 leading zero 없는 decimal이고 0은 해당
window가 끝났다는 sentinel이다. token의 loan ID는 path와 같아야 한다. `nextBefore`는 더 읽을 window가
하나라도 있을 때만 있고, 계속 읽을 window에는 이번 응답의 마지막 no를 exclusive before로, 끝난 window에는
0을 넣는다. malformed/다른 loan token, unknown query와 범위 밖 limit은 HTTP 400 `invalidCommand`다.

installment는 `id · installmentNo · dueGameDay · interestPeriodStartGameDay · elapsedDays · annualRateBp ·
openingPrincipalKrw · scheduledFeeKrw · scheduledInterestKrw · scheduledPrincipalKrw · paidFeeKrw ·
paidInterestKrw · paidPrincipalKrw · remainingDueKrw · status · scheduleRevision`을 가진다. payment는 `applied`
이력만 공개하고 `id · paymentNo · kind(scheduledInstallment|manualPrepayment|leaseMovePayoff|
propertySalePayoff|insolvencyDistribution) · gameDay · amountKrw ·
allocations(max 8)`을 가진다. allocation은 같은 kind를 합친 `kind · amountKrw`만 공개하며 순서는
`overdueFee → overdueInterest → overduePrincipal → currentFee → currentInterest → currentPrincipal →
prepaymentFee → prepaymentPrincipal`이다. payment amount와 allocation 합은 같아야 한다. bucket/allocation,
command와 ledger ID, timestamp는 공개하지 않는다. 각 배열의 정렬·합계·한도나 소유권 검증이 깨지면
부분 응답을 만들지 않고 fail-closed한다.

M4-B부터 `save.debtKrw` projection은 active loan의 원금·미납 이자·미납 비용, active
`essentialArrear`, outstanding `tax_obligation`의 합이다. M4-C 이후 lease·담보 의무를 더한다. M4-B migration은
최소 `tax_obligation` authority를 만들고 annual·employment tax 부족액이 aggregate를 직접 늘리는 경로를
없앤다. 모든 부채 변경 transaction은 이 권위 합을 다시 계산해 projection과 정확히 대조한다.

## 5. 부동산·임대차·담보대출

### 5.1 지역 지수와 매물

M4는 실제 주소 대신 M4-A에서 이미 고정한 `capitalArea · metropolitan · smallCity · rural` region key와
가상 주거 자산을 쓴다. 부동산 모델은 같은 `life_region` 행을 참조하며 별도의 비슷한 지역 enum을 만들지
않는다.
`real_estate_daily`는 `(modelVersion, worldSeed, regionKey, gameDay)`로 결정되는 지역 가격·임대료 지수다.
M1 시장과 같이 과거 경로는 불변 캐시하며, entropy는 counter 기반이라 조회 순서와 worker 수에 무관하다.

매물은 플레이어가 검색한 시점에 무한 랜덤 생성하지 않는다. 월 첫날마다 각 월드·지역의 게시 카탈로그에
대해 `(worldSeed, yearMonth, regionKey, slot)`로 정해진 유한 `property_listing`을 준비한다. 가격·면적·유형,
전세/월세 조건, 거래 가능 기간과 listing ID가 고정된다. 같은 월을 다시 조회해도 같다.

### 5.2 매수·매도와 소유권

매수 명령은 listing, 취득 부대비용, 자기자금, 선택한 mortgage quote를 한 transaction에서 처리한다.
담보대출이 실패하면 부동산만 취득되지 않는다. 원장에는 토지·건물 구분 대신 M4 단순 자산 계정 하나를
쓰되 취득가, 부대비용, policy set과 보유 목적을 `property_holding`에 보존한다.

매도는 즉시 체결 명령이 아니라 `property_sale_order`를 만든다. 체결 후보일은
`(worldSeed, propertyId, orderRevision)` entropy와 지역 유동성 카탈로그로 한 번 정해 저장한다.
가격 변경·취소는 revision을 올리며 기존 후보를 재사용하지 않는다. 체결일에는 소유권, 임대차, 담보,
세금을 다시 검증하고 다음 순서로 proceeds를 배분한다.

`거래비용 → 담보권 대출 상환 → 매도 관련 세금 → 임차보증금 반환 의무 → 지갑 잔액`

부족하면 임의로 음수 지갑을 만들지 않고 매도를 거절하거나 policy가 허용한 부족채무 계약을 함께 만든다.
어느 경로인지는 listing이 아니라 policy의 exact
`deficientSaleProceeds: reject | createDeficiencyLoan { productVersionId }` 규칙이 명시한다. 두 번째 분기는
그 상품의 자격·금리·상환표와 실행 후 cursor까지 같은 transaction에서 검증하며 실패하면 매도 전체를
rollback한다.

### 5.3 임대차

`lease_contract`는 `tenant|landlord`, `depositKrw`, 선택적 `monthlyRentKrw`, 기간, 지급일, 갱신 규칙,
연결 property와 deposit loan을 가진다. 임차보증금은 소비가 아니라 반환받을 권리 자산이고, 임대인이 받은
보증금은 반환 의무 부채다. 두 금액은 순자산에 각각 포함한다.

입주 transaction은 기존 거주 계약 종료, 보증금 회수/반환, 새 보증금 지급, 대출 실행, 이사비와 residence
변경을 원자적으로 처리한다. 계약이 겹치는 특별 상품이 아니면 한 household에 활성 거주지 하나만 허용한다.
월세는 생활비보다 먼저 별도 주거 정산으로 처리하고 미납은 `leaseArrear`가 된다. 강제퇴거 같은 세부 법률
절차는 M4 범위 밖이며, 일정 연체 후 계약 종료 제안이 생기는 게임 상태만 카탈로그로 제공한다.

### 5.4 부동산 세금

취득·보유·임대·양도 관련 세목과 공제·주택수 판정은 `policy_rule`의 typed 계산 규칙이다. 문서에 실제
세율·금액·보유기간을 하드코딩하지 않는다. 각 세금 계산은 원 단위 독립 내림, 적용 rule ID, 과세표준,
세액과 납부 게임일을 `property_tax_event`에 기록한다.

보유세는 policy의 평가일에 holding과 가구 주택수를 pin한 뒤 납부일 settlement를 예약한다. 양도세는
체결 transaction 안에서 취득원가·허용비용·보유기간·주택수 pin을 읽어 계산한다. 이미 끝난 매매의 세금을
새 policy 버전으로 소급 재계산하지 않는다.

### 5.5 M4-C 구현 경계와 첫 개발 fixture

M4-C는 `C1 지역 지수·매물 조회 → C2 임대차·이사 → C3 매수·담보대출 → C4 매도·부동산 세금` 순서로
구현한다. 다음 단계가 앞 단계의 종료 상태를 실제로 소비하게 하며, 아직 실행할 수 없는 매수·임대차 버튼이나
가짜 성공 응답을 먼저 만들지 않는다. 첫 수직 슬라이스 C1은 **결정론적 지역 지수와 월별 유한 매물을 실제
DB에 만들고 `/housing`에서 조회하는 것**까지다. 계약·원장·`residence`를 바꾸는 명령은 C2부터 연다.

C1의 활성 모델 key는 `dev-unranked-m4-real-estate-2026-v1`이고 ranked에서 사용할 수 없는 개발 fixture다.
`real_estate_model_version.parameters`를 런타임 규칙 저장소로 직접 읽지 않는다. 지역별 지수 입력과 매물
profile은 model version을 부모로 둔 typed immutable row이며, canonical manifest가 모든 child를 고정 순서로
포함해야 model을 seal할 수 있다. 기존 런에 pin된 `disabled-m4a-v1`은 그대로 남고 `newRun` assignment만
완성된 C1 migration의 마지막에 활성 모델로 이동한다. 따라서 기존 M4-A·B 런은 영구
`rateUnavailable`이고 새 모델이나 매물을 뒤늦게 받지 않는다.

첫 fixture의 공통 index 하한·상한은 `500,000ppm · 2,000,000ppm`, 매물 가격 variation은
`850,000..1,150,000ppm`, 월세 보증금은 매매가의 `100,000ppm`이다. 나머지 typed profile 값은 다음과
같다. 이 수치는 법정·실측 통계가 아니라 플레이테스트용 가상 시장 입력이다.

| region | 월 slot | 면적 범위(㎡) | 기준 매매가/㎡ | 가격 drift/shock(ppm/일) | 임대 drift/shock(ppm/일) | 전세가율(ppm) | 연 gross rent yield(ppm) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `capitalArea` | 12 | 30..120 | 10,000,000원 | 80 / 1,200 | 50 / 500 | 550,000 | 35,000 |
| `metropolitan` | 12 | 35..135 | 5,000,000원 | 60 / 1,000 | 40 / 400 | 600,000 | 42,000 |
| `smallCity` | 12 | 40..160 | 3,000,000원 | 40 / 800 | 30 / 350 | 650,000 | 48,000 |
| `rural` | 12 | 50..200 | 1,500,000원 | 20 / 600 | 20 / 300 | 600,000 | 55,000 |

property type은 지역 profile의 허용 enum 안에서 별도 entropy로 고르되 `capitalArea`는
`apartment|multiFamily`, `metropolitan|smallCity`는 세 종류 전부, `rural`은
`multiFamily|detached`만 허용한다. slot을 1부터 세어 `((slot - 1) mod 3)`이 0·1·2이면 각각
`sale · jeonse · monthlyRent` offer 하나를 만든다. 이 고정 회전은 모든 지역·월에 각 거래 종류가 네 건씩
있게 하며, 이후 fixture가 복수 offer를 쓰더라도 공개 canonical 순서는 바꾸지 않는다. listing 유효기간은
해당 시장 월의 첫 game day부터 마지막 game day까지다.

면적과 variation은 각각 독립 entropy를 rejection-sampling bounded integer로 바꿔 inclusive 범위에서 고른다.
`baseKrw = area × basePricePerSquareMeter`, `V = variationPpm`, 월 첫날의 가격·임대 index를 `P · R`이라 하면
금액은 다음 i128 식을 각각 한 번 나누어 0원 방향으로 내린다.

- `salePriceKrw = floor(baseKrw × V × P / 1,000,000²)`
- `jeonseDepositKrw = floor(salePriceKrw × jeonseRatioPpm / 1,000,000)`
- `monthlyDepositKrw = floor(salePriceKrw × 100,000 / 1,000,000)`
- `monthlyRentKrw = floor((rentValuationKrw - monthlyDepositKrw) × annualGrossRentYieldPpm /
  (12 × 1,000,000))`

`rentValuationKrw <= monthlyDepositKrw`이거나 어떤 공개 금액도 1원 미만이면 게시 profile 또는 생성 invariant
오류다. 다른 offer를 만들기 위해 필요 없는 금액까지 계산해 실패시키지 않고 해당 slot의 offer 식만
평가한다.

지역별 가격·임대료 지수는 game day 0의 `1,000,000ppm`에서 시작한다. 게시 profile은 일간 drift ppm,
shock 진폭 ppm, 하한·상한 ppm과 매물 가격·임대 조건 산정 입력을 가진다. day `d > 0`의 변화량은
`(worldSeed, modelVersionId, regionKey, d, price|rent)` counter entropy에서 독립적으로 만들고,
`previousIndex × (1,000,000 + drift + shock) + previousRemainder`를 i128에서 계산해 1,000,000으로 나눈다.
몫을 새 index, 나머지를 같은 series의 signed remainder로 보존한 뒤 profile 하한·상한을 적용한다. 하한이나
상한에 닿으면 remainder는 0으로 초기화한다. `real_estate_daily`는 world·model·region·game day와 두 index,
두 remainder를 보존하며 과거 행은 immutable이다. store는 region series cursor를 잠그고 빠진 날을 game day
순서로 채우므로 조회 순서, 재시작과 worker 수가 결과를 바꾸지 않는다. 미래 game day는 준비하지 않는다.

월별 매물은 `(worldSeed, modelVersionId, yearMonth, regionKey, slot)` entropy로 만든다. profile은 지역별
slot 수, 주택 유형, 전용면적 범위, 면적당 기준가, 매매가 대비 전세보증금 ppm, 월세 보증금·전환 입력,
게시 game day 범위를 typed 값으로 가진다. 금액은 해당 월 첫 game day의 price/rent index만 사용하며 모두
원 단위에서 내린다. C1의 주택 유형은 `apartment · multiFamily · detached` 세 enum이고 면적은 정수
`exclusiveAreaSquareMeters`로 공개한다. 각 listing은 `sale`, `jeonse`, `monthlyRent` 중 profile이 허용한
offer를 canonical 순서로 하나 이상 가지며, offer는 다음 strict shape다.

- `sale`: `{ kind, priceKrw }`
- `jeonse`: `{ kind, depositKrw }`
- `monthlyRent`: `{ kind, depositKrw, monthlyRentKrw }`

listing ID는 생성·조회 순서에 영향을 받는 auto increment를 공개 식별자로 쓰지 않는다. 위 entropy key에서
파생한 non-zero 63-bit ID를 사용하고, 같은 ID가 다른 canonical key와 충돌하면 새 난수로 조용히 대체하지
않고 invariant 오류로 실패한다. 행은 `(world, model, yearMonth, region, slot)`으로도 unique하며 생성 뒤
update/delete하지 않는다. 매물 생성은 현재 월의 필요한 지역만 준비한다. 최초 생성 request는
`(world, model, yearMonth, region)` month catalog 행을 멱등 생성한 뒤 잠그고, 모든 expected listing과 각
offer를 저장·재검증한 경우에만 catalog를 단방향 complete로 바꾼다. 따라서 같은 월·지역의 동시 request도
부분 catalog를 노출하거나 unique-key deadlock으로 끝나지 않고 한 결과로 수렴한다. C1에서는 매수·임대차가
없으므로 `available` 행만 반환하고 과거 월 매물 history API는 열지 않는다.

`GET /api/housing/listings`는 query `region?`만 받는다. 생략하면 현재 active residence의 region이고, 값은
위 네 `life_region` key 중 하나여야 하며 unknown query·enum은 HTTP 400 `invalidCommand`다. 응답은 다음
strict object다.

`{ rateStatus, modelVersionId, gameDay, yearMonth, residenceRegionKey, selectedRegionKey, regions,
priceIndexPpm|null, rentIndexPpm|null, listings }`

`rateStatus`는 `active|rateUnavailable`이고 `yearMonth`는 기존 생활비 계약과 같은 strict
`{ year, month }` object다. `gameDay`와 `yearMonth`는 인증 run의 같은 market date를 가리켜야 한다.
`regions`는 `regionKey · displayName`을 `life_region.regionOrder` 순서로 최대 4개 반환한다. listing은
`id · regionKey · propertyType · exclusiveAreaSquareMeters · availableFromGameDay ·
availableToGameDay · offers(max 3)`를 가지며 slot 순서로 최대 24개다. active model은 두 index가 non-null이고
선택 지역의 현재 유효 매물을 반환한다. disabled compatibility model은 성공 응답을 만들되
`rateStatus=rateUnavailable`, 두 index `null`, `listings=[]`로 고정한다. 어떤 응답도 world seed, entropy,
profile 원시 입력, DB timestamp를 공개하지 않는다. 배열 상한·정렬·지역·offer 합계 불변식이 깨지면
부분 응답을 만들지 않고 fail-closed한다. 인증은 됐지만 현재 캐릭터·run이 없으면 HTTP 409
`characterRequired`이고 전역 assignment만 읽어 임의 모델을 보여주지 않는다.

C1의 `/housing` 화면은 현재 거주 지역, 조회 지역 선택, 현재 game day·월, 가격·임대료 지수와 매물별 면적·
offer만 표시한다. mount에서 최대 24개 행을 한 번 만들고 hooks로 내용과 hidden 상태만 갱신한다. 매수·임차·
이사 form은 해당 명령을 구현하는 C2·C3에서 추가한다. 화면은 매매가·보증금·월세를 다시 계산하지 않는다.

C1 단위 테스트는 지수 remainder와 상·하한, entropy stream 독립성, 같은 key의 byte-identical 매물,
지역·월·slot 변경, canonical offer와 금액 내림을 순수 규칙에서 검증한다. protocol 테스트는 strict query,
active/disabled response와 bounded 배열을 검증한다. 실제 MySQL 8에서는 빈 DB와 M4-B DB의 전진 migration,
기존 run pin 보존, 새 run의 active pin, 동일 월 반복·재시작 조회, 다음 달 전진 뒤 새 매물, daily/listing
unique·immutable trigger와 public HTTP `/housing` 조회를 확인한다. 조회만으로 player 원장·state revision·
`residence`가 바뀌지 않아야 한다.

### 5.6 M4-C2 임대차·이사 구현 경계와 첫 fixture

M4-C2는 한 transaction에 월세·연체·대출까지 한꺼번에 넣지 않고 다음 순서로 완성한다.

1. **C2a 현금 전세·원자적 이사** — tenant 전세 계약, 기존 보증금 반환, 새 보증금 지급, 이사비,
   residence 교체와 보증금 자산
2. **C2b1 월세 핵심** — open-ended 월세 계약, phase 300 청구, `leaseArrear`와 수동 상환
3. **C2b2 계약 lifecycle** — 고정기간, 갱신 안내와 연체 기반 계약 종료 검토 상태
4. **C2c 전세자금대출** — `leaseDepositLoan` 상품, DSR·한도·직접 보증금 지급과 이동 transaction 결합

C2a는 `POST /api/housing/leases` 하나가 listing offer 선택과 입주를 함께 수행한다. 별도
`POST /housing/moves`는 임대차와 독립된 소유주택 간 이동 같은 실제 사용례가 구현될 때까지 열지 않는다.
월세 offer는 C1처럼 조회되지만 C2a 명령으로 실행할 수 없고, 성공하는 척하는 버튼이나 응답도 만들지 않는다.

#### C2a 모델·매물·기간 의미

C1의 sealed 모델에는 이사비와 계약 lifecycle 입력이 없으므로 행을 추가하지 않는다. 새 모델 key
`dev-unranked-m4-real-estate-lease-2026-v2`는 C1의 네 지역 profile과 허용 property type을 새 immutable
부모 아래 정확히 복제하고, typed cash-jeonse lease profile과 지역별 이사비를 manifest에 함께 seal한다.
`newRun` assignment만 v2로 이동하며 기존 C1 v1 run은 매물 조회만 계속하고 lease capability는
`unavailable`이다. 기존 `disabled-m4a-v1` run도 그대로 보존한다.

C2a의 이사비는 목적지 지역 기준 `capitalArea 800,000원 · metropolitan 600,000원 · smallCity 450,000원 ·
rural 300,000원`이다. 법정 비용이나 실측 평균이 아니라 `GAME_BALANCE` 개발 fixture이며, 서버는 listing의
지역으로 선택하고 client가 보낸 금액을 받지 않는다. cash-jeonse는 보증금 전액을 지갑과 기존 전세보증금
반환액으로 충당해야 한다. 전세자금대출, 월세, 중개수수료, 보증금 일부납부는 C2a 범위 밖이다.

첫 계약은 `renewalRule=openEnded`다. `effectiveFromGameDay`부터 player가 다음 임대차로 이동할 때까지
유효하고 자동 만료·갱신·임대료 변경이 없다. 중도 이동은 언제든 위약금 없이 가능하며 기존 보증금을 전액
반환한다. 이는 C2a에서 만료 planner를 임시로 흉내 내지 않기 위한 명시적 게임 규칙이고, C2b2는 새 모델
version으로 기간·갱신 규칙을 추가한다. 종료 game day는 exclusive다. 현재 residence가 시작된 game day와
같은 날에는 다시 이동할 수 없고 `contractConflict`다. 따라서 day 0 bridge residence의 첫 이동은 day 1부터
가능하며 한 game day에 길이 0인 residence나 lease history를 만들지 않는다.

C1 listing 하나는 그 달에 발견된 장기 property identity 자체다. C2a에서는 `propertyId=listingId`로 쓰고
listing 기간이 끝나도 체결된 lease가 그 immutable 행을 계속 참조한다. 매물은 전역 품절 자원이 아니다.
다른 run은 같은 shared listing을 독립적으로 선택할 수 있어 player나 worker 실행 순서가 랭킹 결과를 바꾸지
않는다. listing을 계약해도 C1 cache를 update/delete하지 않는다.

#### C2a 계약·회계 transaction

`lease_contract`는 current run의 tenant 계약이며 listing/model, household, 지역, property type, 면적,
`jeonse` offer, 보증금, `openEnded` 규칙, 시작·종료 game day를 복제한다. 금액과 조건은 체결 뒤 immutable이고
활성 계약은 종료 game day만 null에서 현재 game day로 한 번 닫을 수 있다. 한 household에는 active tenant
lease와 active residence가 각각 최대 한 건이다. 새 `residence`는 lease ID를 참조하고 tenure가 `jeonse`여야
하며, 기존 `rentFree|jeonse` residence만 C2a에서 교체한다. 기존 lease와 residence의 owner/run/listing/금액
연결이 하나라도 맞지 않으면 부분 복구하지 않고 fail-closed한다.

이동 transaction은 save, household, 현재 residence·lease와 listing을 잠근 뒤 다음을 원자적으로 수행한다.

1. 기존 jeonse lease가 있으면 종료하고 `leaseDepositAsset -기존보증금 · wallet +기존보증금`을 기록한다.
2. `wallet -새보증금 · leaseDepositAsset +새보증금`으로 새 tenant 보증금 자산을 만든다.
3. `wallet -이사비 · movingExpense +이사비`를 기록한다.
4. 기존 residence를 exclusive 종료하고 새 lease와 jeonse residence를 같은 game day부터 시작한다.
5. 지갑·보증금 자산·원장·residence projection을 대조한 뒤 `stateRevision`을 정확히 1 올리고 receipt를
   저장한다.

지갑 변화는 항상 `returnedDepositKrw - depositKrw - movingCostKrw`다. transaction 안에서 기존 보증금을
먼저 반환한 뒤의 가용액이 새 보증금과 이사비 합보다 작으면 `insufficientWalletCash`이고 아무 행도 남기지
않는다. tenant 보증금은 소비가 아니라 반환받을 자산이므로 top-level `netWorthKrw`는 이동 전후 다른 자산과
부채가 같다면 이사비만큼만 감소한다. `save.debtKrw`는 C2a에서 바뀌지 않는다.

이미 확정된 `living_cost_month`는 이사 뒤에도 다시 계산하지 않는다. `GameSnapshot.life.residence`는 즉시
새 지역·tenure를 가리키지만 `currentMonth`의 입력과 금액은 기존 fingerprint를 유지한다. 다음 달을 처음
확정할 때부터 새 residence 지역과 `jeonse` tenure replacement를 사용한다.

ledger source는 `leaseMove`, 계정은 `leaseDepositAsset · movingExpense`를 추가한다. 한 source transaction의
posting 합은 0이고 0원 posting은 만들지 않는다. `GameSnapshot.life`는
`tenantLeaseDepositKrw`와 `activeLease|null`을 추가한다. active lease snapshot은
`id · listingId · role(tenant) · offerKind(jeonse) · regionKey · propertyType ·
exclusiveAreaSquareMeters · depositKrw · monthlyRentKrw(null) · effectiveFromGameDay ·
effectiveToGameDay(null) · renewalRule(openEnded)`만 공개한다. 내부 household/model/command/ledger ID와
timestamp는 공개하지 않는다. `tenantLeaseDepositKrw`는 active lease의 보증금과 정확히 같고 없으면 0이다.

#### C2a HTTP와 idempotency 계약

`GET /api/housing/leases/current`는 다음 strict object를 반환한다.

`{ leaseCapability, renewalRule|null, movingCosts, tenantLeaseDepositKrw, activeLease|null }`

`leaseCapability`는 `cashJeonse|unavailable`이다. v2는 `cashJeonse`, C1 v1과 disabled 모델은
`unavailable`이며 이때 `renewalRule=null · movingCosts=[] · tenantLeaseDepositKrw=0 · activeLease=null`이다.
v2의 `renewalRule`은 `openEnded`, `movingCosts`는 `regionKey · movingCostKrw`를 region order로 정확히 네
건 반환한다. 조회는 owner/current-run scope이고 player state나 shared listing cache를 바꾸지 않는다.

`POST /api/housing/leases` request는 공통 command/cursor에 canonical decimal `listingId`와
`offerKind=jeonse`만 더한 strict object다. 지역·면적·보증금·이사비·계약 기간은 받지 않는다. fingerprint는
`lifeledger.life.startLease.v1`이고 cursor, listing ID와 offer kind를 포함한다. listing은 현재 run에 pin된
world/model의 현재 월에 속하고 현재 game day가 공개 기간 안이어야 한다. sale·monthlyRent listing, 다른
model이나 과거·미래 월 listing, 같은 날 두 번째 이동과 이미 활성인 같은 listing은 모두
`contractConflict`로 정규화한다.

POST는 같은 client가 먼저 GET을 호출했다고 가정하지 않는다. player transaction에 들어가기 전에 현재
world/model/month의 네 지역 catalog를 C1의 공유 header 잠금 순서로 멱등 준비한다. 그 뒤 save를 잠그고
cursor·현재 날짜와 listing 소속을 다시 검증한다. stale 명령이 준비한 shared immutable catalog는 남을 수
있지만 player 계약·원장·revision은 만들지 않으며, player 잠금과 shared catalog 잠금 순서를 뒤섞지 않는다.

성공 response는 `{ result, replayed, snapshot }`이다. result는
`leaseId · residenceId · listingId · offerKind · regionKey · propertyType · exclusiveAreaSquareMeters ·
depositKrw · returnedDepositKrw · movingCostKrw · walletDeltaKrw · effectiveFromGameDay ·
endedLeaseId|null · renewalRule`을 가진다. result의 새 lease와 `snapshot.life.activeLease`,
`tenantLeaseDepositKrw`, residence, 지갑과 net worth가 서로 맞아야 한다. 같은 command ID와 payload의
응답 유실 재시도는 저장된 result와 최신 snapshot을 반환하고 계약·residence·원장·revision을 다시 만들지
않는다. 같은 ID의 다른 payload는 `idempotencyConflict`다.

malformed ID·unknown field·jeonse가 아닌 request enum은 400 `invalidCommand`, 캐릭터가 없으면 409
`characterRequired`, capability가 없으면 409 `rateUnavailable`, stale cursor·listing/현재 계약 충돌은 409
`contractConflict`, 현금 부족은 409 `insufficientWalletCash`, transient lock exhaustion은 409 `busy`다.
다른 사용자·이전 run의 lease ID는 current snapshot이나 result로 노출하지 않는다. 모든 경로는 session
cookie를 요구한다.

C2a `/housing`은 현재 lease·보증금 자산, 지역별 이사비와 현재 월의 jeonse listing만 선택 가능한 기능 form을
추가한다. 실행 전 `반환 보증금 + 지갑`과 `새 보증금 + 이사비`를 서버 응답 값으로 보여주되 최종 판정은
서버가 한다. capability가 unavailable이면 form을 숨기고 매물 조회는 유지한다. 성공 result와 최신 snapshot을
store에 반영하고 outcome-unknown 재시도는 같은 path/body를 사용한다. DOM은 한 번 만들고 hooks로 노드만
갱신하며 CSS, DOM·라우팅·실제 network 테스트를 추가하지 않는다.

C2a 단위 테스트는 cash/deposit/moving-cost 원장 plan, 기존 전세 교체, 순자산 보존, overflow와 당월 생활비
불변을 검증한다. protocol 테스트는 strict request/result, current lease capability, correlation과 bounded
배열을 검증한다. 실제 MySQL 8에서는 빈 DB·C1 DB 전진, v1 run 보존과 v2 newRun pin, day 1 첫 이동,
전세→전세 이동, replay·stale·잔액 부족 rollback, 원장 합과 지갑·보증금·residence projection, 다음 달 생활비
지역·tenure 반영, 재시작 뒤 동일 snapshot을 public HTTP로 확인한다.

#### C2b1 범위와 월세 model

C2b는 월세의 현금흐름과 계약 lifecycle을 한 migration에 섞지 않는다. C2b1은 open-ended 월세 계약,
월세 청구·연체와 수동 상환까지만 완성한다. C2b2는 별도 새 model version에서 고정기간, 갱신 안내,
연체 기반 계약 종료 검토 상태를 추가한다. C2b1은 강제퇴거, 보증금과 연체 상계, 월세 자동 인상, 연체 자동
상환을 구현하지 않는다.

C2b1 활성 model key는 `dev-unranked-m4-real-estate-rent-2026-v3`이다. C2a v2의 지역 profile 4개, 허용
property type 10개, 현금 전세 profile과 이사비를 정확히 복제하고 typed monthly-rent profile을 더한다.
새 profile은 `offerKind=monthlyRent · renewalRule=openEnded · rentChargeRule=nextMonthStartFull ·
arrearRepaymentRule=manualOnly`다. 이는 법정 임대차 규칙이 아니라 정산 순서와 재현성을 먼저 검증하는
`GAME_BALANCE` fixture다. v2와 v1 row·manifest·run pin은 바꾸지 않고 `newRun` assignment만 sealed v3로
옮긴다. strict projection의 v1·v2 JSON은 byte-identical이어야 하며 v3만 schema version 3과 monthly-rent
profile을 포함한다. view를 교체한 같은 MySQL connection에서 오래된 plan을 재사용하지 않도록 manifest
insert trigger는 새 projection 뒤에 다시 생성한다.

lease capability는 `unavailable · cashJeonse · cashJeonseAndMonthlyRent` 세 값이다. v1은 unavailable, v2는
cashJeonse, v3는 현금 전세와 월세 모두 가능하다. `POST /api/housing/leases`의 `offerKind`는 v3에서
`jeonse|monthlyRent`를 받고 listing에 실제 존재하는 정확한 offer만 실행한다. 월세 입주는 기존 보증금 전액
반환, 새 월세보증금 전액 지급, 목적지 이사비와 residence 변경을 C2a와 같은 transaction에서 처리한다.
첫 월세는 입주 transaction에서 받지 않는다. 현재 달 생활비 pin은 그대로 두고 다음 달부터 residence tenure가
`monthlyRent`가 된다. 기존 월세 연체는 이사 때 보증금에서 임의 상계하지 않고 typed 채무로 남으며, 기존
보증금은 전액 반환한다.

월세 lease도 C2b1에서는 `openEnded`다. 계약은 profile의 `rentChargeRule`과 `arrearRepaymentRule`을
nullable typed 열에 복제해 이후 model 변경과 무관하게 당시 조건을 보존한다. active lease snapshot은
offer별 shape를 엄격히 지킨다. 전세는
`monthlyRentKrw=null · nextRentDueGameDay=null`, 월세는 양의 `monthlyRentKrw`와 다음 청구일을 가진다.
월세 residence만 monthly-rent lease를, 전세 residence만 jeonse lease를 참조해야 한다. 동일 game day 재이사,
다른 world/model/month의 listing, capability에 없는 offer와 잔액 부족은 C2a와 같은 원자적 실패 경계를 쓴다.

#### C2b1 월세 청구와 phase 300

월세는 입주일이 속한 달을 일할 계산하지 않는다. 첫 청구일은 입주 game day보다 엄격히 뒤에 있는 첫 시장
월 1일이고 이후 매월 1일에 listing의 고정 월세 전액을 선불로 청구한다. 월중 입주는 다음 월 1일까지 월세가
없고, 월초 청구 뒤 월중 이사해도 이미 낸 금액을 환급하지 않는다. 이 단순 규칙은 C2b1의 명시적 fixture이며
C2b2의 기간 model이 바꾸려면 새 version을 게시한다.

입주 transaction은 첫 `lease_rent_charge`와 그 `scheduled_settlement`를 미래 청구일에 미리 만든다. 정산이
성공하면 같은 transaction에서 다음 달 charge와 settlement를 한 건만 예약한다. 월세 lease가 중도 종료되면
아직 due가 되지 않은 다음 charge와 settlement를 취소한다. 따라서 due 당일 validation 전에 새 payload를
뒤늦게 만들지 않으며 재시작·retry에도 `(leaseContractId, chargeNo)` 하나로 수렴한다. strict payload는
`{ version:1, leaseContractId, rentChargeId, chargeNo }`, settlement kind는 `leaseRent`, source는
`leaseContract`, occurrence는 charge number다.

due settlement 순서는 기존 M2·M3 `100 → loanInstallment 200 → leaseRent 300 → livingCostMonth 400`이다.
경제적 실행 순서와 DB 잠금 순서를 섞지 않기 위해 due lease ID를 오름차순으로 먼저 잠그고 기존 loan과
scheduled-settlement 잠금 뒤 phase 순서로 분개한다. 월세 정산은 현재 charge만 처리하며 과거 lease arrear를
자동 상환하지 않는다. 지갑이 청구액보다 적으면 `paid=min(wallet,rent)`이고 남은 전액을 한
`lease_arrear`로 만든다. 지갑은 음수가 되지 않는다.

분개는 `leaseRentExpense +청구액 · wallet -실지급액 · leaseArrearLiability -미납액`이고 0원 posting은
만들지 않는다. source는 `leaseRent`, posting은 정확한 rent charge와 arrear를 참조한다. charge·settlement·
원장·지갑·연체·`save.debtKrw`와 다음 charge 예약은 하루의 단일 player transaction에 속한다. phase 300이나
뒤 phase가 실패하면 그 날 전체를 rollback한다. 권위 부채 projection은
`loan + essentialArrear + leaseArrear + taxObligation`이고 모든 기존 debt validation이 이 합을 사용한다.

#### C2b1 연체 조회·수동 상환과 HTTP

`lease_arrear`는 rent charge당 최대 한 건이며 `originalKrw · paidKrw · remainingKrw · active|paid`를
보존한다. 수동 지급은 `lease_arrear_payment`의 prepared→applied 상태와 `leaseArrearPayment` 원장 source를
쓴다. 분개는 `wallet -지급액 · leaseArrearLiability +지급액`이다. 1원 이상 현재 잔액 이하만 허용하고 지갑이
부족하면 `insufficientWalletCash`, 없는·다른 사용자·이전 run·이미 완납한 ID와 초과액은 동일
`contractConflict`로 거절한다. 과거 계약의 연체도 current run 소유라면 상환할 수 있다.

`GET /api/housing/leases/current`는 기존 필드에 `monthlyRentTerms|null · activeArrears(max 20) ·
hasMoreActiveArrears · totalLeaseArrearKrw`를 더한다. v3의 monthlyRentTerms는 charge·상환 rule만 공개하고
model 원시 입력을 노출하지 않는다. arrear는 due month와 charge·lease ID, 원금·지급·잔액·생성 game day를
오래된 순으로 반환한다. `GameSnapshot.life`도 같은 bounded arrear window와 total을
`activeLeaseArrears · hasMoreActiveLeaseArrears · totalLeaseArrearKrw`로 가지며, 기존 필수 생활비
`activeArrears`와 이름을 섞지 않는다. window 합은 `hasMore=false`일 때만 total과 같아야 한다.

월세 입주 성공 result는 C2a result에 `monthlyRentKrw`를 추가한다. 전세는 null, 월세는 요청 listing offer와
같은 양의 금액이다. 수동 상환은 `POST /api/housing/lease-arrears/{arrearId}/payments`에 공통 command/cursor와
`amountKrw`를 받고 `{ result:{ arrearId,paymentId,paidKrw,remainingKrw }, replayed, snapshot }`을 반환한다.
fingerprint는 path ID·amount·최초 cursor를 포함한다. 같은 body replay는 저장된 result와 최신 snapshot을,
같은 ID의 다른 payload는 `idempotencyConflict`를 반환하고 실패 command는 payment·원장·receipt를 남기지
않는다. 모든 경로는 session cookie, current-run owner scope, canonical decimal path ID와 strict JSON을 쓴다.

C2b1 `/housing`은 전세와 월세 offer를 구분해 보증금·이사비·다음 달부터 적용될 월세를 표시하고, 서버가
반환한 최대 20개 연체에 일부·전액 상환 기능을 제공한다. outcome-unknown 입주·상환은 각각 원래 path/body를
보존한다. DOM은 mount에서 한 번 만들고 hooks로 노드만 갱신하며 CSS, DOM·라우팅·실제 network 테스트를
추가하지 않는다.

C2b1 단위 테스트는 월세 현금 배분, 0원 posting 제거, 연체·수동 상환, 다음 청구일과 phase 300 정렬,
overflow·debt projection을 검증한다. protocol 테스트는 offer별 tagged shape, capability, bounded arrear,
strict settlement payload와 replay 상관관계를 검증한다. 실제 MySQL 8에서는 fresh·C2a 전진 migration,
v1/v2 pin과 manifest 보존, 월세 입주, 전액·부분·0원 지급, 연체 수동 상환, 당월 생활비 불변·다음 달 tenure,
월세 strict due envelope와 phase rank, 중도 이사 future charge 취소, 실패 rollback, 재시작과 immutable trigger를
public HTTP·DB에서 확인한다.

#### C2b2 고정기간 model과 갱신 의미

C2b2 활성 model key는 `dev-unranked-m4-real-estate-lifecycle-2026-v4`다. v3의 지역 profile 4개, 허용
property type 10개, 이사비 4개와 월세 청구·상환 규칙을 새 immutable 부모 아래 복제하되 lease profile은
전세·월세 모두 `renewalRule=fixedTermAutoRenew · termMonths=12 · renewalNoticeLeadDays=30`으로 새로
게시한다. 월세 profile만 `terminationReviewRule=oldestActiveArrearAge ·
terminationReviewAfterDays=60`을 더 가진다. 이 수치는 실제 법정 기간이 아니라 lifecycle 재현성을 검증하는
`GAME_BALANCE` fixture다. v1·v2·v3 row·manifest·run pin은 바꾸지 않고 `newRun` assignment만 sealed v4로
옮긴다. strict projection은 schema version 4를 명시적으로 먼저 분기하고, v1·v2·v3 JSON은 null key도
추가하지 않은 byte-identical 상태를 유지한다.

계약 기간은 시장 달력의 12 calendar months다. 시작일과 같은 월·일을 만료 경계로 삼되 해당 월에 그 일이
없으면 말일로 clamp한다. 다음 term을 직전 term의 clamp된 날짜에서 누적 계산하면 말일이 drift할 수 있으므로
항상 최초 계약 시작일에 `termNo × 12개월`을 더해 경계를 계산한다. 각 term은 `[fromGameDay,
toGameDay)`이고 인접 term 사이에는 빈 날이나 겹침이 없다. 계약의 `effectiveToGameDay`는 계속 실제 퇴거일
exclusive만 뜻하며 활성 계약에서는 null이다. 예정 만료일은 별도 term에만 둔다.

종료 30 game days 전에는 확인 command가 필요 없는 정보성 갱신 안내를 게시한다. player가 이사하지 않으면
만료 game day에 보증금·월세·지역·주택 조건을 바꾸지 않고 같은 계약을 자동 갱신한다. 갱신은 계약·residence를
닫거나 원장을 만들지 않고 다음 term과 그 안내·갱신 일정을 정확히 한 벌 만든다. 기존 이사 command는 언제나
갱신 opt-out 역할을 하며 별도 갱신 응답 endpoint는 C2b2에 만들지 않는다. 월세 인상·갱신 거절·강제퇴거는
후속 명시 설계 전까지 비범위다.

#### C2b2 lifecycle 상태와 일일 처리

`real_estate_lease_profile`과 `lease_contract`는 `termMonths · renewalNoticeLeadDays ·
terminationReviewRule · terminationReviewAfterDays`를 nullable typed 값으로 복제한다. `openEnded`인 v2·v3
계약은 네 값이 모두 null이고 lifecycle 행을 소급 생성하지 않는다. v4 전세는 기간·안내만, v4 월세는 네 값을
모두 가진다. runtime은 다음 세 권위 이력을 분리한다.

- `lease_contract_term` — 계약별 term number, 예정 `[from,to)`, `active → renewed|terminated` 이력
- `lease_lifecycle_action` — `renewalNotice|termRenewal|terminationReview`의 strict due payload와
  `pending → applied|cancelled` 이력
- `lease_termination_review` — 계약별 최대 한 건의 `open → resolved` 종료 검토 이력

월세 정산이 만든 가장 오래된 활성 연체가 60 game days 동안 남아 있으면 그 활성 계약에
`underReview`를 연다. 이는 종료 권고를 표시하는 경고 상태일 뿐 계약, residence, 월세 청구와 보증금은
바꾸지 않는다. 검토가 열린 뒤 그 계약의 활성 연체를 전부 상환하면 같은 지급 transaction에서
`arrearsCleared`로 즉시 resolve한다. 이사하면 pending lifecycle action을 `leaseEnded`로 취소하고 open
검토와 active term도 같은 사유로 닫되, 과거 계약의 연체와 수동 상환 가능성은 그대로 남긴다. 검토 중에도
기간 만료 시 자동 갱신한다.

비금전 lifecycle은 원장 의무가 있는 `scheduled_settlement`에 넣지 않는다. 하루 transaction은 due
settlement와 lifecycle payload를 먼저 strict 검증하고 관련 계약 ID를 오름차순으로 prelock한 뒤 기존
`100 → loanInstallment 200 → leaseRent 300 → livingCostMonth 400`을 정산한다. 이어 같은 날 월세가 만든
연체까지 보이는 상태에서 lifecycle action을 `renewalNotice 500 → termRenewal 600 →
terminationReview 700` 순으로 적용하고 credit end-of-day를 수행한다. 어느 단계든 실패하면 그 game day
전체를 rollback한다. action의 source·occurrence unique key와 계약별 active term/open review unique slot으로
multi-day advance, retry와 재시작이 같은 결과에 수렴해야 한다.

신규 v4 계약은 계약·term 1·갱신 안내·자동갱신 action을 입주 transaction에서 함께 만든다. 월세 연체가
생기면 open review와 pending review action이 없을 때 가장 오래된 활성 연체 하나를 기준으로 60일 뒤 action을
예약한다. 기준 연체를 기한 전에 완납하면 action을 취소하고 다음으로 오래된 활성 연체가 있으면 그 원래
생성일 기준 due action으로 교체한다. action due 시 기준 연체가 여전히 활성이고 계약이 실제로 유지 중일
때만 review를 연다. 이사에서는 미래 월세 charge, pending lifecycle action, open review, active term을 먼저
닫은 뒤 계약·residence를 종료해 trigger와 lock 순서를 보존한다.

#### C2b2 HTTP·화면과 검증

`GET /api/housing/leases/current`는 기존 필드에 다음 nullable model 계약을 더한다.

`leaseLifecycleTerms = { termMonths, renewalNoticeLeadDays,
monthlyRentTerminationReview:{ rule:oldestActiveArrearAge, afterGameDays }|null }|null`

v1~v3와 unavailable model은 null이고 v4만 non-null이다. active lease는 기존 실제
`effectiveToGameDay=null`을 유지하면서 다음 상태를 더한다.

- `currentTerm:{ termNo,effectiveFromGameDay,effectiveToGameDay }|null`
- `renewalNotice:{ termNo,publishedGameDay,renewsOnGameDay }|null`
- `terminationReview:{ status:underReview,openedGameDay,triggerArrearId,activeLeaseArrearKrw }|null`

open-ended 계약은 세 값이 모두 null이고 fixed-term 활성 계약에는 currentTerm이 반드시 존재한다. 안내는
게시일부터 해당 term 만료 전까지만, 종료 검토는 open 상태일 때만 공개한다. `GameSnapshot.life.activeLease`도
같은 shape를 쓰며 내부 action·term·review ID, payload, DB timestamp는 공개하지 않는다. 기존 입주·상환
request path/body/fingerprint와 result shape는 그대로 두고 `renewalRule` enum만 새 값을 허용한다.

C2b2 `/housing`은 현재 term 기간, 갱신 예정일과 게시된 안내, 종료 검토와 활성 계약 연체 총액을 읽기 전용으로
표시한다. 갱신을 원하지 않으면 기존 이사 기능을 쓰고, 검토 해소는 기존 연체 상환 기능을 쓴다. DOM은 한 번
만들어 hooks로 필요한 text와 hidden만 갱신하며 CSS, DOM·라우팅·실제 network 테스트는 추가하지 않는다.

순수 테스트는 월말 clamp·윤년·12월 경계, anchor 기반 term 연속성, 30일 전 안내, 59/60일 연체 경계와
overflow를 검증한다. protocol/store 테스트는 legacy null과 v4 fixed tagged shape, strict action payload,
term/action exactly-once, 안내·자동갱신, 연체 완납·이사 시 resolve/cancel과 기간 경계의 월세 청구를 검증한다.
실제 MySQL 8에서는 fresh·v3 전진 migration, v1~v3 manifest byte 보존, v3 run의 open-ended 동작, v4 new
run의 notice·renewal·review 정확한 game day, 실패 rollback·immutable trigger와 재시작 뒤 동일 snapshot을
public HTTP·DB에서 확인한다.

#### C2c 버전·상품과 제도 경계

C2c는 C2b2의 sealed real-estate v4를 바꾸지 않는다. 전세·월세 조건과 lifecycle은 그대로이므로 기존 v4
run의 real-estate pin을 재해석하지 않고, 새 active credit model
`dev-unranked-m4c2c-credit-2026-v3`만 게시한다. 이 model은 M4-B v2의 credit band·default·무담보 상품과
legacy mapping을 새 immutable parent 아래 복제하고 다음 `leaseDepositLoan` 상품을 한 건 더 seal한다.
`newRun` credit assignment만 v3로 이동한다. 기존 credit v1·v2 run에는 상품을 소급 노출하지 않으며,
C2c capability는 **real-estate v4와 credit v3를 함께 pin한 새 run**에서만 활성이다.

첫 상품 key는 `dev-lease-deposit-fixed-bullet-2026-v1`이고 다음 값은 모두 ranked에서 쓸 수 없는
`GAME_BALANCE` fixture다.

| 항목 | 고정 값 |
|---|---|
| kind·channel | `leaseDepositLoan`, `quoteEligible=true · executionEligible=true · startingEligible=false · executionChannel=leaseMove`; `/housing`의 전세 quote·입주 transaction 전용이고 generic `POST /api/loans` 실행 금지 |
| 금리·상환 | 은행권, 고정 400bp, actual/365, `bullet`, 24개월, 매월 말일 이자·마지막 회차 원금, 거치 0 |
| 원금·조기상환 | 1원~400,000,000원, 조기상환 비용 0, `reduceTerm`, DSR product flag는 false |
| 보증금 한도 | `floor(전세보증금 × 800,000 / 1,000,000)`과 상품 최대원금 중 작은 값 |
| 상품 심사 | `prime|standard`, `delinquent|defaulted|restructured` 계약 없음, active loan 최대 8건 |
| 개발 상환여력 | replacement 뒤 다음 12개월 기존 DSR 대상 원리금 + 신규 전세대출 이자만 합산, 검증 연소득 대비 400,000ppm 이하 |

12개월 임대차 자동갱신은 24개월 대출 만기나 조건을 자동으로 연장하지 않는다. 대출은 기존 phase 200
상환표대로 만기 원금을 청구해 완납되거나 연체 상태로 전이하고, 완납된 계약의 lease 연결은 감사 이력으로
남는다. 같은 집에서의 자동 rollover·재대출은 C2c에 숨겨 넣지 않고 별도 quote·command가 생길 때만 연다.

법정 DSR과 마지막 행의 개발 상환여력은 같은 규칙이 아니다. 2025-10-29 시행 금융위원회 기준에서
전세대출 DSR은 1주택자가 수도권·규제지역에서 신규 이용할 때 원금을 빼고 이자상환분만 반영하며,
무주택자 확대는 2026-01-13 현재 확정되지 않았다. C2c에는 property holding 권위가 아직 없고 모든 실행
가능 household의 보유주택 수가 0이므로 법정 treatment는 `excludedNoOwnedHome`, 공개 근거는
`regulatoryDsrApplied=false`로 고정한다. C3가 보유주택·규제지역 provider를 추가하기 전 boolean
`dsrIncluded=true`나 일반대출 scheduled 원리금을 전세대출에 억지로 재사용하지 않는다.

대신 플레이 가능한 첫 상품의 소득 심사는 별도 `leaseDepositAffordability` GAME_BALANCE rule이다.
이는 기존 DSR 순수 엔진의 정수 schedule contribution을 재사용하되 신규 전세대출은 `interestOnly`,
대체 상환되는 기존 전세대출은 분자와 active count에서 제외한다. 비율은 기존과 같이
`floor(numeratorKrw × 1,000,000 / verifiedAnnualIncomeKrw)`이고 항상 적용하므로 양의 검증 연소득이 없으면
`incomeUnavailable`, 400,000ppm을 넘으면 `affordabilityLimit`이다. 신규 전세대출에는 unsecured stress를
더하지 않지만, 분자에 남는 기존 신용대출은 pinned policy의 stress treatment를 그대로 쓴다. 화면과 API는
이를 법정 DSR이라고 표시하지 않는다.

제도와 fixture를 분리한 기준 자료는 금융위원회
[대출 수요관리 강화 방안 FAQ](https://www.fsc.go.kr/po020201/85518)의 문답 5-1~5-6과 한국주택금융공사
[일반전세자금보증 안내](https://www.hf.go.kr/ko/sub02/sub02_01_10.do)의 보증금 80%·상환능력별 한도다.
실제 상품의 지역·주택수·보증기관별 세부 한도를 가장하지 않으며, C2c 수치는 새 unranked model의 manifest가
소유한다.

#### C2c 전세대출 quote와 replacement 심사

전세대출은 listing과 분리해서 심사할 수 없으므로 기존 무담보 `POST /api/loans/quotes`를 넓히지 않는다.
`POST /api/housing/lease-deposit-loan-quotes`는 공통 command/cursor에
`listingId · offerKind=jeonse · productVersionId · principalKrw`만 받는다. listing은 현재 run의 pinned
world/model·현재 시장 월·공개 기간에 속한 exact jeonse offer여야 한다. client는 보증금, 한도, 소득,
비율이나 심사 결과를 보내지 않는다.

지원 real-estate model은 임대차 lifecycle v4와 그 임대차 child를 forward-copy한 매수 v5·매도/세금 v6다.
quote는 pinned model ID, active sealed strict manifest와 이 명시적 version set을 함께 확인한다. future model에
jeonse listing이 존재한다는 이유만으로 자동 허용하지 않으며, 임대차 계약을 그대로 보존하는 후속 model은
설계에 지원 범위를 먼저 추가한 뒤 같은 판정 지점을 확장한다.

quote는 `lifeledger.life.quoteLeaseDepositLoan.v1` fingerprint를 사용하고 state revision을 올리지 않는
durable command다. 생성 game day에만 유효하며 `loan_quote`에는 `purpose=leaseDeposit`, listing·보증금,
한도, 현재 lease에서 대체될 linked loan과 심사 근거를 immutable하게 저장한다. 같은 payload replay는 저장된
result와 최신 snapshot을, 같은 command ID의 다른 payload는 `idempotencyConflict`를 반환한다. 기존
unsecured quote row와 receipt는 `purpose=unsecured` 의미로 보존하고 generic 실행은 그 purpose만 소비한다.

result는 기존 quote 공통 금리·첫 회차·소득 근거에 다음 값을 더한 전용 strict shape다.

`quoteId · listingId · offerKind · productVersionId · requestedPrincipalKrw · depositKrw ·
fundingLimitPpm · maximumFundingKrw · createdGameDay · expiresGameDay · decisionCode ·
decisionReasons(max 8) · verifiedAnnualIncomeKrw|null · verifiedIncomeSource|null ·
existingLoanBalanceKrw · postExecutionBalanceKrw · regulatoryDsrApplied(false) ·
affordability|null · quotedTerms · replacedLoanId|null · replacedLoanPrincipalKrw`

`affordability`는 `numeratorKrw · denominatorKrw · ratioPpm · limitPpm`을 가진다. 기존 active lease에 연결된
전세대출이 `active`, accrued fee·interest와 미납 bucket이 모두 0이고 반환 보증금이 남은 원금 이상이면
quote는 그 원금을 같은 입주 transaction에서 전액상환할 replacement로 고정하고 심사 분자·잔액·active count에서
제외한다. 이미 `paidOff`면 상환 없이 연결만 감사 이력으로 남긴다. `delinquent · defaulted · restructured`이거나
미납이 있는 linked loan은 §4.2의 상환 순서와 §4.3 상태기계를 우회하지 않도록 financed 여부와 무관하게
이사를 `contractConflict`로 막는다. delinquent는 기존 phase 200 정산으로 active 복귀한 뒤 재시도하고,
defaulted·restructured 이동은 M4-E 절차 전까지 의도적으로 비범위다.

`existingLoanBalanceKrw`는 실행 전 현재 총 대출 원금을 뜻하므로 replacement 원금도 포함한다. 심사용 실행 후
잔액만 `postExecutionBalanceKrw = existingLoanBalanceKrw - replacedLoanPrincipalKrw +
requestedPrincipalKrw`로 계산한다. 따라서 기존 generic quote의 existing 필드 의미를 바꾸지 않으면서 같은
transaction에서 사라지는 old loan을 신규 한도에 이중 계상하지 않는다.

정상 shape를 통과한 quote decision priority는
`creditRestricted → collateralLimit → incomeUnavailable → affordabilityLimit → eligible`이다.
credit reason은 기존 다섯 code를 canonical 순서로 재사용한다. 요청 원금이 상품 자체 1..400,000,000원을
벗어나면 400 `invalidCommand`, 상품 범위 안이지만 listing의 `maximumFundingKrw`를 넘으면 200
`collateralLimit` quote다. quote 시점 현금 부족은 결정하지 않고 실행 transaction에서 최종 자기자금과
이사비를 함께 검증한다.

#### C2c 원자적 payoff·직접 지급 transaction

`loan_contract`는 nullable `leaseContractId`를 복제하고 `leaseDepositLoan`이면 정확히 한 tenant jeonse
계약을 참조한다. 한 lease에는 deposit loan이 최대 한 건이다. 새 loan과 lease의 owner, run, model,
listing, 보증금·quote가 어긋나면 fail-closed한다. 종료 lease와 paid-off loan의 연결은 감사 이력으로
update/delete하지 않는다.

모든 이사는 현재 lease에 active clean linked loan이 있으면 새 대출 사용 여부와 관계없이 반환 보증금에서
그 원금을 먼저 전액상환한다. financed jeonse move는 save·household·residence·기존 lease·linked loan과
listing·quote를 잠근 뒤 다음을 한 transaction에서 처리한다.

1. quote의 listing·한도·replacement·금리·credit·소득·상환여력을 최신 상태로 다시 계산한다.
2. 미래 월세 charge와 lease lifecycle을 취소하고 기존 보증금 자산을 회수한다.
3. 기존 clean linked loan 원금을 `leaseMovePayoff` payment와 `prepaymentPrincipal` allocation으로 전액상환해
   계약을 `paidOff`로 만들고 pending installment·settlement를 취소한다.
4. 새 lease를 만들고 eligible quote의 `leaseDepositLoan` 계약·24회 schedule·phase 200 settlement를
   생성해 그 lease에 연결한다.
5. 새 원금은 committed wallet에 노출하지 않고 새 보증금 자산에 직접 충당하며, 나머지 보증금과 이사비만
   반환 보증금의 잔액과 기존 wallet에서 낸다.
6. residence와 v4 term/action을 만들고 cash·loan·lease deposit·debt·net worth projection을 대조한 뒤
   state revision을 정확히 1 올리고 receipt를 저장한다.

복합 `leaseMove` 원장은 기존·신규 lease와 old·new loan posting owner를 모두 가질 수 있다. 보증금 D,
기존 상환 원금 Pold, 신규 원금 Pnew, 이사비 M이면 0원 posting을 제거한 경제적 분개는 다음과 같다.

- 기존: `leaseDepositAsset -Dold · loanPrincipalLiability +Pold · wallet +(Dold-Pold)`
- 신규: `leaseDepositAsset +Dnew · loanPrincipalLiability -Pnew · wallet -(Dnew-Pnew)`
- 이사: `movingExpense +M · wallet -M`

따라서 `walletDeltaKrw = Dold - Pold + Pnew - Dnew - M`,
`debtDeltaKrw = Pnew - Pold`다. 대출 원금이 독립 wallet credit로 commit되는 transaction이나 대출만 성공한
중간 상태는 없다. 새 대출 심사·old payoff·계약·schedule·원장·residence·receipt 중 하나라도 실패하면 모두
rollback한다. cash/monthly 기존 request로 대출 연결 lease를 떠날 때도 같은 old payoff를 수행한다.

#### C2c HTTP·화면과 검증

기존 `POST /api/housing/leases` body는 strict union이다. v1 cash/monthly shape와 fingerprint는 그대로
보존하고, financed v2만 같은 cursor·listing·`offerKind=jeonse`에 `loanQuoteId`를 필수로 가진다.
fingerprint는 `lifeledger.life.startLease.v2`이고 quote ID를 포함한다. monthlyRent+quote, jeonse에 임의
product/principal/금리 필드를 섞는 payload와 unknown field는 400 `invalidCommand`다. quote가 없거나 다른
owner/run/listing, 만료·비적격·소비됨, 최신 재심사 실패면 stable
`contractConflict|creditRestricted|incomeUnavailable|affordabilityLimit|collateralLimit|rateUnavailable`로
거절한다.

입주 result는 기존 필드에 다음 nullable 객체를 항상 명시한다.

- `depositLoanExecution:{ loanId,quoteId,productVersionId,principalKrw,annualRateBp,
  maturityGameDay,firstInstallment }|null`
- `repaidDepositLoan:{ loanId,paymentId,principalKrw }|null`

cash/monthly 또는 old loan이 없는 경로는 해당 객체가 null이다. `activeLease`에는
`depositLoanId|null`, loan detail에는 `leaseContractId|null`을 더하고 양쪽이 서로 일치해야 한다. public
상품 catalog는 `leaseDepositLoan`을 반환하되 `startingEligible=false`; `/loans` generic quote form은 계속
`unsecuredLoan`만 받고 계약 상세·상환표·조기상환은 전세대출도 기존 기능으로 읽고 조작한다.
기존 `leaseCapability`는 매물의 전세·월세 실행 가능성만 뜻하므로 enum을 늘리지 않는다. 전세대출 가능성은
real-estate v4와 credit v3 pin의 호환성, catalog의 `leaseDepositLoan`, 전용 quote 결과로만 판단한다.

`/housing`은 전세 offer를 고른 뒤 pinned product, 요청 원금, 서버 한도·상환여력 quote와 첫 납입을 표시하고
eligible quote만 financed 입주에 쓴다. quote가 없는 기존 현금 전세·월세 form도 유지한다. outcome-unknown
quote와 입주는 각각 최초 path/body/quote ID를 보존한다. mount에서 DOM을 한 번 만들고 hooks로 node만
갱신하며 CSS, DOM·routing·실제 network 테스트는 추가하지 않는다.

순수 테스트는 보증금 800,000ppm 내림·상품 cap·overflow, interest-only affordability, replacement loan
제외, financed move의 wallet/debt 분개와 0원 posting을 검증한다. protocol/store 테스트는 quote purpose와
strict request union, owner/listing/day correlation, active clean payoff, linked cardinality, direct posting,
replay·rollback을 검증한다. 실제 MySQL 8에서는 fresh·v4 전진 migration, 기존 credit v1/v2와 real-estate
v1~v4 pin·manifest 보존, 새 run product, eligible/ineligible quote, 현금+대출 직접 지급, old payoff+new
origination refinance, 월세 이동, 부족 현금 rollback, phase 200 상환, 명령 replay·충돌, 서버 재시작과 newRun
cleanup을 public HTTP·DB에서 확인한다.

#### C3 버전·소유권 capability와 첫 fixture

C3는 C2c의 sealed real-estate v4와 credit v3를 바꾸지 않는다. 새 부동산 model
`dev-unranked-m4-real-estate-purchase-2026-v5`는 v4의 지역·매물·임대차·이사비·lifecycle child를 새 부모
아래 그대로 복제하고 `purchaseProfile`을 추가한다. 새 credit model
`dev-unranked-m4c3-credit-2026-v4`는 v3의 신용 전이·세 상품·legacy mapping을 복제하고 주담대 상품 한 건을
추가한다. `newRun` assignment만 두 v5/v4를 함께 pin한다. **두 버전을 함께 pin한 새 run**에서만 매수와
주담대가 열리고, real-estate v1~v4 또는 credit v1~v3 run은 기존 조회·임대차·대출 의미를 byte-identical하게
유지한다.

첫 `purchaseProfile`은 ranked에서 쓸 수 없는 `GAME_BALANCE` fixture다.

| 항목 | 고정 값 |
|---|---|
| capability | `ownerOccupiedSingleHome`; 가구당 active holding 최대 1건, `sale` offer만 매수 가능 |
| 보유 목적 | `ownerOccupied`만 지원하고 매수 성공과 동시에 그 집으로 이사한다. 투자·임대인·공동소유는 C3 비범위 |
| 부대비용 | `floor(purchasePriceKrw × 10,000 / 1,000,000)`, 최소 1원, 전액 자기자금 |
| LTV 비용 포함 | 부대비용·이사비는 LTV 분자에 넣지 않고 담보대출로 조달하지 않는다 |
| 담보가치 | 실행 game day의 exact `sale.priceKrw`; 별도 감정·일중 재평가는 하지 않는다 |
| 매물 소비 범위 | 같은 shared world listing을 서로 다른 가구가 각자의 모의 세계에서 살 수 있다. 같은 가구·run은 같은 listing을 한 번만 취득한다 |

부대비용은 법정 취득세가 아니라 중개·등기 비용을 단순화한 게임 값이다. 취득세와 보유·양도세를 숨겨 넣지
않으며 C4가 새 real-estate/policy version에서 매수 tax obligation까지 연결한다. C3 holding은 C4가 원가를
재구성할 수 있도록 `acquisitionPriceKrw · acquisitionIncidentalCostKrw · acquisitionPolicySetId ·
acquisitionCreditPolicySetId · realEstateModelVersionId · acquiredGameDay`를 불변으로 보존한다.

`property_holding`은 current run의 가구·listing에 속하고 `active → disposed`만 허용한다. C3에는 disposal
명령이 없으므로 active만 만든다. 공개 목적은 `ownerOccupied`, 상태는 `active`이고 지역·주택 유형·면적과
취득원가를 복제한다. `property_lien`은 holding별 선순위 1번과 mortgage loan을 연결하며 한 C3 holding에는
최대 한 건이다. 담보대출이 조기상환 또는 만기상환으로 `paidOff`가 되면 lien은 같은 transaction에서
`active → released`로 바뀌고 삭제하지 않는다. 공개 `mortgageLoanId`는 open mortgage와 active lien이 함께
있을 때만 채우며, 상환 뒤에는 null로 내려 과거 담보 연결을 내부 이력으로만 보존한다. 현금 매수는 lien이
없다. `residence.tenureType=owner`는 새 holding을 반드시 참조하고,
compatibility run의 과거 owner row가 없으므로 bridge holding을 만들지 않는다.

첫 주담대 상품 key는 `dev-mortgage-fixed-level-payment-2026-v1`이다.

| 항목 | 고정 값 |
|---|---|
| kind·channel | `mortgage`, 은행권, `startingEligible=false · quoteEligible=true · executionEligible=true · executionChannel=housingPurchase`; generic `POST /api/loans` 실행 금지 |
| 금리·상환 | 30년 전기간 고정 400bp, actual/365, `levelPayment`, 360개월, 매월 말일, 거치 0 |
| 원금·조기상환 | 1원~600,000,000원, 조기상환 비용 10,000ppm, `recalculatePayment`, DSR 포함 |
| 상품 심사 | `prime|standard`, `delinquent|defaulted|restructured` 계약 없음, active loan 최대 8건, active holding 0건 |
| 자금 흐름 | 실행 원금은 wallet에 입금하지 않고 매도인 지급액에 직접 충당 |

법정 policy 값과 가상 지역 매핑은 분리한다. 2025-10-16 시행 자료의 무주택 구입목적 주담대 LTV는
규제지역 400,000ppm, 비규제지역 700,000ppm이고 수도권·규제지역의 가격별 총 대출한도는 담보가치
15억원 이하 6억원, 15억원 초과 25억원 이하 4억원, 25억원 초과 2억원이다. C3의 가상 주소는 실제 규제지역이
아니므로 v5 model이 `capitalArea → regulatedCapitalProxy`, 나머지 세 지역을 `nonRegulatedProxy`로
명시적으로 번역한다. 이는 게임 mapping이고 실제 주소 판정이라고 표시하지 않는다. 최대 주담대는
`min(floor(recognizedValue × ltvLimitPpm / 1,000,000), regionalPriceCap, productMaximum)`이다.

은행권 차주단위 DSR의 1억원 초과 gate와 400,000ppm 한도, 다음 12개월 schedule 합산은 기존 sourced
credit policy를 재사용한다. 신규 mortgage는 일반대출 잔액과 DSR 양쪽에 포함하되 신용대출 stress balance에는
넣지 않는다. 30년 전기간 고정 상품은 고정기간/만기 비중 100%라 주담대 stress 적용비율이 0이고 quote는
`stressRateBp=0 · stressTreatment=fullTermFixed`를 근거로 남긴다. 기존 신용대출 contribution은 해당
simulation date의 pinned unsecured stress policy를 계속 적용한다. 첫 fixture는 위 LTV·가격 cap·DSR만
모델링하며 실제 금융기관의 모든 주택구입 제한이나 상품 자격을 가장하지 않는다.

기준 자료는 금융위원회
[2025-10-15 대출수요 관리 강화 방안](https://www.fsc.go.kr/no010101/85432)의 LTV·가격별 한도와
[2026년 상반기 스트레스 DSR 운영방안](https://www.fsc.go.kr/no010101/85824)의 전기간 고정 주담대
적용비율 0%, 국가법령정보센터
[은행업감독규정 제29조의2·별표 6](https://law.go.kr/LSW/admRulLsInfoP.do?admRulId=21829&efYd=0)다.
원문 URL·2026-07-27 확인일·원문 SHA-256을 새 unranked credit policy set에 저장하고 v4 credit model이
pin한다.

#### C3 주담대 quote와 실행 재심사

담보가치와 매물이 없으면 심사할 수 없으므로 generic loan quote를 넓히지 않는다.
`POST /api/housing/mortgage-quotes`는 공통 command/cursor에
`listingId · productVersionId · principalKrw`만 받는다. 현재 run의 pinned world/model, 현재 시장 월과
유효기간에 속한 exact `sale` offer만 허용한다. 상품 자체 원금 범위를 벗어나면 400 `invalidCommand`, 범위
안이지만 LTV·지역 가격 cap을 넘으면 200 `collateralLimit` quote다. client는 가격·담보가치·주택수·LTV·
DSR·자기자금을 보내지 않는다.

quote fingerprint는 `lifeledger.life.quoteMortgage.v1`이고 state revision을 올리지 않는 durable command다.
생성 game day에만 유효하며 `loan_quote.purpose=mortgagePurchase`에 listing·담보가치·region class·LTV·DSR·
현재 lease와 상환할 linked loan·예상 자기자금 근거를 immutable하게 저장한다. 같은 payload replay는 저장된
result와 최신 snapshot을, 같은 command ID의 다른 payload는 `idempotencyConflict`를 반환한다.

심사 우선순위는 `creditRestricted → purchaseRestricted → collateralLimit → incomeUnavailable →
debtServiceLimit → insufficientOwnFunds → eligible`이다. credit reason은 기존 canonical 다섯 code를
재사용한다. `purchaseRestricted` reason은 `activeHolding · residenceChangedToday · leaseExitRestricted`다.
현재 residence가 같은 game day에 시작됐거나, active holding이 이미 있거나, 떠날 lease의 linked loan이
clean active 전액상환 상태가 아니면 실행 가능한 quote를 만들지 않는다. quote의 자기자금은 현재 wallet에
반환 보증금에서 linked loan 원금을 뺀 금액을 더한 `availableBuyerCashKrw`와
`max(0, purchasePrice + incidentalCost + movingCost - requestedPrincipal)`인
`requiredBuyerCashKrw`를 비교한다. 담보한도를 넘는 product-valid 견적도 이 값이 음수가 되지 않으며
`collateralLimit`의 200 결과와 완전한 근거를 반환한다.

result는 다음 strict 근거를 가진다.

`quoteId · listingId · productVersionId · requestedPrincipalKrw · purchasePriceKrw ·
recognizedCollateralValueKrw · ltvRegionClass · ltvLimitPpm · maximumMortgageKrw ·
ltv{ numeratorKrw,denominatorKrw,ratioPpm,limitPpm } · createdGameDay · expiresGameDay ·
decisionCode · decisionReasons(max 8) · verifiedAnnualIncomeKrw|null · verifiedIncomeSource|null ·
existingLoanBalanceKrw · postExecutionBalanceKrw · dsrApplied · dsr|null · stressRateBp ·
stressTreatment · acquisitionIncidentalCostKrw · movingCostKrw · returnedDepositKrw ·
replacedLoanId|null · replacedLoanPrincipalKrw · availableBuyerCashKrw · requiredBuyerCashKrw · quotedTerms`

DSR gate가 꺼져 있으면 소득 없이도 `dsrApplied=false · dsr=null`이고, gate가 켜진 뒤 소득이 없으면
`incomeUnavailable`이다. 계산한 ratio가 한도를 넘으면 완전한 DSR 근거와 `debtServiceLimit`을 반환한다.
LTV는 gate가 없고 항상 완전한 근거를 남긴다. quote가 eligible이어도 실행은 save·household·residence·lease·
holding slot·listing·활성 loan을 전역 순서로 다시 잠그고 현재 sale price, LTV policy, 금리, credit, 소득,
DSR, lease payoff와 자기자금을 모두 재계산한다. 저장된 decision이나 금액을 그대로 신뢰하지 않는다.

#### C3 원자적 매수 transaction·HTTP·검증

`POST /api/housing/purchases`는 공통 command/cursor에 `listingId · mortgageQuoteId|null`만 받는다. null은
현금 매수이고 quote ID가 있으면 그 quote의 exact listing을 담보로 실행한다. fingerprint는
`lifeledger.life.purchaseProperty.v1`이며 null 여부와 quote/listing ID를 포함한다. cash와 financed 모두
매수와 동시에 owner residence로 이동한다. 별도 `/housing/moves`는 C3에서 열지 않고, C4 매도 뒤 이동
의미까지 정할 때 연다.

transaction은 다음 고정 순서다.

1. 현재 cursor와 v5/v4 capability, sale listing, active holding 0건과 residence 시작일을 재검증한다.
2. active lease가 있으면 미래 rent charge·lifecycle action·term·review를 C2b2와 같은 규칙으로 닫고 보증금
   자산을 전액 회수한다. linked lease-deposit loan은 C2c와 같은 `leaseMovePayoff`로 먼저 전액상환한다.
3. 선택한 eligible mortgage quote를 최신 LTV·DSR·credit·소득·금리로 다시 심사하고 holding, 360회 schedule,
   phase 200 settlement와 선순위 lien을 만든다. 원금은 wallet을 거치지 않는다.
4. 취득가·부대비용·policy/model·목적을 가진 holding을 만들고 기존 residence를 닫은 뒤 holding을 참조하는
   owner residence를 만든다. 현재 달 생활비 pin은 보존하고 다음 달부터 owner tenure를 쓴다.
5. 하나의 `propertyPurchase` 원장과 cash·loan debt·property book value projection을 대조하고 state revision을
   정확히 1 올린 뒤 receipt를 저장한다.

가격 V, 부대비용 C, 이사비 M, 신규 mortgage Pnew, 기존 보증금 Dold와 상환 원금 Pold의 0원 제거 분개는
다음과 같다.

- lease exit: `leaseDepositAsset -Dold · loanPrincipalLiability +Pold · wallet +(Dold-Pold)`
- purchase: `propertyAsset +V · acquisitionIncidentalExpense +C · movingExpense +M ·
  loanPrincipalLiability -Pnew · wallet -(V+C+M-Pnew)`

따라서 `walletDeltaKrw = Dold - Pold + Pnew - V - C - M`,
`debtDeltaKrw = Pnew - Pold`, `propertyBookValueDeltaKrw = V`다. wallet이 음수가 되면
`insufficientWalletCash`이고 holding·loan·lease 종료·residence·원장·receipt를 모두 rollback한다. active
lease arrear는 보증금과 임의 상계하지 않고 기존 typed 채무로 남긴다. 같은 command replay는 저장된
result와 최신 snapshot을 반환하고 계약·holding·원장·revision을 늘리지 않으며 다른 payload는
`idempotencyConflict`다.

result는 `holding · residenceId · listingId · purchasePriceKrw · acquisitionIncidentalCostKrw ·
movingCostKrw · returnedDepositKrw · walletDeltaKrw · effectiveFromGameDay · endedLeaseId|null ·
repaidDepositLoan|null · mortgageExecution|null`이다. `mortgageExecution`은 기존 대출 실행 요약에
`propertyHoldingId`를 더한다. loan detail은 `propertyHoldingId|null`, residence는
`propertyHoldingId|null`을 공개하고 lease/owner tagged shape가 어긋나면 fail-closed한다.

`GET /api/housing/holdings`는 `purchaseCapability · maximumActiveHoldings ·
holdings(max 4) · totalPropertyBookValueKrw`를 반환한다. holding은
`id · listingId · status · purpose · regionKey · propertyType · exclusiveAreaSquareMeters ·
acquiredGameDay · acquisitionPriceKrw · acquisitionIncidentalCostKrw · bookValueKrw ·
mortgageLoanId|null`만 공개한다. `GameSnapshot.life`는 같은 bounded active holding window와
`hasMoreActivePropertyHoldings · totalPropertyBookValueKrw`를 가지며 내부 policy row·lien·DB timestamp는
노출하지 않는다.

`/housing`은 sale offer에서 현금 매수 또는 pinned mortgage 상품·원금을 선택하고, 서버가 반환한 LTV·DSR·
자기자금·첫 납입을 표시한 뒤 eligible quote만 financed purchase에 사용한다. 현금 매수도 가격·비용을
클라이언트가 다시 계산하지 않는다. 보유주택과 연결 mortgage를 표시하며 `/loans`의 기존 상세·상환표·
조기상환 기능으로 이동할 수 있다. outcome-unknown quote·purchase는 최초 path/body를 보존한다. mount에서
DOM을 한 번 만들고 hooks로 node만 갱신하며 CSS, DOM·routing·실제 network 테스트는 추가하지 않는다.

순수 테스트는 400,000/700,000ppm LTV 내림, 세 가격 cap 경계, 상품 cap, 분모 0·overflow, fixed mortgage
DSR schedule, 현금/대출 매수 funding과 lease payoff 분개를 검증한다. protocol/store 테스트는 v5/v4
capability, owner/listing/day scope, holding/lien cardinality, quote 재심사, direct funding, lease 종료,
strict nullable union, replay·rollback과 property/debt projection을 검증한다. 실제 MySQL 8에서는 fresh·v4
전진 migration, 기존 real-estate v1~v4·credit v1~v3 manifest와 pin 보존, 새 run 현금 매수, LTV·DSR·credit·
소득·자기자금 실패 quote, financed 매수, C2c lease-deposit payoff, 첫 mortgage 납입, stale quote, 부족 현금
rollback, 동일 listing 재매수 거절, replay·충돌, 다른 user/run ID 비공개, 서버 재시작과 newRun cleanup을
public HTTP·DB projection으로 확인한다.

#### C4 구현 순서·버전과 지원 범위

C4는 `C4a 매도 주문·체결·양도세 → C4b 취득세·보유세 정산과 세금 이력` 순서로 구현하되 최종 C4
완료 전에는 둘을 같은 새 run bundle로 검증한다. 새 real-estate model key는
`dev-unranked-m4-real-estate-sale-tax-2026-v6`, 새 finance policy key는
`dev-unranked-kr-individual-property-2026-v3`다. v6는 v5의 매물·임대차·매수 child를 새 부모 아래 복제하고
typed `saleLiquidityProfile`을 추가한다. 새 policy는 `kr-individual-2026-v2`의 모든 규칙·출처를 복제하고
typed 취득·주택 재산·1세대 1주택 양도 규칙을 추가한다. credit v4는 바꾸지 않는다. `newRun`만
finance v3·real-estate v6·credit v4를 함께 pin하며 기존 v1~v5 run은 기존 응답과 실행 의미를 유지한다.

첫 fixture는 개인 거주자·세대 전체 국내 주택 1채·단독소유·`ownerOccupied`·전용면적 85㎡ 이하만
지원한다. v6의 허용 면적 child 자체를 85㎡ 이하로 게시해 실행 단계에서 우연히 범위 밖 매물을 만들지
않는다. 공동소유, 임대인, 일시적·상속·혼인 2주택, 분양권·입주권, 생애최초·출산·인구감소지역 감면과
추징, 농어촌특별세, 종합부동산세, 조정대상지역의 역사, 2년 미만 단기양도와 다주택 중과는 첫 fixture에서
`policyUnsupported`로 fail-closed한다. 이는 세율을 0으로 간주하는 것이 아니다.

법령 수치는 2026-07-27 확인본을 immutable source document와 SHA-256으로 pin한다. 기준 자료는
[지방세법 제11조](https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joBrNo=00&joNo=0011&lsiSeq=282559&urlMode=lsScJoRltInfoR)의
주택 취득세율, [제20조](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1032970405)의 60일 신고·납부,
[제110조](https://law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029491211)와
[제111조·제111조의2](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1033362253)의
재산세 과표·세율, [제114·115조](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029491445)의
6월 1일 귀속·7월/9월 납기,
[소득세법 시행령 제154조](https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1031481567)의 1세대 1주택,
[국세청 고가주택 계산식](https://www.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=8799&mi=12271),
[소득세법 제95조](https://www.law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1032210681)와
[제103조](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1033240591)다.
정책은 이 기준일의 규칙을 이후 게임 연도에도 고정 적용하는 unranked simulation fixture이며 미래 법령을
예측한다는 뜻이 아니다.

#### C4 매도 주문과 결정론적 체결

매도는 `property_sale_order` header와 immutable revision 이력이다. create는 revision 1을 만들고, 가격 변경은
revision을 1 올려 새 후보일을 만들며, 취소도 `revisionKind=cancellation` terminal revision을 남긴다. listing
revision은 주문가·후보일·settlement를 필수로 갖고 cancellation revision은 세 필드가 모두 null이다. 상태는
`active → filled|cancelled|rejected`뿐이고 active holding별 active order는 최대 한 건이다. 새 run cleanup은
active order를 `cancelled/newRun`으로 닫지만 과거 run 이력을 삭제하지 않는다.

create/reprice는 현재 game day의 저장된 지역 가격지수로
`referenceValueKrw = floor(acquisitionPriceKrw × currentPriceIndexPpm / acquisitionPriceIndexPpm)`을 계산한다.
주문가는 reference의 800,000~1,200,000ppm 범위여야 하며, 비율별 후보 지연은 다음 GAME_BALANCE fixture다.

| 주문가 / reference | 후보 지연 |
|---|---:|
| `800,000..950,000ppm` | 1~3일 |
| `950,001..1,050,000ppm` | 3~7일 |
| `1,050,001..1,200,000ppm` | 7~30일 |

지연은 `H(worldSeed, listingId, orderRevision, "propertySaleCandidate")`를 rejection sampling해 inclusive 범위에서
고른다. 조회·worker 수와 command ID는 entropy에 넣지 않는다. 가격 변경은 변경 game day의 reference와 새
revision을 쓰므로 기존 후보를 재사용하지 않는다. 취소된 revision에는 후보일이 없다.

첫 fixture의 `grossSalePriceKrw`는 accepted revision의 immutable `askingPriceKrw`와 정확히 같다. 유동성
entropy는 후보일만 바꾸고 hidden slippage나 체결일 재평가를 만들지 않는다. 거래비용은
`floor(grossSalePriceKrw × 5,000 / 1,000,000)`, 최소 1원인 GAME_BALANCE 값이다. candidate day에는 exact
active revision, 소유권·owner residence, holding·lien·mortgage, 세금 rule과 최소 보유·거주기간을 다시
검증한다. overdue candidate를 늦게 체결하지 않고 invariant 오류로 하루 전체를 rollback한다.

주문은 취득일과 owner residence 시작일에서 각각 달력 2주년이 지난 뒤에만 만들 수 있다. 이 경계는
2년 미만 단기양도와 실제 조정대상지역 이력을 첫 fixture가 지원하지 않기 위한 명시적 capability다. active
mortgage는 연체 bucket과 accrued fee·interest가 0인 `active`만 체결할 수 있다. 후보일 전 완납돼 lien이
released된 경우에는 대출 상환액이 0이다. 체결할 수 없는 상태나 아래 proceeds 부족은 order를
`rejected(mortgageNotPayable|insufficientProceeds|policyUnsupported)`로 닫고 holding·residence·대출·원장·
지갑을 바꾸지 않는다. 첫 policy의 `deficientSaleProceeds=reject`이며 부족채무 상품은 만들지 않는다.

#### C4 취득·보유·양도 세금 fixture

모든 계산은 i128 중간값과 checked 산술을 쓰고 각 세목을 원 단위로 독립 내림한다. 적용 policy/rule,
법적 기준일, household home count, valuation·과세표준·공제·세율·세액·납부 game day는
`property_tax_event`와 component/payment 이력에 저장한다. 0원 양도세도 `noPaymentRequired` event로 남겨
판정을 생략하지 않았음을 증명한다.

1주택 유상 취득세율은 취득가 6억원 이하 10,000ppm, 9억원 초과 30,000ppm이다. 중간 구간은 법정
`((취득가액 × 2 / 3억원) - 3)%`를 퍼센트 소수 넷째 자리까지 반올림한 것과 같은
`clamp(roundHalfUp(purchasePriceKrw / 15,000) - 30,000, 10,000, 30,000)ppm`을 쓴다. 취득세는
`floor(price × rate / 1,000,000)`, 지방교육세는 같은 과표에 취득세율의 10%를 독립 적용한다. 취득
transaction에서 event와 취득일+60일의 payment settlement를 만들고, 세금은 담보대출 원금이나 매수
자기자금 판정에 섞지 않는다. 납부일 지갑 부족액만 `propertyTaxEvent` tax obligation과
`taxObligationLiability`가 된다.

매년 6월 1일 시작 transaction은 전날 마감 기준 active 1주택 holding을 assessment한다. 공시가격이 없는
가상 주택이므로 `officialValueKrw = floor(referenceValueKrw × 700,000 / 1,000,000)`을 쓰는 비율만
GAME_BALANCE다. 2026 공정시장가액비율은 공시가격 3억원 이하 430,000ppm, 6억원 이하 440,000ppm,
그 초과 450,000ppm이고 `taxBaseKrw=floor(officialValueKrw×ratio/1,000,000)`이다. 공시가격 9억원 이하는
1주택 특례 누진세율 0.05%·0.1%·0.2%·0.35%, 그 초과는 표준 0.1%·0.15%·0.25%·0.4%와 법정 누진공제를
쓴다. 지방교육세는 산출 재산세의 200,000ppm이다. 과표·세부담 상한, 도시지역분, 지역자원시설세와 지자체
조례 가감은 첫 fixture가 지원하지 않아 공개 evidence에 exclusion code로 남긴다. 총액은 두 payment로
나눠 7월 31일에 `floor(total/2)`, 9월 30일에 나머지를 phase 100으로 납부한다. 현금 부족은 payment별
tax obligation을 만들고 매도 뒤에도 소멸하지 않는다.

양도일 현재 세대 1주택이고 달력 2년 보유·거주했으며 총 양도가 12억원 이하면 national/local 양도세는
0원이다. 12억원 초과분은
`gain=max(0, salePrice-acquisitionPrice-acquisitionIncidentalCost-acquisitionTaxes-dispositionCost)`와
`highValueGain=floor(gain×(salePrice-1,200,000,000)/salePrice)`를 쓴다. 장기보유특별공제는 보유 3년
12%에서 매년 4%p·최대 40%, 거주 2년 8%에서 매년 4%p·최대 40%이며 달력상 완료 연수만 센다.
`max(0, highValueGain-longTermDeduction-2,500,000)`에 법정 6%~45% 누진세율·누진공제를 적용하고
개인지방소득세는 같은 과표에 1/10 세율·누진공제를 독립 적용한다. 양도세 event는 체결 transaction에서
즉시 paid 처리하는 GAME_BALANCE 원천공제이며 별도 `tax_obligation`을 만들지 않는다.

#### C4 일일 phase·원자적 매도와 원장

활성 holding이 있으면 일일 pipeline은 player transaction 전에 그 model·region의 필요한
`real_estate_daily`를 준비한다. transaction은 due ID를 먼저 수집하고 다음 순서로 point lock한다.

`save → household → household_member(id) → residence → property_holding(id) → property_sale_order(id) →
property_lien(id) → lease_contract(id)와 child → loan_contract(id)와 schedule/payment →
scheduled_settlement(dueGameDay,id) → property_tax_event(id) → tax_obligation(id)`

6월 1일 assessment를 먼저 pin하고, 기존 settlement phase 100~400을 모두 처리한 뒤 매도 체결을 phase 450,
lease lifecycle을 500~700으로 처리한다. 따라서 같은 날 mortgage installment가 먼저 원금을 줄이고 월세·
생활비도 기존 순서를 지킨 뒤 sale payoff가 남은 원금만 상환한다. assessment·payment·sale·save cursor는 같은
player-day transaction에서 commit되며 중간 실패는 하루 전체를 rollback한다.

체결 waterfall은 `거래비용 C → mortgage 원금 P와 조기상환 수수료 F → 양도세 T → wallet W`이고 첫
owner-only fixture의 임차보증금 반환 D는 항상 0이다. `W=S-C-P-F-T`가 음수면 reject다. mortgage payoff는
`propertySalePayoff` payment와 `prepaymentFee → prepaymentPrincipal` allocation을 만들고 미래 installment·
settlement를 취소한 뒤 계약을 `paidOff`, lien을 `released`로 바꾼다. manual prepayment로 위장하지 않고
composite sale ledger가 생긴 뒤 applied 처리한다.

하나의 `propertySale` 원장은 0원 leg를 생략하고 다음을 기록한다.

- `propertyAsset -B`(holding), `realizedGainLoss +(B-S)`, `propertyDispositionExpense +C`(holding)
- mortgage가 있으면 `loanPrincipalLiability +P · loanFeeExpense +F`(loan)
- 양도세 component마다 `propertyTaxExpense +T`(tax event), `wallet +W`

합은 `-B+(B-S)+C+P+F+T+W=0`이다. 성공은 holding을 `disposed`, active owner residence를 종료하고 같은
region의 `rentFree` residence를 같은 game day부터 만든다. 현재 달 생활비 pin은 바꾸지 않고 다음 달부터
rent-free tenure를 쓴다. `save.propertyBookValueKrw`는 B만큼, loan debt는 P만큼 줄고 cash는 W만큼 늘어난
뒤 모든 권위 합과 projection을 대조한다.

취득세·보유세 payment는 `propertyTaxPayment` settlement와 ledger source를 쓴다. 지갑에서 납부한 금액은
`propertyTaxExpense +paid · wallet -paid`, 부족액은 `propertyTaxExpense +unpaid ·
taxObligationLiability -unpaid`로 같은 transaction에서 권위 tax obligation을 만든다. 이후 tax obligation
상환은 기존 tax debt 계약을 재사용한다.

#### C4 HTTP·클라이언트·검증

공개 명령은 다음 strict shape와 stored result + latest snapshot replay를 쓴다.

- `POST /api/housing/sales`: 공통 cursor + `holdingId · askingPriceKrw`, fingerprint
  `lifeledger.life.createPropertySaleOrder.v1`
- `POST /api/housing/sales/{orderId}/reprice`: 공통 cursor + `askingPriceKrw`, fingerprint
  `lifeledger.life.repricePropertySaleOrder.v1`
- `POST /api/housing/sales/{orderId}/cancel`: 공통 cursor만, fingerprint
  `lifeledger.life.cancelPropertySaleOrder.v1`
- `GET /api/housing/sales?before&limit`: 현재 run의 active·terminal order와 revision 결과, 기본/최대 20
- `GET /api/housing/holdings/{holdingId}/tax-events?before&limit`: active·disposed 본인 holding의 tax event,
  component와 payment 최대 20

create/reprice/cancel 최초 성공은 state revision을 1 올리고 snapshot을 broadcast하지만 원장을 만들지 않는다.
replay는 최초 order result를 반환해 이후 filled 상태로 덮어쓰지 않는다. 조회는 최신 order 상태와 bounded
history를 별도로 제공한다. malformed cursor, 다른 사용자·이전 run의 holding/order/tax ID는 같은 404로
비노출하고 unknown query/field는 400이다. `/housing/moves`는 C4에서 별도 우회 이사 명령으로 열지 않는다.
매도 성공의 same-region rent-free 전환 뒤 기존 lease 또는 purchase 명령으로 다음 주거를 선택한다.

`/housing`은 active holding별 주문 생성, active order 가격 변경·취소, 후보일·상태·rejection reason,
체결가·비용·대출상환·세금·순수령액과 tax history를 서버 값 그대로 표시한다. outcome-unknown mutation은
각 최초 path/body를 보존한다. 고정 DOM slot과 hooks를 사용하고 CSS, DOM·routing·실제 network 테스트는
추가하지 않는다.

순수 테스트는 candidate 지연·revision entropy, reference/주문가 범위, 거래비용, 취득세 중간구간 반올림,
재산세 과표·누진경계·분할, 12억원 비과세·고가주택 안분·장특공제·누진세, proceeds·ledger 합과 overflow를
BDD/DCI로 검증한다. service/protocol 테스트는 phase 450, exact due/replay, order 상태전이, payoff와 rent-free
전환, strict request/response·cursor·소유권을 검증한다. 실제 MySQL 8은 fresh와 v5 전진, 기존 model/policy
manifest·run pin, 2주년 경계, create/reprice/cancel, 지연 체결, mortgage payoff, 부족 proceeds reject,
취득세 60일, 6월 1일 assessment와 7·9월 납부·부족세액, 0원·고가 양도세, 재시작·newRun cleanup, foreign ID,
원장·cash/debt/property projection을 확인한다.

## 6. 데이터 조건식 복지 엔진

### 6.1 M4-D1 구현 경계와 첫 fixture

M4-D1은 복지 조건식·신청·지급만 수직으로 완성한다. 생애 사건과 보험 component는 기존 disabled version을
그대로 pin하고 §7은 다음 slice에서 활성화한다. migration은
`0038_m4d_welfare.sql` 하나이며 active welfare component key는
`dev-unranked-m4-welfare-2026-v1`, 이를 담는 aggregate key는
`dev-unranked-m4-life-welfare-2026-v2`다. 새 aggregate는 migration 직전 `newRun` life catalog가 가리키던
living cost·life event·insurance·corporation component ID를 그대로 복제하고 welfare ID만 바꾼다. 기존 run의
`run_rule_bundle`은 절대 바꾸지 않고 모든 seed와 검증이 끝난 migration 마지막에 `newRun` assignment만 새
aggregate로 한 번 갱신한다. 따라서 기존 disabled-welfare run은 조회 시 `programs=[]`를 유지하고 새 프로그램
ID로 신청할 수 없다.

첫 프로그램 `fictionalRestartGrant`는 실제 제도와 무관한 **GAME_BALANCE·unranked** fixture다. 적용 기간은
pinned run 전체이고, application game day를 `D`라 할 때 조건은 D의 잠긴 권위 상태와 닫힌 기간 값으로
판정한다. 다음 여섯 public condition이 모두 Kleene `True`여야 한다.

| condition code | 조건 |
|---|---|
| `ageWindow` | `character.age`가 22세 이상 67세 이하 |
| `workTransition` | `career.employmentStatus`가 `none|ended` 중 하나이거나 `household.dependentCount >= 1` |
| `recentIncome` | `income.periodTotal(previous30ClosedDays=30) <= 1,234,567원` |
| `policyAsset` | `asset.policyValuation(priorClose) <= 12,345,678원` |
| `residenceKnown` | D에 active residence가 존재함 |
| `notServing` | `military.status != serving` |

상수는 `minimumAgeYears=22 · maximumAgeYears=67 · incomeWindowDays=30 ·
incomeCapKrw=1,234,567 · assetCapKrw=12,345,678 · benefitKrw=333,000`으로 catalog에 typed 저장한다.
급여는 정액 333,000원 한 번이고 신청일 D에는 현금이 변하지 않으며 D+1의 settlement에서 지급한다.
`duplicateGroupKey=fictionalRestartGrant`의 scope는 `(saveId, runRevision)`이고 한 번 승인되면 지급 완료·종료나
program version 교체 뒤에도 같은 run에서는 영구 소진된다. 새 run은 새 scope이므로 다시 신청할 수 있다.

복지·정책상품은 임의 스크립트가 아니라 versioned AST로 표현한다. 지원하는 node는 다음으로 제한한다.

- 논리: `all`, `any`, `not`
- 비교: `eq`, `in`, `lt`, `lte`, `gt`, `gte`, `between`
- 집계: `sum`, `count`, `exists`
- 값: 허용된 fact path, 정책 상수, integer/string/date literal

### 6.2 typed AST, 3값 논리와 fact registry

AST 값의 type은 `boolean · integer · moneyKrw · count · ageYears · date · string · enum(schemaKey)`이며
`moneyKrw`, `count`, `ageYears`, 일반 integer 사이의 암묵 변환은 없다. 비교 양쪽은 같은 type·unit이어야 하고
`lt|lte|gt|gte|between`은 순서가 정의된 scalar만 받는다. `in`의 모든 literal도 왼쪽과 같은 type이고
`between`은 같은 단위의 `lower <= upper`여야 한다. `sum`은 같은 numeric unit의 bounded collection,
`count`와 `exists`는 등록된 bounded collection만 받는다. 프로그램의 public condition은 각각 typed boolean
AST이고 eligibility root는 이 condition 결과를 `all|any|not`으로 합성한다. catalog seal 때 cycle·도달 불가
condition·사용하지 않은 상수·필요 trigger 누락까지 거절한다.

다음 값은 game-engine protocol bound이며 catalog 데이터로 늘릴 수 없다.

- root를 depth 1로 세어 AST depth 최대 12, 프로그램 전체 node 최대 128
- `all|any` child 1~16개, `not` child 정확히 1개, `in` literal 1~32개
- 프로그램 상수 최대 64개, public fact/condition result 최대 32개
- `previousClosedDays` window 1~366일, 집계가 읽는 collection 최대 32행
- string literal은 UTF-8 scalar 1~64개, condition code와 program key는 ASCII canonical
  `[a-z][a-zA-Z0-9]{0,63}`

상한 초과 AST는 게시할 수 없다. 런타임 collection이 32행을 넘거나 window가 완결되지 않았으면 일부만 잘라
계산하지 않고 그 fact를 `Unknown(collectionLimitExceeded|windowIncomplete)`으로 만든다. 정적 type·unit 오류는
catalog/loader invariant 오류이지 `Unknown`이 아니다. 런타임의 누락된 권위 값, 평가가격 부재, checked i128
overflow는 stable unknown reason과 함께 `Unknown`으로 남긴다.

boolean 평가는 Kleene 3값 논리다. `all`은 하나라도 `False`면 `False`, 모두 `True`일 때만 `True`, 나머지는
`Unknown`이다. `any`는 하나라도 `True`면 `True`, 모두 `False`일 때만 `False`, 나머지는 `Unknown`이며
`not(Unknown)=Unknown`이다. Unknown operand를 가진 비교·집계도 Unknown이다. 따라서 다른 확정 실패가 있는
`all(False, Unknown)`은 `False`지만 unknown을 false로 치환한 것은 아니다. eligibility root가 True면
`eligible`, False면 `ineligible`, Unknown이면 `indeterminate`다.

fact path는 다음 schemaVersion 1 registry에 등록한 것만 쓴다. AST에 SQL, 정규식, 임의 함수, 현재 벽시계,
클라이언트 입력 경로는 허용하지 않는다.

| fact path | type·unit/window | M4-D1 권위 |
|---|---|---|
| `character.age` | `ageYears`, D | `career_run.birthDate`와 서버 game date로 계산한 완료 연수 |
| `household.memberCount` | `count`, D | D에 active인 player 포함 household member 수 |
| `household.dependentCount` | `count`, D | D에 active이고 role이 `dependent|partner|child|parent`인 member 수 |
| `residence.exists` | `boolean`, D | D에 active residence가 정확히 한 건인지 여부 |
| `residence.region` | `enum(region)`, D | D의 active residence region; 행이 없으면 Unknown |
| `career.employmentStatus` | `enum(welfareEmployment)`, D | `none|pendingStart|active|ended`; 해당 run에 계약이 전혀 없으면 `none`, 있으면 D의 최신 계약 상태 |
| `military.status` | `enum(military)`, D | pinned career run의 `unserved|serving|completed|exempt` |
| `income.periodTotal` | `moneyKrw`, `previousClosedDays(1..366)` | 해당 run `employment_income_event`의 gross employment income을 적용 game day `[D-N,D)`로 합산 |
| `asset.policyValuation` | `moneyKrw`, `priorClose` | 직전 commit의 schemaVersion 1 gross asset valuation |
| `debt.policyBalance` | `moneyKrw`, `priorClose` | 직전 commit의 권위 aggregate debt projection |

`currentGameDay D` 사실은 D 시작에 효력이 생기는 날짜 기반 상태를 뜻한다. 따라서 start game day가 D인
employment는 `active`, exclusive end game day가 D인 employment는 `ended`, military end game day가 D인
service는 `completed`로 수집한다. DB lifecycle UPDATE가 같은 player-day transaction의 뒤 단계에 있더라도
저장된 D-1 status 문자열을 그대로 읽지 않고 sealed contract/service 날짜로 D 상태를 도출한다. 반면 D의
급여·정산·시장가격 같은 금액 효과는 아직 포함하지 않고 아래 previous-closed/prior-close window를 따른다.

`income.periodTotal` schemaVersion 1은 `employmentPayroll|militaryPay`가 만든
`employment_income_event.gross_employment_income_krw`만 포함한다. 금융·복지·법인 소득은 아직 포함하지 않으며
0건인 완결 window의 합은 알려진 0원이다. D가 run 초반이면 run opening부터 D 직전까지의 존재하는 닫힌
game day만으로 window가 완결된 것으로 보고 미래나 pre-run 소득을 합성하지 않는다.

`asset.policyValuation(priorClose)`은 wallet, open financial-account cash, active cash-product principal,
금융자산의 저장된 정책평가액, tenant lease deposit, physical gold·bond·position 정책평가액, active property
book value를 권위별로 정확히 한 번 더한 부채 차감 전 금액이다. D=0의 prior close는 run opening
transaction이 확정한 상태다. 필요한 평가가격이나 권위행이 없거나 합산이 overflow하면 알려진 항목만 더하지
않고 전체를 Unknown으로 만든다. 이 포함 범위는 welfare component schemaVersion 1에 pin하며 확대·축소는 새
component에서만 한다. `debt.policyBalance`도 부분 채무를 재합산하지 않고 검증된 `save.debtKrw` projection을
읽는다.

collection schemaVersion 1의 행 의미도 같은 권위 경계에 고정한다. `income.entries`는 window 안의
`employment_income_event` 한 건당 gross 금액 한 값이다. `asset.positions`는 wallet 한 건, open financial
account별 cash, active cash-product 계약별 현재 principal, active 군 적금 계약별 principal, active tenant
deposit, LLX·bond·gold·physical-gold position별 정책평가액, active property holding별 book value를 각각 한
값으로 가진다. 적금의 여러 paid installment는 계약 principal 한 값으로 합치며 installment 수를 position
수로 세지 않는다. `debt.positions`는 active loan 계약별 남은 원금·이자·수수료, active essential/lease
arrear별 remaining 금액, outstanding tax obligation별 remaining 금액을 각각 한 값으로 가진다. 세 collection은
값 순서가 아닌 authority key 순서로 수집한 뒤 fingerprint에서 canonical 정렬하고, 전체가 32건을 넘거나 한
position 계산이 overflow하면 일부 collection을 만들지 않고 각각 `collectionLimitExceeded` 또는
`arithmeticOverflow` Unknown이다. 알려진 `asset.positions` 합은 `asset.policyValuation`, 알려진
`debt.positions` 합은 검증된 `save.debtKrw`와 같아야 하며 불일치는 invariant 오류다.

### 6.3 판정 pin, fingerprint와 재평가

`welfare_program_version`은 eligibility graph, 신청 가능 기간, duplicate group, typed 급여, 지급 일정과
재판정 trigger를 가진다. engine은 new run 초기화와 매일 planner에서 active program을 `programKey,id` 순으로
최대 16개 평가한다. `welfare_period_pin`은 evaluation game day, previous-closed window의 시작·끝, prior-close
revision과 참조한 권위 revision을 immutable하게 고정한다. 같은 날 뒤 정산이나 이후 명령이 과거 pin을
재작성하지 않는다.

`factFingerprint`는 `schemaVersion · programVersionId · period bounds`와 fact key순의
`type · unit · window · known value|unknown reason`을 canonical JSON으로 만든 SHA-256이다. user ID, command ID,
벽시계와 조회 순서는 넣지 않는다. 같은 run·program version·fingerprint의 evaluation은 같은 condition 결과를
재사용할 수 있고, 다른 fingerprint는 새 `welfare_evaluation`과 condition evidence를 append한다. 과거 판정은
update/delete하지 않는다.

catalog의 trigger는 eligibility가 참조한 `gameDay/age · household · residence · employment · military ·
income · asset · debt` source의 superset이어야 한다. source가 commit되거나 닫힌 income window가 이동하면 다음
planner가 다시 fact를 모아 fingerprint를 비교한다. 값이 바뀌지 않으면 기존 결과를 재사용하고 바뀌면 새
판정을 남긴다. GET은 DB를 변경하지 않고 현재 game day에 planner가 만든 최신 판정을 읽는다. 신청 명령은
목록에 보인 결과를 신뢰하지 않고 D의 save와 fact authority를 잠근 뒤 같은 evaluator로 다시 판정한다.

신청에 사용한 evaluation ID·period pin·fingerprint·전체 public condition 결과는 application의
`eligibilityAtApplication` evidence로 고정한다. 승인 뒤 D+1 전에 고용·소득·자산·가구가 바뀌어 최신 판정이
ineligible이 되어도 이미 승인된 정액 급여는 취소·환수하지 않는다. 이후 재평가는 미신청 program의 현재
표시와 미래 신청만 바꾸며 과거 application을 소급 변경하지 않는다. 이 첫 slice는 자동수급을 지원하지 않고
항상 플레이어의 명시적 신청이 필요하다.

### 6.4 신청 상태, D+1 지급과 원자성

판정 상태는 `notEvaluated → eligible|ineligible|indeterminate`, application 상태는
`applied → approved|rejected → active → exhausted|terminated`로 분리한다. M4-D1 공개 신청은 locked
재판정이 eligible이고 duplicate group이 비어 있을 때만 application을 만들고, 같은 transaction에서
`applied → approved → active` status event 세 건과 payment 1을 기록한다. ineligible/indeterminate/duplicate
요청은 상태를 일부 남기지 않고 stable API 오류로 rollback한다. `rejected`와 `terminated`는 다음 program
종류를 위한 schema 상태이며 첫 fixture의 정상 경로는 D+1 지급 뒤 `active → exhausted`다.

승인 때 `welfare_payment(applicationId,paymentNo=1,amountKrw=333000,dueGameDay=D+1,status=pending)`와
scheduled settlement를 함께 만든다. settlement kind는 `welfareBenefitPayment`, phase rank는 **150**,
settlement source는 `welfarePayment`다. payload는 다음 네 필드만 허용하며 amount나 fact를 복제하지 않는다.

`{ "version": 1, "welfarePaymentId": "…", "applicationId": "…", "paymentNo": 1 }`

planner는 payload를 strict parse한 뒤 application과 payment를 잠그고 amount·due day·program version을
locked `welfare_payment`에서 다시 읽는다. phase 100의 기존 M2·M3·property-tax settlement 다음,
`loanInstallment=200` 전에 지급하므로 같은 날의 대출 정산은 지급된 333,000원을 shadow wallet에서 볼 수 있다.
성공하면 payment를 `paid`, application을 `exhausted`로 바꾸고 하나의 balanced ledger transaction에
`welfareBenefitIncome -333,000 · wallet +333,000`을 기록한다. account code는 `welfareBenefitIncome`, ledger
source는 `welfareBenefitPayment`이며 source ID는 welfare payment ID다. welfare 지급은 부채·tax obligation을
만들지 않는다.

전역 point-lock 순서의 welfare 구간은
`insurance_claim(id) → welfare_application(id) → welfare_payment(application_id,payment_no,id) →
corporation(id) → … → scheduled_settlement(dueGameDay,id)`로 고정한다. due ID는 lock 전에 수집하고 복수 ID는
위 key로 오름차순 잠근다. payment, settlement source `(welfarePayment,paymentId,paymentNo)`, ledger source
`(welfareBenefitPayment,paymentId)`의 unique key와 application transition unique key가 재시작·큰 step·worker
재시도에서도 한 번만 지급되게 한다. 어느 posting이나 transition이 실패해도 해당 player-day 전체를
rollback한다.

duplicate group claim은 application 승인 시에만 non-null로 설정하고
`(saveId,runRevision,duplicateGroupKey)` unique key로 획득한다. rejected row가 미래 신청을 막지 않고,
approved row의 claim은 exhausted/terminated 뒤에도 null로 되돌리지 않는다. command 재전송은 공통 receipt에서
최초 application result를 반환하고 현재 snapshot만 최신으로 붙이며 payment나 transition을 다시 만들지 않는다.

### 6.5 `0038` schema와 immutable graph

`0038_m4d_welfare.sql`은 다음 catalog graph를 만든다.

- `welfare_fact_definition` — component schema에 허용한 fact type·unit·window·collection bound
- `welfare_program_version` — component, key, application window, root, duplicate group, benefit와 schedule
- `welfare_program_constant` — version별 최대 64개의 typed immutable 상수
- `welfare_program_condition` — version별 최대 32개의 public code, 순서와 strict typed AST
- `welfare_reassessment_trigger` — condition이 읽는 fact source와 다음 planner 재평가 계약

runtime graph는 `welfare_period_pin → welfare_evaluation → welfare_evaluation_condition`과
`welfare_application → welfare_application_transition → welfare_payment`다. evaluation과 condition evidence는
append-only이고 application header와 payment header는 허용된 단방향 상태 전이만 update한다. 모든 runtime
행은 `(save_id,run_revision)` composite FK로 현재 권위를 증명하고 application은 pinned component의 program
version 및 eligibility evaluation을 참조한다. payment는 application과 같은 run·program·금액인지 composite
FK/check/transition trigger로 검증한다.

program/condition key 정규식, AST·상수·program·active-application cardinality, 양의 금액, payment 번호·due day,
fingerprint 64자리 lowercase hex와 상태별 null 교차조건을 DB와 loader 양쪽에서 검증한다. sealed component와
그 child는 update/delete할 수 없고 canonical SHA-256은 fact→program→constant→condition→trigger 순서의
canonical graph를 포함한다. migration은 finance enum/check에
`welfareBenefitIncome · welfareBenefitPayment · welfarePayment`를 추가하고 기존 source/account 의미를 바꾸지
않는다. 기존 life catalog와 run pin 보존, 새 aggregate seal, `newRun` pointer 변경은 같은 migration에서
검증 실패 시 전부 rollback한다.

### 6.6 strict HTTP와 스타일 없는 `/welfare`

`GET /api/welfare/programs`는 query parameter를 받지 않는다. catalog cardinality가 active program 최대
16이므로 pagination이나 silent truncation 없이 `programKey,id` canonical 순서의 전부를 반환한다.
`before|limit`를 포함한 모든 query parameter는 unknown field로 400이다. 응답은
`componentVersionId · gameDay · programs`만 top-level에 두며 각 program은 server가 정한 benefit, 현재
`eligible|ineligible|indeterminate` 상태, `factFingerprint`, public condition 결과 최대 32개, latest
application과 next payment 한 건을 담는다. 내부 AST/policy JSON, 민감한 raw fact와 다른 사용자의 값은 반환하지
않는다. D1은 별도 application history route를 만들지 않는다.

`POST /api/welfare/applications`의 fingerprint는 `lifeledger.life.applyWelfareProgram.v1`이고 body는 공통
`commandId · expectedRunRevision · expectedStateRevision · expectedGameDay`와 `programVersionId`만 받는다.
클라이언트가 fact, condition 결과, amount, due day, duplicate group을 보내는 shape는 400이다. 성공 result는
application ID·program version·status·application/approval game day, `eligibilityAtApplication`의 공개 condition
결과, payment ID·번호·금액·due day·상태를 서버 값으로 반환한다. 새 명령은 state revision을 1 올리고 snapshot을
broadcast하되 D에는 ledger나 wallet 변화가 없다. 동일 command ID·kind·payload·최초 cursor replay는 저장된
result와 최신 snapshot을 `replayed=true`로 반환한다.

wire error는 locked 판정 False 또는 duplicate group 소진에 `ineligible`, Unknown에
`valuationUnavailable`, 존재하지 않거나 다른 user·이전 run·disabled component의 program ID에 동일한 404
`welfareResourceNotFound`를 쓴다. malformed ID/cursor, unknown field/enum/query는 400이고 stale cursor와
command fingerprint 충돌은 기존 공통 오류를 재사용한다. ownership 확인 전에 program/application 존재를
구분해 노출하지 않는다.

`GameSnapshot.life.activeWelfareApplications`는 현재 run의 active application을 application ID 순으로 최대 8건 담는다.
조회는 `LIMIT 9`로 초과를 invariant 오류로 감지하며 8건으로 잘라 성공하지 않는다. D1 fixture는 program 한
건이어서 이 상한에 도달하지 않는다. 미래 component가 동시 active 9건을 만들 수 있게 하려면 snapshot/API
계약을 먼저 새 version으로 확장해야 한다.

`/welfare`는 custom CSS 없이 프로그램명·정액 급여·D+1 일정, 현재 세 가지 판정 상태, public condition별
통과·실패·unknown, 신청 가능 여부, application·payment 상태를 서버 값 그대로 표시한다. 신청 버튼은 eligible
이고 pending command가 없을 때만 활성화한다. outcome-unknown 재시도는 최초 path/body를 보존하고, 고정 DOM
slot을 mount에서 한 번 만든 뒤 hooks/store 구독으로 텍스트와 disabled 상태만 바꾼다. 화면은 fact·자격·지급일을
다시 계산하지 않고 CSS, DOM·routing·실제 network 테스트를 추가하지 않는다.

### 6.7 테스트와 실제 MySQL 8 인수 조건

순수 BDD/DCI 테스트는 AST type·unit·window 검증, depth/node/arity/list/string/collection 상한, Kleene 진리표,
Unknown 전파, canonical fingerprint와 overflow를 검증한다. fixture 경계는 21/22/67/68세,
`none|ended|pendingStart|active`, active이지만 dependent가 있는 경우, income·asset cap의 정확히 같은 값과 1원
초과, residence 부재, `serving`을 각각 고정한다. service 테스트는 trigger 뒤 fingerprint 재평가, 신청 transaction
locked 재판정, run-lifetime duplicate group, `eligibilityAtApplication` 후 상태 변경, D와 D+1 경계,
phase 100→150→200, payment/ledger 한 번, 중간 실패 하루 rollback을 검증한다. protocol 테스트는 strict
GET/POST, 프로그램·condition·snapshot 상한, response correlation, replay·cursor·다른 user/run 404를 검증한다.
테스트 정책에 따라 DOM, routing, 실제 network round trip 테스트는 작성하지 않는다.

격리한 실제 MySQL 8에서는 빈 DB의 0001→0038과 populated 0037→0038을 모두 적용하고 다음을 public HTTP·DB
projection으로 확인한다.

- 기존 welfare-disabled run의 bundle/component/응답이 그대로이고 `newRun`만 welfare v1·life aggregate v2를 pin함
- sealed graph의 canonical hash, update/delete 거절, 잘못된 AST/FK/cardinality/state transition 거절
- six-condition eligible/ineligible/indeterminate, employment가 없는 run의 명시적 `none`, 30 closed-day와
  prior-close 경계, 동일 facts의 동일 fingerprint와 trigger 변경 뒤 새 evaluation
- D 신청에는 cash·ledger 변화가 없고 D+1 phase 150에 wallet +333,000원,
  `welfareBenefitIncome -333,000원`의 balanced ledger와 paid/exhausted 상태가 정확히 한 건 생김
- 승인 뒤 D+1 전에 고용·자산이 바뀌어도 지급하고, 같은 run의 재신청은 막지만 newRun에서는 다시 신청 가능함
- command 응답 유실, 서버 재시작, 작은 step·큰 step 경쟁, settlement 재시도와 강제 posting 실패가 하나의
  application/payment/ledger/receipt로 수렴하거나 전부 rollback함
- 다른 user·이전 run의 program/application/payment ID 비노출, unknown query/field 거절, active 9건 invariant,
  재시작 전후 snapshot·cash·ledger hash 일치

## 7. 결정론적 생애 사건과 보험

### 7.1 사건 생성

생애 사건은 `life_event_version` 카탈로그의 eligibility, hazard weight, cooldown, 최대 발생 횟수, 선택지,
효과 plan으로 정의한다. 사건 종류의 예시는 질병·사고·결혼 제안·출산/입양 제안·가구원 돌봄·상속이지만,
실제 의학 확률이나 성별 차이는 검증된 데이터가 없는 한 직접 계수로 넣지 않는다.

매월 첫날 planner는 활성 카탈로그를 `eventKey` 순으로 훑고, 각 후보에 대해
`H(worldSeed, saveId, runRevision, yearMonth, eventKey, occurrenceNo, "eligibility")`를 사용한다.
추첨 stream은 시장·커리어·매물·법인과 분리한다. 다른 사건을 추가하거나 조회 호출 순서가 바뀌어도 기존
사건의 추첨값이 바뀌지 않는다. 같은 날 여러 사건이 뽑히면 `priority, eventKey` 순으로 적용하며, 서로
배타적인 group은 가장 앞선 한 건만 발생하고 나머지는 그 달 `suppressed`로 기록한다.

즉시 사건은 planner에서 effect를 적용하고, 선택 사건은 `offered`와 기한을 만든다. 상태는
`offered → accepted|declined|expired → resolved`다. 기한까지 명령이 없으면 카탈로그의
`defaultChoiceId`로 `expired` 처리한다. 선택 명령은 canonical command와 expected cursor를 받고,
한 번 resolve된 사건은 다시 선택할 수 없다.

### 7.2 보험

보험 상품 버전은 가입 자격, 보험료 schedule, 보장 event code, deductible, limit, waiting period,
면책 조건과 지급식을 가진다. 실제 상품명이나 보험료를 흉내 내지 않고 가상 상품을 쓴다. 상태는
`pending → active → lapsed|expired|cancelled`이며 미납 grace와 부활 가능 여부는 카탈로그에 명시한다.

사건이 발생하면 당시 active policy와 waiting period를 pin해 `insurance_claim` 후보를 만든다. 지급은
`사건 gross cost → 계약별 deductible → 계약별 limit → 중복보장 coordination order`로 계산하며 계약 ID
순으로 처리한다. 보험금은 먼저 사건 의료비·손해 의무를 줄이고 남는 정액 보장만 지갑에 들어간다.
사건 후 가입한 보험이나 나중에 게시한 상품은 소급 적용하지 않는다.

### 7.3 M4-D2 구현 경계와 첫 사건 fixture

M4-D2는 결정론적 생애 사건의 카탈로그·월 planner·선택·기한 만료·원장 효과와 스타일 없는
`/events-insurance` 화면까지만 수직으로 완성한다. 보험 component와 가입·보험료·claim은 기존
`disabled-m4a-v1`을 유지하고 M4-D3에서 별도 version으로 연다. D2가 보험금이나 claim처럼 보이는 임시 행을
만들지 않으며, 화면도 가입할 수 없는 보험 버튼을 노출하지 않는다.

SQLx `0039`는 active life-event component
`dev-unranked-m4-life-event-2026-v1`과 life aggregate
`dev-unranked-m4-life-catalog-2026-v3`를 새로 게시한다. v3는 M4-D1 life aggregate의 living-cost·welfare와
disabled insurance/corporation을 그대로 복제하고 life-event component만 v1로 교체한다. newRun bridge는
같은 run bundle의 policy·credit·real-estate pin도 그대로 보존한다. 기존
`disabled-m4a-v1`·M4-A~C·D1 run의 bundle과 결과는 바꾸지 않고 `newRun` assignment만 v3로 이동한다.
component와 aggregate는 draft child 전체를 canonicalize한 SHA-256을 가진 뒤에만 seal하며, seal 뒤 header와
definition·choice의 update/delete를 trigger로 거절한다. runtime은 반드시 run에 pin된 component만 읽고 코드
기본 event를 보충하지 않는다.

첫 개발 fixture는 `fictionalDependentCareRequest`이고 공개 이름은 `가족 돌봄 요청`이다. 실제 질병률이나
돌봄 통계를 주장하지 않는 `GAME_BALANCE` 콘텐츠이며 unranked에서만 사용한다. 현재 game day 기준 나이
22~67세, dependent가 한 명 이상, active residence 존재, 병역 `serving` 아님을 모두 만족할 때 월 후보가
된다. `hazardPpm=1,000,000 · maximumOccurrences=1 · cooldownGameDays=365 · priority=100 ·
exclusiveGroupKey=familyCare · offerDurationGameDays=7`이다. 첫 fixture의 100% hazard는 실제 확률을 뜻하지
않고 월 planner와 선택 경계를 확실히 검증하기 위한 값이다. 엔진 단위 테스트는 0·1·999,999·1,000,000 ppm과
여러 event/group을 별도 입력으로 검증한다.

선택은 다음 두 개이고 choice order도 sealed 데이터다.

| choice key | 공개 문구 | 결정 | effect plan |
|------------|-----------|------|-------------|
| `supportNow` | 지금 돕는다 | `accepted` | `fixedWalletExpense`, 120,000원 |
| `decline` | 이번에는 돕지 않는다 | `declined` | `noEffect` |

기본 선택은 `decline`이다. D2의 effect AST는 schema version 1의 `noEffect|fixedWalletExpense`만 허용한다.
`fixedWalletExpense`는 양의 정수 KRW constant와 `lifeEventExpense` 계정만 사용하며 client가 금액·계정·효과를
보내지 않는다. 명시적 `supportNow`는 지갑에 120,000원이 있어야 하고 같은 player transaction에서
`lifeEventExpense +120,000 · wallet -120,000`을 분개한다. 부족하면 `insufficientWalletCash`이고 사건은
`offered`로 남는다. 자동 만료의 default choice는 반드시 `noEffect`여야 component를 게시할 수 있으므로
하루 진행은 잔액 때문에 막히지 않는다. 0원 posting은 만들지 않는다.

### 7.4 카탈로그, entropy와 월 후보 plan

`life_event_definition`은 component, schema version, event key·공개 이름·purpose·ranked availability,
typed eligibility AST, hazard ppm, cooldown, occurrence 상한, priority, exclusive group, offer 기간과 default
choice를 가진다. `life_event_choice`는 definition 아래 choice order·key·공개 문구·decision kind와 strict
effect AST를 가진다. component당 definition은 최대 32개, event당 choice는 2~8개, eligibility AST는
depth 12·node 128을 넘지 못한다. key·enum·금액·기간·cardinality는 DB CHECK와 publish trigger, Rust loader가
같이 검증한다. loader는 알려지지 않은 schema/node/unit/window를 fail-closed하고 sealed DB graph를
코드 fixture와 비교하지 않는다.

eligibility는 M4-D1의 welfare 프로그램을 참조하지 않고 life engine의 versioned fact-registry contract를
재사용한다. D2 schema v1이 허용하는 현재 사실은 `character.age · household.dependentCount ·
residence.exists · military.status`다. 값은 월 plan transaction에서 현재 target game day 기준으로 수집하고,
모르는 권위 값이나 collection 상한 초과는 `indeterminate` 후보가 된다. `indeterminate`를 false로 바꾸거나
사건을 발생시키지 않으며 내부 후보 근거에 상태만 보존한다. 향후 fact나 window를 추가하려면 새 component
schema version을 게시한다.

월 첫날 target day `D`에서 각 definition의 다음 occurrence number를 계산하고 다음 HMAC-SHA256 stream을
독립적으로 평가한다.

`H(worldSeed, "lifeledger.lifeEvent.v1", saveId, runRevision, yearMonth, eventKey, occurrenceNo,
"eligibility")`

문자열은 UTF-8 byte length와 bytes, 정수는 unsigned big-endian으로 canonical encode한다. digest의 첫 u64를
rejection 없는 128-bit multiply-high로 `[0, 1_000_000)`에 매핑해 `rollPpm < hazardPpm`이면 selected다.
event key가 message에 있으므로 definition 추가·정렬, 조회 순서, 다른 도메인의 entropy 사용이 기존 roll을
바꾸지 않는다. engine version이나 encoding을 바꾸면 기존 component를 수정하지 않고 새 schema/stream
version을 게시한다.

`life_event_month_plan`은 save/run/component/year-month마다 하나이고 target game day·authority revision과
완료 상태를 보존한다. `life_event_candidate`는 그 달의 모든 definition을 event-key 순서로 기록하며
`ineligible · indeterminate · notSelected · suppressed · offered` 중 하나다. eligibility가 true인 후보만 roll을
가지며 eligibility false/unknown은 roll이 null이다. selected 후보는 `(priority ASC, eventKey ASC)` 순으로
처리하고 같은 non-null exclusive group에서는 첫 후보만 `offered`, 나머지는 `suppressed`다. suppressed는
발생 횟수나 cooldown을 소비하지 않는다. maximum occurrence와 cooldown은 실제 생성된 instance의
`offeredGameDay`만 기준으로 한다.

승자마다 `life_event_instance` 한 건을 `offered`로 만들고 선택·effect 원본을 그 component에 계속 묶는다.
현재 offered 8건과 새 승자의 합이 snapshot 상한을 넘으면 임의로 자르거나 추가 suppress하지 않고 그 날을
invariant 오류로 rollback한다. candidate·instance·후속 transition은 자동 증가 ID 실행 순서가 아니라 위
canonical key와 unique constraint로 멱등하다. 작은 step, 큰 step, 서버 재시작과 같은 달 재계획은 같은
month plan·candidate·instance 한 건으로 수렴한다.

### 7.5 선택, 만료, 상태와 원자성

instance의 공개 의사결정 상태는 `offered → accepted|declined|expired`이고 effect 적용이 끝나면
`resolved`다. DB는 `life_event_transition`에 중간 상태와 전이 순서를 append-only로 남기고 instance에는 최종
상태·resolution kind·choice·resolved game day를 projection한다. 명시적 선택 transaction은
`offered → accepted|declined → resolved`, 자동 만료는 `offered → expired → resolved`를 모두 기록한다.
중간 전이만 commit되는 상태는 없다. resolved instance는 다시 선택하거나 수정하지 않는다.

`expiresGameDay = offeredGameDay + offerDurationGameDays`는 exclusive다. 현재 save game day가
`expiresGameDay`보다 작을 때만 명시적 선택할 수 있다. 일일 pipeline이 target day를 그 값으로 전진할 때
§10 step 9에서 default choice를 적용한다. 같은 target day에 새로 생성한 사건은 최소 duration 1 때문에 즉시
만료되지 않는다. 기한·default choice·effect 금액은 instance가 참조하는 sealed catalog에서 읽고 현재
`newRun` assignment나 최신 component로 바꾸지 않는다.

명시적 선택은 save, offered instance와 choice를 current-run owner scope로 잠그고 cursor를 다시 검증한 뒤
effect plan, transition, 원장과 command receipt를 한 transaction으로 적용해 `stateRevision`을 정확히 1
올린다. 자동 만료는 그 날의 due settlement·사건 생성·다른 M4 transition과 같은 player-day transaction 및
shadow wallet을 사용한다. effect·posting·candidate·transition 중 하나라도 실패하면 명령 또는 하루 전체가
rollback한다. event별 commit과 savepoint는 없다.

ledger source는 `lifeEventChoice`, occurrence는 instance의 resolution sequence 1이다. source ID는 canonical
decimal instance ID이고 ledger link는 save/run/event/choice/amount를 교차 검증한다. no-effect 선택에는 ledger
transaction을 만들지 않는다. public snapshot과 API는 transition/command/ledger 내부 ID, HMAC input·roll,
world seed, 원시 eligibility fact를 노출하지 않는다.

### 7.6 strict API, snapshot과 기능 화면

`GET /api/life/events`는 strict optional `cursor` 하나만 받고
`{ lifeEventCapability, insuranceCapability, pendingEvents, history, nextCursor }`를 반환한다. 생애 사건
capability는 `deterministicChoices|unavailable`이고 D2 보험 capability는 `unavailable`만 허용한다. 두 값은
현재 run의 life aggregate에 고정된 각 component에서 읽으며 화면이 문자열을 추측하지 않는다. 알 수 없는
active 보험 component는 D3 runtime이 배포되기 전까지 fail-closed한다. 생애 사건이 unavailable인 기존 run은
빈 배열과 null cursor로 성공하고 runtime row를 만들지 않는다. pending은 ID 오름차순 최대 8건이며
`LIMIT 9`로 초과를 invariant 오류로 감지한다.
history는 resolved game day와 ID 내림차순 최대 20건이고 opaque cursor는 component/run과 마지막
`(resolvedGameDay,id)`를 묶는다. 다른 run·malformed·응답과 맞지 않는 cursor와 unknown query는
`invalidCommand`다.

pending event는 `id · eventKey · displayName · offeredGameDay · expiresGameDay · defaultChoiceId · choices`를
공개한다. choice는 `id · displayName · decisionKind · effectSummary`이고 effect summary는
`noEffect|walletExpense(amountKrw)` strict union이다. history는 event identity·offer/resolution day,
`resolutionKind(accepted|declined|expired)`와 선택한 choice summary만 공개한다. 내부 candidate 결과와
eligibility/hazard는 노출하지 않는다. `GameSnapshot.life.pendingEvents`도 같은 pending shape로 최대 8건을
담고 history는 넣지 않는다.

`POST /api/life/events/{eventId}/choices`는 공통 command/cursor에 `choiceId`만 더한 strict body다. path와
choice ID는 canonical decimal이고 금액·결정 종류·effect를 받지 않는다. fingerprint는
`lifeledger.life.resolveEvent.v1`이며 최초 cursor, path event ID와 choice ID를 포함한다. 성공은
`{ result:{ eventId,choiceId,resolutionKind,resolvedGameDay,walletDeltaKrw }, replayed, snapshot }`이다. 같은
command/body replay는 저장 result와 최신 snapshot을 반환하고 transition·원장·revision을 다시 만들지 않는다.
같은 command의 다른 payload는 `idempotencyConflict`다.

malformed/unknown body는 400 `invalidCommand`, 기한이 지났거나 resolved된 current-run 사건은 409
`eventExpired`, 사건에 속하지 않는 choice는 409 `contractConflict`, explicit 비용의 현금 부족은 409
`insufficientWalletCash`, transient lock exhaustion은 409 `busy`다. missing·다른 사용자·이전 run의 event는
모두 404 `eventNotFound`로 구분 없이 응답하고 ownership 확인 전 존재·상태·choice를 노출하지 않는다. 모든
경로는 session cookie를 요구한다.

`/events-insurance`는 custom CSS 없이 capability, pending 사건 문구·기한·선택별 공개 effect, 최근 해결
history와 결과를 표시한다. 선택 버튼은 해당 사건이 pending이고 같은 event의 command가 진행 중이 아닐 때만
활성화한다. outcome-unknown은 최초 path/body를 보존해 재시도하고 성공 snapshot을 store에 반영한다. 보험은
응답의 `insuranceCapability=unavailable`을 `아직 이용할 수 없습니다`로 표시한다. DOM은 mount에서 한 번
만들고 hooks로 고정 slot의 text·hidden·disabled만 갱신하며 DOM·routing·실제 network 테스트와 CSS를
추가하지 않는다.

### 7.7 D2 테스트와 실제 MySQL 8 인수 조건

순수 BDD/DCI 테스트는 HMAC fixed vector와 ppm 경계, event 추가·정렬 독립성, occurrence/cooldown, eligibility
Unknown, priority·exclusive suppression, pending 상한, 기한 exclusive 경계, default choice, fixed expense의
checked arithmetic와 balanced plan을 검증한다. service 테스트는 같은 월 재계획, 작은/큰 step, 명시적 선택과
자동 만료의 원자 전이, replay, 중간 posting 실패 하루 rollback을 고정한다. protocol 테스트는 strict
capability/pending/history/cursor/choice/result와 response 상관관계를 검증한다. 테스트 정책에 따라 DOM,
routing, 실제 network round trip 테스트는 쓰지 않는다.

격리한 실제 MySQL 8에서는 빈 DB의 `0001→0039`와 populated D1 DB의 `0038→0039`를 모두 적용하고 다음을
public HTTP와 DB projection으로 확인한다.

- 기존 run의 disabled life-event pin·복지 결과가 그대로이고 `newRun`만 event v1·life aggregate v3를 pin함
- component/definition/choice canonical hash, update/delete 거절, invalid enum·AST·금액·default effect 거절
- dependent가 없는 run은 후보 ineligible, 있는 run은 첫 월초에 정확히 한 offered instance가 생기며 같은
  HMAC vector·month plan이 재시작과 조회 순서에 무관함
- `supportNow` 선택은 D에 wallet -120,000원과 balanced `lifeEventChoice` 원장 한 건, replay는 추가 행 0건,
  잔액 부족은 event/transition/ledger/receipt/revision 전체 rollback
- `decline`과 D+7 자동 만료는 원장 없이 resolved되고, 작은 step·큰 step이 같은 candidate·transition·snapshot
  hash로 수렴함
- 다른 user·이전 run event/choice 비노출, malformed ID·unknown query/field 거절, pending 9건 invariant,
  서버 재시작 전후 pending/history/cash/ledger hash 일치
- 서버 test/clippy/fmt, 클라이언트 test/typecheck/lint/build와 `git diff --check` 통과

### 7.8 M4-D3 구현 경계, version과 첫 보험 fixture

M4-D3는 가상 보험의 가입·30일 보험료·중도 취소, 사건 발생 시점 보장 고정, 손해액 확정 뒤 청구와 지급을
스타일 없는 `/events-insurance` 화면까지 수직으로 완성한다. 첫 schema는 실제 손해를 넘지 않는
`fixedIndemnity`만 허용한다. 정액 급부, grace 기간, 부활, 자동 갱신, 보험계약 대출, 실제 보험사·상품명과
의학 통계는 범위 밖이며 알 수 없는 값을 코드 기본값으로 보충하지 않는다. v1 catalog는
`graceGameDays=0 · reinstatementAllowed=false`만 게시할 수 있고 이 범위를 넓히려면 새 schema version을
게시한다.

SQLx `0040`은 active 보험 component `dev-unranked-m4-insurance-2026-v1`, 사건 component
`dev-unranked-m4-life-event-2026-v2`, life aggregate
`dev-unranked-m4-life-catalog-2026-v4`를 함께 게시한다. aggregate v4는 D2 aggregate v3의 living-cost,
welfare, corporation component를 그대로 복제하고 event와 insurance만 각각 v2와 v1로 교체한다. 기존
disabled 보험 run, D2 event v1 run과 그 결과는 바꾸지 않고 `newRun` assignment만 v4로 이동한다. 두
component와 aggregate는 모든 child를 canonicalize한 SHA-256을 기록한 뒤에만 seal하며 seal 뒤 header,
fact, product, coverage, event definition과 choice의 update/delete를 trigger로 거절한다.

D3가 보험 component만 활성화하고 D2 event v1을 재사용하면 새 run 시작 transaction의 day 0 월 planner가
가입 화면이 열리기 전에 유일한 사건을 이미 발생시킨다. 그 사건에 나중 계약을 붙이는 것은 §7.2의 비소급
원칙을 어긴다. event v2는 D2 `fictionalDependentCareRequest`의 문구·eligibility·100% hazard·선택·비용을
그대로 복제하되 `maximumOccurrences=2 · cooldownGameDays=30`으로만 바꾼다. day 0 첫 occurrence는 항상
가입 전 사건으로 남고, resolve 또는 D+7 만료 뒤 다음 월초의 두 번째 occurrence가 정상 보장 경로를 연다.
기존 event v1 row와 hash는 수정하지 않는다.

첫 상품은 `fictionalFamilyCareCover`, 공개 이름은 `가족 돌봄 비용 보장`이다. 실제 상품이나 요율을
주장하지 않는 `GAME_BALANCE` fixture이며 unranked에서만 사용한다. 가입 자격은 현재 game day 기준
나이 22~67세, dependent 한 명 이상, active residence 존재, 병역 `serving` 아님이다. D3 보험 fact
schema v1은 이 네 typed fact만 허용하며 D2 event fact나 복지 AST row를 외래 키로 재사용하지 않는다.

| 항목 | 고정 값 |
|------|---------|
| 보장 event·effect | `fictionalDependentCareRequest`의 `fixedWalletExpense` 선택만 보장 |
| 보험료 | 가입일 10,000원 선납, 이후 30 game day마다 10,000원 |
| 계약 기간 | 가입일 D부터 D+360 exclusive, 자동 갱신 없음 |
| waiting | 발생일이 `coverageStartGameDay + 7` 이상일 때 통과 |
| deductible·회당 limit | 20,000원 · 100,000원 |
| 계약 총 limit | 200,000원, paid와 ready reservation을 함께 차감 |
| 청구 기한 | 사건 resolve game day +7 exclusive |
| grace·부활 | 0일 · 불가 |

첫 상품은 같은 save/run에서 active 계약을 하나만 허용한다. 전체 active 보험 계약은 8건, component의 상품은
16건을 상한으로 하며 loader와 DB trigger가 `limit + 1` 상태를 성공으로 자르지 않는다. 상품 eligibility가
unknown이면 가입을 `ineligible`로 바꾸지 않고 `indeterminate`로 공개한 뒤 명령을 fail-closed한다.

### 7.9 계약, 보험료, lapse와 중도 취소

`POST /api/insurance/contracts` 가입은 save, household와 현재 run의 pinned insurance component를 잠그고
자격·중복계약·cursor를 다시 평가한다. 성공 transaction은 `pending → active` 두 transition, 첫
`insurance_premium_charge` paid 행, 계약 기간과 waiting 경계를 만들고
`insurancePremiumExpense +10,000 · wallet -10,000`을 한 원장 transaction으로 기록한다. 첫 보험료가
부족하면 `insufficientWalletCash`이고 contract·charge·transition·ledger·receipt·revision을 모두 남기지
않는다. 성공은 state revision을 정확히 1 올린다.

보험료 cadence는 달력이 아니라 가입일 기준 game-day schedule이다. charge 1은 D에 즉시 결제하고 charge
2~12는 `D+30, D+60, …, D+330`에 `insurancePremium` scheduled settlement로 예약한다. D+360은 계약 종료
exclusive 경계이고 13번째 보험료나 임의의 5일 tail을 만들지 않는다. due 보험료는 phase 250에서 전액만
결제한다. 그 시점 shadow wallet이 10,000원보다 작으면 일부 금액을 가져가거나 원장을 만들지 않고 charge를
`missed`, 계약을 `lapsed`로 전이하며 미래 charge와 settlement를 취소한다. grace가 0이어도 같은 target day
step 6에서 이미 발생한 사건은 직전 paid period의 보장을 받으므로 lapse의
`coverageEndExclusive=targetGameDay+1`이다. 부족한 보험료가 하루 진행 전체를 막지는 않는다.

월초 사건과 보험료가 같은 날이면 사건 offer와 event-time pin이 step 6, 보험료가 phase 250인 step 7이다.
따라서 그 날 보장은 당일 납부 결과가 아니라 직전 paid period와 저장된 exclusive 경계로 판정하고, 당일
missed 결과는 다음 game day 사건부터 반영한다. 첫 fixture의 정상 경로는 D0 가입, D30 두 번째 보험료,
통상 D31인 다음 월초 두 번째 사건 순서다. 작은 step, 큰 step과 재시작이 이 순서를 바꾸지 않는다.

`POST /api/insurance/contracts/{contractId}/cancellations`는 active 계약만 중도 취소한다. D에 취소하면
`coverageEndExclusive=D+1`이고 환급·원장은 없으며 미래 premium charge/settlement만 취소한다. D까지 이미
offer되어 event-time pin이 만들어진 사건과 ready claim은 계약을 취소·lapse·expire한 뒤에도 원래 청구
기한까지 보존한다. 가입·취소 fingerprint는 최초 cursor와 product 또는 path contract ID를 포함한다. 같은
command/body replay는 원장·transition·revision을 다시 만들지 않고 저장 result와 최신 snapshot을 반환하며,
같은 command ID의 다른 payload는 `idempotencyConflict`다.

### 7.10 사건 시점 pin, claim 후보와 indemnity 배분

D3 active 보험 component를 pin한 run에서 event v2 instance를 offer할 때는 선택이나 계약 가입보다 먼저
event instance당 `insurance_claim` header를 `candidate`로 정확히 한 건 만든다. 이 header가 event-time pin의
단일 권위이며 save/run, life aggregate, event·insurance component, event instance와 offered game day를
묶는다. 당시 matching 계약 집합은 contract ID 오름차순 `insurance_claim_contract_pin` child로 복제한다.
child는 product·coverage version, 계약 유효기간, waiting 판정과 deductible·limit을 보존한다. matching
계약이 0건이어도 canonical empty-set digest를 claim header에 기록한다. 이후 가입·취소·lapse,
`newRun` assignment나 catalog 게시가 이 pin을 바꾸지 못한다. 특히 day 0 첫 claim의 empty pin에는 day 0
뒤 가입한 계약을 절대 소급 연결하지 않는다.

사건 offer 시 gross cost는 아직 선택되지 않는다. event resolution이 `noEffect`면 claim은
`notApplicable`, 비용 선택이면 당시 contract pin child만 읽어
계약 ID 순으로 indemnity를 계산한다. 계약 `i`의 원 단위 값은 다음과 같다.

`raw_i = min(max(grossCostKrw - deductible_i, 0), occurrenceLimit_i,
termLimit_i - paid_i - reserved_i)`

`allocation_i = min(raw_i, grossCostKrw - sum(previous allocation))`

deductible은 각 계약의 원 gross cost에 적용하고 마지막 coordination 항이 모든 계약 지급 합계를 실제 손해
이하로 제한한다. 음수·overflow·limit 역전은 checked i128에서 거절한다. amount가 하나라도 양수면 claim은
`ready`가 되고 계약별 allocation을 append-only로 고정하며 그 금액을 term reservation에 더한다. 모두 0이면
`notCovered`다. claim 제출 순서가 allocation을 바꾸지 않도록 ready 전환 transaction에서 전체 배분을
확정한다. active 계약·claim contract pin·allocation은 각각 최대 8건이고 초과는 invariant 오류다.

첫 fixture에서 insured 두 번째 사건의 `supportNow`는 D2와 똑같이 먼저
`lifeEventExpense +120,000 · wallet -120,000` gross 비용을 기록하고 claim 100,000원을 ready로 만든다.
D3 v1은 즉시 지갑 비용만 보장하므로 미결 손해 의무를 새로 가장하지 않는다. 향후 liability형 사건을
추가하면 §7.2대로 그 의무를 먼저 줄이되 새 effect/claim schema를 게시한다. `decline`과 default expiry는
no-effect라 claim이 `notApplicable`이고 reservation이나 원장이 없다.

ready claim의 `filingDeadlineGameDay=resolvedGameDay+7`은 exclusive다. 명시적 청구는 현재 game day가 그
값보다 작을 때만 가능하고, pipeline이 target day를 deadline으로 전진할 때 `ready → expired`와 reservation
release를 같은 player-day transaction에서 수행한다. claim 지급은 reservation을 paid limit으로 옮기고
`wallet +payout · insuranceClaimRecovery -payout`을 source `insuranceClaimPayment`, occurrence 1의 balanced
원장으로 기록한다. claim·allocation·contract aggregate·ledger·receipt·revision 중 하나라도 실패하면 명령
전체를 rollback한다. paid claim은 다시 청구할 수 없다.

### 7.11 DB, 일일 pipeline과 원자성

`0040`의 catalog 경계는 `insurance_fact_definition · insurance_product_version ·
insurance_product_coverage`이고 runtime 경계는 `insurance_contract · insurance_contract_transition ·
insurance_contract_eligibility_pin · insurance_premium_charge · insurance_claim ·
insurance_claim_contract_pin · insurance_claim_allocation · insurance_claim_transition ·
insurance_command_receipt`다. 이름이 달라져도 다음 의미는 지켜야 한다.

- 모든 runtime row는 current save/run과 run-pinned life aggregate·insurance component를 composite FK로
  묶고 claim header는 같은 run의 exact event instance/component만 참조한다.
- product key·공개 이름, fact AST, premium cadence, 기간·waiting·claim window, deductible·occurrence/term
  limit과 coverage event/effect는 strict typed column 또는 bounded canonical JSON으로 저장한다.
- 가입은 D-start typed fact와 eligibility 결과의 canonical digest를 contract-owned pin으로 보존한다.
  aggregate seal은 각 insurance coverage의 event key와 effect kind가 같은 aggregate에 pin될 event
  component에 정확히 존재하는지 검증해 보험 component를 임의의 비호환 event component와 조합하지 못하게
  한다.
- sealed catalog는 child를 포함한 hash와 publish trigger, runtime transition·allocation은 append-only
  trigger, premium·claim 원장은 exact source/reference trigger로 보호한다.
- charge, claim, contract pin, allocation, transition과 command receipt는 canonical unique key로 retry·작은
  step·큰 step·재시작이 한 행에 수렴한다. 삭제나 ID 실행 순서로 멱등성을 만들지 않는다.

§10의 player-day transaction은 월초 event offer와 insurance pin을 step 6에서 먼저 만들고 due settlement를
`기존 M2·M3 100 → welfare 150 → loanInstallment 200 → insurancePremium 250 → leaseRent 300 →
livingCostMonth 400` 순으로 적용한다. 보험료 실패로 lapse해도 뒤 월세·생활비는 같은 shadow wallet에서
계속 실행된다. term expiry는 step 8, ready claim deadline expiry는 사건 default와 함께 step 9에서 적용한다.
전역 잠금은 `insurance_contract(id) → insurance_premium_charge(chargeNo) →
insurance_claim(eventInstanceId,id) → insurance_claim_contract_pin(contractId) →
insurance_claim_allocation(contractId)` 순으로 확장하며 복수 ID는 먼저 수집해 오름차순으로 잠근다.
event resolution과 claim 지급도 save를 먼저 잠그고
이 순서를 건너뛰지 않는다.

runtime은 disabled component와 정확히 알려진 active insurance schema v1만 해석한다. 기존 run은
`unavailable`과 빈 배열로 성공하고 runtime row를 만들지 않는다. sealed이 아니거나 kind·schema가 다르거나
active인데 loader가 모르는 component는 fail-closed한다. client가 보낸 premium, gross cost, deductible,
limit, waiting, payout이나 coverage 판정을 신뢰하지 않는다.

### 7.12 strict API, snapshot과 기능 화면

`GET /api/insurance/contracts`는 strict optional opaque `cursor` 하나만 받고
`{ insuranceCapability, products, contracts, pendingClaims, history, nextCursor }`를 반환한다. capability는
`contractsAndClaims|unavailable`이다. product는 최대 16건이며 ID·key·공개 이름, 서버 판정
`eligible|ineligible|indeterminate`, 공개 reason, covered event 문구, premium/cadence/term/waiting,
deductible·occurrence/term limit과 claim window만 공개한다. 원시 fact AST와 내부 evidence, event roll,
world seed는 노출하지 않는다.

contracts는 생성일·ID 내림차순 최대 20건이고 product identity, `active|lapsed|expired|cancelled`, 시작·waiting·
coverage end, 다음 보험료 일자·금액, paid/reserved/remaining term limit을 공개한다. pending claim은 최대 8건으로
`candidate|ready` strict union이다. candidate는 claim/event identity·offer day와 null gross/payout/deadline,
ready는 양의 gross/payout, 계약별 deductible 요약과 filing deadline을 공개한다. history는
`notApplicable|notCovered|paid|expired` terminal claim을 resolved day·ID 내림차순 최대 20건으로 반환한다.
opaque cursor는 current run/component와 contract,
claim 두 window의 마지막 anchor를 함께 묶고 `LIMIT 21`로 각 초과를 판정한다. 다른 run·malformed·응답과
맞지 않는 cursor나 unknown query는 `invalidCommand`다. unavailable run은 네 배열이 비고 cursor가 null이다.

가입 body는 공통 command/cursor와 `productVersionId`, 취소 body는 공통 command/cursor만 가진다. 가입
성공은 `{ result:{ contractId,productVersionId,status,coverageStartGameDay,waitingEndsGameDay,
coverageEndExclusive,nextPremiumDueGameDay,premiumKrw }, replayed, snapshot }`, 취소는
`{ result:{ contractId,status,coverageEndExclusive }, replayed, snapshot }`이다.
가입 성공의 계약은 항상 active이고 D+30 보험료 회차가 있으므로 result의
`nextPremiumDueGameDay`는 nullable이 아닌 game day 정수다. 계약 목록에서는 종료 상태를 함께 담으므로
같은 필드를 nullable로 유지한다.
`POST /api/insurance/claims`는 공통 command/cursor와 `claimId`만 받고
`{ result:{ claimId,eventId,payoutKrw,paidGameDay }, replayed, snapshot }`을 반환한다. path/body ID는 canonical
decimal이고 money·coverage·status를 받지 않는다.

`GameSnapshot.life`는 `insuranceCapability`, active insurance contracts 최대 8건과 pending claims 최대
8건을 같은 strict summary로 담는다. 전체 contract·claim history는 위 cursor 조회에만 둔다. D2
`GET /api/life/events`의 다섯 필드는 유지하되 active run에서는 서버가 pin에서 읽은
`insuranceCapability=contractsAndClaims`를 반환한다.

malformed ID/body/query는 400 `invalidCommand`, 캐릭터가 없으면 409 `characterRequired`, disabled/unknown
capability는 409 `rateUnavailable`, 자격 불충족·unknown은 409 `ineligible`, 첫 보험료 부족은 409
`insufficientWalletCash`, stale cursor나 상태 충돌은 409 `contractConflict`, 이미 지급·기한 만료·보장 없음은
409 `claimNotCovered`, transient lock exhaustion은 409 `busy`다. missing·다른 사용자·이전 run의
product/contract/claim은 모두 404 `insuranceResourceNotFound`로 구분 없이 응답한다. 모든 경로는 session
cookie, strict JSON과 current-run owner scope를 요구한다.

`/events-insurance`는 기존 사건 기능 DOM에 상품·자격·보험료/기간/보장, 계약 상태·다음 보험료, 가입·취소,
pending claim과 지급 결과를 고정 slot으로 추가한다. 버튼은 서버 capability와 현재 contract/claim 상태,
진행 중 command만 보고 활성화하며 금액·자격·보장 여부를 화면에서 다시 계산하지 않는다. 가입·취소·청구의
outcome-unknown retry는 각각 최초 path/body를 보존하고 성공 snapshot을 store에 반영한다. mount에서 DOM을
한 번 만들고 hooks로 text·hidden·disabled만 갱신하며 CSS, DOM·routing·실제 network 테스트는 추가하지
않는다.

### 7.13 D3 테스트와 실제 MySQL 8 인수 조건

순수 BDD/DCI 테스트는 보험 eligibility unknown, waiting/coverage exclusive 경계, D0·D30…D330 charge와
D360 expiry, phase 250 잔액 배분, lapse/cancel effective day, checked indemnity·deductible·회당/총 limit,
contract-ID coordination, reservation 지급·해제와 balanced premium/claim plan을 검증한다. service 테스트는
day-0 empty claim contract pin의 비소급, D30 보험료와 다음 월 사건 순서, resolve별 claim 상태,
가입·취소·청구 replay, small/big step과 posting 실패 전체 rollback을 고정한다. protocol 테스트는 strict
capability/product/
contract/pending/history/cursor, snapshot과 command/result correlation을 검증한다. 테스트 정책에 따라 DOM,
routing과 실제 network round trip 테스트는 쓰지 않는다.

격리한 실제 MySQL 8에서는 빈 DB의 `0001→0040`과 populated D2 DB의 `0039→0040`을 모두 적용하고 다음을
public HTTP와 DB projection으로 확인한다.

- 기존 disabled/v1 run의 event·insurance 응답과 hash는 그대로이고 newRun만 aggregate v4, event v2와
  insurance v1을 pin함
- component/product/coverage/event v2 canonical hash, seal 뒤 update/delete 거절, invalid AST·enum·기간·
  premium·deductible/limit·coverage 조합 거절
- day-0 첫 사건의 empty contract digest, 그 뒤 가입해도 첫 사건 claim은 notCovered, 가입 10,000원 원장과
  replay 추가 행 0건, 잔액 부족 전체 rollback
- D30 두 번째 보험료와 다음 월초 두 번째 claim contract pin, waiting 통과, supportNow gross 120,000원과 ready
  100,000원, claim 뒤 wallet recovery와 balanced 원장·term limit 정확성
- decline/default expiry의 notApplicable, claim deadline exclusive 만료와 reservation release, 보험료 부족
  lapse와 중도 취소 뒤 새 사건 비보장, 발생 전 pin된 claim은 계속 지급 가능
- 다른 user·이전 run contract/claim 비노출, malformed/unknown 입력, stale/idempotency 충돌, 서버 재시작 전후
  contract·charge·pin·claim·cash·ledger 응답 hash 일치
- 서버 test/clippy/fmt, 클라이언트 test/typecheck/lint/build와 `git diff --check` 통과

## 8. 개인도산·면책·재기

M4는 실제 법원 절차를 축약한 **교육·오락용 게임 상태기계**를 제공한다. 법률 자문이나 실제 사건의 결과를
예측하는 기능이 아니며 `/recovery` 화면과 모든 case 응답에 이 고지를 노출한다. 법령·고시에서 가져온 값,
공식 안내를 바탕으로 한 해석, 재미와 구현 가능성을 위한 게임 규칙을 다음 세 provenance로 구분한다.

- `LEGAL_STATUTE` — 법령·고시의 문언과 공식 수치다. 기준일과 원문을 `policy_rule_source`로 연결한다.
- `OFFICIAL_GUIDANCE` — 법원·행정기관의 절차 안내다. 법률상 절대 금지처럼 표현하지 않는다.
- `GAME_BALANCE` — 자동 승인, 처리 시점, 지원 범위와 신용 제한처럼 게임이 정한 단순화다. 법정 값으로
  표시하지 않고 immutable life component에 둔다.

### 8.1 2026 정책 기준과 해석 경계

M4-E1의 기준일은 **2026-07-28**이고 simulation date가 2026-02-01 전이면 첫 fixture를
`policyUnavailable`로 닫는다. 첫 policy set은 기존 finance policy graph를 exact-clone한 뒤 아래 insolvency
rule과 출처만 더한 `dev-unranked-kr-individual-insolvency-2026-v4`다.

| 구분 | E1에 고정하는 값·판정 | provenance와 근거 |
|------|------------------------|-------------------|
| 자동 압류금지 현금성 재산 | 현금·압류금지 예금 사이 상호 공제를 전제로 개인별 합계 2,500,000원 | `LEGAL_STATUTE`, [민사집행법 시행령 §§2·7](https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joBrNo=00&joNo=0002&lsiSeq=283025&urlMode=lsScJoRltInfoR), 2026-02-01 시행본 |
| 6개월 생계비용 재산 면제 상한 | 2026년 4인가구 기준 중위소득 6,494,738원 × 400,000ppm × 6개월 = 15,587,371.2원. 법령에 1원 미만 처리 규칙이 없어 상한을 넘지 않도록 전체 곱을 마지막에 내려 **15,587,371원**으로 사용 | 산식은 `LEGAL_STATUTE`, 최종 원 단위 내림은 `GAME_BALANCE`. [채무자회생법 §383](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1024710677), [시행령 §16](https://law.go.kr/LSW/lsInfoP.do?lsiSeq=263089&viewCls=lsRvsDocInfoR), [보건복지부 2026 기준 중위소득](https://www.mohw.go.kr/board.es?act=view&bid=0026&list_no=1487112&mid=a10409020000) |
| 비면책채권 | 조세, 벌금·과료·형사비용·추징금·과태료, 고의 불법행위 손해배상, 중과실 생명·신체 침해 손해배상, 근로자 임금·퇴직금·재해보상 등, 악의로 누락한 채권, 양육·부양비 | `LEGAL_STATUTE`, [채무자회생법 §566](https://www.law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1033090089) |
| 학자금대출 | 일반 `studentLoan`은 위 비면책 목록에 없으므로 E1의 일반 무담보 면책 대상이다. 취업 후 상환 학자금대출 비면책 조항도 2022-01-01부터 삭제됐다 | `LEGAL_STATUTE`, [2021년 개정법과 적용례](https://law.go.kr/LSW/lsRvsDocListP.do?chrClsCd=010202&lsId=009930&lsRvsGubun=all) |
| 담보권 | 유치권·질권·저당권·동산채권담보권·전세권은 파산절차 밖에서 행사하고, 담보실행 뒤 확정된 부족액만 일반 파산채권이 될 수 있다 | `LEGAL_STATUTE`, [채무자회생법 §§411~413](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1018130089), [법원 2026 FAQ](https://www.scourt.go.kr/nm/minwon/faq/FaqViewAction.work?bulletinid=48701&functioncode=2120&mode=B&pageIndex=1&pageIndexB=1&searchWord=&search_gubun=16) |
| 신규 신용 | 신청·선고·면책만으로 모든 신규대출을 법률상 일률 금지하는 규칙은 두지 않는다. 지급불능을 숨긴 신용거래의 면책불허가 위험만 법적 근거로 보존한다 | `LEGAL_STATUTE`, [채무자회생법 §564](https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1023718955) |
| 면책 공공정보 | 더 구체적인 서울회생법원 FAQ의 5년 안내를 채택하되, 대법원 일반 안내에 과거 7년 문구가 남아 있고 현행 한국신용정보원 규약 원문은 확인하지 못했다는 한계를 표시한다 | `OFFICIAL_GUIDANCE`, [서울회생법원 FAQ](https://slb.scourt.go.kr/dcboard/new/DcNewsViewAction.work?cbub_code=000221&gubun=47&pageIndex=1&searchWord=&seqnum=164) |

2,500,000원은 자동 제외이고 15,587,371원은 신청과 법원 결정이 필요한 별도 상한이다. 법이 둘을 하나의
확정 합계로 선언한 것은 아니다. E1은 같은 원화를 두 번 세지 않도록 자동 보호분을 먼저 떼고 남은 지갑에서
추가 생계비 면제를 전액 승인하는 **게임 규칙**을 사용한다. 따라서 지원 fixture의 최대 보호 지갑은
18,087,371원이지만, 이 숫자를 실제 사건의 보장액이나 자동 권리로 표시하지 않는다.

신청·선고 중 hard credit lock과 면책 뒤 1,825일 lock도 법정 대출금지가 아니라 `GAME_BALANCE`다. UI는
`게임상 신규 신용 제한`이라고 표시한다. 공공정보 5년은 근거 설명에만 쓰며 정확한 실제 신용점수 하락량,
금리, 승인 가능성을 모델링하지 않는다.

### 8.2 M4-E1 구현 경계 — 지갑·일반 무담보채무 청산

첫 수직 슬라이스는 `cashOnlyLiquidation` 하나만 지원한다. 다음 조건을 **모두** 만족해야 case를 준비할 수
있고 하나라도 판정할 수 없으면 추정하지 않고 `insolvencyCompositionUnsupported`로 거절한다.

- 현재 run이 active insolvency component와 §8.1 policy set을 pin했고 simulation date가 2026-02-01 이상이다.
- `readOnly=false`, `status=defaulted`, `productKind=studentLoan|unsecuredLoan`인 계약이 한 건 이상 있다.
- 잔액이 있는 모든 대출이 위 집합에 속한다. `legacyDebt`, `leaseDepositLoan`, `mortgage`, active·delinquent·
  restructured 계약과 담보권·lien은 지원하지 않는다.
- 지갑은 0원 이상이고, 지원 채권 합계가 지갑 현금보다 크다. 이는 실제 지급불능 법리를 완전히 재현한 것이
  아니라 E1의 결정론적 게임 eligibility다.
- open 금융계좌의 현금, cash-product 원금, 주식·채권·금·연금 position, 보증금 반환채권, 부동산 holding,
  미체결 매각대금이 모두 0이다. 빈 open 계좌 자체는 허용한다.
- 미납 세금, 생활비·월세 arrear, 보증금 반환의무, 임금·양육·손해배상처럼 E1이 분류하지 못하는 비대출
  의무가 없다. 존재하면 버리지 않고 unsupported reason으로 노출한다.
- 같은 run에 `prepared|filed|liquidation|discharged|rebuilding` case가 없다.

`studentLoan`을 보수적으로 비면책 처리하지 않는다. 현행 §566에 없는 예외를 게임이 새로 만들 수 없으므로
지원되는 generic student loan은 일반 무담보채권과 함께 면책한다. 반대로 담보부채는 개인책임과 담보권을
분리해야 하므로 계약 전체를 임의 면책하지 않는다. 이미 담보가 실행되고 확정 부족액을 별도 일반채권으로
계약화하는 기능은 E2 이후다.

E1은 회생계획, 소득기반 변제, 금융계좌·유가증권·부동산 환가, 우선·후순위 채권, 비면책채권의 병존,
사해행위·누락채권·면책불허가 심사, 채권자 이의와 반복 파산을 구현하지 않는다. 이 조합은 단순히 0원으로
처리하지 않고 fail-closed한다.

### 8.3 case 준비와 고정된 구성

`POST /api/insolvency/cases`는 파산을 자동 제출하지 않는다. 현재 save cursor를 잠그고 eligibility를 다시
판정한 뒤 `status=prepared` case, 지갑 asset 한 건, 지원 loan별 claim을 같은 transaction에서 만든다.
준비 응답은 다음 값을 보존한다.

- `policySetId · lifeCatalogSetId · insolvencyComponentVersionId`
- 적용한 `automaticCashProtectionRuleId · additionalLivingExpenseExemptionRuleId`
- `preparedGameDay · compositionSha256`
- `walletCashKrw · automaticProtectedKrw · additionalProtectedKrw · liquidatableKrw`
- `totalClaimKrw · claimCount · unsupportedReasons`

claim allowed amount는 준비 시점 계약의
`remainingPrincipalKrw + accruedInterestKrw + accruedFeeKrw`이며 세 부분을 따로 복제한다. claim은
`generalUnsecured` class와 loan contract ID를 가진다. 지갑, 계약, 미납 bucket, 금융계좌·position·주거·세금
의무의 canonical 요약을 hash에 포함한다. 제출 때 같은 집합과 금액을 다시 읽어 hash가 다르면
`insolvencyCompositionChanged`로 거절하며 저장된 숫자를 현재 사실처럼 사용하지 않는다. 플레이어는 기존
case를 withdraw하고 새로 준비할 수 있다.

한 run에 history case는 여러 건일 수 있지만 non-terminal case는 하나뿐이다. `prepared` case가 다른 명령을
전역으로 막지는 않는다. 대신 cursor나 composition이 바뀐 제출은 실패한다. case가 `rebuilding`에 들어간
뒤에는 회복 종료 전 새 case를 만들 수 없다.

### 8.4 보호 현금, 배분과 면책 원장

순수 규칙은 checked i128로 다음 순서만 사용한다.

```text
automaticProtected = min(walletCash, 2_500_000)
cashAfterAutomatic = walletCash - automaticProtected
additionalProtected = min(cashAfterAutomatic, 15_587_371)
liquidatable = walletCash - automaticProtected - additionalProtected
```

`liquidatable`은 일반 무담보 claim allowed amount 비율로 안분한다. 각 claim 몫은
`floor(liquidatable × claimAllowed / totalClaim)`이고 남은 1원은 loan contract ID 오름차순으로 한 원씩
더한다. 따라서 같은 class의 특정 채권자를 플레이어가 고르거나 생성 순서 밖의 난수로 우대할 수 없다.
claim별 배분액은 §4.2의 기존 대출 상환 allocator에 넣어 비용 → 이자 → 원금 순서와 bucket 정합성을
재사용한다.

`submit` action은 한 MySQL transaction에서 다음 transition을 모두 기록하고 최종 current status를
`rebuilding`으로 만든다.

`prepared → filed → liquidation → discharged → rebuilding`

이 즉시 심사·선고·면책은 실제 법원 소요기간이 아니라 `GAME_BALANCE` 단순화다. 각 transition의 sequence와
같은 game day를 감사 이력에 남긴다.

1. 지갑에서 총 distribution을 차감하고 claim별 `insolvencyDistribution` payment·allocation을 기록한다.
   0원 distribution은 payment나 원장을 만들지 않는다.
   default 전에 물질화되지 않은 미래 원금·이자·비용은 obligation bucket ID가 없는 `current*`
   allocation으로 기록하되, DB trigger가 이 형태를 `insolvencyDistribution` payment에만 허용한다.
2. distribution 원장은 `wallet`과 기존 loan principal/interest/fee 계정을 사용하고 각 claim allocation을
   reference한다.
3. 남은 지원 채무는 계약·installment·obligation bucket을 삭제하지 않고 각각 `discharged`로 끝낸다.
   `remainingPrincipalKrw · accruedInterestKrw · accruedFeeKrw · interestRemainder=0`으로 만들고 미래
   `loanInstallment` settlement를 취소한다.
4. 면책 총액이 양수면 `insolvencyDischargedDebt`와 `insolvencyDischargeGain` 두 posting의 합이 0인
   `insolvencyDischarge` 원장 한 건을 만든다. principal·interest·fee split은 claim row에 보존한다.
5. `save.debtKrw`, credit snapshot과 case 합계를 같은 transaction에서 다시 검증한다. case total은
   `originalClaim = distributed + discharged`여야 한다.

응답 유실 replay는 동일 case·transition·payment·ledger ID를 반환하고 돈이나 state revision을 다시
움직이지 않는다. 일부 claim만 처리한 중간 commit은 없다.

### 8.5 재기 상태와 신용 overlay

신청일을 D라 할 때 `creditRestrictionEndExclusive = D + 1,825` game day다. 제한은
`D <= currentGameDay < endExclusive`에서만 적용한다. 이는 5 calendar year를 정확히 계산한 법정 기간이
아니라 첫 fixture의 `GAME_BALANCE` 값이다.

- `prepared`는 신용을 잠그지 않는다.
- submit 뒤 `rebuilding`은 모든 신규 loan quote·실행을 `creditRestricted`로 거절한다. 기존 credit model의
  raw units나 history를 덮어쓰지 않고 insolvency overlay를 먼저 판정한다.
- 마지막 제한일을 마치는 daily pipeline은 case를 `recovered`로 한 번 전이한다. cursor가
  `endExclusive`인 명령부터 overlay를 적용하지 않는다.
- `recovered` 뒤 실제 신규대출 가능 여부는 당시 credit band, 소득, DSR과 상품 심사를 다시 거친다. 회복이
  승인이나 금리 정상화를 보장하지 않는다.

M4-B credit model v3의 `legalProcedure` penalty 0은 E1에서도 그대로 둔다. E1은 raw credit score를
법원절차와 혼합하지 않고 명시적 overlay로 제한한다. 실제 점수 변화 모델을 도입하려면 새 credit model을
게시하는 별도 E2 설계가 먼저다.

### 8.6 version·schema와 migration 경계

SQLx `0041_m4e_insolvency.sql`은 기존 sealed graph를 수정하지 않고 다음을 추가한다.

- `life_component_version.componentKind`에 `insolvency`를 추가하고 active
  `dev-unranked-m4-insolvency-2026-v1`을 게시한다.
- `life_catalog_set.insolvency_component_version_id`는 legacy aggregate와 sealed hash를 보존하기 위해 nullable
  FK로 추가한다. 기존 schema v1 catalog는 행도 hash도 수정하지 않고 null을 `구조적으로 존재하지 않음`으로
  해석한다. 이는 새 기능의 disabled default가 아니며 runtime은 `unavailable`로만 읽는다.
- 새 draft insert·seal trigger는 schema v2 branch에서 non-null insolvency component와 exact kind를 요구한다.
  새 aggregate `dev-unranked-m4-life-insolvency-2026-v5`만 schema v2 canonical manifest에 새 component ID를
  포함한다. 이후 만드는 모든 catalog는 v2이고 null insert를 허용하지 않는다.
- 현재 newRun finance policy를 exact-clone해 §8.1 rule/source를 가진 policy v4를 게시한다. 기존 run의
  `policySetId · lifeCatalogSetId`는 움직이지 않고 `run_rule_bundle_assignment(newRun)`만 두 새 ID와 증가한
  assignment revision을 원자적으로 가리킨다.
- runtime은 `insolvency_case`, `insolvency_case_transition`, `insolvency_asset`, `insolvency_claim`,
  `insolvency_distribution`, `insolvency_command_receipt`로 분리한다. claim·asset은 case의 save/run/component/
  policy tuple을 FK로 되풀이해 다른 run graph를 섞지 못하게 한다.
- case/claim/asset/transition/receipt는 immutable identity와 허용된 상태 전이만 DB trigger로 보호한다.
  종료 case를 update/delete하거나 sealed policy/component를 바꾸는 경로는 없다.

Migration은 빈 DB `0001→0041`과 populated `0040→0041` 모두에서 existing run pin, newRun의 policy/life 두
pointer 외 모든 assignment field, source link, canonical hash와 draft guard 복구를 barrier table로 검증한다.
`sqlx::migrate!`가 `migrations/` 변경을 다시 embed하도록 `build.rs` 추적도 유지한다.

### 8.7 strict API, snapshot과 기능 화면

| 경로 | E1 계약 |
|------|---------|
| `GET /api/insolvency` | component availability, current eligibility/reasons, current case summary |
| `POST /api/insolvency/cases` | `cashOnlyLiquidation` case 준비 |
| `POST /api/insolvency/{caseId}/actions` | strict `submit|withdraw` action |
| `GET /api/insolvency/{caseId}` | case totals, policy provenance, transition history max 16 |
| `GET /api/insolvency/{caseId}/claims` | `(caseId, claimId)` signed cursor, ID 오름차순, page max 20 |
| `GET /api/insolvency/{caseId}/liquidations` | asset·distribution history, page max 20 |

mutation은 공통 command/cursor를 받고 fingerprint를 각각
`lifeledger.life.insolvencyCase.v1`, `lifeledger.life.insolvencyAction.v1`로 고정한다. action fingerprint에는
case ID와 action을 포함한다. unknown field/enum, malformed ID/cursor는 400, 비인증은 401, 다른 user·run이나
없는 resource는 같은 404, stale cursor·상태 충돌·idempotency payload 충돌은 409다. 지원하지 않는 구성은
422 `insolvencyCompositionUnsupported`로 반환하고 내부 계약 존재 여부는 reason aggregate 이상 노출하지
않는다.

`GameSnapshot.life.insolvency`는 `availability · eligibility · reasons(max 16) · currentCase|null`만 담는다.
case summary는 `id · procedureKind · status · preparedGameDay · submittedGameDay|null · walletCashKrw ·
protectedCashKrw · distributedKrw · dischargedKrw · creditRestrictionEndExclusive|null`이다. claim 목록과 source
URL은 상세 API로 분리한다.

스타일 없는 `/recovery` 화면은 고지, 현재 eligibility reason, 보호/청산/채권 합계, 준비·제출·철회 버튼,
transition·claim·distribution pagination, 재기 남은 game day를 제공한다. 서버가 계산한 금액과 가능 여부만
표시하고 client가 보호액·배분·신용 제한을 재계산하지 않는다. DOM은 mount에서 한 번 만들고 기존 hooks와
store 구독으로 텍스트·disabled·고정 행만 갱신한다.

### 8.8 잠금·일일 순서와 인수 조건

case 준비·제출은 §10.2의 기존 M2·M3·M4 전역 잠금 순서를 그대로 소비한다. 먼저 필요한 mutable resource
ID를 모두 수집하고 각 기존 module의 순서로 잠근 뒤 `insolvency_case → insolvency_claim(id)`을
`scheduled_settlement` 앞에 추가한다. 별도 insolvency store가 금융계좌→대출→주거처럼 두 번째 순서를
만들지 않는다. submit이 loan 상태와 지갑을 바꾸므로 같은 save의 advance·transfer·loan·housing command와
직렬화한다.

순수 BDD/DCI 테스트는 다음을 고정한다.

- 2,500,000원과 15,587,371원 exclusive 경계, 전체 곱 마지막 내림, checked overflow
- claim 비례 안분과 잔여 1원 contract ID tie-break, 0원 distribution
- studentLoan 포함, mortgage·leaseDepositLoan·legacyDebt와 비대출 의무 fail-closed
- composition hash 변경, state transition, withdraw, 1,825일 restriction exclusive 경계
- original = distributed + discharged, payment·discharge ledger 합 0과 debt projection 일치

실제 MySQL 8 인수는 old run disabled 보존, 새 run policy v4·life v5 pin, 지갑이 보호 상한보다 100,001원 많고
지원 defaulted debt가 현금보다 큰 fixture를 사용한다. 준비/replay/변조/stale/withdraw를 확인한 뒤 submit에서
100,001원만 비례 배분하고 나머지를 면책해 계약·bucket·settlement·원장·snapshot을 검증한다. 1일×N과 큰
step, 서버 재시작 전후 restriction 종료 hash가 같아야 한다. 다른 user/run 비노출, strict DTO/cursor,
bounded page, sealed graph 보호, fresh/populated migration, 서버 test/clippy/fmt와 클라이언트
test/typecheck/lint/build도 완료 조건이다.

### 8.9 후속 E2 범위

E2는 이 최소 경계를 넓히기 전에 별도 policy와 설계를 추가한다. 후보는 소득 기반 개인회생,
비면책·우선채권 병존, 담보 실행 뒤 deficiency, 금융자산·부동산 환가, 실제 calendar-year 공공정보 기간,
면책불허가·누락채권, 반복 사건과 관찰기간 조건이다. unsupported 구성을 기존 E1 코드의 숨은 기본값으로
열지 않는다.

## 9. 단순 법인

M4의 법인은 개인과 분리된 최소 원장으로 자본 배분과 급여/배당 선택을 체험하게 한다. 세부 경영은 M5다.

- `corporation` 상태: `draft → active → dormant|insolvent → dissolved`
- 설립 명령: 업종 템플릿, 자본금, 등록 비용, 대표자를 검증하고 개인 지갑에서 법인 현금으로 출자
- 월 손익: `(corpSeed, corporationId, operatingMonth, stream)`으로 결정한 업종 매출에서 카탈로그 고정비와
  선택한 운영규모 비용을 뺀다.
- 대표 급여: M3 payroll 계산기를 재사용하되 민간 `employment_contract/payroll_record`를 가장하지 않고
  `corporation_payroll_record`를 만든다. 같은 transaction에서 개인 `employment_income_event` source
  `corporationOfficerPayroll`과 법인 비용/개인소득 양쪽 원장을 연결한다.
- 배당: 결산된 배당가능이익 범위에서만 명령으로 지급한다. 개인 쪽은 M2 금융소득 event source
  `corporationDividend`로 기록해 다음 금융소득 assessment가 소비하며, 기존 금융상품 분배금으로 가장하지
  않는다.
- 법인세·등록 관련 규칙: policy 데이터로 계산하고 실제 숫자를 코드 기본값으로 두지 않는다.

개인과 법인 사이 임의 이체는 금지한다. 출자, 급여, 비용상환, 배당이라는 typed command만 허용하며 양쪽
ledger transaction은 같은 MySQL transaction의 correlation ID를 가진다. M4는 고객·직원·재고·법인대출을
만들지 않는다.

## 10. 일일 planner·멱등성·잠금

### 10.1 하루의 고정 순서

시장 일봉과 월 매물처럼 공유 불변 캐시는 player transaction 전에 준비할 수 있다. 한 플레이어의 하루는
다음 순서를 **한 MySQL transaction**에서 계획하고 적용한다.

1. `save`의 world, run/state revision, game day, M3 커리어 상태와 pinned rule bundle을 잠근다.
2. 오늘까지 due인 모든 M2·M3·M4 payload를 strict parse하고 잠글 ID를 수집한다.
3. §10.2 순서로 행을 잠근 뒤 due 집합·payload·cursor가 바뀌지 않았는지 다시 확인한다.
4. 정책 평가일이면 전날 마감 상태로 연간 세금·보유세·복지 period pin을 만든다.
5. 오늘 시장·부동산 가격으로 금융·부동산·담보 상태를 평가한다.
6. 월 첫날이면 생활비 청구, 사건 후보, 법인 월 손익과 반복 정산을 고정한다.
7. due settlement를 `(due_game_day, phase_rank, settlement_id)` 순서로 하나의 shadow balance plan에서
   실행한다. 기존 M2·M3 kind는 같은 `phase_rank=100` 안에서 기존 ID 순서를 보존하고,
   `welfareBenefitPayment=150 · loanInstallment=200 · insurancePremium=250 · leaseRent=300 ·
   livingCostMonth=400`으로 고정한다. 따라서 같은 날 M2·M3 결과가 먼저 반영되고 복지 지급 → 정기 대출
   상환 → 보험료 → 월세 → 당월 필수 생활비 → 기존 essentialArrear → 당월 선택 생활비 순서가 된다.
   생활비·대출·임대차·보험·복지·세금도 kind별 별도 commit을 하지 않는다.
8. 당일 납부 결과로 연체일을 갱신하고, 임대차 lifecycle을
   `renewalNotice 500 → termRenewal 600 → terminationReview 700` 순으로 적용하고 보험 계약의 term
   expiry를 확정한 뒤 신용 units와 insolvency plan progress를 갱신한다.
9. 가구·거주·사건의 기한 만료와 명시적 default choice, ready 보험 claim의 filing deadline 만료를
   적용한다.
10. 원장·상태·후속 settlement·event log를 기록하고 game day/state revision을 각각 1 올려 commit한다.
11. commit 뒤 bounded snapshot 하나만 SSE로 보낸다.

앞 정산이 만든 가상 지갑·계좌·부채·가구 상태를 뒤 정산이 읽는다. 어느 계산·posting·conditional transition이
실패해도 그 날 전체를 rollback한다. settlement별 savepoint나 사건별 commit은 없다.

### 10.2 전역 잠금 확장

M2 잠금 순서 뒤에 M3가 확정한 순서를 합성하고, M4 행은 다음 상대 순서를 지킨다.

`save → household → household_member(id) → residence → property_holding(id) → lease_contract(id) →
lease_contract_term(term_no) → lease_rent_charge(charge_no) → lease_arrear(due_month,id) →
lease_lifecycle_action(due_game_day,phase_rank,id) → loan_contract(id) →
loan_installment(contract_id, installment_no) → life_event_instance(id) → insurance_contract(id) →
insurance_premium_charge(contract_id,charge_no,id) → insurance_claim(event_instance_id,id) →
insurance_claim_contract_pin(claim_id,contract_id) → insurance_claim_allocation(claim_id,contract_id) →
welfare_application(id) → corporation(id) → insolvency_case →
insolvency_claim(priority, id) → 기존 scheduled_settlement(due_game_day, id) → 연도별 tax rows`

최종 구현 전에 M2·M3 전체 목록과 합친 단 하나의 전역 순서를 `store` module contract에 둔다. 부모보다
자식을 먼저 잠그지 않고 복수 ID는 먼저 수집해 오름차순으로 잠근다. 불변 policy·catalog·market row는
가변 잠금 순서에 넣지 않는다.

모든 mutation은 M0/M2의 canonical `commandId`, strict payload fingerprint, `command_identity`와 receipt를
재사용한다. 권위 상태 tuple은 `(marketWorldId, runRevision, stateRevision, gameDay)` 네 부분이지만
`marketWorldId`는 인증된 save와 pinned bundle에서 서버가 읽는 값이다. 공개 command body는 기존과 동일한
`expectedRunRevision · expectedStateRevision · expectedGameDay` 세 필드만 받고 client가 world ID를 보내지
않는다. 복합 명령의 원장·계약·사건은 command ID 기반 source identity를 가지며 응답 유실 재시도에서 한
건으로 수렴한다. 일일 자동 step은 settlement/event 고유키로 멱등하다.

## 11. strict API와 bounded snapshot

모든 request는 알 수 없는 필드, 알 수 없는 enum, null/누락 교차조건, 범위 밖 integer를 거절한다.
서버는 클라이언트가 보낸 세금·신용·DSR·LTV·보험금·순자산 계산값을 신뢰하지 않는다. mutation은 공통
command/cursor를 받고 `{ result, snapshot }` envelope와 stable error code를 반환한다.

주요 경로는 다음과 같다.

| 경로 | 역할 |
|------|------|
| `GET/PUT /api/life/budget` | 현재 생활비 산정 근거 조회, 허용 band 변경 |
| `POST /api/life/arrears/{id}/payments` | 필수 생활비 부족 의무의 즉시 일부·전액 상환 |
| `GET /api/credit` | credit band, 공개 reason, 대출 계약 요약 |
| `GET /api/loans/products`, `POST /api/loans/quotes` | 카탈로그와 서버 심사 quote |
| `POST /api/loans`, `POST /api/loans/{loanId}/prepayments` | 실행·조기상환 |
| `GET /api/loans/{loanId}`, `GET /api/loans/{loanId}/installments` | 계약 상세와 cursor 기반 상환표·납부 이력 |
| `GET /api/housing/listings` | 현재 월·지역의 bounded 매물 |
| `POST /api/housing/purchases`, `POST /api/housing/sales` | 매수와 매도 order |
| `POST /api/housing/leases`, `POST /api/housing/moves` | 임대차와 이사 |
| `GET /api/housing/holdings/{id}/tax-events` | 적용 rule·과세표준·세액을 가진 cursor 기반 세금 이력 |
| `GET /api/welfare/programs`, `POST /api/welfare/applications` | 자격 근거와 신청 |
| `GET /api/life/events`, `POST /api/life/events/{id}/choices` | pending 사건과 선택 |
| `GET/POST /api/insurance/contracts`, `POST /api/insurance/claims` | 보험 가입·청구 |
| `POST /api/insurance/contracts/{id}/cancellations` | active 보험 계약 중도 취소 |
| `POST /api/insolvency/cases`, `POST /api/insolvency/{id}/actions` | 도산 신청·절차 action |
| `GET /api/insolvency/{id}`, `GET /api/insolvency/{id}/claims`, `GET /api/insolvency/{id}/liquidations` | case 상세와 cursor 기반 채권·청산 이력 |
| `POST /api/corporations`, `POST /api/corporations/{id}/payouts` | 단순 법인과 급여/배당 |
| `GET /api/corporations/{id}`, `GET /api/corporations/{id}/months` | 법인 상태와 cursor 기반 월 손익·지급 이력 |

`GameSnapshot.life`에는 현재 화면에 필요한 bounded summary만 둔다. 생활비 당월 1건, 활성 거주 1건,
부동산 보유 최대 카탈로그 상한, 활성 loan/welfare와 보험 계약 최대 8건, pending 보험 claim·생애 사건을
각각 최대 8건, 개인 insolvency 상태 1건, 단순 법인 1건을 포함한다. 종료 계약·과거 사건·전체 신용
이력·세금 근거는 cursor pagination
조회로 분리한다. life snapshot의 active 배열은 DB/cardinality 제약으로 상한을 강제하고 `limit + 1`로
읽어 초과하면 잘라서 성공하지 않고 invariant 오류로 감지한다. 예외적으로 `essentialArrear`는 §3.2의
20건 상환 window와 전체 합·`hasMoreActiveArrears`를 함께 반환한다. 기존
`finance.pendingSettlements`와 M3 `pendingCareerSchedule`은 전체 history가 아니라 가장 가까운 20건 window로
명시된 projection이므로 안정된 실행 순서로 잘라 반환한다.

주요 실패 코드는 `ineligible · insufficientWalletCash · debtServiceLimit · collateralLimit ·
incomeUnavailable · valuationUnavailable · creditRestricted · contractConflict · residenceRequired · eventExpired · claimNotCovered ·
insuranceResourceNotFound · insolvencyStateConflict · rateUnavailable · busy · invalidCommand`다. 한국어
message와 code는 분리하고,
소유권 실패는 존재 여부를 드러내지 않는다.

## 12. 기능 중심 화면

M4 화면은 사용자 정의 CSS와 시각적 스타일링 없이 다음 기능을 끝까지 조작하게 한다.

공개 client route는 `/life`, `/loans`, `/housing`, `/welfare`, `/events-insurance`, `/recovery`,
`/corporation`으로 고정한다. 대시보드는 각 route로 가는 링크와 현재 bounded summary만 제공하고 상세
history를 중복 렌더링하지 않는다.

- 지역·가구·CPI 산정 근거와 항목별 생활비 band, 다음 청구액
- 대출 상품, DSR/LTV 심사 근거, 계약별 상환표·연체·조기상환
- 월별 지역 매물, 매수·매도 order, 임대차·보증금·월세·이사
- 부동산 취득·보유·양도세의 적용 policy 기준일과 계산 내역
- 복지 eligible/ineligible/indeterminate 근거와 신청·지급 상태
- pending 생애 사건의 선택지·기한, 보험 계약·보험료·청구 결과
- 파산/회생 절차, 채권 목록, 보호/청산 자산, 변제 진행과 재기 조건
- 단순 법인 설립, 월 손익, 대표 급여·배당

DOM은 mount에서 한 번 만들고 hooks와 store path 구독으로 텍스트·disabled 상태·고정 행 슬롯만 갱신한다.
화면이 세금·상환·자격을 재계산하지 않는다. DOM·라우팅·실제 네트워크 왕복·snapshot 테스트는 작성하지
않는다.

## 13. 테스트와 실제 MySQL 검증

### 13.1 단위·protocol 테스트

테스트는 순수 규칙과 서비스 planner에만 둔다.

- CPI·지역·가구 계수, 중간 i128, 월말·중도 시작 일할, remainder와 필수/선택 부족 처리를 검증한다.
- equal principal, integer-searched level payment, bullet, 변동금리 reset, 조기상환, 마지막 1원과 연체
  bucket 순서를 고정 벡터로 검증한다.
- 시작 부채가 한 번만 계약화되고 aggregate가 계약·원장 합과 항상 같은지 검증한다.
- DSR/LTV 포함 source, 분모 0, ppm 내림, 담보 선순위와 quote 후 cursor 변경 거절을 검증한다.
- 매물 entropy, 매도 지연, 임대보증금 자산/부채, 이사 원자성과 부동산 세금 rule pin을 검증한다.
- 복지 AST type check, 알 수 없는 fact, trigger 재평가, 중복수급과 동일 fingerprint 결과를 검증한다.
- 사건 stream 독립성, 추가 카탈로그가 기존 사건을 바꾸지 않는지, 충돌 priority, default choice와 보험
  보장 순서를 검증한다.
- 도산 상태의 허용/금지 전이, 회생 1원 배분, 청산 순서, 면책 제외 채무와 rebuilding을 검증한다.
- 일일 shadow plan의 두 번째 M4 정산이나 M3 연동을 강제 실패시켜 하루 전체 rollback을 검증한다.
- strict tagged payload, 알 수 없는 field/version, command replay/fingerprint 충돌, bounded array를 검증한다.

30년 회귀는 제품 규칙 검증용 고정 시드 몇 개와 통계 분포용 다수 고정 시드를 분리한다. 전자는 최종 원장
hash·계약 상태·순자산을 byte-for-byte 비교하고, 후자는 파산률·주거 선택·보험 가입 같은 플레이테스트
지표를 보고할 뿐 임의의 현실 범위를 자동 정답으로 단정하지 않는다.

### 13.2 실제 MySQL 8 스모크

PII 없는 격리 MySQL 8에서 다음을 검증한다.

- 빈 DB와 M3 완료 DB에서 forward migration, 기존 M0~M3 런·원장·시장 경로 보존
- sealed policy/catalog/model의 update/delete 거절과 run bundle pin
- aggregate debt·순자산과 loan/lease/property/ledger 재대조
- 같은 세이브의 진행·조기상환·매수·이사·사건 선택·도산 action 경쟁이 전역 잠금 순서로 수렴하는지
- 중복 command, worker 재시작, due 집합 변경, 중간 실패 뒤 하나의 계약·원장·receipt만 남는지
- 다른 사용자·이전 run의 property, contract, claim, case ID가 조회·mutation에서 노출되지 않는지
- 고정 seed의 30년을 작은 step, 30일 step, 온라인 배속으로 각각 실행해 최종 hash가 같은지

서버 test/clippy/fmt와 클라이언트 test/typecheck/lint/build도 통과해야 한다.

### 13.3 M4-A 완료 기록 (2026-07-27)

M4-A는 schema, 순수 생활비 규칙, 일일 정산 runtime, strict API와 스타일 없는 `/life` 화면까지 완료했다.

- SQLx `0024→0026`은 빈 MySQL 8에서 `0001→0026` 전진하고, 실제 M3 trigger 356개를 보존한 clone에서도
  적용됐다. 최종 schema는 migration 26개, table 157개, trigger 439개이며 기존 v1~v3 compatibility와
  v4 active run bundle을 각각 보존한다.
- public HTTP로 캐릭터 시작, 9개 항목 월 확정, 다음 달 예산 변경, 현금 부족 월말 정산, 수동 일부·전액
  상환을 실행했다. 4개월 동안 24개 연체를 만든 fixture에서 API는 앞 20개와 전체 합을 반환했고, 가장
  오래된 연체를 완납한 뒤 숨겨진 다음 ID가 같은 20개 window에 들어왔다.
- 예산과 연체 상환 명령은 같은 payload 재전송에서 저장된 result와 최신 snapshot을 반환했고, 같은 command
  ID의 다른 payload는 `idempotencyConflict`, 잔액 부족은 `insufficientWalletCash`로 거절됐다.
- 실제 DB의 월 4개·항목 36개는 header 합계와 일치했고 모든 필수/선택 outcome 불변식을 지켰다. 수동 상환
  3건은 모두 `applied`였으며 payment 합, 연체 잔액, `save.debtKrw` projection이 일치했다. 공개 장부 11개
  transaction과 life source link는 모두 균형·소유자 검증을 통과했다.
- 서버 단위 테스트 806개와 clippy/fmt, 클라이언트 17 suite·258개 테스트와 typecheck/lint/build,
  `git diff --check`를 통과했다. 인증·소유권 scope, strict command identity, bound SQL, 오류 비노출도
  security checklist로 다시 확인했다.

### 13.4 M4-B1·B2 완료 기록 (2026-07-27)

M4-B1·B2는 versioned catalog와 schema, 순수 상환·신용 규칙, 시작 계약화부터 일일 상환·연체·신용
runtime까지 완료했다. M4-B3의 조회·심사·수동 명령과 `/loans` 화면은 이 기록의 범위가 아니다.

- SQLx `0027→0029`는 대출 상품·신용 model·계약·상환표·납부 bucket·세금 의무를 추가하고, 기존 opaque
  debt를 read-only legacy 계약과 권위 원장으로 bridge한다. 빈 M4-A schema와 실제 2-save upgrade에서
  sealed catalog/model/manifest, run bundle 보존, 알 수 없는 부채의 fail-closed를 검증했다. 1,024 byte를
  넘는 manifest는 정렬된 `JSON_ARRAYAGG`로 hash해 MySQL 기본 `GROUP_CONCAT` 절단에 의존하지 않는다.
- 새 시작 요청 v2는 exact `startingLoans` union으로 상품 version과 원금을 선택한다. v1의
  `character.studentLoanKrw/creditLoanKrw` 요청과 fingerprint는 그대로 보존하며 두 shape의 혼합, 중복 상품,
  비정규 순서와 알 수 없는 필드를 거절한다.
- 실제 public HTTP에서 학자금 1,200만원과 무담보 300만원을 시작했다. 첫 월말 현금 부족 run은 두 계약을
  연체시키고 credit을 `limited`로 낮췄으며, 다음 날 변동금리 reset과 미래 회차 재계산을 수행했다. 현금
  3,000만원 run은 첫 회차 175,523원을 정상 배분했고 남은 원금 14,857,394원이 `save.debtKrw`와 일치했다.
  같은 진행 명령 replay는 추가 신용 history나 납부를 만들지 않았다.
- 금융소득세·연말정산 부족액은 `tax_obligation`과 `taxObligationLiability` posting을 먼저 만든 뒤
  outstanding으로 전환하며, 대출 원금·필수 생활비 연체와 함께 단 하나의 권위 debt projection으로
  검증한다.
- 서버 테스트 864개와 clippy/fmt, 클라이언트 17 suite·261개 테스트와 typecheck/lint/build,
  `git diff --check`를 통과했다. 실제 스모크용 DB는 검증 후 제거했다.

### 13.5 M4-B3 완료 기록 (2026-07-27)

M4-B3는 상품·신용 조회에서 quote, 신규 무담보 대출 실행, 조기상환, 계약 상세와 상환표·납부 이력,
스타일 없는 `/loans` 기능 화면까지 완료했다.

- strict 상품·신용 응답은 pinned credit model과 현재 run만 읽고, quote는 DSR·소득·신용 사유와 첫 회차를
  서버가 확정한다. 실행은 같은 game day의 quote를 최신 cursor에서 재검증해 계약·60회 상환표·원장·현금·
  부채 projection을 한 transaction으로 만들었다. quote·실행의 동일 command replay와 다른 payload 충돌,
  cursor 변경 거절을 실제 public HTTP와 MySQL 8에서 확인했다.
- 조기상환은 수수료·지갑 차감·납부 allocation·복식원장·schedule을 서버가 다시 계산한다. 실제 fixture에서
  equal-principal 학자금 1,500만원 중 100만원을 `reduceTerm`으로 상환해 112회만 남겼고, level-payment
  무담보대출은 200만원과 2만원 수수료를 차감한 뒤 800만원·60회를 `recalculatePayment`로 재작성했다.
  남은 학자금 1,400만원 전액상환은 120개 회차와 settlement를 모두 취소하고 계약을 `paidOff`로 만들었다.
- 같은 선상환 command의 replay는 납부·allocation·원장·revision을 늘리지 않았다. 다른 payload는
  `idempotencyConflict`, 전액상환 뒤 재요청과 1원 잔액만 남기는 재산정 불가 요청은 `contractConflict`,
  현금 0원 fixture는 `insufficientWalletCash`로 거절됐고 실패 요청은 어떤 command receipt도 남기지 않았다.
  세 납부의 amount와 allocation 합, 세 `loanPrepayment` transaction의 posting 합 0, active loan 합과
  `save.debtKrw=8,000,000`을 다시 대조했다.
- owner/current-run scoped 상세 API는 전액상환 계약도 `finalInstallmentDueGameDay=null`과 immutable
  `prepaymentAllowed=true`로 반환했다. dual window는 cancelled 회차의 `remainingDueKrw=0`, applied payment
  DESC, kind별 allocation 집계와 `v1.l{loanId}.i{before}.p{before}` cursor를 보존했다. `i0|p0` exhausted
  window, path와 다른 cursor·unknown query 400, missing/foreign/prior-run/no-character의 동일 404
  `loanNotFound` 경계를 검증했다.
- 클라이언트는 response 상관관계·금액 합·정렬·cursor를 zod에서 strict 검증하고, outcome-unknown 명령을
  같은 path/body로 복구한다. `/loans`는 서버 산정 quote·실행·선상환 결과와 계약별 최대 50개 회차·납부
  window를 표시하며 DOM·라우팅·네트워크 테스트나 CSS를 추가하지 않았다.
- M4-B3 클라이언트 경계는 20 suite·343 tests와 typecheck/lint/build를 통과했다. 최종 합본 서버 검사는
  926 tests와 clippy/fmt를 통과했으며 이 수에는 다음 단계에서 먼저 추가한 M4-C1 순수 코어 10 tests가
  포함된다. bound SQL, session 인증, 현재 run 소유권, 동일 404 비노출, opaque command/ledger ID와 민감정보
  비공개를 security checklist로 다시 확인했고 `git diff --check`도 통과했다.

### 13.6 M4-C1 완료 기록 (2026-07-27)

M4-C1은 결정론적 지역 가격·임대료 지수, 월별 유한 매물, strict 조회 API와 스타일 없는 `/housing`
기능 화면까지 완료했다. 매수·임대차·이사 명령은 설계한 순서대로 C2 이후에만 연다.

- SQLx `0031`은 네 지역의 typed profile과 허용 주택 유형, strict manifest, 지역별 daily cursor,
  immutable 지수·매물·offer, 월 catalog 완료 header를 추가했다. 빈 MySQL 8의 `0001→0031` 전진과 실제
  M4-B DB clone의 전진을 모두 통과했고, 이 과정에서 `0023`의 오래된 canonical guard SHA 두 개도 현재
  fixture와 일치하도록 바로잡았다. 기존 run의 `disabled-m4a-v1` pin은 보존하고 `newRun`만
  `dev-unranked-m4-real-estate-2026-v1`으로 이동했다.
- 순수 코어는 SHA-256 counter entropy, unbiased rejection sampling, non-zero 63-bit listing ID, signed
  remainder를 가진 일별 지수, 월별 12개 매물과 `sale · jeonse · monthlyRent` offer 회전을 구현했다.
  store는 지역 series cursor와 월 header를 잠그고 저장된 모든 행을 재검증한 뒤에만 완료 catalog를
  공개한다. 앞선 run이 미래 지수를 만든 뒤 느린 run이 과거를 조회하는 경계와 MySQL 1205·1213 재시도도
  별도 회귀 테스트로 고정했다.
- 실제 public HTTP에서 서로 다른 두 active 사용자가 같은 1월 수도권을 동시에 최초 조회해 바이트 단위로
  같은 응답을 받았다. DB에는 daily 1개, 완료 header 1개, listing 12개와 offer 12개만 남았고 세 offer
  종류는 네 건씩이었다. 반복 조회와 농촌 조회도 같은 불변식을 지켰다.
- 한 run을 game day 31의 2월로 전진한 뒤 수도권 daily는 `0..31`의 연속 32개, cursor는 32, 1·2월
  catalog와 매물은 월별 정확히 12개였다. game day 0에 남은 다른 run은 기존 1월 응답을 그대로 받았고,
  서버 재시작 전후의 1월·2월 응답도 각각 byte-identical이었다. daily·listing·완료 header의 변경·삭제
  시도는 immutable trigger가 모두 거절했다.
- disabled 기존 run은 `rateUnavailable`, null index와 빈 매물로 성공했고 해당 model의 cache는 한 행도
  생성하지 않았다. no-character는 409, unknown query·region은 400이며, 조회 전후 player
  `stateRevision · gameDay · cash · debt · ledger · residence`는 바뀌지 않았다. 공개 응답과 OpenAPI에는
  seed, entropy, profile 원시 입력, DB timestamp가 없고 Housing 응답 및 중첩 schema 일곱 개가 모두
  등록됐다.
- 클라이언트는 zod strict 계약으로 서버 응답의 지역·정렬·offer shape·금액·상한을 검증하고, `/housing`은
  최대 24개 행을 mount에서 한 번 만든 뒤 hooks로 내용과 hidden 상태만 갱신한다. 매수·임차 form이나 CSS,
  DOM·라우팅·네트워크 테스트는 추가하지 않았다.
- 최종 합본은 서버 943 tests와 clippy/fmt, 클라이언트 21 suite·360 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. session 인증, 현재 run/model pin, bound SQL, strict query, 소유자 상태
  불변과 스모크 credential 비포함을 security checklist로 다시 확인했다.

### 13.7 M4-C2a 완료 기록 (2026-07-27)

M4-C2a는 현금 전세 계약과 기존 보증금 반환, 새 보증금 지급, 이사비, residence 전환을 한 transaction으로
처리하고 strict API와 스타일 없는 `/housing` 기능 form까지 완료했다. 월세·연체·전세자금대출은 각각
C2b·C2c에 남긴다.

- SQLx `0032`는 기존 C1 model을 그대로 보존하면서 lease profile과 지역별 이사비까지 seal한
  `dev-unranked-m4-real-estate-lease-2026-v2`, tenant `lease_contract`, residence·원장 연결과 immutable
  trigger를 추가했다. fresh MySQL 8의 `0001→0032`에서 v2는 지역 profile 4개·허용 주택 유형 10개·lease
  profile 1개·moving cost 4개를 가졌고 manifest·projection JSON과 SHA가 일치했다. `newRun`만 v2로
  이동하고 C1 v1은 listing-only capability를 유지했다.
- fresh 검증 중 `0023`의 pre-augmentation guard SHA 두 개가 당시 `0022` fixture와 어긋난 회귀, 같은
  migration connection에서 `CREATE OR REPLACE VIEW` 뒤 manifest trigger가 이전 projection plan을 쓰는
  MySQL 경계, lease posting trigger가 비임대차 `wallet`까지 막는 회귀를 발견했다. guard는 정확한 이전
  fixture SHA로 복구하고, view 교체 직후 trigger를 재생성하며, 비임대차 분기는 lease 전용 계정만 막도록
  최소 수정했다. 각 실패는 player transaction을 남기지 않았고 수정 뒤 fresh migration과 정상 캐릭터
  시작을 다시 통과했다.
- public HTTP의 첫 이동은 game day 0에서 `contractConflict`로 부작용 없이 거절된 뒤 day 1에 농촌 전세
  보증금 142,955,341원과 이사비 300,000원을 적용했다. 지갑은 143,255,341원 감소하고 보증금을 포함한
  순자산은 이사비만큼 감소했다. 같은 command body는 `replayed=true`, 같은 ID의 다른 payload는
  `idempotencyConflict`, 같은 날 새 이사는 `contractConflict`였으며 실패 command는 identity·receipt·
  lease·원장을 남기지 않았다.
- 1월 생활비 1,142,000원은 첫 이사 뒤에도 최초 rent-free residence에 고정됐다. game day 31의 2월은
  농촌·전세 residence를 pin해 총 1,061,098원과 housing 61,304원을 확정했다. day 32에는 광역시 전세로
  다시 이동해 기존 보증금 142,955,341원을 반환하고 새 보증금 146,101,945원과 이사비 600,000원을
  처리했다. 2월 pin은 농촌 residence를 계속 가리켰고 lease 기간은 `[1,32) · [32,null)`, residence는
  `[0,1) · [1,32) · [32,null)`로 빈틈 없이 이어졌다.
- 두 leaseMove 원장은 transaction별 합이 0이고 누적 보증금 자산 146,101,945원은 활성 계약과 일치했다.
  최종 지갑은 9,851,856,055원, 부채 0원, 순자산 9,997,958,000원이었으며 활성 lease와 residence는 각각
  한 건뿐이었다. 현금 0원 사용자는 `insufficientWalletCash`로 거절되고 state·계약·원장이 바뀌지 않았다.
  서버 재시작 전후 state·현재 lease·매물 응답은 각각 byte-identical했고 재시작 뒤 원 command replay도
  같은 receipt를 반환했다. profile·moving cost·manifest·lease history 변조는 모두 SQLSTATE `45000`으로
  거절됐다.
- 최종 합본은 서버 956 tests와 clippy/fmt, 클라이언트 22 suite·382 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. session 인증, current-run 소유권, strict JSON·canonical ID, bound SQL,
  replay 격리, 이전 run 비노출, checked 산술과 원장 projection을 security checklist로 다시 확인했다.

### 13.8 M4-C2b1 완료 기록 (2026-07-27)

M4-C2b1은 open-ended 월세 계약, 다음 달 1일 phase 300 청구, typed 연체와 수동 상환, 스타일 없는
`/housing` 기능 화면까지 완료했다. 고정기간·갱신 안내·연체 기반 계약 종료 검토는 C2b2에 남긴다.

- SQLx `0033`은 C2a v2를 byte-identical로 보존하면서 월세 profile, 월세 청구·연체·지급, 정산·원장
  불변식을 추가하고 `newRun`만 sealed `dev-unranked-m4-real-estate-rent-2026-v3`로 옮겼다. 정상 UTF-8
  fresh `0001→0033`은 migration 33개, table 189개, trigger 538개로 전진했다. v1·v2·v3 manifest와
  projection은 각각 byte-identical이고 v3는 지역 4개·허용 주택 유형 10개·전세와 월세 profile 각 1개·
  이사비 4개를 가진다.
- public HTTP에서 day 1 월세 입주는 보증금 60,575,928원과 이사비 800,000원만 즉시 지급하고 첫 월세
  1,590,118원을 day 31에 예약했다. 첫 청구는 전액 납부됐고, 다음 청구는 지갑 795,059원을 모두 지급한 뒤
  같은 금액의 연체를 만들었다. 수동 상환 397,529원과 397,530원은 부채를 정확히 0원으로 줄였다.
- 지갑 0원 상환은 `insufficientWalletCash`, 초과액·완납 ID는 `contractConflict`, 같은 command의 다른
  payload는 `idempotencyConflict`로 부작용 없이 거절됐다. 같은 body replay는 같은 payment ID와 저장된
  result를 반환했고, 서버 재시작 뒤에도 최신 snapshot과 `replayed=true`로 수렴했다.
- 연체가 남은 채 새 월세로 이사해도 과거 계약 연체는 상환 가능 상태로 유지됐다. 이전 계약의 미래 청구와
  settlement는 `leaseEnded`로 취소되고 cancellation ledger는 없었다. 새 계약의 다음 월세 1,180,986원은
  공개 이체 API로 지갑을 0원으로 만든 fixture에서 전액 연체됐으며 0원 wallet posting은 생성되지 않았다.
- 전액·부분·0원 월세와 두 수동 상환의 원장은 모두 posting 합 0이었다. `leaseRentExpense`, `wallet`,
  `leaseArrearLiability` 금액과 charge·arrear reference, `save.debtKrw`가 DB 권위 projection과 일치했다.
  첫 실제 character-start smoke에서 essential arrear와 lease arrear의 생성 열 이름이 서로 뒤바뀐 debt
  조회를 발견해 각 권위 열로 바로잡고 동일 fresh DB·HTTP 경로를 처음부터 다시 통과시켰다.
- state·현재 lease·매물 응답은 서버 재시작 전후 각각 byte-identical이었다. 클라이언트는 capability와
  offer별 strict zod shape, 월세 입주, 최대 20개 연체와 일부·전액 상환, outcome-unknown retry를 제공하고
  CSS·DOM·라우팅·실제 network 테스트는 추가하지 않았다. 최종 합본은 서버 972 tests와 clippy/fmt,
  클라이언트 22 suite·402 tests와 typecheck/lint/build, `git diff --check`를 통과했다. session 인증,
  current-run 소유권, strict JSON·canonical ID, bound SQL, 실패 receipt 비생성과 민감정보 비공개도 다시
  확인했다.

### 13.9 M4-C2b2 완료 기록 (2026-07-27)

M4-C2b2는 12개월 고정기간, 30일 전 갱신 안내와 같은 조건 자동 갱신, 월세 연체 60일 종료 검토를
권위 lifecycle로 연결하고 스타일 없는 `/housing` 읽기 화면까지 완료했다. 갱신 수락·거절, 임대료 인상과
강제퇴거는 추가하지 않았으며 다음 부동산 경계는 C2c 전세자금대출이다.

- SQLx `0034`는 v1~v3 projection과 manifest를 byte-identical로 보존하고 sealed
  `dev-unranked-m4-real-estate-lifecycle-2026-v4`와 term/action/review 이력을 추가했다. 정상 UTF-8 fresh
  `0001→0034`는 migration 34개, table 190개, trigger 549개로 전진했으며 v4 전세·월세 profile은 모두
  `fixedTermAutoRenew · 12개월 · 30일 전 안내`, 월세만 `oldestActiveArrearAge · 60일`을 가진다.
- 순수 규칙은 최초 계약일 anchor에 term number를 곱해 월말 clamp와 윤년에도 기간이 drift하지 않게 했다.
  HTTP day 1 계약은 term 1 `[1,366)`과 안내 day 336을 만들었고, day 336에는 안내가 정확히 한 번 게시되고
  day 366에는 계약·residence·원장 변경 없이 term 2 `[366,731)`과 다음 action 한 벌로 갱신됐다.
- day 365 월세 연체는 59일째인 day 424까지 검토가 없고 정확히 60일째 day 425에 `underReview`를 열었다.
  1원 일부 상환은 검토와 남은 금액을 유지하고 전액 상환은 같은 transaction에서 `arrearsCleared`로
  resolve했다. 전액 상환 command replay는 같은 payment ID와 저장 result를 반환했다.
- 두 번째 검토가 열린 day 515에 새 월세로 이사하자 이전 term과 미래 action은 `leaseEnded`로 닫히고 검토도
  resolve됐지만, 이전 계약 연체는 새 계약 snapshot에 계속 남아 수동 상환 가능했다. 새 계약은 독립 term 1과
  다음 월세·안내·갱신 action을 정확히 한 벌 만들었다.
- 서버 재시작 전후 state·현재 lease·매물 응답 SHA-256은 각각 동일했다. 실제 새 게임 전환 smoke에서
  `leaseRent` 전용 settlement trigger가 일반 `newRun` cleanup보다 먼저 실행돼 rollback되는 회귀를 발견했다.
  charge와 lifecycle을 먼저 닫고 전용 trigger가 `newRun` 사유를 검증하도록 고친 뒤 fresh DB에서 run 2 시작,
  이전 charge·settlement·term·action의 `newRun` 이력과 새 run 비노출을 다시 확인했다.
- due payload를 먼저 검증한 뒤 관련 계약 ID를 수집해 오름차순 point lock하고, 금융 phase 100~400 뒤
  lifecycle 500~700과 credit을 적용한다. overdue review schedule gap은 DB trigger에 의존하지 않고 명시적
  invariant 오류로 차단한다. session 인증, current-run 소유권, strict JSON·canonical ID, bound SQL,
  checked 산술, 내부 lifecycle ID와 민감정보 비공개도 security checklist로 다시 확인했다.
- 최종 합본은 서버 982 tests와 clippy/fmt, 클라이언트 22 suite·410 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. 클라이언트는 legacy null과 v4 상관관계를 strict zod로 검증하고 current term,
  게시된 안내와 open review만 표시하며 CSS·DOM·라우팅·실제 network 테스트는 추가하지 않았다.

### 13.10 M4-C2c 완료 기록 (2026-07-27)

M4-C2c는 전세자금대출 전용 quote, 보증금 직접 지급, 기존 linked loan 대체상환을 임대차 이동 transaction에
결합하고 스타일 없는 `/housing` 기능 form까지 완료했다. 일반 무담보대출 실행과 전세대출 실행 경계는
분리했으며, 다음 부동산 경계는 C3 매수·담보대출이다.

- SQLx `0035`는 기존 credit v1·v2와 real-estate v1~v4 manifest를 바꾸지 않고 sealed
  `dev-unranked-m4c2c-credit-2026-v3`와 `dev-lease-deposit-fixed-bullet-2026-v1`을 게시했다. 새 run만
  credit v3와 real-estate v4를 함께 pin하며, 상품은 보증금 800,000ppm 한도·고정 400bp·24개월
  만기일시상환·수수료 0원이고 일반 대출 실행 API에서는 거절된다.
- 전용 quote는 listing·상품·원금을 서버에서 재평가해 `creditRestricted → collateralLimit →
  incomeUnavailable → affordabilityLimit → eligible` 순서로 결정한다. 법정 DSR은 무주택 C2c 경계에서
  `regulatoryDsrApplied=false`이고, 별도 GAME_BALANCE 상환여력은 대체상환 뒤 12개월 원리금과 신규 이자를
  검증 연소득의 400,000ppm과 비교한다. DB check도 eligible/affordabilityLimit에 완전한 비율 증거와 결정에
  맞는 경계를 요구하고, 그 전 단계만 nullable affordability를 허용한다.
- fresh 35개 migration DB에서 공개 커리어 API만으로 연 46,000,000원 active 고용을 만든 뒤 소득 없음,
  197,162,222원 담보 한도 초과, 50,000,000원 적격 quote를 확인했다. 첫 financed move는 보증금
  246,452,778원에 대출 50,000,000원을 직접 지급해 지갑 `-197,252,778원`, 부채 `+50,000,000원`을
  정확히 만들었고 독립 `loanOrigination` 원장은 없었다.
- day 30 첫 이자 21,917원을 phase 200에서 납부한 뒤, 다음 전세 이동은 기존 50,000,000원을 반환 보증금에서
  상환하고 신규 60,000,000원을 실행했다. 다음 날 현금 월세로 이동하자 그 60,000,000원도 같은 command에서
  자동상환됐고 active monthly lease의 `depositLoanId`는 null, save 부채는 0이 됐다. 세 leaseMove 원장은
  모두 합계 0이고 최종 wallet posting 합은 save cash와 일치했다.
- quote와 입주 replay는 추가 row나 revision을 만들지 않았고 같은 ID의 다른 payload는
  `idempotencyConflict`였다. 서버 재시작 뒤 `(runRevision=1,stateRevision=60,gameDay=31)`의 월세 계약·현금·
  부채가 그대로 복구됐다. 이 과정에서 fresh HTTP가 발견한 credit schema v4 기본 파서와
  `incomeUnavailable + affordability=null` trigger 회귀를 수정하고 같은 DB 경로를 처음부터 다시 통과했다.
- 최종 합본은 서버 991 tests와 clippy/fmt, 클라이언트 22 suite·437 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. session·current-run 소유권, foreign quote 비노출, bound SQL, checked 산술,
  direct-funding 원장과 receipt 소유권을 다시 확인했다. 인증 사용자의 새 command ID 기반 durable quote
  증폭은 기존 unsecured quote와 같은 운영 rate-limit 경계로 남으며, 외부 공개 전 계정별 제한을 추가한다.

### 13.11 M4-C3 완료 기록 (2026-07-27)

M4-C3는 owner-occupied 단독 보유 capability, 현금 매수와 주담대 quote·직접 지급, 소유 residence와
holding·lien, 부대비용 원장과 순자산 projection, 스타일 없는 `/housing` 매수 기능까지 완료했다. 매도와
취득·보유·양도세는 C4에 남긴다.

- SQLx `0036`은 기존 real-estate v1~v4와 credit v1~v3를 보존하고 sealed
  `dev-unranked-m4-real-estate-purchase-2026-v5`, `dev-unranked-m4c3-credit-2026-v4`, 30년 고정 4%
  `dev-mortgage-fixed-level-payment-2026-v1`을 게시했다. 새 run만 두 model을 함께 pin하며 compatibility
  run의 조회·임대차·대출 계약은 바꾸지 않는다.
- 주담대 quote는 exact sale offer와 400,000/700,000ppm 지역 LTV proxy, 가격별 총액 cap, 상품 cap,
  차주단위 DSR gate와 전기간 고정금리 stress 0을 서버에서 다시 계산한다. 600,000,000원 요청은
  `collateralLimit · requiredBuyerCashKrw=0`, 100,000,000원 요청은 `eligible`로 저장됐고 quote 조회·replay는
  cursor를 전진시키지 않았다.
- 현금 매수는 매매가·1% 부대비용·이사비를 지갑에서 차감했고, 담보 매수는 대출 원금을 지갑 수입으로
  만들지 않고 매도인 지급액에 직접 충당했다. 두 경로 모두 holding·owner residence·원장·receipt·상태를
  한 transaction으로 만들고, 같은 body replay는 같은 holding을 반환하며 같은 command ID의 다른 payload는
  `idempotencyConflict`로 거절했다.
- 100,000,000원 주담대 전액 조기상환은 수수료 1,000,000원과 원금을 한 원장에 기록하고 계약을
  `paidOff/0`, 담보권을 같은 game day의 `released`로 바꿨다. 대출 상세는 과거 holding 연결을 유지하지만
  공개 active holding의 `mortgageLoanId`는 null이며 property book value는 순자산에 계속 포함된다.
- 실제 HTTP 스모크에서 owner의 임대차 이동은 `contractConflict`, 다른 사용자의 loan ID는 404,
  비인증 housing·loan 조회는 401이었다. 현금·담보 매수와 완납 원장은 모두 posting 합 0이고
  `save.cashKrw · debtKrw · propertyBookValueKrw`가 wallet·open loan·active holding 합계와 일치했다.
- fresh MySQL 8.4의 `0001→0036`과 M4-B baseline 전진, 공개 서버 MySQL 8.0.46의 격리 DB에서 migration과
  담보 매수·완납을 통과했다. 실제 스모크가 발견한 음수 buyer cash trigger, 25자 origin kind 열 폭,
  일반 `loanPrepayment` principal을 막던 lease-reference trigger를 수정한 뒤 같은 경로를 처음부터 다시
  검증했다. 공개 서버의 일시적 migration 권한은 즉시 회수했고 로컬·원격 격리 DB·계정·터널을 모두
  삭제했다.
- 최종 합본은 서버 1,014 tests와 clippy/fmt, 클라이언트 22 suite·467 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. session 인증, current-run 소유권, strict JSON·canonical ID, bound SQL,
  checked 산술, quote 재심사, 원장 reference와 민감정보 비공개를 security checklist로 다시 확인했다.

### 13.12 M4-C4 완료 기록 (2026-07-28)

M4-C4는 보유주택 매도 주문·지연 체결, 주담대 일괄상환, 취득·보유·양도 관련 세금과 스타일 없는
`/housing` 조작 화면을 완료했다. 첫 fixture의 단독 owner 보유 한도는 유지하며 다주택·임대소득은 이후
catalog version에 남긴다.

- SQLx `0037`은 매도 주문·revision·체결과 tax event/component/payment, 취득일 가격지수, sealed C4
  real-estate·policy graph를 추가했다. fresh MySQL 8.4의 `0001→0037`은 migration 37개, table 217개,
  trigger 615개, view 8개로 전진했고 기존 v5 run pin과 이전 manifest는 그대로였다.
- 현금 매수 fixture는 600,868,480원 매수 뒤 day 61 취득세 6,647,888원, day 151·516의 6월 1일 보유세
  assessment와 7·9월 분할 납부를 만들었다. day 731 이후 510,000,000원 매도는 단독 1주택 비과세로
  양도세 0원을 명시적으로 기록하고, disposition cost 2,550,000원과 wallet 507,450,000원을 포함한 원장
  합계가 0인 채 holding을 disposed, residence를 같은 지역 rent-free로 바꿨다.
- create/reprice/cancel은 같은 body replay, 같은 command ID의 다른 payload 충돌, terminal 상태와
  candidate day를 보존했다. 다른 사용자의 holding·order·tax ID는 같은 404, 비인증은 401, unknown
  query와 malformed ID는 400이었고 매도 history unsigned decoder 회귀를 실제 HTTP에서 찾아 바로잡았다.
- 별도 주담대 fixture는 187,722,684원 주택의 50,000,000원 대출을 24회 납부한 뒤 day 746에
  226,064,686원으로 매도했다. 남은 원금 48,190,604원과 수수료 481,906원을 `propertySalePayoff`로
  상환하고 미래 336회 installment·settlement를 취소했으며, lien released·loan paidOff·wallet
  176,261,853원과 cash/debt/property projection이 정확히 일치했다.
- C4 경계의 서버 1,053 tests와 clippy/fmt, 클라이언트 22 suite·486 tests와 typecheck/lint/build,
  `git diff --check`가 통과했다. current-run 소유권, strict body/query, bound SQL, checked i128 산술,
  payment·ledger reference와 원장 balance를 security checklist로 다시 확인했다.

### 13.13 M4-D1 완료 기록 (2026-07-28)

M4-D1은 typed 복지 조건식·자격 근거, 신청·승인·지급 상태기계와 스타일 없는 `/welfare` 기능 화면을
완료했다. 생애 사건과 보험 component는 disabled 상태로 보존하고 D2·D3에서 각각 활성화한다.

- SQLx `0038`은 DB-driven 복지 카탈로그·조건식·신청·지급·전이 이력과 sealed manifest를 추가했다.
  fresh MySQL 8.4의 `0001→0038`은 table 220개, view 9개, trigger 653개로 전진했고, 새 런만
  `dev-unranked-m4-welfare-2026-v1`과 life aggregate v2를 고정했다. 기존 disabled pin과 이전 런의
  bundle assignment는 그대로 보존됐다.
- 조건식 엔진은 boolean·enum·collection·prior-closed·typed M2-D valuation fact를 strict AST로 판정하고,
  D-start snapshot과 evidence digest를 함께 고정한다. 같은 exclusive group의 중복 신청, 지원 종료 뒤
  재신청, 알 수 없는 fact/operator/type 조합과 SQL `NULL`의 3-valued 우회는 모두 거절한다.
- 실제 fixture `fictionalRestartGrant`는 부양가족 1명인 런에서 333,000원으로 자격 판정됐다. day 0 신청은
  현금을 바꾸지 않고 application·payment·세 전이를 만들었으며, D+1 phase 150에서 지급해 지갑을
  10,333,000원으로 만들고 합계 0인 `welfareBenefitIncome` 원장을 남겼다. 복지 지급은 phase 200 대출
  정산보다 먼저 처리됐다.
- 동일 명령 replay는 같은 application·payment ID를 반환했고 payload 변경, duplicate group, stale
  revision은 409로 거절됐다. unknown query/body·잘못된 ID는 400, 비인증은 401, 다른 런·없는 프로그램은
  같은 404였으며 거절된 명령은 identity를 남기지 않았다.
- 서버 재시작 전후 state·welfare·ledger 응답 hash가 같았고, sealed program/component와 지급 원장
  reference는 DB trigger로 보호됐다. session 인증, current-run 소유권, bound SQL, strict 공개 DTO,
  checked 산술, command fingerprint와 민감 fact·seed 비공개를 security checklist로 재확인했다.
- D1 합본은 서버 1,107 tests와 clippy/fmt, 클라이언트 24 suite·501 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다.

### 13.14 M4-D2 완료 기록 (2026-07-28)

M4-D2는 결정론적 생애 사건 catalog·월 후보 planner·선택/자동 만료·원장 효과와 스타일 없는
`/events-insurance` 사건 기능 화면을 완료했다. 보험 component는 disabled pin을 유지하며 D3 active
schema가 배포되기 전 알 수 없는 보험 component를 fail-closed한다.

- SQLx `0039`는 event fact·definition·choice, 월 plan·candidate, instance·transition·command receipt와
  원장 reference를 추가했다. 실제 MySQL 8.4의 populated D1 `0038→0039`는 tables 220→227,
  views 9→10, triggers 653→677로 전진했고 fresh `0001→0039`도 39/39 migration과 같은
  227 tables·10 views·677 triggers로 완료됐다.
- 기존 run은 life-event/insurance가 모두 unavailable인 pin과 빈 응답을 그대로 보존했다. 새 run만
  `dev-unranked-m4-life-event-2026-v1`과 life aggregate v3를 고정했고, dependent 0은 ineligible,
  dependent 1인 여섯 run은 day 0에 plan/candidate/offered instance를 정확히 한 건씩 만들었다.
- `supportNow`는 지갑 120,000원, state revision 1을 차감하고 accepted/resolved 세 transition과 합계 0인
  `lifeEventChoice` 원장 한 건을 만들었다. 같은 command replay는 transition·원장·revision을 늘리지
  않았고 잔액 부족은 event·transition·ledger·receipt 전체를 rollback했다. `decline`은 원장 없이
  resolved됐다.
- D+7 자동 만료는 1일×7과 7일×1 진행이 같은 candidate·transition·snapshot hash로 수렴했고 원장이
  없었다. 재시작 전후 events/state 응답과 history·cash·ledger DB hash도 각각 동일했다.
- 다른 user·이전 run event는 같은 404로 가렸고 unknown query/body, malformed path와 cursor, foreign·변조
  cursor는 400으로 거절했다. stale cursor, choice 충돌, command ID payload 충돌과 resolved 재선택은
  409였으며 거절 명령은 권위 행을 남기지 않았다.
- sealed component/definition/choice update/delete와 invalid enum·AST·decision·0원 expense, 비용 default
  choice publish를 실제 DB가 거절했다. pending 8건 fixture는 공개 상한을 지켰고 9번째 offered insert는
  invariant trigger가 막았다. session 인증, current-run 소유권, bound SQL, strict DTO, checked i128,
  balanced ledger, HMAC seed·원시 fact 비공개를 security checklist로 재확인했다.
- D2 합본은 서버 1,141 tests와 clippy/fmt, 클라이언트 27 suite·518 tests와 typecheck/lint/build,
  `git diff --check`를 통과했다. production build의 596 KiB 크기 경고는 기존 비차단 경고이며 기능 오류는
  아니다.

### 13.15 M4-D3 검증 진행 기록 (2026-07-28)

M4-D3의 가상 보험 계약·보험료·중도 취소·사건 시점 pin·비소급 claim·지급·만기와 스타일 없는
`/events-insurance` 기능은 커밋 `4b2b3db`에 구현했다. 기능 구현, 로컬 gate, 실제 MySQL/HTTP와 마지막
재시작 후 global invariant까지 모두 끝났으며 M4-D3는 완료다.

- SQLx `0040`은 insurance fact/product/coverage catalog와 contract·premium·claim·allocation·transition·
  receipt runtime을 추가했다. fresh MySQL 8.4에서 migration 40 success, 239 base tables·11 views·719
  triggers, 보험 12 tables·36 triggers와 세 draft guard 복구를 확인했다.
- 기존 unavailable run은 빈 보험 응답을 유지했고 새 run만 life aggregate v4, event v2와 insurance v1을
  고정했다. component/product/coverage hash와 manifest projection 재계산도 저장값과 일치했다.
- D0 사건은 계약이 없는 contract pin 0건으로 고정돼 뒤 가입 후에도 `notCovered`였다. 가입과 replay는
  계약·receipt와 10,000원 차감 원장을 한 번만 만들었고 stale cursor는 `contractConflict`였다.
- D30 두 번째 보험료, D31 insured 사건의 contract pin 1건, ready 100,000원 claim과 지급/replay를 확인했다.
  DB에는 `notCovered:pin0`, `paid:pin1`이 남았고 가입·보험료·claim 원장은 모두 balance 0이었다.
- 장기 정상 경로는 D300 charge 11, D330 charge 12, D360 `expired/termEnded`까지 통과했다. 12개 premium
  charge와 원장이 정확히 한 번씩 존재했고 13번째 charge와 D360 이후 보장은 없었다.
- 동일 scratch DB에서 서버를 재시작한 뒤에도 보험 aggregate와 `/api/state` hash가 각각 동일했고 계약
  API가 HTTP 200이었다. 당시 최종 aggregate·pin·orphan invariant SQL을 재실행하기 전에 scratch
  container·volume을 정리해 검증 공백이 생겼지만, 아래 production 최종 인수로 그 공백을 닫았다.
- D3 합본은 서버 1,170 tests와 check/clippy/fmt, 클라이언트 30 suite·551 tests와
  typecheck/lint/build, Docker build check와 `git diff --check`를 통과했다.

사용자 지시에 따라 2026-07-28의 마지막 인수는 별도 MySQL·schema·dump를 만들지 않고 development 용도의
production DB에서 수행했다. 기존 DataGSM 계정과 save는 그대로 보존하고, 식별 가능한 Google 인수 계정
`codex-m4d3-prod-20260728-v1`과 save 29만 추가했다. runtime history는 의도적으로 append-only이므로 이
계정은 provenance로 남기되 원문 session token과 session row는 삭제했다.

- public HTTP 주 경로는 run 2에서 D0 empty contract pin 0건 → 가입 뒤 gross 120,000원 `notCovered` →
  D30 두 번째 premium paid → D31 contract pin 1건·waiting 통과 → ready 100,000원 → paid와 exact replay로
  끝났다. DB에는 `notCovered:pin0`, `paid:pin1`, 지급 benefit 100,000원, reservation 0원이 남았다.
- 첫 인수 도구는 가입 함수가 subshell에서 snapshot을 갱신해 stale cursor를 보낸 탓에 HTTP 409 `busy`로
  멈췄다. 서버 권위 cursor가 올바른 것을 DB로 확인한 뒤 도구의 cursor 전달만 고쳤다. 그 preflight run 1은
  다음 run 시작 때 `expired/newRun`으로 닫혔고, run 3의 D0 취소 계약은 `cancelled/playerCancellation`으로
  닫혔다. 중단 뒤 열린 transaction과 인수 session은 모두 0건이었다.
- production MySQL 8.0.46은 migration `40/40`, 실패 0, migration 40 checksum 48 bytes였다. schema는
  239 base tables·11 views·719 triggers, 보험 12 tables·36 direct triggers, integration trigger 6개,
  event draft guard 3개였다.
- canonical hash는 insurance v1
  `77bbed525cdc4d7014dacce75eaafe2bf833fcb145009a4405603af28bb27a0c`, event v1
  `6657664680f683073740cefe3317334b3c30963940dae0f2b91adec4b3c4104c`, event v2
  `7b924fffb11b93e823885f668e15e2bb988da96988ed614eeb312c43478275ea`였고 세 manifest·projection rehash와
  stored digest가 모두 일치했다. `newRun` assignment revision 14와 인수 run 1~3은 aggregate v4·event v2·
  insurance v1을 고정했다.
- 동일 container를 실제로 다시 시작한 전후 `/api/state`, `/api/insurance/contracts`, save, run bundle,
  command identity, event instance, 모든 D3 contract/charge/pin/claim/allocation/receipt, settlement와 원장까지
  18개 hash를 비교해 모두 동일했다. 잘못된 `command_identity.run_revision` 정렬로 빈 hash가 된 첫 보조
  검사는 실제 column `initial_run_revision`으로 고쳐 전체 검사를 다시 실행한 뒤에만 통과로 기록했다.
- 재시작된 DB에서 claim pin digest, eligibility fingerprint, paid/reserved aggregate, claim allocation,
  contract·pin cardinality, event/claim·allocation orphan, charge/settlement, ledger source ownership, 보험 원장
  balance와 wallet tie-out 위반이 전부 0이었다. sealed insurance product update와 coverage delete는 각각
  SQLSTATE 45000 `immutable`로 거절됐고 coverage 1건, 열린 transaction 0건을 재확인했다.
- 종료 시 container는 healthy, 내부와 외부 `/api/health`는 HTTP 200, 인수 session과 열린 DB transaction은
  0건이었다. recovery dump는 만들지 않았고 전송한 임시 스크립트·SQL·hash 파일은 모두 제거했다.

운영 CD는 `main`의 `server/**` push에서 source 전송 → Docker image build → container 교체 → health
validation을 수행하며, 새 binary 시작 시 `sqlx::migrate!()`가 외부 요청 bind 전에 pending migration을
적용한다. 따라서 migration은 별도 job이 아니라 성공적으로 빌드·기동된 배포 안에서 자동 실행된다.

- `4b2b3db` 실행 `30311828314`는 정확한 source를 전송했지만 7분간 조용한 Rust release build 중 SSH 실행
  채널이 끊겨 BuildKit이 runtime image 단계 전에 취소됐다. `aea745d`가 20초 build heartbeat와 종료 코드
  보존을 추가해 이 원인을 수정했다.
- `aea745d` 실행 `30313085477`은 이미지를 만들고 container를 교체했지만 Docker Desktop의
  `host.docker.internal` host-port 경로에서 SQLx migration 연결이 멈춰 health가 실패했다. 같은 image와
  DB를 `readygsm-net`의 `mysql:3306`에 직접 연결한 격리 probe는 `0001→0040`과 health를 통과했다.
  `0b2c20b`는 compose service를 이 외부 network에 붙이고 production `SERVER_ENV`도 `mysql:3306`으로
  변경했다.
- `0b2c20b` 실행 `30313998197`에서는 이전 host-gateway 연결이 남긴 transaction·SQLx lock을 정확히
  해제한 뒤 populated production DB에서 migration `0023`의 기존 결함이 드러났다. `0019`가 모든
  `spec_evidence` update를 막은 상태에서 `0023`이 `credited_experience_days`를 backfill하고 있었으며,
  empty fresh DB에서는 대상 행이 없어 숨었던 문제였다. `eac0b92`는 backfill 직전에 해당 trigger만 내리고
  즉시 같은 immutable trigger를 복원한다.
- 실패 직후 production DB의 mode 600 논리 dump를 만들어 복제본에서 수정된 `0023` tail과
  `0024→0040`, health를 먼저 통과시킨 뒤 같은 복구를 production에 적용했다. 사용자·save·character
  각 1건과 기존 경력 증빙을 보존했고 `0023` checksum도 새 source와 일치시켰다. 최종 운영 검증 뒤 이
  복구용 dump도 삭제해 별도 복구 artifact는 남아 있지 않다.
- 최종 실행 [`30314779686`](https://github.com/it-play/lifeledger/actions/runs/30314779686)은 6분 4초 만에
  성공했다. production은 migration `40/40`, 실패 0건,
  container restart 0·healthy이며 `172.19.0.6`에서 MySQL container network로 직접 연결됐다. 내부
  `http://127.0.0.1:10105/api/health`와 외부 `https://kimtaeeun.site/lifeledger/api/health`가 모두 HTTP
  200을 반환했다. 두 probe container·DB, 전송한 임시 SQL과 복구용 dump는 삭제했다.

### 13.16 M4-E1 기능 구현 진행 기록 (2026-07-28)

§8.1~§8.8의 순수 core, SQLx `0041`, server store/state/strict API와 client `/recovery`를 연결했다. 이 절은
구현 provenance이며 production 인수 결과가 아니다.

- core는 `cashOnlyLiquidation` eligibility와 fail-closed composition, 자동 2,500,000원과 추가 보호액,
  liquidatable cash, claim 비례 배분·ID 기준 1원, 비용→이자→원금 배분, canonical SHA-256, 같은 날
  prepared→filed→liquidation→discharged→rebuilding, 철회와 D+1,825 exclusive recovery를 구현했다.
- `0041_m4e_insolvency.sql`은 기존 sealed graph를 수정하지 않고 insolvency component v1, policy v4,
  life aggregate v5를 게시하며 newRun assignment만 이동한다. runtime은 case, transition, wallet asset,
  claim, distribution, command receipt로 분리했다. loan installment/payment와 ledger enum/check/authority trigger는
  discharge와 insolvency source를 명시적으로 허용한다.
- store는 준비 시 wallet·claim·policy/component identity와 composition hash를 고정한다. 제출 시 현재 구성을
  다시 hash해 달라지면 거절하고, 청산 배분·payment/allocation·원장·면책·save projection을 한 transaction에
  적용한다. submit에서 현재 prepared 사건 자체를 기존 사건으로 오판하지 않도록 eligibility 조회 경계를
  분리했고, 할부 일부 배분은 상태와 지급액을 함께 전이한다.
- recovery overlay는 raw credit history를 덮어쓰지 않고 rebuilding 사건을 먼저 확인한다. 무담보·전세보증금·
  주택담보 견적뿐 아니라 저장된 eligible quote의 실행 시점 재평가에도 같은 제한을 적용한다. 일일 pipeline은
  credit end-of-day 다음에 restriction 종료 사건을 recovered로 전이한다.
- API는 overview, prepare, submit/withdraw action, detail, claims, liquidations 여섯 경로다. body/query는 unknown
  field를 거절하고 command cursor와 current-run 소유권을 요구한다. claim/distribution page는 signed cursor와
  최대 20건, transition은 최대 16건이며 snapshot에는 availability·eligibility·reasons·current case만 둔다.
- client는 모든 응답을 zod 경계에서 검증하고 command response가 제출 cursor에서 정확히 한 번 전진했는지
  확인한다. `/recovery`는 overview/detail/claim/liquidation을 읽고 준비·제출·철회를 수행하며, 불명확한
  transport 결과는 동일 명령으로 재시도한다. DOM은 mount에서 한 번 만들고 fixed slot만 갱신한다.
- 구현 우선 원칙에 따라 전체 회귀를 매 단계 반복하지 않았고 배포 직전에 한 번 수행했다. 서버 1,180건,
  클라이언트 551건과 clippy·fmt·typecheck·lint·production build·diff gate가 통과했다. 실제 MySQL migration,
  공개 HTTP와 재시작 invariant의 실제 결과는 §13.17에 이어 기록한다.

### 13.17 M4-E1 production 주 경로 인수 기록 (2026-07-29)

사용자 지시에 따라 별도 Docker MySQL·격리 schema·recovery dump 없이 development production DB에서
M4-E1을 인수했다. 인수 fixture는 식별 가능한 user 4/save 118/run 1이며 기존 사용자는 수정하지 않았다.
검증 종료 뒤 session row를 삭제했고 열린 InnoDB transaction은 0건이었다. fixture의 case·loan 이력은
D+1,825 경계 재개를 위해 보존한다.

- 최초 `0041` CD는 게시 순서가 assignment trigger와 충돌해 실패했다. production을 기존 데이터 삭제 없이
  compatibility edge와 revision을 맞춰 복구하고, `c30301c`가 finance policy를 먼저 게시하도록 migration
  순서를 고쳤다. 이어 `daily.market_world_id` 오타를 `world_id`로 고친 `3a3b539`, policy v4에 C4
  property-tax profile을 forward-copy한 `0042`/`5e4e99d`를 배포했다.
- 인수 run은 현금 25,000,000원, 학자금 50,000,000원과 신용 3,000,000원으로 시작했다. day 30 advance는
  proxy 504 뒤 같은 command를 재개해 state 0/day 0에서 state 30/day 30으로 정확히 한 번 전진했고,
  day 31에 defaulted fixture를 준비했다. prepare/replay, cash 변조의
  `insolvencyCompositionChanged`, withdraw와 재준비를 확인했다.
- default가 미래 installment를 취소해 미물질화 원금 bucket이 없던 문제는 `a9a2c12`가 claim 잔여분을
  `current*` 배분으로 완성했다. `0043`/`7db510a`는 bucket ID 없는 current allocation을
  `insolvencyDistribution` payment에만 trigger로 허용하고, 대출 이력의 `discharged` installment와
  `insolvencyDistribution` payment 공개 계약도 server/client에 추가했다. `0044`/`7cd6259`와
  `0045`/`0de740f`는 기존 loan·lease posting guard가 도산 principal reference를 보존하도록 좁게 확장했다.
  각 trigger migration은 직전 정의와 기계적으로 비교해 해당 source 조건 외 본문이 같음을 확인했다.
- 성공한 관련 CD는
  [`30374857904`](https://github.com/it-play/lifeledger/actions/runs/30374857904),
  [`30375536509`](https://github.com/it-play/lifeledger/actions/runs/30375536509),
  [`30376325937`](https://github.com/it-play/lifeledger/actions/runs/30376325937),
  [`30378000824`](https://github.com/it-play/lifeledger/actions/runs/30378000824),
  [`30378960851`](https://github.com/it-play/lifeledger/actions/runs/30378960851),
  [`30379506651`](https://github.com/it-play/lifeledger/actions/runs/30379506651),
  [`30380186674`](https://github.com/it-play/lifeledger/actions/runs/30380186674)다. 최종 server 시작 로그는
  MySQL 연결과 migration 적용을 확인했고 production은 `45/45`, 실패 0, public health HTTP 200이다.
- case 2 submit은 state 34→35에서 현금 23,312,715원 중 18,087,371원을 보호하고 5,225,344원을 두 claim에
  4,931,222원·294,122원으로 비례 배분했다. 원 claim 52,540,728원에서 47,315,384원을 면책해
  `prepared→filed→liquidation→discharged→rebuilding`을 같은 day 31에 기록했다. 같은 command replay는
  `replayed=true`, state 35/day 31/현금 18,087,371원/채무 0원을 유지했다.
- detail·claims·liquidations와 두 대출 이력 API는 HTTP 200이었다. claim별
  `allowed=distributed+discharged`, distribution=payment=allocation, 세 insolvency ledger transaction의
  posting 합 0, loan 3·4 `discharged/0`, save debt=active debt=0, submit receipt 1건, case·claim·payment·ledger
  orphan 0을 production SQL로 확인했다. unknown query와 limit 0·51은 400 `invalidCommand`였다.
- 무담보 product 12와 주담대 product 14 quote는 `creditRestricted`와 단일 reason
  `insolvencyRebuilding`을 반환했다. 전세 product 13은 현재 jeonse listing 두 건에서 overlay 전에
  `contractConflict`가 나므로 통과로 기록하지 않고 §1.1 재개 항목으로 남겼다.
- container 재시작 전후 case, claims, liquidations, loan 3·4 history의 canonical SHA-256은 각각
  `9effd639…d6d7f3`, `65664bf8…ad6e8`, `184e9528…64630`, `f81fc7d4…0b14d`,
  `6f08504b…0165c`로 동일했다. 재시작 뒤 health 200과 submit replay를 다시 확인했다.
- 로컬 보정 gate는 Rust 표적 BDD 1건(1 pass, 1,180 filtered), `cargo check`, `cargo fmt --check`,
  client contract 204 tests와 typecheck, `git diff --check`만 실행했다. 이미 배포 전 전체 gate를 통과했으므로
  결함마다 전체 1,180/551 회귀를 반복해 병목을 만들지 않았다.

M4-E1의 production 주 경로와 재시작 불변식은 통과했지만, §1.1의 전세 overlay와 D+1,825 일일 recovery가
남아 있으므로 이 절은 M4-E1 전체 완료 선언이 아니다.

## 14. M4 완료 조건

1. M3에서 취업한 캐릭터의 가구·지역·CPI 기반 생활비가 급여와 같은 원장에서 매월 정산된다.
2. 모든 시작 부채가 실제 계약·상환표·원장으로 존재하고 연체, 신용 band, DSR/LTV가 신규 대출에 반영된다.
3. 임차, 이사, 주택 매수·담보대출·매도와 취득·보유·양도 관련 세금이 원자적으로 이어진다.
4. 새 복지 프로그램을 코드 변경 없이 typed 조건식 데이터로 추가하고 자격 근거와 지급을 재현한다.
5. 고정 시드에서 사건·선택 기한·보험 보장이 실행 순서와 재시작에 무관하게 같다.
6. 연체 위기에서 회생 또는 파산을 신청하고 청산·면책·재기 상태까지 끝낼 수 있다.
7. 개인 자금과 분리된 단순 법인을 설립해 월 손익, 대표 급여, 배당을 처리한다.
8. 스타일 없는 기능 화면에서 위 흐름을 조작할 수 있다.
9. 정상·연체·파산·재기 경로를 포함한 고정 30년 시나리오가 원장 불일치나 음수 현금 없이 끝난다.
10. 단위/protocol 테스트, 서버·클라이언트 검사와 실제 MySQL 8 격리 스모크가 통과한다.

## 15. 플레이테스트 전까지 의도적으로 남기는 조정값

다음 값은 구현 의미가 아니라 밸런스이므로 첫 외부 플레이테스트 전 카탈로그 초안에서 정하고 결과에 따라
새 버전으로 조정한다.

- 지역·가구원별 생활비 기준액과 소비 band 계수
- 가상 대출의 spread, 기간, 조기상환 비용, 신용 model penalty/recovery와 band 경계
- 지역별 매물 수, 전세가율·월세 전환, 매도 대기 분포와 거래비용
- 사건별 hazard, cooldown, 충돌 priority, 선택 기한과 게임용 비용
- 가상 보험의 보험료·deductible·limit·grace
- 재기 난이도를 좌우하는 arrear 완충과 관찰기간 중 게임 밸런스 부분
- 단순 법인의 업종별 매출 분산, 고정비와 운영규모 계수

법정 세율·한도·기간은 플레이테스트 값이 아니다. 구현 시 원문과 기준일을 확인해 policy version으로 게시한다.
