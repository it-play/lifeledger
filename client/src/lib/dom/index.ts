/**
 * 최소 DOM 헬퍼. 가상 DOM 을 만들지 않는다 —
 * "만들 때는 el(), 바꿀 때는 바꿀 노드만 직접 갱신" 이 이 프로젝트의 렌더 규약이다.
 */

type Child = Node | string | null | undefined | false;

export interface ElementAttributes {
  readonly class?: string;
  readonly id?: string;
  readonly type?: string;
  readonly name?: string;
  readonly value?: string;
  readonly href?: string;
  readonly disabled?: boolean;
  readonly dataset?: Readonly<Record<string, string>>;
  readonly attrs?: Readonly<Record<string, string>>;
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: ElementAttributes = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { dataset, attrs, disabled, ...rest } = attributes;

  for (const [key, value] of Object.entries(rest)) {
    if (value === undefined) continue;
    if (key === 'class') node.className = String(value);
    else node.setAttribute(key, String(value));
  }
  if (disabled === true) node.setAttribute('disabled', '');
  for (const [key, value] of Object.entries(dataset ?? {})) node.dataset[key] = value;
  for (const [key, value] of Object.entries(attrs ?? {})) node.setAttribute(key, value);

  append(node, children);
  return node;
}

export function append(parent: Node, children: readonly Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    parent.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
  }
}

/** 텍스트만 바꾸는 바인딩. 값이 같으면 DOM 을 건드리지 않는다. */
export function bindText(node: Node): (text: string) => void {
  let last: string | undefined;
  return (text) => {
    if (text === last) return;
    last = text;
    node.textContent = text;
  };
}

/** 이벤트 리스너를 붙이고 해제 함수를 돌려준다 (bag 에 넣어 쓴다). */
export function on<K extends keyof HTMLElementEventMap>(
  target: HTMLElement,
  type: K,
  handler: (event: HTMLElementEventMap[K]) => void,
): () => void {
  target.addEventListener(type, handler as EventListener);
  return () => target.removeEventListener(type, handler as EventListener);
}
