# 시장데이터 API 키와 운영 규약

이 문서는 LifeLedger의 시장데이터 배치가 읽는 키, 발급 위치와 데이터 사용 경계를 정의한다.
실제 키 값은 저장소에 커밋하지 않고 로컬 `server/.env` 또는 production GitHub Secret
`SERVER_ENV`에만 둔다.

## 필요한 환경 변수

| 환경 변수                | 필수 시점                 | 역할                                                               | 발급 위치                                                                                                                                                                |
|--------------------------|---------------------------|--------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `DATA_GO_KR_SERVICE_KEY` | 국내 종목 카탈로그 동기화 | 금융위원회 KRX 상장종목정보와 주식시세정보. 카탈로그 기준 공급자   | 공공데이터포털의 [KRX 상장종목정보](https://www.data.go.kr/data/15094775/openapi.do), [주식시세정보](https://www.data.go.kr/data/15094808/openapi.do) 각각 활용 신청     |
| `KRX_AUTH_KEY`           | KRX 교차 검증 실행        | KOSPI·KOSDAQ·KONEX 종목 기본정보 검증                              | [KRX Open API](https://openapi.krx.co.kr/)에서 인증키 발급 후 [서비스 목록](https://openapi.krx.co.kr/contents/OPP/INFO/service/OPPINFO004.cmd)의 사용할 API별 이용 신청 |
| `OPENDART_API_KEY`       | 기업 메타데이터 보강      | 고유번호 파일과 기업개황의 업종·법인 정보 연결                     | [OpenDART 인증키 신청](https://opendart.fss.or.kr/uss/umt/EgovMberInsertView.do)                                                                                         |
| `FMP_API_KEY`            | 선택적 글로벌 보정        | 벤더가 제공하는 심볼/프로필을 내부 교차 확인. 클라이언트 표시 금지 | [FMP Dashboard](https://site.financialmodelingprep.com/developer/docs/dashboard)                                                                                         |
| `ECOS_API_KEY`           | 선택적 거시 보정          | 한국은행 기준금리·물가·통화 시계열의 캘리브레이션 후보             | [한국은행 ECOS Open API](https://ecos.bok.or.kr/api/#/)                                                                                                                  |
| `KOSIS_API_KEY`          | 선택적 인구·소득 보정     | KOSIS 통계표의 캘리브레이션 후보                                   | [KOSIS 공유서비스 키 신청](https://kosis.kr/openapi/index/index.jsp?serviceCD=2)                                                                                         |

`DATA_GO_KR_SERVICE_KEY`는 포털에서 제공하는 **Decoding 키**를 넣는다. 두 공공데이터 서비스가
개별 활용 신청 목록에서 승인됐는지도 확인한다. 나머지 공급자가 비어 있어도 기준 카탈로그는 발행할
수 있으며, 비어 있는 공급자는 동기화 보고서에 `notConfigured`로 기록한다.

## 선택적 데이터셋 설정

ECOS와 KOSIS는 키만으로 어떤 통계를 받을지 결정할 수 없으므로 배치 실행 시 아래 값도 함께 둔다.

```dotenv
# ECOS StatisticSearch 경로의 통계표코드/주기/항목코드. 쉼표로 여러 항목을 지정할 수 있다.
ECOS_STATISTIC_CODE=
ECOS_PERIOD_CODE=
ECOS_ITEM_CODES=

# KOSIS 통계자료 API의 기관 ID/통계표 ID. 상세 조건은 별도 배치 버전에 고정한다.
KOSIS_ORG_ID=
KOSIS_TABLE_ID=
```

설정하지 않은 선택 데이터셋은 호출하지 않는다. 게임 서버의 시작과 플레이어 요청은 이 값들과
무관하다.

## 공급자별 사용 경계

- 공공데이터포털 상장종목정보는 공개 카탈로그의 기준이다. `basDt`, `srtnCd`, `isinCd`, `mrktCtg`,
  `itmsNm`, `crno`, `corpNm`만 정규화해 저장한다. 같은 기준일의 주식시세정보는 전체 페이지의 계약과
  행 수를 검증해 동기화 해시만 남기며 실제 가격 행을 검색 API로 전달하지 않는다.
- KRX API는 비상업적 교차 검증에만 사용하며 호출 상한과 출처표시 의무를 지킨다. KRX 원문 응답을
  제3자에게 재제공하지 않는다.
- OpenDART는 고유번호 파일을 한 번 받고 상장 법인의 기업개황을 최대 4개 동시 요청으로 읽어 업종코드를
  보강한다. 수천 회 호출될 수 있으므로 검색 요청마다 호출하지 않고 배치 결과만 저장한다.
- FMP 응답은 현재 개인용 라이선스에서 화면 표시·재배포하지 않는다. 내부 검증 결과의 상태와 해시만
  남기며, 공개 표시가 필요해지면 별도의 데이터 표시/재배포 계약을 먼저 체결한다.
- ECOS와 KOSIS 원문은 새 캘리브레이션 버전 후보를 만드는 오프라인 입력이다. 진행 중인 시장 월드를
  수정하지 않는다.

## 실행과 보안

```sh
cd server
set -a
. ./.env
set +a
cargo run --bin market-data-sync
```

로컬에서는 `server/.env.example`을 복사한 ignored `server/.env`에 실제 값을 채운 뒤 실행한다.

배치는 먼저 모든 원격 응답을 검증하고 새 버전을 저장한 뒤 마지막에 활성 assignment를 바꾼다.
로그에는 공급자명, 데이터셋, 상태, 행 수와 콘텐츠 SHA-256만 남기며 키·인증 URL·원문 응답은 남기지
않는다. HTTP 오류를 변환할 때도 인증 파라미터가 포함된 원래 URL을 오류 체인에 넣지 않는다.

키가 노출되면 해당 공급자 콘솔에서 즉시 폐기·재발급하고 `server/.env` 또는 `SERVER_ENV`만 교체한다.
DB에는 키를 저장하지 않는다.
