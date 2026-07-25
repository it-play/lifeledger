# 홈서버(Mac mini, arm64)에서 네이티브로 빌드한다. 크로스 컴파일 없음.
FROM rust:1.97.1-slim-bookworm AS builder

WORKDIR /build

# 의존성만 먼저 빌드해 레이어로 굳힌다 — 소스만 바뀐 배포는 이 레이어를 재사용한다
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --locked \
 && rm -rf src target/release/lifeledger-server target/release/deps/lifeledger_server*

COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim

# ca-certificates: sqlx rustls 가 TLS 검증에 쓴다 / curl: 헬스체크
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --uid 10001 app
USER app
WORKDIR /home/app

COPY --from=builder /build/target/release/lifeledger-server /usr/local/bin/lifeledger-server

# 컨테이너 안에서는 모든 인터페이스에 바인드한다 (기본값은 127.0.0.1 이라 밖에서 못 붙는다)
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

CMD ["lifeledger-server"]
