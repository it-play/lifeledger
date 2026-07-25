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

/** Builds the next state by shallow-copying only the objects along the path. */
export function setAtPath<S>(state: S, path: StatePath, value: unknown): S {
  const keys = splitPath(path);
  if (keys.length === 0) return value as S;

  const head = keys[0];
  if (head === undefined) return state;

  if (!isPlainObject(state)) throw new TypeError(`state cannot accept path: ${path}`);
  const rest = keys.slice(1).join('.');
  const nextChild = rest === '' ? value : setAtPath(state[head] ?? {}, rest, value);
  if (state[head] === nextChild) return state;
  return { ...state, [head]: nextChild } as S;
}

/**
 * Diffs two states into the list of paths that changed.
 * Identical references stop the walk, which is close to free given structural sharing.
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
  // Unchanged children mean this node is unchanged too
  return changed;
}

/**
 * Whether a subscribed path and a changed path affect each other. Subscribing to `'a'`
 * must react to `'a.b'` changing, and subscribing to `'a.b'` to `'a'` being replaced.
 */
export function pathsIntersect(watched: StatePath, changed: StatePath): boolean {
  if (watched === '' || changed === '') return true;
  if (watched === changed) return true;
  return watched.startsWith(`${changed}.`) || changed.startsWith(`${watched}.`);
}
