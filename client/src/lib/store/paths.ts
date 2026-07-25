import type { StatePath } from './types.js';

type Unknown = Record<string, unknown>;

const isPlainObject = (v: unknown): v is Unknown =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** `'a.b.c'` → `['a','b','c']` */
export const splitPath = (path: StatePath): readonly string[] =>
  path === '' ? [] : path.split('.');

export function getAtPath(state: unknown, path: StatePath): unknown {
  let cursor: unknown = state;
  for (const key of splitPath(path)) {
    if (!isPlainObject(cursor)) return undefined;
    cursor = cursor[key];
  }
  return cursor;
}

/** 경로상의 객체만 얕게 복사해 새 상태를 만든다 (구조 공유). */
export function setAtPath<S>(state: S, path: StatePath, value: unknown): S {
  const keys = splitPath(path);
  if (keys.length === 0) return value as S;

  const head = keys[0];
  if (head === undefined) return state;

  if (!isPlainObject(state)) throw new TypeError(`경로를 적용할 수 없는 상태: ${path}`);
  const rest = keys.slice(1).join('.');
  const nextChild = rest === '' ? value : setAtPath(state[head] ?? {}, rest, value);
  if (state[head] === nextChild) return state;
  return { ...state, [head]: nextChild } as S;
}

/**
 * 두 상태를 비교해 바뀐 경로 목록을 만든다.
 * 참조가 같으면 하위는 보지 않는다 — 구조 공유가 되어 있으면 비용이 거의 없다.
 */
export function diffPaths(prev: unknown, next: unknown, prefix = ''): readonly StatePath[] {
  if (prev === next) return [];
  if (!isPlainObject(prev) || !isPlainObject(next)) return [prefix];

  const changed: StatePath[] = [];
  const keys = new Set([...Object.keys(prev), ...Object.keys(next)]);
  for (const key of keys) {
    const childPath = prefix === '' ? key : `${prefix}.${key}`;
    changed.push(...diffPaths(prev[key], next[key], childPath));
  }
  // 하위가 전부 같으면 이 노드도 바뀌지 않은 것으로 본다
  return changed;
}

/**
 * 구독 경로와 변경 경로가 서로 영향을 주는지 판단한다.
 * `'a'` 구독은 `'a.b'` 변경에 반응하고, `'a.b'` 구독도 `'a'` 교체에 반응해야 한다.
 */
export function pathsIntersect(watched: StatePath, changed: StatePath): boolean {
  if (watched === '' || changed === '') return true;
  if (watched === changed) return true;
  return watched.startsWith(`${changed}.`) || changed.startsWith(`${watched}.`);
}
