# M5 확장 상세 스펙

- 작성: 2026-07-26
- 상태: M4 production 완료, **M5-A·M5-B production 완료, M5-C 시즌·리그 다음**
- 상위 계획: [`development-plan.md` §2, §3, §4.2, §9, §11, §12](./development-plan.md)
- 선행 마일스톤: M0~M4 전체, 특히 M3 커리어와 M4의 장기 결정론·도산·법인 기반

## 1. 목표와 단계

M5는 완성된 30년 시뮬레이션을 여러 출발선으로 반복하고 공정하게 비교하며, 접속하지 않은 동안에도
명시적으로 허용한 런만 안전하게 진행되도록 만든다. 마지막 목표는 기능 추가 자체가 아니라
**버전을 고정한 외부 플레이테스트를 운영하고 결과를 재현할 수 있는 상태**다.

1. **M5-A 실행 모드·포인트 예산** — ranked preset, ranked custom, sandbox의 서버 계약 분리
2. **M5-B 콘텐츠 배포** — 자격증·공고·업종·복지·사건·프리셋 seed와 immutable bundle
3. **M5-C 시즌·리그·랭킹** — 시즌 seed, 프리셋별 리그, 30년 고정시점 세후순자산 결산
4. **M5-D 오프라인 진행** — 별도 worker, opt-in, DB lease, catch-up cap, 온라인 진행과 경쟁 제어
5. **M5-E 법인 상세** — 고객계약·인력·운전자금·결산까지의 제한된 경영 수직 슬라이스
6. **M5-F 배포·관측·플레이테스트** — 운영 마이그레이션, 대시보드, runbook, 외부 테스트

M5는 M3·M4의 규칙을 새로 정의하지 않는다. ranked run은 M3 커리어 콘텐츠와 M4 생활·대출·사건·도산을
모두 포함하며, 그중 하나라도 결정론적 버전 pin과 30년 회귀를 제공하지 못하면 해당 시즌을 게시하지 않는다.
시각적 리디자인·스타일링, 모바일 전용 UX, 실시간 다인 상호작용, 거래 가능한 보상, 실제 회사 경영 ERP는
범위 밖이다.

### 1.1 현재 재개 지점 (2026-07-29)

M4는 [`m4-life.md` §13.24](./m4-life.md)의 development production 검증으로 완료했다. M4 인수 기준 server는
`c95edef`, DB는 migration `51/51`이었으며 같은 시작 draft의 1일 step 10,950회와 30일 step 365회가 day
10950에서 정규화 SHA-256
`a1021ed3a8b9e49416a25b1fdfe6b9138a42bfacabe4fffcebfcd993378692ee`로 수렴했다. 장기 연체 30년과
도산 재기 exclusive 경계도 별도 append-only run에서 완료했다. M5는 이 결과를 바꾸지 않고 새 run의
manifest와 실행 모드를 추가한다.

M5-A의 첫 세 구현 단계는 다음과 같이 고정한다.

1. **완료** — 기존 character start·save·`run_rule_bundle` 경계를 조사한 뒤 `run_manifest`, versioned preset·point
   budget catalog와 기존 run의 명시적 unranked backfill을 migration으로 추가한다. sealed version은
   update/delete하지 않는다.
2. **완료** — option ID를 canonical 정렬해 fixed/perUnit/tiered와 exclusive/requires/forbids를 i64 checked arithmetic로
   평가하는 순수 point ledger를 만들고 `GET /api/run-options`, `POST /api/runs/point-preview`를 strict API로
   공개한다. preview 합계는 시작 transaction에서 다시 계산한다.
3. **완료** — `rankedPreset · rankedCustom · sandbox`의 필수·금지 필드, immutable manifest hash, command
   replay를 `POST /api/runs`와 스타일 없는 `/new` 화면에 연결해 server/client production에서 인수했다.
   ranked season·league의 실제 게시와 랭킹 계산은 M5-C가 소유하므로 현재 두 ranked start는
   `modeUnavailable`로 닫고 catalog 확인과 point preview만 허용한다.

2026-07-29 현재 세 단계의 server/client 구현·development production 인수를 마쳐 **M5-A를 완료**했다.

- `0052_m5a_run_modes_point_budget.sql`은 sealed preset 5개, point budget 1개, exclusive group 5개,
  option 12개, tier 3개, 활성 assignment 1개와 immutable `run_manifest`를 추가했다. 기존
  `run_rule_bundle` 9건은 모두 `sandbox · rankingEligible=false · legacyRun` manifest 9건으로 백필됐고,
  bundle/manifest authority 불일치는 0건이다.
- `server/src/runs/`의 순수 evaluator는 option ID canonical 순서, fixed/perUnit/tiered 누진 정수 비용,
  exclusive/condition/effect와 checked i64 ledger를 처리한다. `server/src/store/runs.rs`가 sealed catalog만
  읽고, `GET /api/run-options`와 `POST /api/runs/point-preview`가 strict JSON 경계로 이를 공개한다.
- point budget v1 시작 materializer는 preview와 같은 evaluator 결과만 사용한다. engine base는 25세,
  `other · completed · capitalArea · independent · highSchool`, 경력·자격 0, 현금·학자금·신용대출 0,
  `normal`, 부양가족 0으로 고정하고 허용 목록에 없는 effect path/type은 catalog 오류로 닫는다. base 변경은
  engine과 budget schema/version을 함께 올리는 설계 변경이다.
- 기존 `POST /api/characters`는 계속 sandbox 전용 호환 경로이며, 시작 transaction 안에서
  `legacyStartEndpoint` manifest를 함께 기록한다. manifest 없는 새 legacy run을 만들지 않는다.
- 최초 CD `30416923330`은 migration 52의 `tr_point_budget_version_seal_only` 조건 괄호 누락으로 실패했다.
  새 server를 중지한 상태에서 실제 production MySQL parser로 수정 SQL의 생성을 확인하고 확인용 trigger를
  즉시 삭제했다. 실패 실행이 만든 preset/budget의 부분 table·trigger와 `_sqlx_migrations`의 version 52
  실패 row만 역순 제거했으며 migration 51과 기존 bundle 9건은 보존했다. 복구 dump나 별도 DB는 만들지
  않았다.
- `3336425`가 trigger 구문을 복구했고 CD `30417524665`에서 migration `52/52`가 성공했다. 이후 catalog
  effect JSON의 variant 내부 camelCase를 Rust가 snake_case로 읽던 500을 `136a484`에서 바로잡았고 CD
  `30417912895`가 성공했다. 최종 server container는 healthy, startup migration 실패 0, 새 container
  WARN/ERROR 0건, 열린 InnoDB transaction 0건이다.
- public HTTP에서 run options는 세 mode, active season null, preset 5개, budget 1개, group 5개, option
  12개를 반환했다. 기준 선택은 total 100, spent 10, remaining 90, ledger line 5개, failure 0으로 유효했고,
  빈 선택은 필수 group 누락 5개를 반환했다. unknown request field는 `400 invalidCommand`로 거절됐다.
- strict `POST /api/runs`는 공통 cursor·UUID 외에 mode별 필드만 받는다. sandbox는 기존 V2 start 검증과
  transaction을 재사용하되 별도 fingerprint와 `sandboxMode` manifest를 쓰고, ranked preset/custom은 게시된
  season이 생길 때까지 version 존재 여부를 노출하지 않고 `409 modeUnavailable`을 반환한다. 구현 계약은
  `64c3506`, server 코드는 `1e59716`이며 CD `30419059942`가 5분 38초에 성공했다.
- production QA save 29의 run 3/state 4/day 2에서 sandbox를 생성해 `200`, run 4/state 0/day 0,
  manifest SHA-256 `5a18fc381a418dfd51a16b69b680c4e417f1d22b12fa8f7aa182c5a1f6ddf64a`를 받았다. 같은
  command/body 재전송은 같은 hash와 `replayed=true`, 같은 command의 변조 body는
  `409 idempotencyConflict`였다. ranked preset은 `409 modeUnavailable`, ranked 요청에 sandbox 필드를 섞으면
  `400 invalidCommand`였다.
- DB는 migration `52/52`를 유지했다. 새 run의 manifest는 `sandbox · rankingEligible=false · sandboxMode`,
  canonical selection 0개이며 API hash와 generated DB hash가 같았다. `run_rule_bundle/run_manifest`는
  `10/10`, 양방향 누락 0, 해당 `startGame` command identity/receipt 각 1, 열린 InnoDB transaction 0이었다.
  새 container는 healthy이고 smoke 동안 WARN/ERROR/panic/failed 로그가 없었다. 검증용 raw token은 DB에
  저장하지 않았고 임시 session row를 삭제해 QA user의 session 0건을 확인했다.
- 검증은 기능에 필요한 범위로 제한했다. run start protocol/OpenAPI, fingerprint 구분, durable command
  replay/conflict, point materializer의 production 조합과 unknown path를 작은 BDD로 확인하고
  check·clippy·fmt와 실제 production HTTP/DB smoke를 실행했다. DOM/network/DB 단위 테스트와 전체 회귀는
  추가하지 않았다.
- `6eaccdd`는 `client/src/api/contracts.ts`, `client/src/api/game-api.ts`에 run options·point preview·mode별
  start의 strict zod 경계와 response-command 상관 검증을 추가하고, `game-command-retry`에 mode·payload·최초
  cursor·UUID를 보존하는 run start seam을 추가했다. `9494dc0`은 기존 `/new` 화면을 세 mode 선택,
  versioned preset, option 수량·server preview, ranked season 부재 고지, sandbox start로 전환했다. 다른 모드
  필드는 client union에서도 거절하며 unknown 결과만 같은 request로 재시도한다.
- client는 `client-foundation`의 hook·bag·form·HTTP 경계를 사용한다. catalog와 대출 조회는 화면 생명주기에
  묶어 취소하고, server response를 zod로 검증하기 전에는 store에 쓰지 않는다. active season이 null인 동안
  ranked 설정과 point preview는 볼 수 있지만 두 start button은 비활성이고 sandbox만 `/api/runs`를 호출한다.
  DOM interaction/rendering test나 시각 스타일은 추가하지 않았다.
- client typecheck·lint·production build와 계약/API/retry 3개 targeted suite 251건이 통과했다. production
  bundle은 기존 크기 권고만 남은 681,784 bytes이며 local/Vercel SHA-256이
  `c6044641657b8167df184c66f6b0a640056dbadb1c667979c2595feeeb44918e`로 일치했다. canonical `/new`는
  HTTP 200, app script 1개였고 bundle에서 새 화면과 세 API 경로를 확인했다.
- Vercel same-origin `/api/health`는 200, run options는 mode 3·season null·preset 5·budget 1·group 5·option
  12를 반환했다. 기준 point request는 total 100·spent 10·remaining 90·line 5·failure 0이었다. 비로그인
  브라우저의 `/new`는 OAuth 버튼이 있는 login view로 보호됐고 `/api/auth/me`, `/api/state`는 401이었다.
  사용자의 OAuth를 대신 진행하거나 새 session/run을 만들지 않았으므로 server 인수의 DB 10/10 상태는
  그대로 보존했다.

2026-07-29에 **M5-B를 development production까지 완료**했다.

- `0053_m5b_content_bundle.sql`은 immutable `content_bundle` header·member·canonical manifest·`newRun`
  assignment를 추가했다. 기존 typed authority 12개를 8개 kind로 묶은 bundle
  `dev-unranked-m5-content-2026` v1은 ID 1, schema 1, `rankedEligible=false`, sealed이며 canonical SHA-256은
  `49e0f5522b690c762672f6ff4177987696a4b04590305ed19558ea77c11f9398`이다. 실제 계산 payload는 원본 typed
  table에 남고 bundle은 exact ID·key·version·SHA만 고정한다.
- Rust publish validator는 참조 존재, sealed 상태, 중복 kind/ID, canonical 순서·hash 일치를 순수 규칙으로
  검증한다. start transaction은 활성 bundle assignment와 revision을 같은 transaction에서 읽고
  `content_bundle_id`와 SHA를 schema 2 canonical run manifest에 함께 기록한다.
- 첫 CD `30421402763`은 `0052`가 이미 후속 확장을 위해 예약한 nullable `run_manifest.content_bundle_id`를
  `0053`이 다시 추가해 실패했다. 새 server를 중지하고 production DB에서 실패한 `0053`이 만든 M5-B table
  4개와 migration 53 실패 row만 역순 제거했다. 기존 migration·10개 run manifest·사용자 데이터는 건드리지
  않았고 별도 schema나 복구 dump를 만들지 않았다. `f01d852`에서 예약 컬럼을 재사용하도록 수정한 뒤 CD
  `30421802244`가 성공했고 migration 52·53은 모두 성공 상태다.
- 기존 run manifest 10개는 역사 hash를 보존해 `content_bundle_id/content_bundle_sha256=null/null` 그대로다.
  production HTTP에서 QA save 29의 sandbox를 run 5/state 0/day 0으로 만들었고 새 manifest는 bundle 1과 위
  SHA를 고정했다. API와 DB manifest SHA는
  `706ac8026b0b606cfed25248c9fa1244ad48202c2ed279bea3507a1e695c992c`로 일치하며 같은 command/body 재전송은
  같은 cursor·hash와 `replayed=true`였다. `run_rule_bundle/run_manifest`는 11/11, bundle join 불일치 0,
  해당 `startGame` identity/receipt 각 1, 열린 InnoDB transaction 0이었다.
- container는 healthy이고 배포·smoke 구간 WARN/ERROR/panic/failed 로그가 없었다. public same-origin
  `/api/health`도 200이었다. 검증용 raw token은 DB나 문서에 저장하지 않았고 임시 session row를 삭제해 QA
  user의 session 0건을 확인했다. check·clippy·fmt와 publish validator·manifest의 작은 BDD만 실행했고
  DOM/network/DB 단위 테스트나 무관한 전체 회귀는 추가하지 않았다.

다음 재개는 **M5-C 시즌·리그·랭킹의 첫 vertical slice**다. 먼저 `database-schema`·`migration-guide`·
`api-design`·`security-checklist` 스킬과 이 문서 §2, §4.2, §5.1~§5.3, §8을 읽고 다음 순서로 진행한다.

1. migration `0054` 설계를 이 문서 §5에 먼저 기록한다. immutable ranking rule, season manifest와 상태 전이,
   preset version/point budget version별 league definition, 공개 이름·참가·완주 상태의 exact key·index·FK를
   확정한다. 현재 M5-B bundle은 의도적으로 unranked이므로 이를 ranked season에 억지로 연결하지 않는다.
2. ranked 자격을 갖춘 exact authority set과 content bundle을 새 version으로 게시한 뒤 development season과
   preset/custom league를 게시한다. `GET /api/run-options`와 `POST /api/runs`의 닫혀 있던 ranked 경로를 이
   게시 authority에만 연다. season·league·market·policy·content·engine·목표 day가 모두 일치하지 않으면
   시작 transaction을 실패시킨다.
3. 목표 day 도달 시 실제 보유 상태를 바꾸지 않는 immutable `run_finalization`·liquidation line과 안정된
   ranking 정렬을 구현하고 `GET /api/seasons/{id}/leagues`, `GET /api/leagues/{id}/rankings`,
   `GET /api/runs/{id}/finalization`의 최소 server vertical slice를 연결한다. 순수 결산·동점 규칙만 작은 BDD로
   검증하고 production migration·ranked start/replay·결산 재시도·공개 조회를 직접 인수한다.

새 `POST /api/runs`가 client에 연결되기 전에도 manifest 없는 run을 허용하지 않는다. 기존
`POST /api/characters` v1/v2는 호환 기간 동안 **sandbox 전용 legacy start**로 해석하고, 기존 command
fingerprint·replay를 유지한 채 같은 start transaction에서 `rankingEligible=false`, reason
`legacyStartEndpoint`인 immutable manifest를 기록한다. ranked mode는 이 경로로 시작할 수 없다.

별도 MySQL·격리 schema·recovery dump는 만들지 않는다. server 변경은 `main` push 뒤 development
production DB의 startup migration으로 직접 전진시키고 health·migration·기존 run 보존을 확인한다. 접속
경로와 공개 base는 [`m4-life.md` §1.1](./m4-life.md)을 따르며 credential과 session token은 문서에 남기지
않는다. 구현을 먼저 진행하고 순수 core/service 규칙만 BDD로 검증하며 DOM·network·DB 단위 테스트와
무관한 전체 회귀로 병목을 만들지 않는다.

## 2. 실행 manifest와 세 모드

모든 새 런은 `run_manifest`를 가지며 시작 뒤 immutable이다.

- `mode: rankedPreset | rankedCustom | sandbox`
- `seasonId` — ranked는 필수, sandbox는 null
- `leagueDefinitionId` — ranked는 필수
- `marketWorldId`, `policySetId`, M3/M4/M5의 모든 catalog/model bundle ID
- `characterPresetVersionId` 또는 `pointBudgetVersionId`와 canonical 선택 목록
- `engineVersion`, `offlinePolicyVersionId`, manifest canonical SHA-256
- 시작 game day, 목표 game day, run revision

세 모드의 권한은 다음처럼 고정한다.

| 항목 | `rankedPreset` | `rankedCustom` | `sandbox` |
|------|----------------|----------------|-----------|
| 시작 조건 | 게시된 프리셋 그대로 | 게시된 포인트 예산 안에서 선택 | 허용된 전체 범위 자유 입력 |
| 시장 seed | 시즌 공통 | 시즌 공통 | 개인 seed 또는 명시적 seed |
| rule/content 버전 | 시즌 manifest 고정 | 시즌 manifest 고정 | 생성 시 활성 버전 또는 사용자가 고른 호환 버전 |
| 치트성 조작 | 금지 | 금지 | 명시적으로 허용된 override만 가능 |
| 오프라인 진행 | 시즌 정책에 따름 | 시즌 정책에 따름 | 사용자 선택 |
| 랭킹 | 프리셋별 | 예산 버전별 | 제외 |

모드는 run 중 변경할 수 없다. ranked를 sandbox로 변환해 계속 실험하려면 현재 game day 상태를 복제한
새 sandbox run을 만들고 새 run revision·manifest를 부여한다. 반대 방향 변환은 금지한다. sandbox의
seed나 override가 ranked row에 들어가면 DB CHECK와 서비스 검증 모두 실패해야 한다.

## 3. versioned point budget

### 3.1 예산 데이터

`point_budget_version`은 총 예산, 허용 파라미터 schema, 선택지별 point delta, 교차조건, 선택 상한과
표시 설명을 가진 sealed 데이터다. 실제 시작 자금·부채·나이 값과 point 비용은 서로 다른 typed 필드다.
클라이언트는 임의 값을 보내지 않고 `optionId`와 수량만 보낸다.

지원하는 비용 형태는 다음으로 제한한다.

- `fixed` — 선택 시 고정 point
- `perUnit` — 명시된 정수 단위마다 point
- `tiered` — 게시된 구간별 정수 point
- `exclusiveGroup` — 한 group에서 정확히 하나
- `requires` / `forbids` — 다른 option 또는 character fact와의 교차조건

선택은 `(optionId, quantity)`이고 같은 option ID를 두 번 보내는 것은 거절한다. `fixed`는 quantity 1만,
`perUnit`은 version에 게시된 최소·최대 정수 수량만 허용한다. `tiered`는 수량 1부터 선택 수량까지를 서로
겹치지 않는 연속 구간으로 나누고 각 구간의 **단위당 point delta**를 적용하는 누진 방식이다. 구간의
공백·중복이나 option 수량 범위를 덮지 못하는 sealed catalog는 게시할 수 없다. `exclusiveGroup`은 그
group에 속한 option 중 정확히 하나가 선택돼야 한다는 별도 제약이며 비용 종류와 섞지 않는다.

각 option은 시작 draft에 적용할 strict tagged effect 하나를 가진다. v1 effect는 정수 fact 설정·수량당
정수 fact 증가·enum fact 설정으로 제한하고, unknown effect kind·field를 거절한다. 서버는 option ID
오름차순으로 effect를 적용해 canonical draft와 fact map을 만든 뒤 기존 character 정합성을 검사한다.
같은 fact를 서로 다른 값으로 설정하는 조합은 catalog 조건이 빠졌더라도 거절한다. preview와 start는 같은
point evaluator와 effect materializer를 사용한다.

point budget schema v1의 effect 적용 전 base draft는 engine contract로 고정한다. `name=커스텀 실행`, age 25,
gender other, military completed, region capitalArea, background independent, education highSchool,
careerYears 0, certifications 0, startingCashKrw 0, studentLoanKrw 0, creditLoanKrw 0, health normal,
dependents 0이다. v1에서 effect가 수정할 수 있는 field는 이 draft의 이름과 정수·enum field뿐이며 unknown
path나 값 type은 catalog 오류다. base를 바꾸면 engine version과 point budget schema/version을 함께 올려
기존 manifest의 결과를 보존한다.

임의 수식이나 서버 코드는 예산 데이터에 넣지 않는다. 캐릭터 정합성은 기존 §3 규칙을 먼저 검사하고,
그 다음 option ID 오름차순으로 point ledger를 합산한다. positive·negative delta를 모두 i64에서 더하되
중간·최종 범위를 검증한다. `spentPoints <= totalPoints`면 유효하고 남은 point는 버린다. 미사용 point를
돈이나 랭킹 보너스로 바꾸지 않는다. 같은 예산 버전 사용자끼리만 비교하므로 일부러 덜 쓰는 것은 허용된
자기 제약이다.

예산 검증 응답은 `total · spent · remaining`과 option별 delta, 교차조건 실패 code를 반환한다. ranked
start transaction은 이 결과, draft hash, budget version을 다시 계산해 character·manifest와 함께 commit한다.
preview 결과나 클라이언트 합계를 신뢰하지 않는다.

### 3.2 프리셋과 리그 경계

`character_preset_version`은 완성된 canonical character draft와 M3/M4 시작 계약을 참조한다. 표시명이 같아도
내용이 바뀌면 새 version ID다. ranked preset은 정확한 preset version별 별도 리그이고, 서로 다른 프리셋을
난이도 계수로 한 표에 섞지 않는다. ranked custom은 `pointBudgetVersionId`별 리그이며 budget 총점이 같아도
선택지나 가격표 버전이 다르면 섞지 않는다.

## 4. 콘텐츠 seed·versioning·게시

### 4.1 콘텐츠 bundle

M5는 다음 도메인의 seed를 채운다.

- 캐릭터 프리셋과 point option
- M3 자격증, 교육, 직무, 공고 템플릿, 플랫폼, 업종
- M4 지역, 생활비, 주거 매물 템플릿, 대출·보험, 복지, 생애 사건
- M4/M5 법인 업종, 고객계약, 직원 역할, 비용 템플릿

각 도메인은 `content_item(key, version, schemaVersion, payload, sourceNote, status)` 형태의 개념적 저장 경계를
가지되 도메인 계산이 자주 조회하는 값은 typed table로 정규화할 수 있다. JSON을 쓰는 경우 strict tagged
schema로 decode하고 unknown field를 거절한다. 콘텐츠는 `draft → validated → sealed → retired`만 허용하고,
sealed payload는 update/delete하지 않는다. retired는 새 bundle에서 선택하지 않는다는 뜻이며 기존 런은
계속 읽는다.

`content_bundle`은 도메인별 exact item version 목록과 canonical manifest hash를 가진다. 게시 도구는 다음을
한 번에 검증한다.

1. 모든 참조가 존재하고 sealed이며 순환 `requires`가 없는지
2. policy·engine·market model의 호환 범위와 맞는지
3. key·version과 canonical payload hash가 중복되지 않는지
4. ranked 필수 콘텐츠와 기본 choice가 모두 있는지
5. 고정 seed 30년 dry-run과 schema/protocol fixture가 통과하는지

운영 서버가 YAML/JSON seed 파일을 기동 때 upsert하지 않는다. CI가 명시적 migration 또는 publish artifact를
만들고, checksum이 다른 기존 key/version은 배포를 실패시킨다. 외부 데이터 출처가 있는 수치는 source,
기준일, 라이선스 메모를 남기되 원시 시세를 클라이언트에 재배포하지 않는다.

#### M5-B 첫 content bundle 계약 (2026-07-29)

M5-B는 기존 typed authority의 payload를 generic JSON으로 복제하지 않는다. `content_bundle` header와
`content_bundle_member`는 아래 원본의 **ID·key·version·기존 canonical SHA**만 묶고, 실제 계산은 계속 원본
typed table을 읽는다. member의 `sourceNote`는 provenance 메모이지 payload 대체물이 아니다.

| canonical kind 순서 | production 원본 권위 | ID / key / version | SHA·상태 | M5-B source note |
|---:|---|---|---|---|
| 10 | `career_catalog_bundle` | `1 / dev-unranked-m3-v1 / 1` | SHA 없음, published, unranked | M3-A/M3-D typed career catalog; legacy published authority has no standalone digest. |
| 20 | `recruitment_ruleset` | `1 / dev-unranked-m3-recruitment-v1 / 1` | SHA 없음, published, unranked | M3-B typed recruitment rules; legacy published authority has no standalone digest. |
| 30 | `employment_policy_set` | `1 / dev-unranked-m3-employment-2026-v1 / 1` | SHA 없음, published, unranked | M3-C/M3-D typed employment policy; legacy published authority has no standalone digest. |
| 40 | `life_catalog_set` | `7 / dev-unranked-m4-life-corporation-2026-v6 / 6` | `7638c390…abea2f0`, sealed, unranked | M4 typed life catalog aggregate. |
| 50 | `credit_model_version` | `5 / dev-unranked-m4c3-credit-2026-v4 / 4` | `d878df2c…28e6cf8`, sealed, unranked | M4 typed credit and loan model. |
| 60 | `real_estate_model_version` | `7 / dev-unranked-m4-real-estate-sale-tax-2026-v6 / 6` | `fe870274…d1ea541`, sealed, unranked | M4 typed housing and real-estate model. |
| 70 | `character_preset_version` | IDs `2,3,5,1,4` / `early-start,late-start,restart,rookie,supported` / 각 `1` | 각 generated SHA, sealed, unranked | M5-A sealed character preset. |
| 80 | `point_budget_version` | `1 / dev-unranked-custom-2026 / 1` | `a340fcc4…36ca553`, sealed, unranked | M5-A sealed point budget. |

첫 header는 `dev-unranked-m5-content-2026 / version 1 / schemaVersion 1`이며 source note는
`M3/M4 typed development authorities with M5-A start catalogs.`로 고정한다. `newRun` assignment는 이
sealed bundle을 가리키고 revision 1에서 시작한다. preset 다섯 개를 개별 member로 기록하므로 총 member는
12개다.

canonical member 순서는 kind의 고정 rank `10→80`, 같은 kind 안에서는 ASCII binary
`authorityKey → authorityVersion → authorityId` 오름차순이다. canonical JSON은 다음 필드 순서를 byte
contract로 사용한다.

```json
{"bundleKey":"…","members":[{"authorityId":"…","authorityKey":"…","authorityKind":"…","authoritySha256":null,"authorityVersion":1,"sourceNote":"…"}],"rankedEligible":false,"schemaVersion":1,"sourceNote":"…","version":1}
```

SHA-256 입력은 위 UTF-8 JSON 자체이며 공백·개행을 넣지 않는다. 원본 SHA가 있는 member는 64자리 소문자
hex가 정확히 일치해야 한다. 독립 digest가 생기기 전 게시된 M3 세 authority만 현재 **unranked** bundle에서
`authoritySha256=null`을 허용한다. ranked bundle은 모든 member의 SHA, 원본 ranked eligibility, sealed 또는
published 상태가 필요하므로 이 development bundle을 ranked season에 쓸 수 없다. 같은 bundle 안에서
`(kind,key,version)`과 non-null SHA는 중복될 수 없다. 이 첫 slice는 generic `requires` edge를 만들지 않고
typed authority의 FK·publication trigger가 참조 그래프를 소유하므로 content-bundle 층의 순환은 구조적으로
없다.

DB 경계는 다음과 같다.

- header는 `draft → sealed → retired`만 허용하고 active `newRun` assignment가 가리키는 동안 retire할 수
  없다. member와 canonical manifest는 draft일 때만 insert하고 이후 update/delete하지 않는다.
- member insert와 header seal은 kind별 typed table을 다시 조회해 ID/key/version/SHA와 published/sealed
  상태를 검증한다. header seal은 canonical JSON의 generated SHA와 정확히 일치해야 한다.
- `run_manifest`에는 nullable `content_bundle_id/content_bundle_sha256` pair를 추가한다. 기존 10건은 둘 다
  null인 immutable 역사로 남기고, migration 뒤 새 run은 active sealed bundle의 ID/SHA를 둘 다 pin한다.
  새 sandbox canonical manifest는 이 두 필드와 `schemaVersion: 2`를 포함한다.
- start의 read/lock 양쪽이 content assignment revision을 기존 active-run ABA 비교에 포함한다. assignment가
  바뀌거나 bundle이 retire되면 준비한 start를 재시도하며, 같은 command replay는 처음 commit된 manifest를
  그대로 반환한다.

### 4.2 entropy namespace

시장, 채용, 매물, 사건, 법인 결과는 각각 고정 namespace를 쓴다. 새 콘텐츠는 자기 `contentKey` stream만
추가하며 기존 item의 난수 호출 횟수를 바꾸지 않는다. 공통 형태는
`H(engineVersion, worldSeed, runEntropyId, namespace, contentKey, period, occurrence, drawKind)`다.
ranked에서 `runEntropyId`가 필요한 개인 사건은 서버가 run manifest에서 결정론적으로 유도하고 사용자에게
시즌 seed의 원문을 노출할 필요는 없다. 알고리즘이나 key 구성이 바뀌면 engine version과 시즌을 새로 낸다.

## 5. 시즌·리그·고정시점 랭킹

### 5.1 시즌 게시와 수명주기

`season` 상태는 `draft → registrationOpen → active → locked → finalized → archived`다. 시즌 manifest는
공통 market seed/world, engine·policy·content bundle, 허용 리그, 시작 가능 벽시계 기간, 목표 game day,
offline policy와 결산 규칙 버전을 pin한다. active 뒤에는 수정하지 않는다. 치명적 버그는 manifest를
고치는 대신 시즌을 `locked`하고 새 시즌을 게시한다.

등록 기간과 시즌 운영 기간은 벽시계지만, 플레이 성과는 각 run의 game day로만 계산한다. 시즌 종료
벽시계까지 목표 day에 도달하지 못한 run은 해당 시즌의 최종 랭킹에 들지 않고 `unfinished`로 남는다.
ranked run은 목표 day를 넘어 전진할 수 없으며 마지막 advance는 남은 일수까지만 허용하거나 strict하게
요청을 거절하는 정책 중 **고정 기본값으로 remaining days까지만 실행**하고 receipt에 truncated days를
명시한다.

수동 전진 receipt는 원래 요청을 식별하는 `requestedDays`를 유지하고 실제 commit한 `committedDays`와
`truncatedDays = requestedDays - committedDays`를 함께 반환한다. 목표일 직전 요청은 남은 날만 같은 command
identity 아래 commit하고, 이미 목표일에 도달한 뒤 들어온 새 전진 요청은 `invalidCommand`로 거절한다.
자동 시계는 목표일을 감지하면 추가 day를 commit하지 않고 paused 상태를 방송한다. 기존 비랭크 receipt는
재조회할 때 cursor 차이로 두 값을 복원한다.

#### M5-C 첫 ranked authority 계약 (2026-07-29)

M5-B의 `dev-unranked-m5-content-2026` v1과 그 12개 원본 authority를 이름만 바꿔 복제하지 않는다. 이미
published/sealed 뒤 update/delete가 막힌 exact authority 조합을, 같은 binary로 완료한 30년 회귀 evidence와
함께 새 immutable `ranked_ruleset_release`로 인증한다. 원본 row와 `content_bundle.ranked_eligible=false`는
수정하지 않는다. ranked 자격은 원본 하나하나가 아니라 **release가 고정한 전체 조합**에만 생기며 다른
market·policy·content·engine 조합에는 전이되지 않는다.

첫 release는 다음 값을 모두 직접 pin한다.

- market world, finance policy, career catalog, employment policy, life catalog, credit model, real-estate model
- M5-B content bundle ID와 canonical SHA-256
- server engine version
- 검증 evidence key와 paired 30년 최종 상태 SHA-256
- 위 필드의 canonical JSON과 generated release SHA-256

`ranking_rule_version`은 목표 game day, metric, liquidation/carry policy, 동점 순서를 canonical JSON으로 봉인한다.
첫 규칙은 target day 10,950, `afterTaxNetWorthKrw` 내림차순 뒤 insolvency day·player command 수·run ID
오름차순이다. 결산 planner가 구현되기 전에는 이 규칙으로 완주 row를 만들지 않는다.

`season`은 release와 ranking rule의 `(id, sha)` 쌍, 등록·운영 벽시계, 표시 이름을 pin한다. 의미 필드는 생성
뒤 바꾸지 않고 상태만 revision과 함께
`draft → registrationOpen → active → locked → finalized → archived`로 전진한다. `season_assignment`의
`rankedRun` 한 행만 새 ranked 시작에 쓰며 assignment revision으로 ABA를 막는다.

`league_definition`은 한 season에 속하고 mode가 `rankedPreset`이면 정확한
`character_preset_version_id` 하나, `rankedCustom`이면 정확한 `point_budget_version_id` 하나만 가진다. 첫
season은 content bundle에 들어 있는 preset 5개와 point budget 1개를 각각 별도 league로 게시한다. 서로 다른
preset이나 budget version은 같은 ranking 표에 섞지 않는다.

ranked run manifest는 기존 `season_id · league_definition_id` 외에 release와 ranking rule의 `(id, sha)` 쌍을
함께 기록한다. 시작 transaction은 활성 season/league, release의 모든 direct authority, 현재 assignment,
content SHA, engine, 요청한 preset/budget을 `FOR SHARE` 아래 다시 대조한다. 어느 하나라도 달라지면 run·command
identity·receipt를 만들지 않고 `modeUnavailable`로 닫는다. sandbox manifest의 네 ranked authority field는
항상 null이다.

### 5.2 결산과 세후순자산

랭킹 지표는 manifest의 고정 목표일(초기 의도는 game 30년 시점)에 만들어진 `run_finalization`의
`afterTaxNetWorthKrw`다. 목표일 하루 transaction의 모든 정산·사건·세금을 끝낸 뒤 다음 순서로 순수
liquidation planner를 실행한다.

1. 지갑·계좌 현금과 확정 수취채권
2. 금융자산을 목표일 종가로 매도한다고 가정한 proceeds와 비용·세금
3. 부동산을 결산 규칙의 당일 인정가액으로 매도한다고 가정한 proceeds, 담보·보증금·거래비용·세금
4. 법인 지분의 결산 규칙상 순자산가치와 개인 귀속 시 세금
5. 보험 해지환급·복지 미수급처럼 결산 규칙이 포함한다고 명시한 권리
6. 모든 대출·연체·세금·보증금 반환·도산계획 채무

실제 보유 상태를 매각하지 않고 immutable `liquidation_line`으로 계산한다. 각 line은 gross, cost, tax,
net, policy/rule ID를 보존하고 합계는 i128 뒤 BIGINT 범위를 검증한다. 미래 매도 대기나 미래 세율을
추정하지 않는다. 목표일에 가격이 없는 자산의 carry 규칙은 결산 규칙 version에 반드시 있고, 없으면
finalization을 실패시켜 운영자가 수정 시즌을 내게 한다.

finalization source key는 `(runId, targetGameDay, rankingRuleVersion)`이고 한 번 성공하면 immutable이다.
재시도는 같은 line hash로 수렴해야 한다. 랭킹 row는 다음 순서로 정렬한다.

1. `afterTaxNetWorthKrw` 내림차순
2. 목표일까지 누적 insolvency 상태 일수 오름차순
3. 목표일까지 실행한 player command 수 오름차순
4. run ID 오름차순

2·3은 동점 순서만 정하며 순자산에 가감하지 않는다. 공개 화면은 순위·표시 이름·프리셋/예산 버전·세후
순자산·완주 시각만 보여 주고, seed·OAuth 식별자·상세 자산은 노출하지 않는다. 사용자는 랭킹 표시 이름을
별도로 정하며 부적절한 이름은 숨길 수 있다.

### 5.3 무결성과 재계산

ranked mutation은 manifest compatibility, command identity, cursor, engine version을 감사 로그에 남긴다.
관리자 수동 잔액 변경, sandbox override, 미등록 binary version이 한 번이라도 적용된 run은
`rankingEligible=false`가 되며 되돌려도 복구되지 않는다. offline worker도 온라인 API와 같은 domain
service와 binary version을 사용한다.

랭킹 게시 전 verifier가 opening 원장부터 최종 원장 hash chain, pinned manifest, game day 연속성,
finalization 합계를 재대조한다. 전체 30년을 매번 재시뮬레이션하는 것이 아니라 저장 감사 상태를 검증하고,
표본 run은 고정 binary로 full replay한다. 오류 row는 조용히 제외하지 않고 리그를 `provisional`로 표시한다.

## 6. 별도 offline worker

### 6.1 프로세스와 opt-in

오프라인 진행은 API 서버의 타이머가 아니라 같은 workspace의 별도 Rust binary/service로 배포한다.
worker는 시장 준비와 M0~M4의 **동일한 one-day domain planner**를 호출하고 HTTP를 거치지 않는다.

사용자는 `offline_progress_setting`을 명시적으로 켜야 한다. 레코드는 `enabled`, policy version,
`absenceStartedAt`, `accruedThrough`, `pendingDays`, `processedDays`, 최대 catch-up 경계와 revision을 가진다.
기본값은 off다. 켜기 전의 부재 시간을 소급하지 않으며, 마지막 SSE 연결이 닫힌 DB 시각에 absence window를
연다. 첫 SSE 연결이 생기면 그 DB 시각까지 마지막 accrual을 하고 window를 닫는다. 여러 탭 중 하나가
남아 있으면 online이다. 토글을 끄면 future accrual과 아직 시작하지 않은 pending day를 취소하고
`cancelledPendingDays` 감사값을 남긴다. 이미 commit된 날은 되돌리지 않는다. ranked에서 시즌 offline
policy가 금지하면 토글 자체를 거절한다.

벽시계에서 게임일로 환산하는 cadence와 absence window당 catch-up cap은 `offline_policy_version` 데이터다.
초기 제품 의도인 최대 약 90일은 플레이테스트 후보일 뿐 이 문서의 상수가 아니다. worker는 열린 window의
`floor((min(now, accrualLimit) - accruedThrough) / cadence)`만 pending에 더하고, window cap과 목표일까지
남은 일수 중 최솟값만 처리한다. `accruedThrough`는 실제 반영한 cadence만큼만 전진해 짧은 잔여 시간을
보존한다. 서버·DB 시간은 UTC를 쓰고 음수 경과는 0으로 처리하며, clock rollback을 future accrual로
보상하지 않는다.

### 6.2 lease와 온라인 경쟁

모든 진행 주체는 `progress_lease(save_id, holderKind, holderToken, expiresAt, generation)`를 얻어야 한다.
DB의 `CURRENT_TIMESTAMP(6)`가 lease 시간의 권위이고 프로세스 로컬 시계는 판정에 쓰지 않는다.

- worker는 후보를 bounded batch로 읽고 `FOR UPDATE SKIP LOCKED`로 claim한다.
- lease 획득은 만료 행 또는 자기 token만 조건부 update하고 generation을 증가시킨다.
- 하루 commit마다 같은 token·generation·미만료를 확인하고 짧게 갱신한다.
- worker가 죽으면 lease 만료 뒤 다른 worker가 마지막 committed game day부터 이어 간다.
- lease를 잃은 worker는 추가 쓰기나 receipt 생성을 하지 않는다.

온라인 command가 도착하면 `onlineIntentAt`을 기록한다. worker는 매 하루 commit 뒤 intent를 확인하고 다음
날 전에 lease를 양보한다. 이미 실행 중인 하루 transaction을 강제 중단하지 않으므로 온라인 요청은 bounded
시간 동안 `progressBusy`를 받을 수 있고 같은 command ID로 재시도한다. 온라인 주체도 lease 없이 진행하지
않으며, 만료 전 worker lease를 훔치지 않는다. 이 우선권과 하루 단위 양보로 이중 전진과 긴 사용자 대기를
함께 막는다.

온라인 SSE 연결 자체가 opt-in을 끄지는 않는다. 연결 중에는 새 offline day를 accrual하지 않지만 이미
확정된 `pendingDays`도 지우지 않는다. 온라인 intent가 있는 동안 worker는 양보하고, 마지막 온라인 연결이
닫히면 새 absence window를 열어 기존 pending부터 처리한다. 수동·배속 온라인 진행은 pending day를 대신
처리한 것으로 세지 않는다. 한 세이브의 게임일은 어떤 주체든 lease 아래 한 줄로만 전진한다.

### 6.3 실패·부하·관측

하루 domain error는 전체 rollback하고 `offline_progress_attempt`에 공개 error code, manifest version,
game day, 재시도 횟수를 남긴다. 영구 schema/policy 오류는 해당 setting을 `pausedBySystem`으로 바꾸고
무한 재시도하지 않는다. deadlock·일시 DB 오류만 제한된 지수 backoff로 재시도한다.

worker는 run당 한 번에 처리할 최대 day batch, 전체 동시 transaction, 시장 cache 선행 생성 범위를 설정으로
제한한다. 설정은 성능값일 뿐 결과 의미를 바꾸면 안 된다. batch 크기 1과 큰 batch의 최종 원장 hash가
같아야 한다. 배포 중 binary version이 시즌 manifest와 다르면 ranked run을 claim하지 않는다.

## 7. 법인 상세 경영

M5는 M4 단순 법인을 다음 제한된 수직 슬라이스로 확장한다. 복잡한 회계 ERP나 주식회사의 모든 법률 행위는
범위 밖이다.

### 7.1 운영 모델

- 업종별 `business_catalog_version`: 고객계약 template, 역할별 인력, 임차·도구·마케팅 비용, 운전자금 규칙
- 고객계약: `offered → accepted → active → completed|failed|cancelled`
- 직원: M3 직무·급여 band를 참조하는 `vacant → hired → active → resigned|terminated`
- 월 운영계획: 생산 capacity, 선택한 고객계약 우선순위, 마케팅 band와 현금 buffer
- 자금조달: 추가 출자, retained earnings, 게시된 법인대출만 허용

고객 제안과 성과 entropy는
`H(corpSeed, corporationId, operatingMonth, contractTemplateKey, occurrence, drawKind)`를 쓴다.
직원 수, 대표의 M3 시간 투입, 도구 수준이 deterministic capacity가 되고, capacity 부족 시 플레이어가
정한 계약 priority와 contract ID 순으로 뒤 계약이 지연되거나 실패한다. 결과를 좋게 만들기 위해 조회를
반복할 수 없도록 월 제안은 미리 materialize한다.

### 7.2 회계·세금·개인 경계

법인은 별도 지갑과 복식 원장, receivable/payable, 고정자산, loan, tax year를 가진다. 월 마감 순서는
`고객 매출 인식 → 수금 → 급여·사업비 → 이자·대출 → 세금 reserve → 감가/결산 → 다음 달 제안`으로
고정한다. 부가세·법인세·급여 관련 실제 규칙은 versioned policy이며 문서에서 숫자를 단정하지 않는다.

개인과의 경로는 `capitalContribution · documentedExpenseReimbursement · salary · dividend ·
shareholderLoan` typed command뿐이다. shareholder loan은 게시 상품과 계약·상환이 있어야 하며 임의 이체가
아니다. 양쪽 원장은 하나의 correlation ID로 같은 transaction에 기록한다.

법인 지급불능은 개인 도산과 자동 합쳐지지 않는다. 법인 책임과 개인 보증은 계약 data로 구분하고,
보증이 있는 채무만 M4 개인 insolvency claim으로 이어진다. 해산은 미수·재고 없는 M5 단순 모델에서
채권·세금·직원 급여를 우선순위와 ID 순으로 정산하고 남은 자산만 지분 비율로 분배한다.

## 8. API·기능 화면

strict API는 M4 공통 command/cursor와 unknown-field 거절을 유지한다.

| 경로 | 역할 |
|------|------|
| `GET /api/run-options` | 활성 season, preset, budget, sandbox 호환 버전 |
| `POST /api/runs/point-preview` | 서버 point ledger와 정합성 preview |
| `POST /api/runs` | mode별 immutable manifest와 새 run 생성 |
| `GET /api/seasons/{id}/leagues` | 공개 리그와 상태 |
| `GET /api/leagues/{id}/rankings` | cursor 기반 provisional/final ranking |
| `GET /api/runs/{id}/finalization` | 자기 liquidation line과 결산 hash |
| `PUT /api/offline-progress` | opt-in/out와 setting revision |
| `GET /api/offline-progress/status` | accrued/processed day, lease와 공개 오류 상태 |
| `GET/POST /api/corporations/{id}/operations` | 월 운영계획·고객계약·인력 |

M5-A의 `POST /api/runs` v1은 `mode`로 구분하는 strict tagged request다. 세 variant는
`commandId · expectedRunRevision · expectedStateRevision · expectedGameDay`만 공통으로 가진다.

- `rankedPreset`은 `characterPresetVersionId`만 추가로 받는다.
- `rankedCustom`은 `pointBudgetVersionId`와 `(optionId, quantity)` selection만 추가로 받는다.
- `sandbox`는 기존 v2 character profile과 최대 두 개의 canonical starting loan만 받는다.
- variant에 속하지 않는 필드와 unknown field는 `400 invalidCommand`다. active season이 없는 동안 두 ranked
  variant는 version 존재 여부나 내부 상태를 노출하지 않고 `409 modeUnavailable`로 닫는다.
- explicit sandbox의 immutable manifest는 selections를 빈 배열로 저장하고
  `rankingEligible=false · rankingIneligibilityReason=sandboxMode`를 기록한다. legacy
  `POST /api/characters`의 `legacyStartEndpoint`와 구분한다.
- 성공 응답은 `mode`, DB가 생성한 `manifestSha256`, 기존 start receipt와 최신 game snapshot을 반환한다.
  응답 조립 실패 뒤 같은 command를 재시도해도 같은 manifest hash와 run revision을 반환해야 한다.

기능 화면은 스타일링 없이 모드 비교, 프리셋 선택, point 사용 내역과 remaining, sandbox 고지, 시즌/리그
상태, 30년 목표 진행률, 결산 line, 랭킹, offline opt-in·catch-up 상태, 법인 운영계획·계약·직원·현금흐름을
조작한다. ranked와 sandbox는 항상 눈에 보이는 label로 구분한다. 클라이언트가 point, 순위, 세후순자산,
catch-up day를 재계산하지 않는다.

DOM은 기존 client foundation대로 mount 한 번과 path 구독을 사용한다. 긴 ranking과 콘텐츠 목록은 cursor
pagination과 고정 행 슬롯을 쓴다. 시각적 스타일·애니메이션·브랜드 작업은 별도 후속 범위다.

## 9. 배포와 관측

### 9.1 배포 경계

배포 단위는 `server API`, `offline worker`, `client`, `migration/content artifact`다. API와 worker 이미지는
같은 engine source와 version을 사용하되 별도 process/health check/scale 설정을 가진다.

1. 외부 사용자가 없는 development 단계는 현재 production DB에 startup migration을 직접 적용하고
   migration row·기존 run·health를 확인한다. 외부 플레이테스트 gate 이후에는 PII 없는 production clone에서
   빈 DB·현재 DB forward migration과 이전 이미지 read-only 호환을 먼저 검증한다.
2. content/policy bundle을 sealed 상태로 먼저 적재하되 assignment나 season은 활성화하지 않는다.
3. API를 배포하고 schema·bundle health를 확인한다.
4. worker를 drain 후 배포하고 manifest 호환 run만 claim하는지 확인한다.
5. client를 배포한다.
6. 새 run assignment와 season registration을 마지막에 연다.

rollback은 immutable bundle과 기존 run을 삭제하지 않는다. 새 registration/assignment를 닫고 이전 API·worker
이미지로 돌아간다. 이미 새 engine으로 시작한 run을 이전 binary가 지원하지 않으면 그 run은 maintenance
상태로 두고 결과를 추측하지 않는다. DB backup·복구 연습과 OAuth/provider 장애 fallback을 release gate로 둔다.

### 9.2 로그·메트릭·trace

구조화 로그는 `requestId`, hash 처리한 user key, save/run ID, manifest/engine version, command ID,
game day, worker lease generation, stable error code를 가진다. OAuth token, cookie, 이메일, policy 원문,
캐릭터 민감 필드는 기록하지 않는다.

최소 메트릭은 다음이다.

- API request latency/error와 domain error code
- one-day planner latency, rollback, deadlock retry, state revision conflict
- SSE 연결·재연결과 snapshot 지연
- offline queue run/day 수, oldest accrual age, lease wait/loss, processed day/sec, paused setting
- season별 run start/target completion/finalization failure/ranking verification failure
- DB pool wait, transaction latency, migration/content checksum failure

trace는 HTTP/worker claim → market cache → player transaction → ledger/finalization까지 correlation을 잇되 돈
금액과 개인 상세를 attribute로 싣지 않는다. alert는 오류율, queue age, lease loss 급증, finalization 실패,
DB backup 실패에 둔다. 단순 이용자 행동 analytics는 명시적 플레이테스트 동의 범위에서만 수집한다.

운영 runbook은 migration 실패, worker backlog, stuck lease, season lock, ranking provisional, OAuth 장애,
DB 복구, 개인정보 요청의 확인·완화·복구·사후 검증 절차를 적는다.

## 10. 테스트와 검증

### 10.1 순수 규칙·protocol

- mode별 필수/금지 필드, ranked↔sandbox 변환, manifest hash와 불변성을 검증한다.
- point fixed/perUnit/tiered, negative delta, 교차조건, 미사용 point, i64 overflow와 option 순서 독립성을 검증한다.
- 콘텐츠 canonical hash, 참조·순환·호환 검증, sealed 뒤 mutation 거절 fixture를 검증한다.
- entropy namespace에 새 콘텐츠를 추가해도 기존 시장·채용·사건·법인 고정 벡터가 같은지 검증한다.
- target day truncation, liquidation line 순서·내림·세금, tie-break와 finalization replay를 검증한다.
- offline accrual, cap, opt-in 이전 비소급, clock rollback, 목표일 경계와 pause를 검증한다.
- lease acquire/renew/loss, online intent 양보, worker crash 후 resume와 command replay를 서비스 테스트한다.
- 법인 capacity, 계약 priority, 월 마감 순서, 개인/법인 양쪽 원장과 보증 경계를 검증한다.
- strict response status, bounded arrays, cursor, 알 수 없는 enum/field, ranking privacy를 검증한다.

### 10.2 실제 MySQL 8·장기·부하 스모크

격리 MySQL 8에서 다음을 검증한다.

- M4 완료 DB에서 forward migration, 모든 기존 run의 manifest backfill과 ranking 제외 기본값
- season/content publish 경쟁, 중복 start, 동일 budget draft replay가 한 run으로 수렴하는지
- 여러 API 인스턴스와 worker가 같은 save를 claim할 때 game day·원장·receipt가 정확히 한 번 증가하는지
- lease owner process kill, DB connection loss, 만료 직전 online command, worker deployment drain
- catch-up batch 크기와 수동/배속 실행의 30년 최종 state·ledger·finalization hash 동일성
- 목표일 동시 도달과 ranking refresh에서 immutable finalization 한 건, stable tie-break, cursor 중복/누락 없음
- 다른 사용자의 manifest 상세·finalization line·offline setting·법인 ID 접근 거절
- 예상 동시 플레이테스트 규모의 API/worker 혼합 부하에서 DB pool, lock wait, SSE 지연이 운영 budget 이내인지

성능 budget의 구체 숫자는 배포 환경 baseline을 측정해 release config에 기록한다. 결과 의미를 바꾸는 자동
정산 생략이나 낮은 정밀도 모드는 허용하지 않는다.

## 11. 외부 플레이테스트 gate

외부 참가자를 받기 전에 다음이 모두 준비돼야 한다.

1. 한 개의 sealed season manifest, 최소 한 개 ranked preset league와 한 개 ranked custom league
2. sandbox 시작과 ranked 제외 표시
3. M3/M4 콘텐츠 bundle의 source/license 검토와 투자 조언·단순 법률/보험 모델 고지
4. 30년 고정 시나리오와 실제 MySQL·혼합 부하 검증 결과
5. API·worker·DB dashboard, alert, backup, 복구 연습, season lock runbook
6. 피드백 폼과 익명 run manifest/결과 hash, 명시적 analytics 동의·철회 경로
7. 알려진 문제, 데이터 삭제/계정 문의, 장애 공지 경로

플레이테스트 중 policy/content/point 값을 in-place 수정하지 않는다. 치명적 무결성 버그는 시즌을 lock하고
새 version/season을 내며, 밸런스 문제는 다음 시즌 후보로 기록한다. 참가자의 실제 재산·소득·건강 정보를
수집하지 않고 캐릭터 값은 허구임을 명확히 한다.

## 12. M5 완료 조건

1. ranked preset, ranked custom, sandbox가 서로 다른 strict start 계약과 immutable manifest로 생성된다.
2. point budget preview와 start 재검증이 같은 ledger를 만들고 예산·교차조건을 우회할 수 없다.
3. 콘텐츠 seed를 sealed bundle로 게시하고 기존 run 결과를 바꾸지 않은 채 새 version을 추가할 수 있다.
4. 시즌 공통 seed의 프리셋별/예산별 리그에서 목표 30년의 세후순자산 finalization과 안정된 순위가 나온다.
5. 기본 off인 offline 진행이 별도 worker에서 opt-in, cap, lease를 지키고 온라인 진행과 중복되지 않는다.
6. worker 중단·재배포·lease 만료 뒤 마지막 commit부터 이어도 수동 진행과 최종 hash가 같다.
7. 법인이 고객계약, 직원, 운전자금, 급여·세금·배당, 지급불능/해산까지 개인 원장과 분리돼 동작한다.
8. 스타일 없는 화면에서 모드 생성, 시즌·랭킹, 결산, offline 상태, 법인 상세를 끝까지 조작한다.
9. 배포 순서, rollback, 관측, alert, backup/restore와 장애 runbook을 실제 환경에서 rehearsal한다.
10. 외부 플레이테스트 gate를 통과하고 한 시즌을 열어 익명 피드백과 재현 가능한 run hash를 수집한다.

## 13. 플레이테스트 전까지 의도적으로 남기는 조정값

- point 총예산, 선택지별 비용·환급과 ranked custom 허용 조합
- 프리셋 구성, 시즌 등록 기간·운영 기간과 목표일 도달 UX
- offline cadence, 1회·누적 catch-up cap, worker batch·동시성 성능값
- 각 도메인의 콘텐츠 개수와 자격증·공고·사건·매물 출현 밀도
- 법인 고객계약 보상·실패분포, 직원 비용, capacity와 운전자금 난이도
- 랭킹 공개 범위, 리그 최소 인원과 provisional 표시 정책
- 실제 배포 환경에서 측정할 latency·queue age·lock wait alert threshold

30년 목표라는 제품 기준, 세후순자산 결산 순서, mode 격리, immutable version, entropy key, 원 단위 반올림,
lease 단일 진행과 기본 opt-in off는 플레이테스트로 흔들지 않는 계약이다.
