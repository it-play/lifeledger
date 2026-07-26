# M4 생애 상세 스펙

- 작성: 2026-07-26
- 상태: 구현 전 설계 확정안
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
시장·policy assignment와 같은 `assignment_revision` 아래 새 런에 할당한다.

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
draft의 부양가족 수·관계에서 만들고, 이후 생애 사건은 기존 행을 덮어쓰지 않고 유효기간을 닫고 새 관계를
추가한다. 나이는 캐릭터와 같은 게임 달력의 생년월일에서 계산하며 snapshot에 증가값을 중복 저장하지 않는다.

`cost_of_living_profile`은 다음 typed 필드를 가진 불변 카탈로그다.

- 기준 CPI index와 기준 월의 `housing · food · transport · communication · utilities · healthcare ·
  education · dependentCare · discretionary` 원 금액
- 지역 key별 항목 계수 ppm
- 가구원 역할·연령 band별 한계비용 계수 ppm
- 소유·전세·월세·무상거주별 주거 항목 대체 규칙
- 각 항목의 필수 여부와 미납 시 부족분 처리 방식

월 `m`의 항목별 청구액은 `기준액 × 현재 CPI / 기준 CPI × 지역계수 × 가구계수` 순서로 곱하지 않고,
모든 분자를 i128에서 한 번 곱한 뒤 모든 분모로 나눈 몫을 원 금액으로 쓴다. 계산 순서가 바뀌어 원 단위가
달라지지 않게 식과 분모 순서를 고정하고, remainder는 `(household, category)`별로 다음 달에 넘긴다.
CPI는 런에 고정된 M2 시장 월드의 해당 게임일 값을 읽는다. 미래 CPI나 서버 현재 날짜는 읽지 않는다.

지역은 캐릭터의 출신지가 아니라 현재 `residence.regionKey`다. 이사 완료 전에는 기존 지역, 완료 transaction
뒤 다음 생활비 산정부터 새 지역을 쓴다. 가구 변경도 같은 원칙으로 해당 변경이 commit된 다음 청구부터
반영한다.

### 3.2 예산과 월 청구

플레이어는 카탈로그의 허용 band 중 `frugal · standard · generous` 같은 소비 수준을 항목별로 선택한다.
표시명과 계수는 카탈로그 데이터이며 엔진은 임의의 문자열을 받지 않는다. 선택하지 않은 새 항목은
카탈로그의 명시적 `defaultBandId`를 쓴다. 필수 항목은 0으로 낮출 수 없다.

생활비는 각 게임 월의 첫날에 그 달 값을 확정하고 월 말 정산을 예약한다. 게임 시작일이 월 중간이면
`remainingCalendarDays / daysInMonth`로 일할 계산하고 원 미만을 버린다. 이후 가구원이 달 중간에 바뀌면
다음 달부터 반영하며, 사건이 만든 즉시 의료비·돌봄비는 별도 정산으로 처리한다. 이 단순화는 월 청구를
과거 날짜에 재산정하지 않기 위한 게임 규칙이다.

정산 시 지갑에서 `필수 항목 → 주거 계약 → 선택 항목` 순으로 납부한다. 같은 그룹 안에서는 고정
category enum 순서다. 필수 생활비가 부족하면 가능한 금액을 먼저 납부하고 나머지는 `essentialArrear`
무이자 의무로 만들며, 선택 항목은 현금 범위에서 축소하고 부채를 만들지 않는다. 실제 생계비 채무를
모사한 것이 아니라 게임이 음수 현금을 만들지 않기 위한 고정 기본값이다. 미납 필수액은 복지·도산 판정의
입력이지만 대출 신용 연체와 같은 것으로 세지 않는다.

`living_cost_month`는 입력 CPI·지역·가구 fingerprint, 항목별 gross·paid·arrear와 remainder를 보존한다.
같은 `(household_id, year_month)`는 한 번만 확정하며 버전 변경으로 과거를 다시 계산하지 않는다.

## 4. 대출·상환·연체와 신용

### 4.1 시작 부채의 실제 계약화

M4 이후 `save.debt_krw`는 조회용 합계일 뿐 권위가 아니다. 모든 부채는 `loan_contract`의 남은 원금과
연체 원리금 합계로 설명돼야 한다. 새 캐릭터 draft의 시작 부채는 금액만 받지 않고 다음 strict variant다.

- `studentLoan { productVersionId, principalKrw }`
- `unsecuredLoan { productVersionId, principalKrw }`
- `leaseDepositLoan { productVersionId, principalKrw, initialLeaseTemplateId }`
- M3 이전 호환 draft는 서버가 게시된 명시적 migration mapping을 가진 경우에만 위 variant로 변환

캐릭터 시작 transaction은 카탈로그 자격을 검증하고 계약, 상환 스케줄, opening 원장, 필요한 임대차를
한 번에 만든다. 어느 하나라도 유효하지 않으면 런 전체를 만들지 않는다. 기존 런의 aggregate 부채는
마이그레이션 시 출처별 매핑 가능한 행만 계약화하고, 매핑 불가능한 잔액은 숨겨진 기본 상품이 아니라
표시 가능한 `legacyDebt` 계약으로 한 번 고정한다.

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
나머지는 다음 회차로 넘기고, 마지막 종료에는 remainder를 버리며 숨은 1원 청구를 만들지 않는다.
변동금리는 상품에 적힌 reset 게임일의 확정 시장금리와 spread를 합쳐 그 다음 이자 구간에만 적용한다.
마이너스 금리 처리와 금리 상·하한은 반드시 상품 버전에 있어야 한다.

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

## 5. 부동산·임대차·담보대출

### 5.1 지역 지수와 매물

M4는 실제 주소 대신 `capital · metro · city · rural` 같은 versioned region key와 가상 주거 자산을 쓴다.
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
어느 경로인지는 listing이 아니라 정책 규칙이 명시한다.

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

## 6. 데이터 조건식 복지 엔진

복지·정책상품은 임의 스크립트가 아니라 versioned AST로 표현한다. 지원하는 node는 다음으로 제한한다.

- 논리: `all`, `any`, `not`
- 비교: `eq`, `in`, `lt`, `lte`, `gt`, `gte`, `between`
- 집계: `sum`, `count`, `exists`
- 값: 허용된 fact path, 정책 상수, integer/string/date literal

fact path는 `character.age`, `household.memberCount`, `household.dependentCount`, `residence.region`,
`career.employmentStatus`, `military.status`, `income.periodTotal`, `asset.policyValuation`,
`debt.policyBalance`처럼 schema registry에 등록한 항목만 쓴다. AST에 SQL, 정규식, 임의 함수, 현재 벽시계,
클라이언트 입력 경로는 허용하지 않는다. 모든 숫자 단위와 기간 창은 node에 명시하고 알 수 없는 fact는
false로 조용히 바꾸지 않고 해당 판정을 `indeterminate`로 실패시킨다.

`welfare_program_version`은 eligibility AST, 신청 가능 기간, 중복수급 group, 급여 계산식, 지급 일정,
필요한 재판정 trigger를 가진다. 결과는 다음 상태기계다.

`notEvaluated → eligible|ineligible|indeterminate → applied → approved|rejected → active → exhausted|terminated`

자동으로 돈을 주지 않는다. 명시적 자동수급 정책을 가진 프로그램 외에는 플레이어 신청이 필요하다.
동일 입력 fingerprint와 program version의 판정은 캐시할 수 있지만, 가구·소득·자산·지역·고용·병역
trigger가 commit되면 다음 planner에서 재평가한다. 승인과 지급은 원장·settlement source identity로
중복되지 않는다. API는 성공 여부뿐 아니라 통과·실패한 공개 condition code를 반환하되 내부 policy JSON과
다른 사용자의 수치를 노출하지 않는다.

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

## 8. 파산·회생·면책·재기

M4는 실제 법원 절차를 축약한 **게임용 상태기계**를 제공한다. 화면에는 교육·오락용 단순화이며 법률
자문이 아니라는 고지를 둔다. 신청 요건, 보호재산, 변제기간, 면책 제외 채무, 금융 제한 기간 같은 실제
제도 관련 값은 기준일·출처가 있는 policy에 둔다.

### 8.1 절차 상태

개인의 insolvency 상태는 다음 하나다.

`solvent → distressed → filingPrepared → filed → {restructuring|liquidation} → discharged → rebuilding → recovered`

`filingPrepared|filed`는 `dismissed`로 끝나 `distressed`로 돌아갈 수 있다. `restructuring`은 계획 이행 성공 시
`discharged`, 실패 시 `distressed` 또는 policy가 허용한 `liquidation`으로 간다. 숨은 자동 파산은 없다.
부채초과·연체 조건은 신청 가능성을 열 뿐 플레이어 command 없이 제출하지 않는다.

신청 command는 현재 자산·채무·소득·가구를 pin한 `insolvency_case`를 만들고, 모든 loan·arrear·세금 의무를
채권 목록에 복제한다. 누락된 의무가 있거나 총계가 원장과 다르면 제출을 거절한다. filing 이후 채권별
collection status는 case가 권위를 가지며 원계약을 삭제하지 않는다.

### 8.2 회생과 청산

회생 planner는 policy의 가용소득 정의로 월 변제 가능액을 계산하고, creditor class·priority·contract ID
순으로 정수 원을 배분한다. 나눗셈 잔여 1원은 그 순서의 앞 채권부터 한 원씩 배분한다. 플레이어가 임의로
특정 채권자를 우대할 수 없다. 매달 실제 납부한 금액만 plan progress가 된다.

청산은 `현금성 자산 → 금융자산 → 비거주 부동산 → policy가 허용한 거주자산` 순으로 매각 plan을 만들고,
각 그룹 안에서는 asset ID 순이다. 보호재산과 면책 제외 채무는 policy result로 명시한다. 시장 휴장이나
매물 지연 때문에 즉시 현금화할 수 없는 자산은 case-owned liquidation order로 기다리며, 미래 가격을
미리 읽지 않는다.

면책은 원계약을 삭제하지 않고 남은 eligible principal·interest를 `discharged` 원장 transaction으로
상계한다. 제외 채무는 그대로 남는다. credit band, 신규 대출·신용상품·법인 배당 제약은 case 상태와
credit model이 함께 판정한다.

`rebuilding`의 기본 목표는 연체 없음, 필수 생활비 arrear 해소, policy가 정한 관찰기간 충족이다. 충족하면
`recovered`로 전이하되 과거 사건 이력은 지우지 않는다. M4의 재기 프리셋은 이 상태와 실제 남은 계약을
초기 데이터로 만들며 단순한 음수 점수만 주지 않는다.

## 9. 단순 법인

M4의 법인은 개인과 분리된 최소 원장으로 자본 배분과 급여/배당 선택을 체험하게 한다. 세부 경영은 M5다.

- `corporation` 상태: `draft → active → dormant|insolvent → dissolved`
- 설립 명령: 업종 템플릿, 자본금, 등록 비용, 대표자를 검증하고 개인 지갑에서 법인 현금으로 출자
- 월 손익: `(corpSeed, corporationId, operatingMonth, stream)`으로 결정한 업종 매출에서 카탈로그 고정비와
  선택한 운영규모 비용을 뺀다.
- 대표 급여: M3 급여·원천징수 경로를 재사용하고 비용/개인소득으로 양쪽 원장을 연결한다.
- 배당: 결산된 배당가능이익 범위에서만 명령으로 지급하며 M2/M3 세금 경로를 재사용한다.
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
7. due settlement를 `(due_game_day, settlement_id)` 순서로 하나의 shadow balance plan에서 실행한다.
   생활비·대출·임대차·보험·복지·세금도 kind별 별도 commit을 하지 않는다.
8. 당일 납부 결과로 연체일·계약 상태·신용 units·insolvency plan progress를 갱신한다.
9. 가구·거주·사건의 기한 만료와 명시적 default choice를 적용한다.
10. 원장·상태·후속 settlement·event log를 기록하고 game day/state revision을 각각 1 올려 commit한다.
11. commit 뒤 bounded snapshot 하나만 SSE로 보낸다.

앞 정산이 만든 가상 지갑·계좌·부채·가구 상태를 뒤 정산이 읽는다. 어느 계산·posting·conditional transition이
실패해도 그 날 전체를 rollback한다. settlement별 savepoint나 사건별 commit은 없다.

### 10.2 전역 잠금 확장

M2 잠금 순서 뒤에 M3가 확정한 순서를 합성하고, M4 행은 다음 상대 순서를 지킨다.

`save → household → household_member(id) → residence → property_holding(id) → lease_contract(id) →
loan_contract(id) → loan_installment(contract_id, installment_no) → insurance_contract(id) →
insurance_claim(id) → welfare_application(id) → corporation(id) → insolvency_case →
insolvency_claim(priority, id) → 기존 scheduled_settlement(due_game_day, id) → 연도별 tax rows`

최종 구현 전에 M2·M3 전체 목록과 합친 단 하나의 전역 순서를 `store` module contract에 둔다. 부모보다
자식을 먼저 잠그지 않고 복수 ID는 먼저 수집해 오름차순으로 잠근다. 불변 policy·catalog·market row는
가변 잠금 순서에 넣지 않는다.

모든 mutation은 M0/M2의 canonical `commandId`, 최초 expected 네 부분 cursor, strict payload fingerprint,
`command_identity`와 receipt를 재사용한다. 복합 명령의 원장·계약·사건은 command ID 기반 source identity를
가지며 응답 유실 재시도에서 한 건으로 수렴한다. 일일 자동 step은 settlement/event 고유키로 멱등하다.

## 11. strict API와 bounded snapshot

모든 request는 알 수 없는 필드, 알 수 없는 enum, null/누락 교차조건, 범위 밖 integer를 거절한다.
서버는 클라이언트가 보낸 세금·신용·DSR·LTV·보험금·순자산 계산값을 신뢰하지 않는다. mutation은 공통
command/cursor를 받고 `{ result, snapshot }` envelope와 stable error code를 반환한다.

주요 경로는 다음과 같다.

| 경로 | 역할 |
|------|------|
| `GET/PUT /api/life/budget` | 현재 생활비 산정 근거 조회, 허용 band 변경 |
| `GET /api/credit` | credit band, 공개 reason, 대출 계약 요약 |
| `GET /api/loans/products`, `POST /api/loans/quotes` | 카탈로그와 서버 심사 quote |
| `POST /api/loans`, `POST /api/loans/{id}/prepayments` | 실행·조기상환 |
| `GET /api/housing/listings` | 현재 월·지역의 bounded 매물 |
| `POST /api/housing/purchases`, `POST /api/housing/sales` | 매수와 매도 order |
| `POST /api/housing/leases`, `POST /api/housing/moves` | 임대차와 이사 |
| `GET /api/welfare/programs`, `POST /api/welfare/applications` | 자격 근거와 신청 |
| `GET /api/life/events`, `POST /api/life/events/{id}/choices` | pending 사건과 선택 |
| `GET/POST /api/insurance/contracts`, `POST /api/insurance/claims` | 보험 가입·청구 |
| `POST /api/insolvency/cases`, `POST /api/insolvency/{id}/actions` | 도산 신청·절차 action |
| `POST /api/corporations`, `POST /api/corporations/{id}/payouts` | 단순 법인과 급여/배당 |

`GameSnapshot.life`에는 현재 화면에 필요한 bounded summary만 둔다. 생활비 당월 1건, 활성 거주 1건,
부동산 보유 최대 카탈로그 상한, 활성 loan/insurance/welfare 요약, pending 사건 최대 8건, 개인 insolvency
상태 1건, 단순 법인 1건을 포함한다. 종료 계약·과거 사건·전체 신용 이력·세금 근거는 cursor pagination
조회로 분리한다. 배열 상한을 초과하면 잘라서 성공하지 않고 invariant 오류로 감지한다.

주요 실패 코드는 `ineligible · insufficientWalletCash · debtServiceLimit · collateralLimit ·
creditRestricted · contractConflict · residenceRequired · eventExpired · claimNotCovered ·
insolvencyStateConflict · rateUnavailable · busy · invalidCommand`다. 한국어 message와 code는 분리하고,
소유권 실패는 존재 여부를 드러내지 않는다.

## 12. 기능 중심 화면

M4 화면은 사용자 정의 CSS와 시각적 스타일링 없이 다음 기능을 끝까지 조작하게 한다.

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
