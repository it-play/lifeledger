import type { Disposable, Unsubscribe } from './types.js';

/**
 * 정리 함수를 모아 한 번에 해제한다.
 * 화면(view)이나 클라이언트가 자기 구독을 흘리지 않도록 하는 기본 도구.
 */
export interface DisposableBag extends Disposable {
  add(cleanup: Unsubscribe | Disposable): void;
  readonly size: number;
  readonly disposed: boolean;
}

export function createDisposableBag(): DisposableBag {
  const cleanups = new Set<Unsubscribe>();
  let disposed = false;

  return {
    add(cleanup) {
      const fn = typeof cleanup === 'function' ? cleanup : () => cleanup.dispose();
      // 이미 정리된 뒤에 들어온 자원은 즉시 해제해 누수를 막는다
      if (disposed) {
        fn();
        return;
      }
      cleanups.add(fn);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      // 등록 역순으로 해제한다 (의존 관계가 있는 자원 대비)
      for (const fn of [...cleanups].reverse()) {
        try {
          fn();
        } catch {
          // 하나가 실패해도 나머지는 정리한다
        }
      }
      cleanups.clear();
    },
    get size() {
      return cleanups.size;
    },
    get disposed() {
      return disposed;
    },
  };
}
