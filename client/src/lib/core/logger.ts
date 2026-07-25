import type { Logger, LogLevel } from './types.js';

const LEVEL_ORDER: Record<LogLevel, number> = { debug: 10, info: 20, warn: 30, error: 40 };

export interface ConsoleLoggerOptions {
  readonly minLevel?: LogLevel;
  readonly scope?: string;
}

/** 개발용 기본 구현. 운영에서 다른 싱크로 교체할 수 있다. */
export function createConsoleLogger(options: ConsoleLoggerOptions = {}): Logger {
  const minLevel = options.minLevel ?? 'info';
  const scope = options.scope ?? '';

  const logger: Logger = {
    log(level, message, context) {
      if (LEVEL_ORDER[level] < LEVEL_ORDER[minLevel]) return;
      const prefix = scope === '' ? '' : `[${scope}] `;
      const line = `${prefix}${message}`;
      const method = level === 'debug' ? 'log' : level;
      if (context === undefined) console[method](line);
      else console[method](line, context);
    },
    child(childScope) {
      return createConsoleLogger({
        minLevel,
        scope: scope === '' ? childScope : `${scope}/${childScope}`,
      });
    },
  };

  return logger;
}

/** 테스트나 로깅을 끄고 싶을 때. */
export function createNullLogger(): Logger {
  const logger: Logger = {
    log() {},
    child() {
      return logger;
    },
  };
  return logger;
}
