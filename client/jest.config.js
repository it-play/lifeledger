/**
 * 단위 테스트만 돌린다 (AGENTS.md 의 테스트 정책 참고).
 * ESM 소스를 @swc/jest 로 변환하고, `./x.js` 형태의 확장자 있는 상대 경로를 .ts 로 되돌린다.
 */
module.exports = {
  testEnvironment: 'node',
  roots: ['<rootDir>/src'],
  testMatch: ['**/*.test.ts'],
  transform: {
    '^.+\\.ts$': ['@swc/jest', { jsc: { target: 'es2022', parser: { syntax: 'typescript' } } }],
  },
  moduleNameMapper: {
    '^(\\.{1,2}/.*)\\.js$': '$1',
  },
  clearMocks: true,
};
