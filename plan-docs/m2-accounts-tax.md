# M2 계좌·세제 상세 스펙

- 작성: 2026-07-26
- 상태: M2-A~M2-E 기능 구현 및 자동/실제 MySQL 8 검증 완료
- 상위 계획: [`development-plan.md` §4.2, §6, §12](./development-plan.md)
- 선행 조건: [`m1-market-core.md` §11](./m1-market-core.md)의 M1-C 완료

## 1. 목표와 단계

M2의 완료 목표는 같은 시장 수익을 얻어도 일반계좌·ISA·연금저축·IRP의 선택과 만기·중도해지에
따라 실제 사용 가능한 세후 금액이 달라지는 한 바퀴를 만드는 것이다. 기능을 다음 순서로 세운다.

1. **M2-A 원장·정산 기반** — 런에 고정한 제도 버전, 금융계좌, 복식 원장, 예약 정산, 일일
   파이프라인의 원자적 실행
2. **M2-B 현금성 상품** — 입출금 지갑, CMA, 정기예금·적금, 이자 원천징수, 만기와 중도해지,
   예금자보호 노출
3. **M2-C 절세 계좌** — 일반 투자계좌, ISA, 연금저축, IRP의 납입·인출·손익통산·한도·제약
4. **M2-D 자산 확장** — 고정금리 국채, LLX 분배금, KRX 금 현물, 계좌별 매매비용과 연간 세금 마감
5. **M2-E 기능 화면** — 계좌 개설·이체·상품 가입·매매·만기/해지·세금 원장을 스타일 없이 조작

해외주식·환율, 개별 국내주식, 부동산과 대출은 이 문서의 기반을 재사용하지만 각각 M4 또는 후속
콘텐츠 단계에서 연다. M2는 LLX 지수상품·국채·금으로 계좌와 세제 차이를 먼저 완성한다.

## 2. 결정론적 제도 버전

실제 세법은 바뀌지만 이미 시작한 게임의 결과가 배포 시점에 따라 달라지면 안 된다. 시장 월드와 같은
불변 버전 경계를 제도에도 둔다.

- 최초 규칙 세트 키: `kr-individual-2026-v1`
- 기준일: `2026-01-01`
- 새 런 할당 키: `newRun`
- `save.policy_set_id`는 새 런 시작 때 활성 규칙 세트를 고정하고 런 도중 바뀌지 않는다.
- 활성 규칙 세트 할당은 시장 월드와 같은 `assignment_revision`을 가진다. 새 런은 시장 day 0과 규칙
  세트를 먼저 검증한 뒤 한 트랜잭션에서 두 할당 행을 잠그고 `(id, revision)`을 다시 확인한다.
  어느 한쪽이라도 바뀌면 런을 수정하지 않고 준비부터 제한 횟수 안에서 다시 시도한다.
- 법령·한도·세율을 고칠 때 기존 JSON을 수정하지 않고 새 규칙 세트와 새 할당을 추가한다.
- 2027년 이후 아직 알 수 없는 실제 제도는 v1에서 마지막 2026 규칙이 계속된다는 **시뮬레이션 가정**을
  쓴다. 향후 확정 법령은 새 세트에 유효기간 레코드로 추가한다.
- API는 현재 규칙 세트 키와 주요 한도의 기준일을 노출해 실제 현재 제도와 혼동하지 않게 한다.
- v1에서 참조할 모든 `policy_rule` 행은 같은 배포 마이그레이션 묶음에서 채운 뒤 `newRun` 할당을
  활성화한다. 활성화된 세트에 나중에 규칙을 덧붙이지 않고 변경은 새 policy set으로 낸다.

테이블은 `policy_set`, `policy_rule`, `policy_set_assignment`으로 나눈다. `policy_set`은 초안에서만
규칙을 받을 수 있고 모든 규칙 검증 뒤 `sealed_at`을 한 번 기록해 게시한다. `newRun`은 게시된 세트만
가리킬 수 있으며 게시된 세트에는 rule insert도 막는다. `policy_rule`은
`(policy_set_id, domain, rule_key, effective_from)`을 유니크하게 하고 `effective_to`는 nullable 종료일이다.
같은 세트·도메인·키의 기간이 겹치는지는 MySQL 제약만으로 표현하지 않고 하나의 저장소 삽입 경로가
트랜잭션 잠금 뒤 검사한다. 적용된 세트와 규칙은 update/delete 트리거로 불변화한다.

규칙 JSON은 금액을 원, 금리를 bp, 세율을 ppm 정수로만 저장한다. Rust의 도메인별 tagged enum으로
역직렬화하며 알 수 없는 필드·부분 구조·범위 밖 값은 서버 시작 또는 규칙 조회 때 실패시킨다.

v1은 `tax/generalFinancialIncome · tax/basicIncomeBrackets · deposit/protection ·
isa/eligibilityAndTax · pension/contributionAndWithdrawal · gold/krxWithdrawal` 행으로 이 문서의 확정 수치를
나눠 저장한다. JSON 키는 camelCase이고 아직 확정하지 않은 CMA 스프레드나 가상 금 인출수수료는 넣지
않는다. 그 값들은 구현 전 상품 버전 절에서 정한 뒤 새 policy set이 아니라 별도 불변 상품 카탈로그로
관리한다.

`isa/eligibilityAndTax`는 최소 나이 `19`, 근로소득 예외 최소 나이 `15`, 종합과세 이력 조회연수 `3`,
서민형 총급여·종합소득 경계 `50,000,000원 · 38,000,000원`, 연 납입한도 `20,000,000원`, 총 한도
`100,000,000원`, 최대 납입연수 `5`, 의무기간 `3년`, 일반형·서민형 비과세 한도와 분리과세율을 모두
가진다. `pension/contributionAndWithdrawal`은 두 공제 한도, 소득 경계, 소득세·지방세 공제율,
최소 나이·가입기간, IRP 위험한도, 연령별·종신·연금외 세율, 연금수령한도 배율 `1,200,000ppm`과 적용
연차 `10`, 이연퇴직소득 연차별 배율을 가진다. 이 값 가운데 하나라도 빠지거나 알 수 없는 키·범위 밖
값이 있으면 해당 policy set을 사용한 명령과 snapshot을 실패시키며 코드 상수로 보충하지 않는다.

## 3. 돈과 계좌의 단일 권위

### 3.1 즉시 사용 지갑

기존 `save.cash_krw`는 없애거나 복제하지 않고 **즉시 사용 가능한 정산 지갑**으로 유지한다. 캐릭터 시작
자금, 급여, 생활비와 외부 계좌 입출금의 관문이다. 금융계좌 내부 현금은 `financial_account.cash_krw`에
별도로 존재하고, 지갑과 계좌 사이 이동은 반드시 원장 트랜잭션을 만든다.

`financial_account`는 다음 타입을 지원한다.

- `taxableBrokerage` — 일반 투자계좌
- `cma` — 매일 이자가 생기는 현금성 계좌
- `isaGeneral`, `isaLowIncome` — 일반형·서민형 ISA
- `pensionSavings` — 연금저축계좌
- `irp` — 개인형 퇴직연금
- `krxGold` — 금 현물 전용계좌

계좌는 `open · matured · closed` 상태를 가진다. 한 세이브에 ISA는 합계 하나만 열 수 있고, 기본 일반
투자계좌는 새 런에 한 개 자동 생성한다. 계좌를 닫아도 원장과 세금 기록은 지우지 않는다.

모든 금융계좌는 `run_revision`을 가진다. 새 캐릭터를 시작하면 이전 런의 열린 계좌를 `closed`로,
미실행 예약 정산을 `cancelled`로 전이하고 새 revision의 기본 일반계좌를 만든다. 과거 계좌·원장·체결은
감사 이력으로 남지만 현재 스냅샷과 명령은 세이브의 현재 `run_revision`만 읽는다. 따라서 같은 세이브에서
캐릭터를 여러 번 시작해도 계좌 ID나 source identity가 새 런과 충돌하지 않는다.

### 3.2 M1 포지션 이전

M1의 `(save_id, symbol)` 포지션과 체결 원장에는 기본 일반 투자계좌 ID를 추가한다. 마이그레이션은 각
기존 체결의 distinct `(save_id, run_revision)`과 현재 세이브 revision마다 기본 계좌를 먼저 만든 뒤
`asset_position`은 현재 런 계좌로, `trade_execution`은 해당 체결의 런 계좌로 backfill하고,
새 유니크 키를 `(save_id, account_id, symbol)`로 바꾼다. 서버를 먼저 dual-read하는 단계 없이 한 배포에서
바꿀 수 있도록 마이그레이션과 새 바이너리는 함께 검증하되, 운영 롤백용 이전 이미지는 보존한다.
append-only 체결의 update 트리거는 이 backfill 구간에만 제거했다가 같은 마이그레이션에서 즉시 복구한다.

주문은 `accountId`를 받고 계좌 소유권·상태·상품 허용 여부를 검증한다. 기존 M1 주문 행의 수수료·세금은
0원 그대로 남으며 M2 규칙을 소급하지 않는다.

## 4. append-only 복식 원장

잔액 테이블은 현재 상태를 빠르게 읽는 권위이고, 원장은 모든 금액 이동을 설명하는 감사 기록이다.

- `ledger_transaction` — `(save_id, run_revision, source_kind, source_id)` 유니크 idempotency, 게임일,
  설명, 제도 버전
- `ledger_posting` — 트랜잭션별 계정 코드와 signed KRW 금액
- 한 트랜잭션의 posting 합은 항상 0이다.
- 지갑·계좌·세금원천징수·이자수익·수수료·상품원금·분배금 계정 코드를 고정 enum으로 둔다.
- update/delete는 트리거로 막고 잘못된 기록은 반대 posting을 가진 새 정정 트랜잭션으로만 되돌린다.
- 원장 기록과 잔액/포지션 변경은 같은 MySQL 트랜잭션에서 커밋한다.

금액이 플레이어 자산 밖으로 나갈 때도 상대 posting이 있어야 한다. 예를 들어 예금 이자 10,000원과
원천징수 1,540원은 계좌 현금 `+8,460`, 이자수익 `-10,000`, 원천징수채무 `+1,540`으로 균형을 맞춘다.
API는 내부 차변/대변 기호 대신 사용자 관점의 `grossAmountKrw · taxKrw · feeKrw · netAmountKrw`를 제공한다.

M2 전환 전 잔액과 체결을 억지로 과거부터 재구성하지 않는다. 각 세이브의 현재 런에
`m2OpeningBalance` 원장 거래를 한 번 만들고 지갑, 기존 LLX 취득원가, 양수로 저장된 부채를 각각
`wallet · productPrincipal · debtPrincipal` posting으로 옮긴 뒤 `openingEquity`가 합계를 맞춘다.
기존 금융계좌 현금은 전환 시 0원이다. 이후 새 런도 시작 현금·부채를 같은 개장 거래로 기록한다.
M3·M4에서 급여·대출을 붙일 때 `save.cash_krw`와 `save.debt_krw`를 이 원장 범위 안에서 함께 갱신하며,
별도 설명되지 않는 잔액 변경은 허용하지 않는다.

원장의 부호는 플레이어 자산·비용·원천징수 유출을 양수, 부채·수익·자본을 음수로 둔다. 이 부호는
내부 감사용이고 API 표시 금액은 사용자 관점으로 변환한다. posting은 최소 두 개, 각 금액은 0이 아닌
`BIGINT`이며 i128 합계가 정확히 0일 때만 한 저장 경로가 전부 삽입한다.

## 5. 예약 정산과 일일 파이프라인

`scheduled_settlement`은 미래 게임일에 실행할 의무를 저장한다.

- 필수값: save, due game day, kind, immutable payload, source identity, 상태
- 상태: `pending · settled · cancelled`
- `settled` 결과는 `applied · noMovement`로 나눈다. 돈이 움직인 `applied`는 실행 원장 transaction을
  반드시 가리키고, 1원 미만 CMA 이자나 적금 미납처럼 정상 처리됐지만 돈이 움직이지 않은
  `noMovement`는 원장 없이 고정 결과 사유를 남긴다. 0원 또는 가짜 1원 posting은 만들지 않는다.
- `(save_id, run_revision, source_kind, source_id, occurrence)` 유니크 키로 중복 예약을 막는다.
- CMA 정산의 source kind는 `cmaAccount`, 적금 만기는 `savingsMaturity` kind를 각각 사용한다.
  예금 계약과 적금 계약, 납입 회차와 만기 의미를 같은 enum 값으로 겸용하지 않는다.
- 취소는 삭제가 아니라 상태 전이와 사유를 남긴다. pending 의무는 아직 돈을 움직이지 않았으므로
  취소만을 위한 가짜 1원 원장을 만들지 않는다. 이미 원장이 생긴 정산을 되돌려야 할 때만 별도 correction
  transaction을 만들고 원본 settlement는 settled 상태로 보존한다.

하루 진행은 다음 순서를 한 플레이어 트랜잭션에서 실행한다.

1. 네 부분 커서 `(world, runRevision, stateRevision, gameDay)`와 policy set을 잠근다.
2. 이미 보장된 다음 시장일을 입력으로 포지션을 평가한다.
3. 그 날의 CMA 이자와 일 단위 보수·분배금 권리를 계산한다.
4. due settlement를 `(due_game_day, id)` 순으로 잠그고 각각 한 번만 실행한다.
5. 세금 누계·계좌·포지션·지갑·원장·정산 상태를 함께 갱신한다.
6. game day와 state revision을 각각 1 증가시키고 커밋한다.
7. 커밋 뒤 하나의 완성된 스냅샷만 SSE로 보낸다.

정산 하나마다 revision을 올리지 않는다. 하루 진행 전체가 한 상태 전이다. 실패하면 그 날의 모든 플레이어
변경을 롤백하며, 재시도는 같은 source identity로 같은 원장에 수렴한다. 시장 캐시는 공유 불변 데이터라
기존 M1처럼 플레이어 트랜잭션보다 먼저 준비할 수 있다.

정산 payload는 잠금 대상을 찾기 위한 불변 ID만 가진 strict tagged JSON이다. 모든 payload에
`version: 1`을 두고 알 수 없는 필드·버전·종류 조합은 하루 전체를 실패시킨다. CMA는
`accountId · cmaTermsId`, 예금 만기는 `accountId · contractId`, 적금 회차는
`accountId · contractId · installmentNo`를 저장한다. 금액·금리·세율은 payload에 복제하지 않고 잠근
계약·상품 버전·policy에서 읽는다.

일일 정산 저장 경계는 다음과 같다.

1. due 후보 payload를 먼저 해석해 잠글 계좌·계약 ID를 수집한다.
2. 계좌를 ID 오름차순으로 잠그고 그 자식 상품 상태를 잠근다.
3. 향후 포지션 정산이 생기면 `(account_id, symbol)` 순으로 잠근 뒤 due settlement 전체를
   `(due_game_day, id)` 순으로 잠근다.
4. 잠금 전 후보와 최종 행을 다시 비교한 뒤 순수 planner가 앞 정산의 잔액 효과를 뒤 정산 입력에
   이어서 하루 전체 계획을 만든다.
5. 잔액·상품·세금·원장·후속 예약을 기록하고 각 pending 행을 조건부로 한 번만 settled 전이한다.

두 번째 이후 정산의 계산·원장·상태 기록이나 조건부 전이 하나라도 실패하면 첫 정산을 포함해 그 날의
변경을 모두 롤백한다. 정산마다 savepoint나 별도 commit을 두지 않는다. 원장 source ID는 settlement ID를
사용해 예약 source unique key와 별도로 실행 중복을 한 번 더 막는다.

### 5.1 고정소수점 잔여분

연이율에서 일 이자를 계산할 때 매일 내림하면 장기 결과가 과도하게 작아진다. 상품별로
`interest_remainder`를 저장하고 `principal × annual_rate_bp + prior_remainder`를 `365 × 10,000`으로
나눈 몫만 원으로 지급하며 나머지를 다음 날로 넘긴다. 윤년에도 상품 계약상 일수 기준이 365이면 365를
쓴다. 상품이 실제/365와 다른 day-count를 쓰면 상품 버전 데이터로 명시한다. 모든 중간 계산은 i128이고
음수·오버플로·알 수 없는 기준은 오류다.

## 6. 현금성 상품

### 6.1 CMA

M2는 RP형과 발행어음형 두 카탈로그를 제공한다. 금리는 그 날 v3 시장의 `treasury3mBp`에 상품별 고정
스프레드를 더하고 0 미만이면 거절한다. 일 이자를 계좌에 재투자하고, 지급 시 일반 이자소득 원천징수와
연간 금융소득 누계를 함께 기록한다. 상품 스프레드와 최소 잔액은 콘텐츠 버전 데이터다.

계좌 개설 때 다음 게임일의 `cmaInterest`를 `occurrence: 1`로 예약한다. 실행 뒤에는 처리한 게임일보다
뒤인 다음 정산을 occurrence를 1 증가시켜 예약하며 0회차 CMA 예약은 허용하지 않는다. 일 이자 몫이
0원이면 remainder만 갱신하고 `noMovement`로 끝내며, 양수이면
gross 이자·원천세·세후 재투자를 한 균형 원장에 기록한다.

M2-B 최초 불변 상품 카탈로그는 다음 두 개다. 둘 다 가상 상품이고 예금자보호 비대상이다.

| key | 표시명 | 기관 | 일 금리 | 최소 이자 계산 잔액 | day-count |
|-----|--------|------|---------|----------------------|-----------|
| `cma-rp-2026-v1` | 라이프 CMA RP형 | `life-bank-a` | 당일 `treasury3mBp + 0bp` | 10,000원 | 365 |
| `cma-issued-note-2026-v1` | 라이프 CMA 발행어음형 | `life-bank-b` | 당일 `treasury3mBp + 20bp` | 1,000,000원 | 365 |

계산 금리가 0bp 미만이면 그 날 전체를 실패시키며 임의로 0에 고정하지 않는다.

### 6.2 정기예금·적금

- 정기예금은 가입 때 원금·연이율·만기 게임일을 고정한다.
- 정기적금은 월 납입일과 회차별 원금을 기록하며 잔액 부족 회차는 미납으로 남긴다.
- 만기 이자는 계약의 day-count와 납입별 보유일수로 정수 계산한다.
- 중도해지는 계약에 고정한 중도해지율로 그 날까지 다시 계산하고 만기 예약을 취소한다.
- 이자는 지급 시점에 과세하며 원금은 과세하지 않는다.
- 예금자보호 표시는 동일 가상 금융기관의 보호 대상 원금과 소정 이자를 합산한다. 한도를 넘겨도 가입을
  막지 않고 보호/비보호 금액을 나눠 보여 준다.

적금 납입일에 잔액이 충분하면 계좌 현금에서 상품 원금으로 옮기는 원장을 만들고 회차를 `paid`로,
부족하면 원장 없이 회차를 `missed`로 확정한다. 2..12회차와 만기 정산은 가입 transaction에서 모두
미리 예약해 중간 실행 결과와 무관하게 원래 일정을 보존한다. 마지막 납입과 만기는 별도
`savingsInstallment · savingsMaturity` 정산으로 처리한다.

가상 부보기관은 `life-bank-a`(라이프은행 A), `life-bank-b`(라이프은행 B) 두 곳이다. M2-B에서 예금·적금은
열린 `taxableBrokerage`의 현금을 출금·만기 입금 계좌로 사용한다. M2-C가 ISA·연금 상품 허용표를 추가할
때 이 allowlist를 확장하며 CMA 계좌 자체에는 별도 예적금 계약을 넣지 않는다.

각 기관에 다음 상품을 한 개씩 둔다. 가입일의 `treasury3mBp + spreadBp`를 0bp 이상인지 확인해 계약
연이율로 고정한다. 계산된 계약 연이율이 상품의 중도해지율보다 낮으면 그 날은 `rateUnavailable`로
가입을 거절하며 두 금리를 뒤집거나 임의 보정하지 않는다.

| kind | key suffix | A spread | B spread | 기간·납입 | 중도해지율 | 금액 범위 | 보호 |
|------|------------|----------|----------|-----------|------------|-----------|------|
| 정기예금 | `term-deposit-12m-2026-v1` | +20bp | +35bp | 365일 | 연 50bp | 100,000원..1,000,000,000원 | 대상 |
| 정기적금 | `installment-savings-12m-2026-v1` | +50bp | +65bp | 12개월·12회 | 연 50bp | 회당 10,000원..10,000,000원 | 대상 |

기관별 전체 key 앞에는 `life-bank-a-` 또는 `life-bank-b-`를 붙인다. 적금 1회차는 가입 transaction에서
즉시 납입하고, 2..12회차는 가입일의 매월 같은 일자에 납입한다. 해당 일자가 없는 달은 그 달 말일로
당기며, 만기는 가입일의 12개월 뒤 같은 규칙으로 정한다. 따라서 마지막 납입과 만기는 같은 날 경쟁하지
않는다. 가입 시 첫 회차 현금이 부족하면 명령을 거절하고, 이후 부족 회차는 `missed/noMovement`로 남긴다.

이자 gross는 `principal × annualRateBp × heldDays / (365 × 10,000)`의 원 미만을 버리되 CMA는 remainder를
다음 날로 넘긴다. 지급 때 소득세와 지방소득세는 각각 `floor(gross × ratePpm / 1,000,000)`으로 독립
계산하고 net은 gross에서 두 세액을 뺀 값이다. 예금자보호 화면은 기관별 보호 대상 계약의 원금과 그 날까지
발생한 gross 소정이자를 합산해 `min(합계, 100,000,000원)`을 보호, 나머지를 비보호로 표시한다.

`cash_product_version`은 임의 parameters JSON을 쓰지 않고 `rate_reference · spread_bp ·
minimum/maximum_amount_krw · term_days/term_months · installment_count · early_termination_rate_bp ·
day_count_denominator`의 typed nullable 열을 가진다. product kind별 CHECK가 필요한 열과 금지되는 열의
shape, 금액·기간·금리 범위를 강제한다. 가입 계약에는 계산에 쓰는 조건을 다시 복사해 카탈로그를 조회하지
않아도 이미 시작한 런의 결과가 고정되게 한다. 기관·상품 버전은 게시 뒤 update/delete할 수 없다.

2025-09-01부터 동일 부보금융회사별 1인당 보호 대상 예금의 원금과 소정의 이자를 합하여 1억원까지
보호한다. 게임은 부보금융회사를 가상 기관 ID로 대응하고 서로 다른 두 기관을 제공해 분산 선택을 시험할
수 있게 한다. 한도를 넘긴 가입도 허용하되 보호 금액과 비보호 금액을 분리한다.

- 근거: [금융위원회 예금보호한도 시행 안내](https://www.fsc.go.kr/no010101/85200)
- 근거 시행일: `2025-09-01`
- 근거 확인일: `2026-07-26`

## 7. 2026-v1 세제 규칙

게임은 법률 상담이나 세무 신고 도구가 아니다. 아래 값은 교육용 시뮬레이션의 고정 규칙이며 화면에
기준 버전과 비실거래 고지를 함께 표시한다.

### 7.1 일반 금융소득

- 별도 특례가 없는 예금·CMA 이자, 일반 유통 국채의 이자, LLX 분배금은 지급 때 소득세 14%와
  개인지방소득세 1.4%, 합계 15.4%를 원천징수한다.
- `gross_financial_income_krw`에는 해당 과세기간의 종합과세 대상 이자·배당만 합산한다. 비과세소득,
  무조건 분리과세소득과 출자공동사업자 배당 등 법정 비교과세 제외 항목은 별도 bucket에 기록한다.
- `financial_income_year`는 일반 누계와 섞이지 않는
  `tax_exempt_financial_income_krw · separate_tax_financial_income_krw ·
  separate_withheld_income_tax_krw · separate_withheld_local_income_tax_krw`를 함께 가진다. 정상·법정
  부득이 ISA 해지는 비과세분과 9%·0.9% 분리과세분을 이 열에 기록하고, 3년 전 일반 해지만
  `gross_financial_income_krw`와 일반 원천세 열을 증가시킨다. 이후 종합과세 추가세액에서 차감하는
  원천세에는 일반 열만 사용한다.
- `gross_financial_income_krw`가 20,000,000원을 초과하면 다음 해 1월 1일에 §8.6의 세액을
  확정·고정하고 실제 제도와 같은 5월 31일 확정신고 정산을 예약한다.
- 소득세법 제62조의 비교산식은 다음 두 금액 중 큰 값이다. `F`는 종합과세 대상 금융소득, `O`는 금융소득
  외 다른 종합소득금액에 공제·결손금 등 policy data를 적용한 값, `basicTax(x)`는 2026 기본세율 산출세액이다.
  1. `basicTax(max(F - 20,000,000, 0) + O) + 20,000,000 × 14%`
  2. `F`의 소득 종류별 원천징수세율로 계산한 세액의 합계 `+ basicTax(O)`
- v1의 금융소득 종류는 모두 일반 14% 대상이지만 계산기는 소득 종류별 세율을 입력받아 비교산식의 두
  항을 그대로 보존한다. 개인지방소득세도 같은 비교과세 구조와 대응 세율로 별도 계산한다.
- 종합소득 기본세율 구간은 14,000,000원 이하 6%, 50,000,000원 이하 15%, 88,000,000원 이하 24%,
  150,000,000원 이하 35%, 300,000,000원 이하 38%, 500,000,000원 이하 40%, 1,000,000,000원 이하
  42%, 그 초과 45%와 각 구간 누진액을 policy data로 둔다.
- 산출세액에서 원천징수세액과 적용 가능한 세액공제를 차감한 결과가 음수면 환급, 양수면 추가 납부
  posting을 만든다.
- 실제 제도에는 2026-01-01 이후 지급되는 법정 고배당기업 특례배당소득의 14%~30% 분리과세가
  도입되었다. LLX는 법정 내국법인·공시 요건을 판정하지 않는 가상 지수상품이므로 v1에서 명시적으로
  제외한다.
- M2는 해외자산을 제공하지 않는다. 2025년 귀속분부터 바뀌어 2026년 신고에 처음 적용된 펀드
  외국납부세액공제와 ISA·연금계좌의 국외원천소득 조정은 v1 비대상이다.

- 근거: [소득세법 제14조](https://law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1032724885),
  [제55조](https://law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1023583825),
  [제62조](https://www.law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029623903),
  [제129조](https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1017632265)
- 2026 변경 근거: [국세청 고배당기업 배당 분리과세 안내](https://nts.go.kr/nts/na/ntt/selectNttInfo.do?mi=2201&nttSn=1349597),
  [국세청 펀드 외국납부세액공제 안내](https://www.nts.go.kr/nts/na/ntt/selectNttInfo.do?mi=2207&nttSn=1350542)
- 근거 시행일: `2026-01-01`(2026 지급분·귀속분), 현행 법령판 `2026-07-01`
- 근거 확인일: `2026-07-26`

### 7.2 ISA

v1 ISA 규칙은 다음과 같다.

- 가입일 또는 연장일에 19세 이상이거나, 15세 이상이면서 직전 과세기간에 비과세소득만이 아닌 근로소득이
  있어야 한다.
- 가입일 또는 연장일 직전 3개 과세기간 중 한 번이라도 금융소득종합과세 대상자였으면 ISA 과세특례를
  적용하지 않는다.
- 1인 1계좌이고 계약기간은 3년 이상이다.
- 총 납입한도는 100,000,000원이다. 해당 시점 납입 가능액은
  `max(0, min(100,000,000 - 누적납입액, 20,000,000 × (1 + min(가입후경과연수, 4)) - 누적납입액))`으로
  계산해 연 20,000,000원의 미사용분을 이월한다.
- 해지일에 계좌의 법정 이자·배당소득에서 조세특례제한법 시행령이 손실로 인정하는 금액만 차감한다.
  화면 평가손익이나 모든 자산의 가격 손실을 무조건 통산하지 않고 상품별 `isa_tax_profit_krw`와
  `isa_deductible_loss_krw`를 별도로 누계한다.
- 일반형은 통산 순이익 2,000,000원, 서민형은 4,000,000원까지 비과세한다. 서민형은 직전 과세기간에
  근로소득만 있거나 근로소득과 종합과세 제외소득만 있는 사람 중 총급여액 50,000,000원 이하인 경우,
  또는 총급여액 50,000,000원을 초과하지 않으면서 종합소득금액 38,000,000원 이하인 경우로 판정한다.
- 비과세 한도 초과 순이익은 소득세 9%와 개인지방소득세 0.9%, 합계 9.9%로 분리과세한다.
- 최초 계약일부터 3년 전까지는 계약기간 중 누적 납입액을 초과해 인출하면 그 날 중도해지된 것으로 본다.
  누적 납입액 이내 인출은 허용하지만 인출액만큼 납입한도가 복원되지는 않는다.
- 최초 계약일부터 3년 전 전체 해지는 사망·해외이주 등 policy data에 등록된 법정 부득이한 사유가 아니면
  과세특례를 추징하고 계좌에서 발생한 소득을 일반 과세로 다시 계산한다.

v1의 3년 전 일반 해지는 `max(isa_tax_profit_krw, 0)` 전체에 일반 금융소득의 소득세 14%와 지방소득세
1.4%를 적용한다. 이때 ISA 인정손실은 통산하지 않고 gross 금융소득 누계에도 더한다. 3년을 채운 정상
해지는 `max(isa_tax_profit_krw - isa_deductible_loss_krw, 0)`에서 유형별 비과세 한도를 차감한 초과분에
9%와 0.9%를 적용하며 종합과세 금융소득에는 넣지 않는다.

M3 전의 새 런은 나이, 직전년도 소득 0원, 직전 3개 과세기간 금융소득종합과세 대상 이력 `false`를 명시적인
캐릭터 정책 입력으로 생성한다. M3부터는 확정 세금연도 기록을 사용하며 기록이 없으면 자격 판정을
통과시키지 않는다.

- 근거: 조세특례제한법 [제91조의18](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1017631659),
  [제129조의2](https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joBrNo=02&joNo=0129&lsiSeq=286597&urlMode=lsScJoRltInfoR),
  조세특례제한법 시행령 [제93조의4](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=129144)
- 근거 시행일: `2026-01-01`, 현행 법령판 `2026-07-01`
- 근거 확인일: `2026-07-26`

### 7.3 연금저축과 IRP

- 연금저축 세액공제 대상 납입 한도는 연 6,000,000원이다.
- 연금저축과 IRP 합산 세액공제 대상 한도는 연 9,000,000원이다.
- 종합소득금액 45,000,000원 이하 또는 근로소득만 있을 때 총급여 55,000,000원 이하는 소득세 15%와
  지방세 효과를 합친 16.5%, 그 밖은 소득세 12%와 지방세 효과를 합친 13.2%를 예상 공제액으로 표시한다.
- 실제 공제액은 그 해 산출세액을 넘지 않는다. M2 화면은 납입액과 예상 공제액을 구분하고, M3의 연말
  정산이 생기기 전에는 공제되지 않은 가상 현금을 지급하지 않는다.
- 연금계좌 잔액은 `taxExcludedContribution · deferredRetirementIncome · creditedContribution · earnings`의
  세원 층위로 보존한다. 인출에는 과세제외 납입액, 이연퇴직소득, 세액공제 받은 납입액과 운용수익 순서를
  적용하고 각 층위의 원금과 수익을 합쳐 버리지 않는다.
- 세법상 연금수령은 만 55세 이후 개시를 신청하고 가입일부터 5년이 지난 뒤 연금수령한도 안에서 인출할
  때다. 이연퇴직소득이 계좌에 있으면 가입 후 5년 요건을 적용하지 않는다.
- 연금수령한도는 연금수령연차 10년까지
  `과세기간 개시일 현재 계좌 평가액 ÷ (11 - 연금수령연차) × 120%`이고 원 미만은 버린다.
  연금수령연차가 11년 이상이면 이 계산식의 한도를 적용하지 않는다.
- 이 개시일 평가액은 요청 시점의 현재액으로 역산하지 않는다. 연금계좌를 연 과세연도에는 계좌가
  개시일에 존재하지 않았으므로 `0원`을 저장하고, 다음 과세연도부터는 1월 1일 일일 transaction에서
  그 날의 이자·만기·분배금 등 정산을 적용하기 **전** 전년도 말 계좌 평가액을 계좌별 연도 행에 한 번만
  고정한다. 당해 납입·운용손익·인출은 이 값을 바꾸지 않으며, 연금 개시·인출·snapshot은 저장된 값을
  사용한다.
- 일반 연금수령 요청액이 해당 연도 남은 연금수령한도를 넘으면 요청 전체를 거절하거나 낮은 세율을
  적용하지 않는다. 남은 한도까지는 연금수령, 초과분은 연금외수령으로 한 transaction 안에서 자동 분할해
  각 세율을 적용한다. 법정 부득이 인출은 한도와 무관하게 연금수령 세율을, 명시적인 일반 중도인출은
  전액 연금외수령 세율을 적용한다.
- 세액공제 받은 납입액과 운용수익을 한도 내 연금으로 받으면 70세 미만 5.5%, 70세 이상 80세 미만
  4.4%, 80세 이상 3.3%를 적용한다. 법정 종신계약은 3.3%를 적용한다. 이 비율은 개인지방소득세를
  포함한다.
- 이연퇴직소득의 연금수령 세율은 연금외수령 퇴직소득세율의 10년 이하 70%, 11~20년 60%, 20년 초과
  50%다. M2에는 이연퇴직소득 유입이 없지만 원장과 규칙 구조는 이 층위를 보존한다.
- 법정 부득이한 사유가 아닌 연금외수령에서 과세제외 납입액은 과세하지 않고, 이연퇴직소득은 퇴직소득세,
  세액공제 받은 납입액과 운용수익은 기타소득세 16.5%를 적용한다. 의료 목적·천재지변 등 소득세법
  시행령 제20조의2의 부득이한 인출은 연금소득으로 처리한다.
- IRP는 매수 직후 70% 한도 대상 위험 운용방법의 합계가 적립금의 70%를 넘으면 주문을 거절한다. 단순
  가격 상승으로 사후 초과한 경우 강제매도하지 않되 위험자산 추가 매수를 막는다. LLX는 적격
  집합투자증권으로 정의해 위험자산에 포함하고 현금·예금과 v1 국채는 안전자산에 포함한다.
- KRX 금 현물은 IRP의 허용 운용방법으로 보지 않고 v1 편입 대상에서 제외한다. 향후 금 ETF를 추가하면
  KRX 금 현물과 다른 상품으로 등록하고 감독규정에 따른 위험자산 여부를 별도로 판정한다.
- IRP 중도인출 사유 enum은 `homePurchase · housingDeposit · medicalCare · disaster · bankruptcy ·
  rehabilitation · securedLoanRepayment`으로 고정한다. 각각 무주택자의 본인 명의 주택구입, 무주택자의
  주거 목적 전세금·보증금, 본인·배우자·부양가족의 6개월 이상 요양 의료비, 재난 피해, 신청 전 5년 이내
  파산선고, 신청 전 5년 이내 개인회생절차 개시, 법정 담보대출 원리금 상환 요건을 검사한다. 전세금·보증금의
  법정 횟수 제한과 의료비 임금총액 요건은 가입자 유형별 policy field로 판정한다. 사유 없는 일반 인출은
  거절한다.
- 퇴직급여법상 IRP 연금 급여는 55세 이상이고 지급기간이 5년 이상이어야 한다. 이는 세법상 계좌 가입 후
  5년 요건과 별개의 검증이다.

- 근거: 소득세법 [제59조의3](https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1032884269),
  [제129조](https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1017632265), 소득세법 시행령
  [제20조의2](https://law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=126774),
  [제40조의2](https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1032953507)
- IRP 근거: 근로자퇴직급여 보장법 시행령 [제2조](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1032588759),
  [제14조](https://law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=71026),
  [제18조](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=71031),
  [금융위원회 퇴직연금 위험자산 한도 안내](https://www.fsc.go.kr/no010101/73294?curPage=417&srchBeginDt=&srchCtgry=&srchEndDt=&srchKey=&srchText=)
- 근거 시행일: 소득세 법령판 `2026-07-01`, 퇴직급여법 시행령 `2026-03-24`
- 근거 확인일: `2026-07-26`

## 8. LLX·국채·금·연간 세금 마감

### 8.1 M2-D v4 시장과 불변 상품 묶음

M2-D는 `m2-2026-calibration-v4`와 `m2-2026-v4` 월드를 추가한다. v4는 v3의 주식 레짐,
주식 수익률, 정책금리 평균회귀, 기간구조와 `equityRateShockPpm` 계산을 수정 없이 그대로
재사용하고 CPI, LLX 보수 반영 가격, 금 가격을 독립 상태로 덧붙인다. v1·v2·v3의
캘리브레이션·월드·일봉·고정 회귀 벡터는 바꾸지 않는다. 기존 런은 저장된 월드를 계속 쓰며,
v1~v3 일봉의 M2-D 필드는 부분 기본값이 아니라 전부 `NULL`이다. 일봉을 읽을 때는 v4 필드 전부가
있거나 전부가 없어야 하며 부분 행은 손상으로 처리한다.

v4 CPI와 금은 다음 고정 정수 규칙을 쓴다.

- CPI day 0은 `1,000,000`, 연 명목 상승률은 `20,000ppm`, day-count는 Actual/365다.
  매 달력일 `numerator = priorCpiIndex × 20,000 + priorRemainder`,
  `increase = floor(numerator / (365 × 1,000,000))`를 더하고 나머지를 다음 날로 이월한다.
- 금 day 0 종가는 1g당 `120,000원`이다. 휴장일은 가격을 유지하고 혁신값을 소비하지
  않는다. 개장일 수익률은 직전 개장일 이후 누적 CPI 등락률에
  `-250ppm × treasury10yChangeBp`와 독립 고정 정수 정규 근사 혁신값을 더한다. 혁신 표준
  크기는 `11,000ppm`이며 주식·금리와 다른 entropy stream을 쓴다.
  `treasury10yChangeBp`는 현재 개장일 10년 수익률에서 직전 개장일 10년 수익률을 뺀 signed bp다.
- 누적 CPI 등락률은
  `roundHalfUp((currentCpiIndex - priorOpenCpiIndex) × 1,000,000 / priorOpenCpiIndex)`,
  금 종가는 `roundHalfUp(priorGoldClose × (1,000,000 + returnPpm) / 1,000,000)`이다.
  가격이 0원 이하가 되거나 범위를 넘으면 포화시키지 않고 그 날 생성을 실패시킨다.

`roundHalfUp(n / d)`는 음수가 아닌 분자에서 `floor((n + floor(d / 2)) / d)`다. 세액,
수수료, 분배금·쿠폰·이자의 원 미만은 각 규칙에서 명시한 대로 버리고 가격만 이
반올림을 쓴다. 모든 곱셈·덧셈·나눗셈 중간값은 checked `i128`이며, DB에 내리기 전에
원·bp·ppm·수량 범위를 다시 검증한다. 부동소수점은 생성·가격·세금 경로에 쓰지 않는다.

M2-D는 sealed `kr-individual-2026-v1`을 고치지 않고, v1의 모든 rule을 값 변경 없이
복제한 sealed `kr-individual-2026-v2` policy set을 만든다. `tax/annualFinancialIncomeAssessment`
행에 신고일, 종합과세 기준, 비교산식, source별 세율, 현금 부족 처리를 strict typed 값으로
넣고 M2 금융소득 외 종합소득·세액공제 기본값 0원도 필수 필드로 둔다. 계좌 허용·원천징수·ISA·
연금·금 VAT 등 제도 판정은 반드시 현재 런이 고정한
`policy_rule` 행에서 읽고 코드 상수로 보충하지 않는다. 기준일은 v1과 같은 `2026-01-01`이지만
API의 policy key로 둘을 구분한다.
v2를 seal할 때 `tax/generalFinancialIncome`의 원천징수율과
`tax/annualFinancialIncomeAssessment`의 source 세율이 서로 다르면 게시를 거절해 두 pinned row 중 하나를
코드가 임의로 선택하는 상태를 막는다.

제도가 아닌 상품 수치는 `index_product_version · bond_product_version ·
gold_product_version`에 둔다. 보수·매매수수료·인출수수료를 포함한 **모든 상품 fee**는
게시된 카탈로그 행의 typed 컬럼에 고정하고 update/delete를 막는다.
`market_world_product_bundle`은 v4 월드와 정확한 상품 버전 목록을 한 번만 묶는다. 새 런은
동일 transaction에서 게시된 v4 번들과 sealed v2 policy assignment을 검증·고정한다. 기존 런에
상품 번들이나 v2 policy를 나중에 덧붙이지 않는다.

M2-D 스키마는 M2-C가 완료된 다음 `0014` 이상의 새 마이그레이션으로만 추가한다. 기존
`0001`~`0013`을 수정하지 않는다.

### 8.2 LLX 보수·분배금·계좌별 손익

LLX는 국내주식형 지수상품이다. v4의 기존 `closeKrw`는 v3 회귀를 보존하는 benchmark
equity 가격이고, 실제 체결은 별도 `llxCloseKrw`를 쓴다. M2-D 최초 불변 상품
`llx-domestic-equity-2026-v1`은 day 0 가격 `100,000원`, 연 보수 `1,500ppm`(0.15%),
연 분배율 `20,000ppm`(2%), Actual/365, 매수·매도 수수료와 거래세 `0ppm`을 가진다.

보수는 따로 현금을 빼지 않고 가격에 내재한다. 매 달력일
`annualManagementFeePpm + priorFeeRemainder`를 365로 정수 나눗셈한 결과를 당일 보수 ppm으로 누적하고
나머지를 이월한다. 휴장 동안은 가격을 고정하되 보수를 pending으로 누적하고, 다음 개장일에
`benchmarkDailyReturnPpm - accumulatedFeePpm`을 직전 LLX 종가에 적용해 원 단위로
`roundHalfUp`한다. 따라서 benchmark 가격 경로는 v3과 같고 LLX만 보수만큼 달라진다.
v1~v3 런은 기존 benchmark 종가로 계속 체결하고 보수·분배금을 소급하지 않는다.

분배금 기준일은 3·6·9·12월의 **마지막 KRX 개장일**, 지급일은 그 다음 두 번째
KRX 개장일(T+2)이다. 기준일 일일 transaction에서 계좌별 보유 수량을 고정하고, 1주당
분배금을 `floor(recordLlxCloseKrw × 20,000 / (4 × 1,000,000))`로 계산한다.
`llx_distribution_entitlement`은 기준일 수량·1주당 금액·gross·지급일·계좌를 불변으로
보존하며, 기준일 후 매도해도 권리가 바뀌지 않는다. 0원은 지급일에 `noMovement`로 완료한다.
남은 pending 권리가 있는 ISA는 지급 전 폐쇄하면 세원이 사라지므로 `accountNotEmpty`로 거절한다.

v4 LLX 매매 허용 계좌는 `taxableBrokerage · isaGeneral · isaLowIncome · pensionSavings ·
irp`다. IRP에서는 위험자산이며 매수 즉시 사후 평가액으로 70% 한도를 검증한다. v1~v3 기존
런은 M2-C 체크포인트 규칙을 유지해 `taxableBrokerage`에서만 기존 LLX를 매매하고 절세계좌는
`accountTypeNotAllowed`로 남긴다. 매도 실현손익은 기존 이동평균 취득원가 제거 규칙으로 계산한다.

- 일반계좌의 매매차익은 M2 금융소득에 넣지 않고, 분배금만 14%·1.4% 원천징수 후
  `llxDistribution` source에 누계한다.
- ISA는 분배금과 실현 매매이익을 `isa_tax_profit_krw`, 실현 매매손실의 절대값을
  `isa_deductible_loss_krw`에 넣고 즉시 원천징수하지 않는다.
- 연금저축·IRP는 분배금을 `earnings` 층에 더한다. 매매손익은 이미 매일 시가평가하므로
  매도 시 다시 세원에 넣지 않는다.

### 8.3 고정금리 가상 국채

M2-D는 매월 첫 KRX 개장일에 3년·10년 만기 고정금리 가상 국채 시리즈를 각각 하나씩
발행한다. `bond_product_version`의 `kr-government-bond-3y-2026-v1`과
`kr-government-bond-10y-2026-v1`은 만기 `3 · 10년`, 액면 `10,000원`, 개별
계좌·시리즈 최대 보유 `100,000유닛`, 매수·매도 수수료 `0ppm`을 고정한다. 이는 전용계좌·
소유권 이전 제한이 있는 개인투자용 국채가 아니라 매매 가능한 일반 유통 국고채의 교육용 모사다.
`bond_series`는 `(market_world_id, product_version_id, issue_date)`를 유니크하게 두어 다른 월드의
금리 경로·시리즈를 섞지 않는다.

발행일 해당 만기의 `treasury3yBp` 또는 `treasury10yBp`를 `issueYieldBp`로 고정하고,
`couponRateBp = floor((issueYieldBp + 12) / 25) × 25`로 가장 가까운 25bp에 반올림한다.
쿠폰일은 발행일의 달력상 6개월 기념일마다이며 해당 일자가 없으면 그달 말일로 당긴다.
휴장일이어도 그 달력일에 정산한다. 1유닛 연 쿠폰은
`annualCouponKrw = floor(10,000 × couponRateBp / 10,000)`이다. 연 쿠폰원이 홀수이면
첫 반기에 `floor(annualCouponKrw / 2)`,
둘째 반기에 나머지를 지급해 매 12개월 합계가 정확히 연 쿠폰이 되게 한다. 만기일은 최종
쿠폰과 액면원금을 하나의 `bondMaturity` 정산·원장 transaction으로 지급하고, 보유 수량과
남은 FIFO lot 원가를 전부 제거한다.

거래·평가 가격은 경과이자를 포함한 1유닛 dirty price다. 오늘 이미 정산할 현금흐름은
제외하고 남은 각 쿠폰·원금 `cfKrw`와 남은 달력일 `remainingDays`에 대해
`roundHalfUp(cfKrw × 3,650,000 / (3,650,000 + yieldBp × remainingDays))`를 구한 뒤 모두 더한다.
3년물은 그 날의 `treasury3yBp`, 10년물은 `treasury10yBp`를 쓴다. 휴장일에도 남은 일수로
평가하지만 주문은 거절한다. 쿠폰·만기와 주문이 같은 개장일이면 일일 정산을 먼저 끝내고 남은
수량만 거래한다.

주문은 한 번에 `1..=100,000` 유닛이고, 매수 후 계좌·시리즈 수량도 100,000을 넘을 수 없다.
`bond_lot`은 취득 수량과 취득원가를 보존한다. 매도는 가장 오래된 lot부터 FIFO로 소진하고,
부분 lot은 `floor(lotCostBasisKrw × removedUnits / unitsBefore)`를 제거하되 전량은 남은 원가 전부를
제거한다. `bond_execution`은 gross, fee, tax, 제거 원가와 실현손익을 append-only로 남긴다.
계좌가 시리즈를 첫 매수할 때 남은 모든 쿠폰과 만기 정산을 계좌·시리즈·현금흐름 회차로
멱등 예약한다. 중간에 전량 매도해도 예약은 지우지 않고 지급일 보유량 0이면 `noMovement`로
완료한다. 같은 시리즈를 다시 매수해도 기존 미도래 예약을 중복하지 않는다.

국채는 `taxableBrokerage · isaGeneral · isaLowIncome · pensionSavings · irp`에서 매매할 수 있고 IRP
안전자산이다. 쿠폰 권리자는 별도 기준일이 아니라 **지급일 정산 시점의 보유자**다.

- 일반계좌의 채권 매도·만기상환 실현손익은 금융소득에 넣지 않고 쿠폰만 14%·1.4%
  원천징수 후
  `bondCoupon` source에 누계한다.
- ISA는 쿠폰과 매도·만기상환 실현이익을 tax profit, 실현손실의 절대값을 deductible loss에
  넣고 즉시 원천징수하지 않는다.
- 연금저축·IRP는 gross 쿠폰을 `earnings`에 더하고 매매손익을 매일 시가평가에서만 반영한다.

dirty price의 경과이자를 매도자·매수자 보유기간으로 안분하지 않고 지급일 보유자에게 전액
귀속시키는 것은 교육용 단순화다. API와 화면에 이 한계를 표시한다. 개인투자용 국채를
후속으로 추가할 때는 전용계좌, 5·10·20년 만기, 양도 제한, 만기 보유 매입액 총 2억원까지의
14% 분리과세를 별도 상품·policy로 모델링한다.

- 근거: [재정경제부 국채시장 개인투자용 국채 안내](https://ktb.moef.go.kr/personalInvGovBonds.do),
  [소득세법 제129조](https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1017632265)
- 근거 시행일: `2026-01-01`, 현행 법령판 `2026-07-01`
- 근거 확인일: `2026-07-26`

### 8.4 KRX 금 현물과 실물 인출

M2-D 금 상품 `krx-gold-2026-v1`은 1g 정수 거래, 매수·매도 수수료 `0ppm`, 매매세
`0ppm`, 100g bar당 가상 인출수수료 `20,000원`, 1kg bar당 `100,000원`을 고정한다.
현재 런에 열린 `krxGold` 계좌는 최대 하나이고, 지갑↔계좌 현금 이체를 두 방향 모두 허용한다.
LLX·국채·예적금은 금 계좌에 넣을 수 없고, 금은 다른 계좌에서 매매할 수 없다.
금 주문은 KRX 개장일에만 직전 생성된 당일 종가로 전량 체결하고 휴장일은 `marketClosed`다.

매수는 `priceKrwPerGram × quantityGram + feeKrw`를 금 계좌 현금에서 빼고 수량과 총
취득원가에 더한다. 매도·실물 인출의 제거 원가는 이동평균법으로
`floor(totalCostBasisKrw × removedGram / quantityBefore)`를 쓰되 전량은 남은 원가 전부를 제거한다.
매도대금은 금 계좌 현금으로 들어오며, 계좌 안 매매차익에는 양도·배당·이자소득세와 VAT를
부과하지 않고 금융소득에도 넣지 않는다.

실물 인출 수량은 `barSizeGram(100|1000) × barCount`다. VAT는
`floor(removedCostBasisKrw × 100,000 / 1,000,000)`, 수수료는 상품 버전의 bar별 고정액에
`barCount`를 곱한 금액이다. 인출할 금 수량과 같은 transaction에서 금 계좌 현금으로 VAT+수수료를
납부할 수 있어야 하며 부족하면 아무 상태도 바꾸지 않고 `insufficientAccountCash`다.
`gold_withdrawal`과 원장은 수량·제거원가·VAT·fee를 append-only로 남긴다.

인출된 bar는 사라지는 소비재가 아니라 `physical_gold_holding`에 bar 크기별 수량으로 남고,
현재 금 종가로 평가한 금액이 순자산에 계속 포함된다. M2에서는 실물 금의 재매도·계좌 재입고·
분할·소비를 제공하지 않는다.

- 근거: [KRX 금시장 안내](https://open.krx.co.kr/contents/OPN/01/01050206/OPN01050206.jsp),
  [KRX 금시장 부가가치세 안내](https://regulation.krx.co.kr/contents/RGL/04/04010201/RGL04010201.jsp)
- 근거 시행일: `2026-01-01` 현재 적용 제도
- 근거 확인일: `2026-07-26`

### 8.5 연금계좌 매일 시가손익

`pension_valuation_state`는 연금저축·IRP 계좌별 직전 평가 게임일과 LLX·국채 시가평가액을
보존한다. 매수는 체결가만큼 평가 기준을 더하고, 매도는 체결 시점 시가만큼 기준을 줄여
현금↔포지션 전환을 손익으로 오인하지 않게 한다. 다음 일일 transaction은 오늘의 LLX 종가와
국채 dirty price로 새 평가액을 구해 기준과의 차이를 한 번만 적용한다.

양의 시가손익은 전액 `earnings`에 더한다. 음의 시가손익 `lossKrw`는
`earnings → creditedContribution → deferredRetirementIncome → taxExcludedContribution` 순으로 각 층의
남은 금액까지 소진한다. 손실이 전체 세원층 합을 넘거나 적용 후 세원층 합과 계좌 총가치가
다르면 손상 상태로 보고 하루 전체를 롤백한다. 이후 이익은 소진된 납입 층을 복원하지 않고
새 `earnings`로 쌓인다.

쿠폰·LLX 분배금·예적금 이자는 시가손익과 별개로 gross를 `earnings`에 더한다. 쿠폰일
국채 평가는 당일 지급 현금흐름을 뺀 ex-coupon dirty price를 써서 가격 하락과 gross 쿠폰이
이중 계산되지 않게 한다. 만기일 액면원금은 수익이 아닌 포지션→현금 대체이므로,
평가 planner가 `ex-maturity position value + due principal`을 직전 평가액과 비교하고 원금을 `earnings`에
다시 넣지 않는다. 최종 쿠폰만 별도 gross 수익이다. 거래 fee가 0원이 아닌 후속 상품은 fee로 줄어든
계좌 가치도 같은
loss waterfall에 넣는다. 각 변화는 변경 전·후 층, 시가손익, 원인을 `tax_account_value_event`에
append-only로 남긴다.

각 과세연도 1월 1일에는 §7.3의 연금수령한도용 opening value를 **당일 시가평가·쿠폰·
분배금·예적금·기타 정산 전** 직전일 마감 계좌 가치로 한 번만 고정한다. 연도 중간 연금
개시·인출·snapshot은 현재가로 역산하지 않고 이 행만 쓴다.

### 8.6 연간 금융소득 확정과 5월 신고

`financial_income_source_year`은 현재 런·과세연도·source별 gross, 소득세 원천징수,
지방소득세 원천징수를 누계한다. M2-D source enum은
`cmaInterest · depositInterest · bondCoupon · llxDistribution · isaEarlyClose`다.
`isaEarlyClose`는 3년 전 일반 해지만 포함하고, 정상·법정 부득이 해지의 비과세·분리과세는
종합과세 `F`에 넣지 않는다. 원장 source와 source-year 누계는 같은 transaction에서 갱신한다.

과세연도가 시작하면 assessment를 `open`으로 만든다. 다음 해 1월 1일 일일 transaction에서
그 날의 수입·정산 전에 직전 과세연도를 한 번만 확정한다. M2의 금융소득 외 종합소득 `O`와
적용 세액공제는 모두 0원으로 고정한다.
직전 연도 전이·예약에 성공한 뒤 같은 transaction에서 현재 연도 `open` 행을 멱등 생성하고 그 날의
새 source 누계는 현재 연도에만 넣는다.
`basicTax(x)`는 각 policy 구간의 `floor(taxableSliceKrw × ratePpm / 1,000,000)`을 합한 값이고,
소득세와 지방소득세를 각각 계산한다.

- `F <= 20,000,000원`이면 종합과세 추가세액과 환급을 둘 다 0원으로 확정하고 원천징수로
  종결한다. API의 두 비교산식과 확정세액은 각각 해당 소득세·지방소득세 원천징수
  합계로 저장해 `finalizedNoFiling` union을 부분 nullable로 만들지 않는다.
- `F > 20,000,000원`이면 소득세 비교산식 A는
  `basicTax(F - 20,000,000 + O) + floor(20,000,000 × 140,000 / 1,000,000)`,
  B는 `sum(floor(sourceGrossKrw × sourceIncomeTaxRatePpm / 1,000,000)) + basicTax(O)`다.
  지방소득세도 각 해당 구간·source 세율로 A·B를 따로 구한다.
- 소득세와 지방소득세의 각 `max(A, B)`를 확정세액으로 저장한다. 확정세액에서 각 원천징수액을
  뺀 합계가 양수면 `additionalTaxKrw`, 음수면 절대값이 `refundKrw`이며 둘 중 하나만
  0원보다 클 수 있다.

`financial_income_assessment`는 `(save_id, run_revision, tax_year)` 하나만 만들고 F, O, 두 비교산식,
확정·원천징수·추가·환급세액, policy set, nullable 5월 31일 신고일을 보존한다. DB 상태는
`open · finalizedNoFiling · filingPending · filed`로 고정한다. 1월 1일에 `F <= 20,000,000원`이면
`open → finalizedNoFiling`으로 끝내고 신고 정산을 만들지 않는다. `F > 20,000,000원`이면
세액 확정과 함께 `open → filingPending`으로 전이하고 5월 31일 `financialIncomeFiling`을 한 번만
예약한다. 확정 후 세액을 다시 계산하지 않으며, 신고일이 휴장일이어도 5월 31일에
`filingPending → filed`로 정산한다. `finalizedNoFiling`은 종결 상태이며 `filed`로 바꾸지 않는다.

신고일에 환급액은 지갑에 넣고, 추가세액은 지갑 현금을 먼저 전액 사용한 뒤 부족분을
`save.debt_krw`의 **무이자 aggregate debt**로 더한다. M2에서 이 부족분에 이자·연체·상환스케줄을
임의로 붙이지 않고 M4 대출 엔진이 부채 종류를 분리할 때 이관한다. 추가·환급액이 0원이어도
`filingPending`인 연도는 신고 정산을 `noMovement` 사유 `zeroTaxDue`로 완료하고 assessment를
`filed`로 바꾸며,
가짜 1원 posting을 만들지 않는다.

스키마는 불변 카탈로그·번들 외에
`llx_distribution_entitlement · bond_series · bond_position · bond_lot · bond_execution ·
gold_account_contract · gold_position · gold_execution · gold_withdrawal · physical_gold_holding ·
pension_valuation_state · tax_account_value_event · financial_income_source_year ·
financial_income_assessment`로 나눈다. 체결·권리·인출·가치 event는 append-only, position·assessment 상태는
명시된 조건부 전이만 허용하고 모든 현재 런 행은 save/run/account 복합 FK로 소유권을 강제한다.

## 9. 명령·API 계약

모든 상태 변경 요청은 canonical UUID `commandId`와
`expectedRunRevision · expectedStateRevision · expectedGameDay`를 가진다. 같은 command ID와 같은
payload는 한 번만 적용되고 현재 스냅샷과 최초 결과를 재생한다. 다른 payload 재사용은 충돌이다.

`command_receipt`는 `(save_id, command_id)`를 유니크하게 두고 run revision, 명령 종류, canonical
payload의 SHA-256, 커밋된 커서, 최초 결과 JSON과 연결 원장 ID를 저장하는 append-only 영수증이다.
성공한 명령과 영수증은 같은 트랜잭션에서 커밋하고 검증 거절은 저장하지 않는다. replay는 hash와 명령
종류가 모두 같을 때 최초 결과와 현재 전체 스냅샷을 반환하며, 하나라도 다르면 `idempotencyConflict`다.
기존 LLX 주문의 `orderId`는 하위 호환을 위해 필드 이름을 유지하지만 이 `commandId`와 같은 의미·형식의
식별자다. 주문 body에 별도 `commandId`를 중복해서 받지 않는다.

완료 receipt 앞에는 전역 `command_identity`를 둔다. `(save_id, command_id)`를 유니크하게 하고 명령
종류, payload hash, 최초 cursor를 append-only로 보존한다. trade·transfer·캐릭터 시작·수동 진행과 이후
모든 금융 명령은 save row를 잠근 뒤 이 identity를 먼저 검사하거나 같은 player transaction에서 만든다.
기존 receipt와 M1 체결 UUID도 identity로 backfill해 명령 종류를 가로지른 재사용을 막는다.

수동 `advance`는 여러 일일 transaction을 의도적으로 유지하므로 `command_receipt`만으로 진행 중 상태를
표현하지 않는다. `(save_id, command_id, step_no)`별 전후 cursor를 append-only
`advance_command_step`에 날짜 변경과 함께 기록하고 마지막 step에서만 receipt를 쓴다. 중간 실패 뒤
같은 payload는 저장된 다음 step부터 재개하고, 완료 replay는 날짜와 SSE를 추가하지 않는다. 상세 HTTP
계약과 클라이언트 retry 수명은 [`m0-game-loop.md` §2](./m0-game-loop.md)에 둔다. 선언적으로 같은 속도를
설정하는 runtime-only clock 요청과 서버가 만든 자동 tick은 이 durable command 진행표 대상이 아니다.

DB의 `BIGINT UNSIGNED` 리소스 ID는 API에서 모두 10진 문자열로 내보내고 요청도 canonical 10진 문자열로
받는다. JavaScript 안전 정수 범위에 기대지 않는다. 원장 조회의 `before`도 transaction ID 문자열이고,
`limit` 기본값은 50, 허용 범위는 `1..=200`이다.

초기 API는 다음과 같다.

- `GET /api/finance/accounts`
- `POST /api/finance/accounts`
- `POST /api/finance/transfers`
- `POST /api/finance/deposits`
- `POST /api/finance/deposits/{id}/close`
- `POST /api/portfolio/orders` — M1 경로를 유지하며 `accountId` 추가
- `GET /api/finance/bonds`
- `POST /api/finance/bonds/orders`
- `GET /api/finance/gold-products`
- `POST /api/finance/gold/orders`
- `POST /api/finance/gold/withdrawals`
- `POST /api/finance/isa/{id}/close`
- `POST /api/finance/pensions/{id}/start`
- `POST /api/finance/pensions/{id}/withdrawals`
- `GET /api/finance/ledger?before=&limit=`
- `GET /api/finance/tax-years/{year}`

### 9.1 M2-A 기능 경계

M2-A에서는 새 런의 기본 `taxableBrokerage` 한 개를 자동 개설하고 다음 세 경로를 먼저 연다.

- `GET /api/finance/accounts` — 현재 런 policy key와 계좌 목록을 반환한다.
- `POST /api/finance/transfers` —
  `commandId · expectedRunRevision · expectedStateRevision · expectedGameDay · accountId · direction · amountKrw`를
  받는다. direction은 `walletToAccount · accountToWallet`, 금액은 양의 원 단위 정수다.
- `GET /api/finance/ledger?before=&limit=` — 현재 런의 거래를 최신순으로 읽는다.

이체는 열린 본인 계좌만 대상으로 지갑과 계좌 현금을 반대 방향 posting으로 같은 금액만큼 움직이고
state revision을 한 번 증가시킨다. 성공한 command receipt·원장·두 잔액은 한 트랜잭션이다. M2-A의
계좌 생성 API는 아직 기본계좌 외 타입을 만들지 않으며, CMA와 절세계좌를 여는 검증은 각 후속 단계에서
활성화한다. 일일 전진은 due settlement가 없어도 정산 조회·커서 커밋 경계를 통과하고, 테스트용으로 넣은
정상·실패 정산은 하루 전체 원자성과 멱등성을 검증한다.

명령 성공 응답은 결과와 갱신된 전체 `GameSnapshot`을 함께 제공한다. 오류는 도메인별 고정 code와 한국어
message를 쓰고, 내부 SQL·정책 JSON·다른 사용자의 식별자는 노출하지 않는다. 모든 리소스 조회와 명령은
인증된 `user_id → save_id` 소유권 조인으로 제한한다.

M2 공통 실패 코드는 `invalidCommand · characterRequired · accountNotFound · accountClosed ·
accountTypeNotAllowed · insufficientWalletCash · insufficientAccountCash · policyNotEligible · limitExceeded ·
settlementConflict · idempotencyConflict · busy`로 고정한다. 세부 상품은 이 집합에 명시적인 코드를
추가하되 같은 상황을 자유 문자열 코드로 만들지 않는다.

`GameSnapshot`에는 계좌별 현금·포지션·상품 계약, 총자산 분해, 누적 원천징수와 다음 예약 정산 요약을
추가한다. 상세 원장은 별도 페이지 API로 제한해 스냅샷이 무한히 커지지 않게 한다.

M2-A 스냅샷의 `finance`는 `policySet { key, basisDate }`, 현재 런의 `accounts`, 최대 20개의
`pendingSettlements` 요약을 가진다. 계좌 항목은 `id · type · status · cashKrw · isDefault`, 포지션은
기존 필드에 `accountId`를 추가한다. 이 단계에서 `GET /finance/accounts`는 같은 policy와 전체 현재 런
계좌를 반환한다. 이체 성공은
`transfer { commandId, accountId, direction, amountKrw, replayed }`와 갱신된 `snapshot`을 반환한다.
원장 페이지는 transaction별 `id · gameDay · description · sourceKind · postings`와 nullable
`nextBefore`를 반환하고 posting은 `accountCode · accountId? · amountKrw`를 가진다.

### 9.2 M2-B 현금상품 계약

M2-B는 다음 경로를 추가한다.

- `GET /api/finance/cash-products` — 게시된 CMA·예금·적금 상품 버전과 가상 기관을 반환한다.
- `POST /api/finance/accounts` — M2-B에서는 `type: cma · productVersionId`만 새로 연다.
- `POST /api/finance/accounts/{id}/close` — M2-B에서는 non-default CMA만 닫을 수 있다. 현금이
  0원이어야 하며, 아직 열려 있으면 다음 일일 이자 예약을 `accountClosed` 사유로 취소한다. 기본
  `taxableBrokerage`와 다른 계좌 유형은 `accountTypeNotAllowed`, CMA 현금이 남아 있으면
  `accountNotEmpty`로 거절한다.
- `POST /api/finance/deposits` — `kind: termDeposit|installmentSavings · productVersionId ·
  settlementAccountId · amountKrw`로 가입한다. `amountKrw`는 예금 원금 또는 적금 회당 금액이다.
- `POST /api/finance/deposits/{id}/close` — 현재 게임일까지 중도해지율로 다시 계산하고 닫는다.
- `GET /api/finance/tax-years/{year}` — 해당 런의 gross 금융소득과 소득세·지방소득세 원천징수를 읽는다.

모든 POST body는 공통 `commandId`와 세 expected cursor를 함께 가진다. 경로 ID와 body 리소스 ID는
canonical decimal string이다. 계좌 개설·종료처럼 돈이 움직이지 않는 성공 명령도 identity와 receipt를
남기고 state revision을 한 번 올리되 ledger ID는 null이다.

M2-B 요청·응답 envelope는 다음으로 고정한다.

- 상품 목록은 `{ products }`이고 각 상품에 기관 요약을 중첩한다.
- CMA 개설 body는 공통 명령 필드와 `type: cma · productVersionId`, 성공은
  `{ account: { commandId, accountId, productVersionId, replayed }, snapshot }`이다.
- 계좌 종료 body는 공통 명령 필드만 가지며 경로 ID를 명령 fingerprint에 포함한다. 성공은
  `{ accountClose: { commandId, accountId, replayed }, snapshot }`이다.
- 예금·적금 가입 성공은
  `{ deposit: { commandId, contractId, kind, productVersionId, settlementAccountId, amountKrw,
  replayed }, snapshot }`이다.
- 중도해지 body는 공통 명령 필드만 가지며 경로 ID를 fingerprint에 포함한다. 성공은
  `{ depositClose: { commandId, contractId, grossInterestKrw, incomeTaxKrw,
  localIncomeTaxKrw, netPayoutKrw, replayed }, snapshot }`이다.
- 세금 연도에 아직 지급된 금융소득이 없으면 404 대신 요청 연도와 0 누계를 반환한다.

상품 목록 항목은 `id · key · kind · displayName · institution { id, key, displayName } ·
protectionEligible · rateReference · spreadBp · minimumInterestBalanceKrw? ·
minimum/maximumContributionKrw? · termDays/termMonths? · installmentCount? ·
earlyTerminationRateBp? · dayCountDenominator`를 가진다. `minimumInterestBalanceKrw`는 CMA의 일 이자
계산 하한이고, contribution 범위는 예금 원금 또는 적금 회당 금액에 적용한다. 서로 다른 의미의 값을 같은
필드로 겸용하지 않는다. 서버는 카탈로그 kind와 요청 kind가 다르거나 게시되지 않은 ID면 mutation 전에
거절한다.

현재 스냅샷의 `finance`에는 기존 계좌·예정 정산에 다음 bounded 요약을 더한다.

- `cmaAccounts` —
  `accountId · productVersionId · annualRateBp? · minimumInterestBalanceKrw · interestRemainder`.
  오늘의 시장 금리 팩터가 없는 보존 월드에서는 annual rate가 null이다. 새 CMA 개설은 현재
  `treasury3mBp`가 없거나 spread를 더한 값이 음수이면 `rateUnavailable`로 거절하므로, null은 이미 저장된
  보존·이관 상태를 안전하게 표현하기 위한 값이다.
- `cashContracts` —
  `contractId · productVersionId · settlementAccountId · kind · status · annualRateBp ·
  currentPrincipalKrw · installmentAmountKrw? · paidInstallmentCount · missedInstallmentCount ·
  openedGameDay · maturityGameDay · expectedGrossInterestKrw? · expectedIncomeTaxKrw? ·
  expectedLocalIncomeTaxKrw? · expectedNetPayoutKrw?`. 현재 원금과 예상값은 active 계약 기준이며, 종료된
  계약은 `currentPrincipalKrw: 0`과 nullable 예상값을 반환한다.
- `depositProtection` —
  `institutionId · eligibleAmountKrw · protectedAmountKrw · unprotectedAmountKrw`. 기관 이름은 같은 ID의
  상품 카탈로그에서 읽는다.
- `currentTaxYear` —
  `taxYear · grossFinancialIncomeKrw · withheldIncomeTaxKrw · withheldLocalIncomeTaxKrw`.

`currentTaxYear`의 연도는 현재 시장일의 달력 연도다. 게임일 숫자를 연도로 해석하지 않는다.

배열은 현재 런만 읽고 계좌 32개, cash contract 100개, 기관 요약 16개를 상한으로 둔다. 더 긴 감사 이력은
후속 cursor 조회로 분리한다. 예금·적금 가입 결과는 생성된 contract와 snapshot, 중도해지는
`grossInterestKrw · incomeTaxKrw · localIncomeTaxKrw · netPayoutKrw`와 snapshot을 반환한다.

M2-B 추가 실패 코드는 `productNotFound · contractNotFound · contractClosed · accountNotEmpty ·
rateUnavailable`이다.
금액·기간·상품 shape 오류는 `invalidCommand`, 계좌·계약 수 상한은 `limitExceeded`, 허용되지 않은 정산
계좌는 `accountTypeNotAllowed`, 현재 cursor 경쟁은 `busy`를 재사용한다.

### 9.3 M2-C 절세계좌 계약

M2-C는 일반 투자계좌와 CMA의 기존 이체를 유지하면서 ISA·연금저축·IRP의 개설, 납입, 제한된 인출을
연다. v1 게임 범위에서는 한 현재 런에 열린 ISA는 두 유형 합계 하나, 연금저축 하나, IRP 하나까지만
허용한다. 여러 금융회사에 같은 종류의 연금계좌를 나누는 현실의 선택은 콘텐츠가 아니라 합산 한도와
화면 복잡도만 늘리므로 후속 범위다. 닫힌 계좌와 모든 세금 이동 이력은 삭제하지 않는다.

현재 나이는 캐릭터 시작 나이에 시장 월드 시작일부터 현재 시장일까지 지난 달력상 생일 횟수를 더한다.
2월 29일처럼 그 해에 같은 날짜가 없으면 해당 월 마지막 날을 기념일로 쓴다. ISA 3년과 연금 가입 5년도
`365 × 연수`가 아니라 같은 달력 기념일 규칙으로 game day를 구한다.

가입 판정 입력은 `(save_id, run_revision)`의 불변 `run_tax_profile`에 고정한다. 기존 런과 M3 이전 새 런은
`source: m2Default`, 직전년도 근로소득·총급여·종합소득 0원, 직전 3개 과세기간 금융소득종합과세 이력
false로 기록한다. M3부터 새 런은 `source: taxYearRecords`로 확정 연도 기록을 복사하며 필요한 기록이
없으면 ISA 가입을 `policyNotEligible`로 거절한다. 명령 때 클라이언트가 소득이나 이력을 보내지는 않는다.

`POST /api/finance/accounts` body는 기존 CMA variant 외에 공통 command/cursor와 다음 variant를 받는다.

- `{ type: isaGeneral|isaLowIncome }`
- `{ type: pensionSavings|irp }`

성공 envelope는 `{ account: { commandId, accountId, type, replayed }, snapshot }`이다. CMA variant만 기존처럼
`productVersionId`를 추가로 가진다. `taxableBrokerage`와 `krxGold`는 이 경로에서 만들지 않는다. ISA는
나이·소득·종합과세 이력, 서민형은 소득 기준까지 pinned policy로 판정한다. 같은 종류의 열린 계좌가 이미
있으면 `accountAlreadyExists`, 자격 미달은 `policyNotEligible`이다.

기존 `POST /api/finance/transfers`는 계좌 유형에 따라 다음 원자적 부수 상태를 함께 갱신한다.

- `taxableBrokerage · cma` — 기존처럼 지갑과 계좌 현금만 이동한다.
- ISA `walletToAccount` — 남은 이월 납입한도 안에서만 허용하고 누적 납입액을 증가시킨다.
- ISA `accountToWallet` — 계좌 현금과 `누적납입액 - 누적원금인출액` 이하만 허용한다. 누적 원금인출액을
  증가시키며 납입한도는 복원하지 않는다. 이 경로로 수익을 인출할 수 없고 수익은 전체 해지에서 정산한다.
- 연금저축·IRP `walletToAccount` — 현금과 해당 연도 납입액, 세원층을 함께 증가시킨다. M3 연말정산에서
  실제 공제를 확정하기 전까지 전액 `taxExcludedContribution`에 두고, 예상 공제액만 표시한다.
- 연금저축·IRP `accountToWallet` — 우회 인출을 막기 위해 `accountTypeNotAllowed`로 거절하고 아래 전용
  인출 경로만 사용한다.

M2-C의 예금·적금 허용표는 `taxableBrokerage · isaGeneral · isaLowIncome · pensionSavings · irp`이며 CMA와
KRX 금 계좌에는 가입할 수 없다. 일반계좌 상품은 기존처럼 이자 지급 때 원천징수와 금융소득 연도 누계를
갱신한다. ISA 상품은 원천징수하지 않고 gross 이자를 `isa_tax_profit_krw`에 더하며, 연금저축·IRP 상품은
원천징수하지 않고 gross 이자를 `earnings` 세원층에 더한다. 원금 이동은 어느 계좌에서도 소득이 아니다.
만기·중도해지·적금 회차 정산은 부모 계좌 유형을 같은 transaction에서 잠가 이 분기를 적용하고, 절세계좌의
예상 지급 세액은 0원으로 표시한다. 예금자보호 집계는 세제 유형과 무관하게 같은 가상 금융기관의 보호 대상
원금과 소정 이자를 합산한다.

연금 예상 공제대상액은 명령 순서와 무관하게 매번 연도 합계를 다시 계산한다. 연금저축은 먼저 최대
6,000,000원, 그 다음 IRP를 포함한 합계 최대 9,000,000원까지 배분한다. 해당 프로필의 16.5% 또는 13.2%를
곱한 금액은 `expectedCreditKrw`일 뿐 지갑에 지급하지 않는다. M3 연말정산이 실제 산출세액 한도 안에서
확정하면 그때만 공제받은 금액에 대응하는 원금을 `taxExcludedContribution`에서
`creditedContribution`으로 한 원장 transaction과 함께 재분류한다.
한 계좌 납입이 다른 연금계좌의 공제대상 배분을 바꿀 수 있으므로 `pensionContribution` event에는 명령
대상만이 아니라 그 연도의 연금저축·IRP **전체 사후 배분**을 account ID 순서의 배열로 저장한다. 이 배열은
각 계좌의 누적 납입액·공제대상액·예상 공제율·예상 공제액을 모두 포함해 append-only event만으로 mutable
연도 요약을 복원할 수 있어야 한다.

`POST /api/finance/isa/{id}/close`는 공통 command/cursor만 받는다. 열린 포지션이나 active 상품 계약이
남아 있으면 `accountNotEmpty`이며 먼저 현금화해야 한다. 정상 해지와 3년 전 일반 해지의 세금은 §7.2대로
계산하고 계좌 현금에서 세금을 뺀 전액을 지갑으로 옮긴 뒤 계좌를 `closed`로 만든다. 성공 envelope는
`{ isaClose: { commandId, accountId, grossTaxProfitKrw, deductibleLossKrw, incomeTaxKrw,
localIncomeTaxKrw, netPayoutKrw, replayed }, snapshot }`이다. M4가 사망·해외이주 상태를 제공하기 전에는
부득이 해지 사유를 요청으로 받지 않는다.

`POST /api/finance/pensions/{id}/start`는 공통 command/cursor와
`paymentYears: 5..=100 · lifetime: boolean`을 받는다. 만 55세, 가입 5년을 검증하고 IRP는 지급기간
5년 이상도 별도로 확인한다. M2에는 이연퇴직소득이 없으므로 5년 예외는 발생하지 않는다. 성공 envelope는
`{ pensionStart: { commandId, accountId, startTaxYear, paymentYears, lifetime, replayed }, snapshot }`이다.

`POST /api/finance/pensions/{id}/withdrawals`는 공통 command/cursor와 양의 `amountKrw`,
`type: pension|nonPension|unavoidable`, nullable `reason`을 받는다. `amountKrw`는 계좌에서 줄어드는 gross이고
지갑에는 세후 net이 들어간다. `pension`은 개시 후에만 가능하며 남은 연간 한도와 초과분을 §7.3대로 자동
분할한다. `unavoidable`은 고정 법정 사유가 저장된 플레이어 상태로 입증될 때만 연금세율을 적용한다.
`nonPension`은 연금저축에서 허용하되 IRP는 고정 중도인출 사유가 상태로 입증될 때만 허용한다. M4 이전
기본 프로필은 어느 사유도 충족하지 않으므로 IRP 중도인출은 `policyNotEligible`이다. 성공 envelope는
`{ pensionWithdrawal: { commandId, accountId, grossAmountKrw, pensionAmountKrw,
nonPensionAmountKrw, taxFreeAmountKrw, taxKrw, netPayoutKrw, replayed }, snapshot }`이다.

세원층은 `taxExcludedContribution → deferredRetirementIncome → creditedContribution → earnings` 순서로
소진한다. 한 인출에서 연금·연금외 부분이 갈리면 이 순서를 전체 gross에 한 번만 적용하고 각 부분의 층별
금액을 영수증용 typed event에 보존한다. `pensionWithdrawal` event는 자동 분할 결과만이 아니라 원 요청의
`requestKind`와 nullable `reason`도 함께 보존해, 남은 한도가 0인 일반 연금 요청과 명시적 연금외 요청을
감사 이력에서 구분한다. M2에는 이연퇴직소득 유입이 없지만 0원 층과 계산 분기를 제거하지 않는다. 모든
납입·인출·개시·해지는 command identity/receipt, 원장, mutable summary, append-only typed tax-account
event를 같은 transaction에서 기록한다.

스키마는 `run_tax_profile`, `isa_account_contract`, `pension_account_contract`, 현재 4개 층을 가진
`pension_tax_balance`, 연도별 `pension_contribution_year · pension_withdrawal_year`, append-only
`tax_account_event`로 나눈다. 각 행은 save/run/account 복합 FK를 사용한다. 현재 잔액·누계는 빠른 조회
권위이고 event와 원장은 감사 권위다. 새 런은 열린 절세계좌 계약을 `cancelled`로 전이하고 기존 account
종료 뒤 새 프로필을 만든다.

`finance` snapshot에는 bounded `isaAccounts`와 `pensionAccounts`를 추가한다. ISA 항목은
`accountId · type · openedGameDay · minimumTermGameDay · totalContributionKrw ·
principalWithdrawalKrw · contributionCapacityKrw · taxProfitKrw · deductibleLossKrw ·
expectedCloseIncomeTaxKrw · expectedCloseLocalIncomeTaxKrw`를 가진다. 연금 항목은
`accountId · type · openedGameDay · eligiblePensionStartGameDay · pensionStarted ·
taxLayers { taxExcludedContributionKrw, deferredRetirementIncomeKrw, creditedContributionKrw,
earningsKrw } · currentYearContributionKrw · currentYearCreditEligibleKrw · expectedCreditKrw ·
currentYearPensionLimitKrw? · currentYearPensionWithdrawnKrw · riskAssetValueKrw · totalValueKrw ·
riskAssetRatioPpm`을 가진다. ISA는 최대 1개, 연금 항목은 최대 2개다.

M2-C 추가 실패 코드는 `accountAlreadyExists` 하나만 늘린다. 현금 부족, 한도, 자격, 빈 계좌 요구,
허용되지 않은 계좌/인출, 현재 cursor 경쟁은 각각 기존 `insufficientAccountCash · limitExceeded ·
policyNotEligible · accountNotEmpty · accountTypeNotAllowed · busy`를 재사용한다.

M2-C 체크포인트에서는 절세계좌의 LLX 주문을 `accountTypeNotAllowed`로 막는다. 계좌만 먼저 열어 기존
M1 주문을 통과시키면 ISA 손익 bucket, 연금 earnings 층, IRP 70%가 갱신되지 않는 잘못된 상태가 생기기
때문이다. M2-D가 세 분기를 주문 transaction에 함께 연결한 뒤 ISA·연금저축·IRP의 LLX 주문을 동시에
연다.

### 9.4 M2-D 자산·시가손익·연간 세금 계약

M2-D 모든 request/response는 strict object다. 알 수 없는 필드, 부분만 채운 tagged variant,
부동소수점·지수표기·음수 금액을 `invalidCommand`로 거절한다. 모든 DB 리소스 ID는 canonical
양의 10진 문자열, `commandId`와 기존 LLX `orderId`는 canonical UUID, 날짜는 `YYYY-MM-DD`다.
수량·bar 개수는 양의 정수이고 checked DB 범위를 넘으면 저장 전에 거절한다. 모든 POST는 §9의
공통 command/cursor를 포함하고 경로 ID와 본문 전체를 fingerprint에 넣는다. 단, 기존
`POST /api/portfolio/orders`는 `commandId` 대신 같은 의미의 `orderId`만 유지하고 둘을 중복해 받지 않는다.

#### 국채 조회·주문

`GET /api/finance/bonds`는 `{ marketVersion, products, series }`를 반환한다. v4 런은 게시된
product 최대 2개와 만기 전 series 최대 160개를 보여 준다. v1~v3 런은 과거 월드에 상품을
소급하지 않으므로 월드 버전과 빈 두 배열을 반환한다.

- product는 `id · key · displayName · termYears · faceValueKrw · maxOrderUnits ·
  maxPositionUnits · buyFeePpm · sellFeePpm`을 가진다.
- series는 `id · productVersionId · issuedDate · maturityDate · couponRateBp · issueYieldBp ·
  nextCouponDate · dirtyPriceKrw · currentYieldBp`를 가진다. 만기 시리즈는 이 조회에 포함하지
  않고 체결·원장 감사 이력에만 남는다.

`POST /api/finance/bonds/orders` body는 공통 command/cursor와
`accountId · seriesId · side: buy|sell · bondUnits: 1..=100000`을 받는다. 성공 envelope는
`{ bondOrder, snapshot }`이고 `bondOrder`는
`commandId · executionId · accountId · seriesId · side · bondUnits · dirtyPriceKrw ·
grossAmountKrw · feeKrw · taxKrw · removedCostBasisKrw · realizedGainLossKrw · replayed`를 가진다.
매수는 두 원가·손익 필드가 0원이고 매도는 FIFO 결과를 반환한다. `grossAmountKrw`는
dirty price×수량이며 매수 현금 유출은 `gross + fee + tax`, 매도 현금 유입은
`gross - fee - tax`다.

#### 금 계좌·조회·주문·실물 인출

`POST /api/finance/accounts`는 공통 command/cursor와
`{ type: krxGold, productVersionId }` variant를 추가한다. 게시된 v4 번들의 금 상품만 받고
성공 envelope는 `{ account: { commandId, accountId, type, productVersionId, replayed }, snapshot }`이다.
현재 런에 열린 금 계좌가 이미 있으면 `accountAlreadyExists`다.

`GET /api/finance/gold-products`는 `{ marketVersion, products }`를 반환한다. v4에서 products는 최대
1개이고 각 항목은 `id · key · displayName · unit: gram · buyFeePpm · sellFeePpm · buyTaxPpm ·
sellTaxPpm · withdrawalBars`를 가진다. `withdrawalBars`는
`{ barSizeGram: 100|1000, feeKrw }` 두 항목이다. v1~v3 런은 빈 products를 반환한다.

`POST /api/finance/gold/orders` body는 공통 command/cursor와
`accountId · side: buy|sell · quantityGram`을 받는다. 성공 envelope는 `{ goldOrder, snapshot }`이고
`goldOrder`는 `commandId · executionId · accountId · side · quantityGram · priceKrwPerGram ·
grossAmountKrw · feeKrw · taxKrw · removedCostBasisKrw · realizedGainLossKrw · replayed`를 가진다.
매수는 제거 원가·실현손익이 0원이고 매도는 이동평균 제거 결과를 반환한다.

`POST /api/finance/gold/withdrawals` body는 공통 command/cursor와
`accountId · barSizeGram: 100|1000 · barCount`를 받는다. 성공 envelope는
`{ goldWithdrawal, snapshot }`이고 `goldWithdrawal`는
`commandId · withdrawalId · accountId · barSizeGram · barCount · quantityGram · removedCostBasisKrw ·
vatKrw · feeKrw · cashChargedKrw · replayed`를 가진다. `cashChargedKrw = vatKrw + feeKrw`여야 한다.

#### 기존 LLX 주문과 세금연도 확장

`POST /api/portfolio/orders`는 M1의 request 경로·`orderId`·`execution`·`snapshot` envelope를 유지한다.
`execution`에 `removedCostBasisKrw · realizedGainLossKrw`를 추가하고 `feeKrw · taxKrw`를
0 literal이 아닌 음이 아닌 원 금액으로 확장한다. 매수는 제거 원가·실현손익 0원이며,
매도는 `grossAmountKrw - removedCostBasisKrw - feeKrw - taxKrw`를 실현손익으로 반환한다.
v4 런은 `llxCloseKrw`, v1~v3 런은 기존 benchmark `closeKrw`로 체결한다. 기존 체결의
fee·tax·상품 손익을 소급 재계산하지 않는다.
`GET /api/markets/LLX/history`의 기존 `closeKrw · dailyReturnPpm`은 benchmark로 그대로 둔다. 각 point에
`llxCloseKrw · llxDailyReturnPpm`을 nullable로 추가해 v4에서는 보수 반영 거래 가격·수익률, v1~v3에서는
null을 반환한다. 거래 가격 차트는 v4에서 LLX 필드, 기존 월드에서 benchmark 필드를 쓴다.

`GET /api/finance/tax-years/{year}`는 다음 base 필드를 가진 status tagged union을 직접 반환한다.

- base: `taxYear · status · sources · grossFinancialIncomeKrw · withheldIncomeTaxKrw ·
  withheldLocalIncomeTaxKrw`
- `sources`는 최대 5개의
  `{ source, grossFinancialIncomeKrw, withheldIncomeTaxKrw, withheldLocalIncomeTaxKrw }`이며 source는
  §8.6의 고정 enum이다.
- `status: notApplicable|open`은
  `comparisonAIncomeTaxKrw · comparisonALocalIncomeTaxKrw · comparisonBIncomeTaxKrw ·
  comparisonBLocalIncomeTaxKrw · assessedIncomeTaxKrw · assessedLocalIncomeTaxKrw ·
  additionalTaxKrw · refundKrw · filingDueDate · filedGameDay`가 모두 null이다.
  `notApplicable`은 v1~v3 런의 M2-D 비소급 연도를 위해 DB 행 없이 합성하는 API 상태이고,
  `open`은 DB assessment의 아직 마감하지 않은 v4 연도다.
- `status: finalizedNoFiling`은 위 비교·확정·세액 필드가 non-null, `additionalTaxKrw · refundKrw`는
  0, `filingDueDate · filedGameDay`는 null이다.
- `status: filingPending`은 계산·세액 필드와 `filingDueDate`가 non-null, `filedGameDay`만 null이다.
- `status: filed`는 모든 계산·세액·신고 필드가 non-null이다.

해당 연도 source 행이 없어도 404가 아니라 0 누계와 런·연도에 맞는 status를 반환한다.
`additionalTaxKrw`와 `refundKrw`는 총 소득세+지방소득세 차액이며 서로 배타적이다.

#### bounded snapshot

`GameSnapshot.market`의 기존 benchmark 필드는 유지하고
`m2Factors: null | { cpiIndex, llxCloseKrw, goldCloseKrwPerGram }`를 추가한다. v4에서는 객체 전체가
필수이고 v1~v3에서는 null이다. 클라이언트는 null을 0원·0 CPI로 표시하지 않는다.

`GameSnapshot.finance`는 기존 필드에 다음 요약만 더한다.

- `productBundle: null | { indexProduct, bondProductVersionIds, goldProductVersionId }` —
  `indexProduct`는
  `{ id, key, displayName, annualManagementFeePpm, annualDistributionRatePpm, dayCountDenominator,
  buyFeePpm, sellFeePpm, sellTaxPpm }`, bond ID는 3년·10년 순서의 정확히 2개, v1~v3에서
  전체 null
- `llxDistributionEntitlements` 최대 8개 — pending만
  `id · accountId · recordDate · paymentDate · quantity · grossAmountKrw · status: pending`
- `bondPositions` 최대 640개 —
  `accountId · seriesId · bondUnits · totalCostBasisKrw · dirtyPriceKrw · marketValueKrw ·
  unrealizedGainLossKrw`
- `goldAccounts` 최대 1개 —
  `accountId · productVersionId · quantityGram · totalCostBasisKrw · averageCostKrwPerGram(nullable) ·
  closeKrwPerGram · marketValueKrw · unrealizedGainLossKrw`
- `physicalGoldHoldings` 최대 2개 —
  `barSizeGram · barCount · totalQuantityGram · closeKrwPerGram · marketValueKrw`
- `latestFinancialIncomeAssessment` — 없으면 null, 있으면 가장 최근
  `finalizedNoFiling|filingPending|filed` 연도의
  tax-year response에서 `sources`를 뺀 정확히 한 객체

기존 `currentTaxYear`는 위 tax-year tagged union 형태로 확장하고, `pensionAccounts`의
`totalValueKrw · riskAssetValueKrw · riskAssetRatioPpm · taxLayers`는 최신 시가손익 적용 후 값이어야 한다.
순자산은 지갑, 모든 계좌 현금, 현금상품 원금, LLX·국채·계좌 금·실물 금 시가를
모두 더하고 aggregate debt를 뺄 값과 일치해야 한다. 만기 시리즈, paid 권리, 체결·가치 event,
과거 assessment는 snapshot에 쌓지 않고 별도 bounded/cursor 조회로 둔다.

M2-D가 공통으로 추가하는 실패 코드는 `marketClosed · insufficientQuantity · positionLimit`이다.
M1 LLX의 같은 코드는 이 공통 의미로 통합한다. 잘못된 shape·오버플로는 `invalidCommand`, 번들에
없는 상품·시리즈는 `productNotFound`, v4 factor·수익률이 없는 런은 `rateUnavailable`, 잔액 부족은
`insufficientAccountCash`, 잘못된 계좌는 `accountTypeNotAllowed`를 재사용한다. 금·국채 매도와 금 인출의
보유 부족은 `insufficientQuantity`, 국채 보유 상한은 `positionLimit`이다. 세금 신고 현금 부족은 부채로
정산하므로 실패 코드가 아니다. `invalidCommand`는 새 finance 경로에 적용하고, 기존 LLX 경로는
하위 호환을 위해 잘못된 주문 shape에 `invalidOrder`를 계속 쓴다. v1~v3의 기존 LLX 주문은 v4 factor를
필요로 하지 않으므로 `rateUnavailable`로 막지 않는다. 소유권·정책 JSON·SQL 내부 정보는 어떤 실패에도
노출하지 않는다.

## 10. 기능 중심 화면

M2 화면은 기존 대시보드에 다음 기능만 추가하며 사용자 정의 CSS는 만들지 않는다.

- 계좌 목록과 현금·평가액·세제 유형 텍스트 표
- 계좌 개설 폼, 지갑↔계좌 이체 폼
- CMA 선택, 예금/적금 가입·중도해지 폼과 만기 표시
- LLX·국채·금 주문 계좌 선택, 국채 쿠폰·dirty price, IRP 위험자산 비율 표시
- 금 100g·1kg 실물 인출 폼과 VAT·수수료·실물 보유 요약
- ISA 납입 여력·의무기간·예상 종료세금
- 연금 납입액·공제 대상액·예상 공제액과 인출 제약
- 예정 정산과 최근 원장 표
- 연간 금융소득 gross·원천세·종합과세 추가세액 표

DOM은 mount에서 만들고 신호·hooks로 값과 disabled 상태만 갱신한다. 큰 원장 목록은 서버 cursor pagination과
고정된 행 슬롯을 사용한다. 계산 결과는 모두 서버 응답이며 클라이언트가 세금·이자를 다시 계산하지 않는다.

## 11. 테스트와 실제 MySQL 검증

### 11.1 전역 SQL 잠금 순서

모든 player write와 일일 transaction은 다음 전역 순서를 공유한다.

`save → financial_account(id) → isa_account_contract/pension_account_contract(account_id) →
pension_tax_balance(account_id) → pension_contribution_year/pension_withdrawal_year(account_id, tax_year) →
asset_position(account_id, symbol) → bond_position(account_id, series_id) → bond_lot(id) →
gold_position(account_id) → cash_product_contract(id) →
savings_installment(contract_id, installment_no) → llx_distribution_entitlement(id) →
scheduled_settlement(due_game_day, id) → financial_income_year(tax_year) →
financial_income_source_year(tax_year, source) → financial_income_assessment(tax_year)`

각 단계 안에서는 표시한 키 오름차순으로 `FOR UPDATE`하고 실제 경로에 없는 단계만 건너뛴다.
자식 계약·포지션을 부모 계좌보다 먼저 잡지 않고, 복수 계좌·포지션은 ID를 먼저 수집해 정렬한다.
`command_identity`는 save를 잡은 직후 fingerprint를 검사·생성하며 unique race는 같은 멱등 결과로 수렴한다.
게시된 policy·상품 버전·월드 일봉·만료 체결은 불변이므로 이 잠금 순서에 가변 행인 것처럼 끼워 넣지
않는다. `gold_account_contract`도 계좌-상품 바인딩을 생성한 뒤 불변이고 열림·닫힘 권위는
`financial_account`에 있으므로 별도 가변 잠금 단계를 만들지 않는다.

메모리의 사용자별 operation 잠금은 한 프로세스의 UX 직렬화일 뿐이다. 여러 서버 인스턴스의
정합성은 이 SQL 순서, 네 부분 커서, unique/conditional transition이 보장한다. deadlock을 잠금 순서 위반의
대체물로 재시도하지 않고, DB가 감지한 동시 경쟁은 제한 횟수 내에서 전체 transaction을 재시도한다.

### 11.2 M2-D 일일 planner 순서

시장 캐시와 매월 첫 개장일 국채 series는 기존 월드 캐시와 같은 불변·멱등 준비 경계에서
player transaction 전에 보장할 수 있다. player 하루는 다음 순서의 한 MySQL transaction이다.

1. save의 `(world, runRevision, stateRevision, gameDay)`, pinned policy set, v4 product bundle을
   잠그고 expected cursor를 검증한다.
2. 현재 날짜까지의 due 후보를 읽어 모든 payload를 strict tagged union으로 해석한다. 알 수 없는
   kind·version·필드가 하나라도 있으면 잠금·쓰기 전에 하루를 실패시킨다.
3. payload에서 계좌·계약·포지션·권리·세금연도 ID를 수집해 §11.1 순서로 모두 잠그고,
   잠금 전 후의 due 집합·payload·상태가 같은지 다시 확인한다.
4. 1월 1일이면 당일 가치·수익·정산 전 직전 마감가로 연금 opening value를 고정하고
   직전 금융소득 연도를 확정한다. `F <= 20,000,000원`은 `finalizedNoFiling`으로 끝내고,
   `F > 20,000,000원`은 `filingPending`으로 전이하며 같은 계획에 5월 31일 정산을 예약한다.
5. 오늘의 LLX·국채 가격으로 포지션을 평가하고 연금 시가손익·IRP 위험비율을 갱신한다.
   쿠폰·만기 일의 채권 가격은 당일 due cash flow를 제외한다.
6. 3·6·9·12월 마지막 개장일이면 평가 후 LLX 수량으로 분배금 entitlement을 만들고
   T+2 개장일 정산을 예약한다.
7. 기존 `cmaInterest · depositMaturity · savingsInstallment · savingsMaturity`와
   M2-D `llxDistribution · bondCoupon · bondMaturity · financialIncomeFiling`을 kind별로
   따로 commit하지 않고 **전역 `(due_game_day, settlement_id)` 순서**의 하나의 shadow plan으로 계산한다.
   앞 정산의 가상 잔액·수량·세원층·세금 누계를 뒤 정산 입력으로 전달한다.
8. shadow plan과 일치하는 잔액·포지션·세원층·권리·세금·원장·event를 기록하고 각 pending
   settlement를 조건부로 한 번만 전이한다. game day·state revision을 각 1 올려 한 번 commit한 뒤
   완성된 snapshot 하나만 SSE로 보낸다.

요소 하나의 계산·규칙 조회·posting 합·조건부 전이가 실패하면 opening pin과 첫 정산까지 모두
롤백한다. settlement별 savepoint·commit은 두지 않는다. 0원 쿠폰·분배금·세금 신고는 명시적
`noMovement` 사유로 settled하되 원장을 만들지 않는다.

### 11.3 단위·protocol 테스트

단위 테스트는 순수 규칙, protocol parser, 순수한 서비스 오케스트레이션에만 둔다.

- v1·v2·v3 주식·금리 고정 벡터가 byte-for-byte 같고 M2-D 필드는 null인지, v4를 재시작·호출 순서가
  달라진 후에도 같은 고정 벡터로 생성하는지 검증한다.
- CPI Actual/365 잔여분, 금의 11,000ppm 독립 혁신·10년금리 민감도·휴장 carry, LLX 0.15%
  보수·2% 분배율·분기 마지막 개장일·T+2·pending ISA 폐쇄를 경계 날짜로 검증한다.
- 3년·10년 쿠폰 25bp 반올림, 홀수 반기 쿠폰 잔여, 달력 6개월 말일 보정, 휴장일 지급,
  dirty-price 수식, 금리 상승 시 3년물보다 10년물 가격이 더 큰 폭으로 내리는지, FIFO lot을 검증한다.
- 금 이동평균 제거원가, 전량 잔여 제거, 100g·1kg bar fee, VAT 내림, 현금·수량 부족 원자성,
  실물 금 순자산 포함과 재매도 금지를 검증한다.
- 연금 양의 시가손익→earnings, 손실 waterfall의 네 층 경계 바로 아래·같음·위, 쿠폰·분배금 이중
  계산 금지, 1월 1일 opening pin이 당일 정산보다 먼저인지 검증한다.
- 금융소득 `19,999,999 · 20,000,000 · 20,000,001원`, 구간별 내림, 비교산식 A·B, 1월 1일 확정,
  `open → finalizedNoFiling` 및 `open → filingPending → filed` 전이, 5월 31일 환급·현금 납부·
  무이자 부채 부족분·0원 `noMovement`를 검증한다.
- 이자 잔여분·만기/중도해지·ISA 손익통산·연금 세액공제·IRP 70%, 음수·i128·DB 범위 오버플로,
  command replay/fingerprint 충돌, cursor 경쟁, 일일 shadow plan 중간 실패 전체 롤백을 회귀 검증한다.
- strict API schema는 canonical ID·UUID, tagged tax-year status, 배열 상한, 교차 필드·알 수 없는 필드
  거절, 도메인 실패 코드가 한국어 message와 독립적인지를 검증한다.

DOM·라우팅·실제 네트워크 왕복·snapshot 테스트는 만들지 않는다.

### 11.4 실제 MySQL 8 스모크

PII가 없는 격리 DB에서 다음을 검증한다.

- 빈 DB의 `0001`부터 M2-D `0014+`까지와 M2-C 스키마·데이터가 있는 DB의 forward migration
- v1~v3 월드·런·고정 일봉·M1 체결 보존, M2-D nullable 필드, v4 bundle·v2 policy의 new-run 고정
- 게시 후 policy·상품·번들·series·권리·체결·event update/delete 트리거와 복합 FK 소유권 거절
- LLX 기준일·T+2, 휴장일 채권 쿠폰·만기, 금 실물 인출, 1월 확정·5월 신고의 원장·잔액·
  세원층·source-year·assessment 재대조
- 동일 계좌의 동시 주문·이체·정산, 중복 command, due 집합 변경, 두 번째 정산 강제 실패의 전체 롤백,
  재시도 후 하나의 receipt·체결·원장·지급에 수렴하는지
- 인증된 `user_id → save_id → run_revision → account_id`를 모든 조회·명령에서 다시 조인해 다른 사용자·
  이전 런 ID, 내부 SQL·policy JSON이 노출되지 않는지

운영 DB를 쓸 때는 먼저 백업하고 PII 없는 격리 복제 DB에서 전체 마일스톤·롤백 이미지를 검증한다.
그 전에는 운영 DB에 마이그레이션을 적용하지 않는다.

## 12. M2 완료 조건

1. M1 포지션과 현금이 손실 없이 기본 일반계좌로 이어지고 v1~v3 시장 경로가 바뀌지 않는다.
2. 하루 진행 중 여러 정산이 모두 커밋되거나 모두 롤백되며 재시도해도 원장과 지급이 중복되지 않는다.
3. CMA·예금·적금의 만기와 중도해지 세후 금액이 정수 규칙 테스트와 일치한다.
4. 같은 LLX 수익 경로에서 일반계좌·ISA·연금저축·IRP의 세후 결과와 사용 가능 시점이 달라진다.
5. ISA 납입/비과세/통산, 연금 공제 한도, IRP 위험자산 70%, 금융소득 2천만원 경계를 검증한다.
6. 3년·10년 국채의 금리 상승 시 가격 하락, 쿠폰·만기 정산과 금 실물 인출 VAT를 확인한다.
7. 계좌·이체·가입·매매·해지·원장·세금 조회를 스타일 없는 화면에서 끝까지 조작할 수 있다.
8. 서버 test/clippy/fmt, 클라이언트 test/typecheck/lint/build, 실제 MySQL 8 격리 스모크가 통과한다.
