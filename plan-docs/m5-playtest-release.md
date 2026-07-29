# M5 개발 플레이테스트 안내와 알려진 문제

- 기준일: 2026-07-30
- 적용 환경: 사용자 없는 development production
- 상세 설계: [`m5-expansion.md` §9.5·§11](./m5-expansion.md)

## 제품 고지

LifeLedger의 계정, 캐릭터, 소득과 자산은 모두 게임 안의 허구 데이터다. 세금·투자·대출·보험·복지·도산
모델은 교육·오락용으로 단순화한 규칙이며 투자, 세무, 법률, 보험 또는 신용 조언이 아니다. 실제 의사결정의
근거로 사용하지 않는다.

## source와 license 검토

- 세율·보험료율·병역·도산처럼 외부 사실에 기대는 규칙은 M3·M4 상세 문서와
  `policy_source_document`에 공식 기관 URL, 기준일, 원문 SHA-256을 연결한다. 요약 기사나 실제 개인 자료를
  seed로 사용하지 않는다.
- M5 content bundle은 원시 외부 자료를 복제하지 않고 기존 typed authority의 ID·version·canonical hash와
  provenance note만 묶는다. 자격·공고·사건·프리셋 등 게임 표현은 프로젝트가 만든 가상 콘텐츠다.
- 프로젝트 본문은 저장소의 [`LICENSE`](../LICENSE)를 따른다. HTML5 Boilerplate 고지는
  [`client/LICENSE-html5-boilerplate.txt`](../client/LICENSE-html5-boilerplate.txt)에, vendored SQLx의
  Apache-2.0/MIT license는 `server/vendor/sqlx-core/`에 원문을 유지한다.
- 외부 원문·시세·개인정보를 client bundle이나 feedback 저장소로 재배포하지 않는다.

## 데이터와 삭제

- usage analytics는 수집하지 않는다.
- 피드백은 별도 명시 동의 뒤에만 제출하며 활성 본문은 제출 후 최대 90일 보관한다.
- 피드백 한 건 삭제, 전체 동의 철회, 계정 전체 삭제를 owner UI에서 수행할 수 있다. 계정 전체 삭제는 모든
  session·save·동의·피드백을 영구 삭제하며 현재 development production에는 복구 backup이 없다.
- 피드백에 이메일, OAuth profile, session token, 실제 재산·소득·건강 정보, 원 단위 금액이나 캐릭터 상세를
  적지 않는다.

## 알려진 문제와 현재 제한

1. 실제 외부 참가자 시즌은 아직 열지 않았다. 현재 season과 ranking은 내부 development 검증용이다.
2. 현재 단계는 backup artifact를 만들지 않으며 DB 장애 시 빈 DB 재구축과 개발 데이터 유실을 허용한다.
3. usage analytics가 꺼져 있어 행동 funnel이나 자동 이용 통계는 제공하지 않는다.
4. 시각 스타일링과 모바일 전용 UX는 기능 인수 뒤 작업이다.
5. production client bundle은 708 KiB로 webpack 권고 크기를 넘는다. 기능에는 영향이 없지만 스타일링 단계에서
   화면 단위 lazy loading 후보로 다룬다.
6. DataGSM·Google provider 노출과 각 authorization redirect까지 production에서 확인했다. 실제 외부
   계정의 OAuth authorization/callback은 계정 접근 승인이 필요한 owner 수동 smoke다.

내부 development acceptance에서는 ranked preset과 ranked custom을 각각 30일 command 365개로 day 10,950까지
완주했고, 두 run 모두 completed finalization·9개 canonical line·공개 league 1위로 반영됐다. 이는 실제 참가자
성과나 밸런스 표본이 아니라 기술 경로 검증 결과다. sandbox offline worker도 opt-in 뒤 하루를 정확히 한 번
commit하고 opt-out했으며 최종 운영 경고는 0이었다.

## 문의와 장애 보고

알려진 문제, 장애, 삭제 문의는 [GitHub Issues](https://github.com/it-play/lifeledger/issues)를 사용한다.
issue에는 이메일, OAuth profile, session token, 실제 금융·건강 정보, 캐릭터 상세 또는 피드백 본문을 쓰지
않는다. 계정 삭제는 문의로 대리 요청하지 않고 로그인한 dashboard의 owner 삭제 기능을 사용한다.
