/**
 * Minimal DOM helpers, with no virtual DOM. The render convention here is: build with
 * `el()`, then update only the nodes that actually changed.
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

/** Text-only binding that leaves the DOM alone when the value is unchanged. */
export function bindText(node: Node): (text: string) => void {
  let last: string | undefined;
  return (text) => {
    if (text === last) return;
    last = text;
    node.textContent = text;
  };
}

/** Attaches a listener and returns its remover, for registering with a bag. */
export function on<K extends keyof HTMLElementEventMap>(
  target: HTMLElement,
  type: K,
  handler: (event: HTMLElementEventMap[K]) => void,
): () => void {
  target.addEventListener(type, handler as EventListener);
  return () => target.removeEventListener(type, handler as EventListener);
}
