import type { EventStreamParser, SseMessage } from './types.js';

const LF = '\n';
const CR = '\r';
const BOM = '\uFEFF';
const NUL = '\u0000';

/**
 * WHATWG HTML "event stream" 해석 알고리즘 구현.
 * https://html.spec.whatwg.org/multipage/server-sent-events.html
 *
 * 스펙에서 실수하기 쉬운 지점들을 그대로 지킨다.
 *  - 줄 구분자는 CRLF / LF / CR 세 가지 모두
 *  - 스트림 맨 앞의 BOM 하나만 제거
 *  - `:` 로 시작하는 줄은 주석 (keep-alive 로 쓰임)
 *  - 콜론 없는 줄은 필드명만 있고 값은 빈 문자열
 *  - 값 앞의 스페이스 하나만 제거 (두 개면 하나는 값에 남는다)
 *  - data 는 여러 줄이 LF 로 이어지고, dispatch 때 마지막 LF 하나만 제거
 *  - data 가 비어 있으면 이벤트를 발생시키지 않는다
 *  - id 에 NUL 이 있으면 그 필드는 무시
 *  - lastEventId 는 dispatch 후에도 유지된다 (event/data 버퍼만 초기화)
 *  - EOF 시 미완성 데이터는 폐기
 */
interface LineBoundary {
  readonly line: string;
  readonly nextCursor: number;
  /** CR 로 버퍼가 끝난 경우. 다음 청크의 첫 LF 하나를 CRLF 의 뒷짝으로 보고 건너뛰어야 한다. */
  readonly deferLf: boolean;
}

/**
 * buffer[cursor..] 에서 다음 줄 하나를 떼어낸다. 줄바꿈이 없으면 undefined.
 * 줄 구분자 세 종류(CRLF/LF/CR)를 여기 한 곳에서만 다룬다.
 */
function nextLine(buffer: string, cursor: number): LineBoundary | undefined {
  const nextLf = buffer.indexOf(LF, cursor);
  const nextCr = buffer.indexOf(CR, cursor);
  if (nextLf === -1 && nextCr === -1) return undefined;

  const crFirst = nextCr !== -1 && (nextLf === -1 || nextCr < nextLf);
  if (!crFirst) {
    return { line: buffer.slice(cursor, nextLf), nextCursor: nextLf + 1, deferLf: false };
  }

  const line = buffer.slice(cursor, nextCr);
  if (nextCr === buffer.length - 1) {
    // 줄 자체는 이미 끝났으므로 지금 넘긴다. 줄 전체를 보류하면 청크 경계에서
    // 이벤트가 한 청크 늦게 나가거나, 다음 LF 가 빈 줄로 오인된다.
    return { line, nextCursor: nextCr + 1, deferLf: true };
  }
  const skipLf = buffer[nextCr + 1] === LF ? 1 : 0;
  return { line, nextCursor: nextCr + 1 + skipLf, deferLf: false };
}

interface LineScan {
  readonly lines: readonly string[];
  readonly remainder: string;
  /** 마지막 줄이 CR 로 끝나 다음 청크의 첫 LF 를 건너뛰어야 하는지. */
  readonly deferLf: boolean;
}

/** 버퍼에서 완성된 줄을 모두 떼어내고 나머지를 남긴다. 상태를 갖지 않는다. */
function scanLines(buffer: string): LineScan {
  const lines: string[] = [];
  let cursor = 0;
  let deferLf = false;

  for (;;) {
    const boundary = nextLine(buffer, cursor);
    if (boundary === undefined) break;
    lines.push(boundary.line);
    cursor = boundary.nextCursor;
    if (boundary.deferLf) {
      deferLf = true;
      break;
    }
  }

  return { lines, remainder: buffer.slice(cursor), deferLf };
}

export function createEventStreamParser(): EventStreamParser {
  /** 아직 줄바꿈을 만나지 못한 잔여 문자열. */
  let pending = '';
  let bomChecked = false;
  /** 직전 청크가 CR 로 끝났을 때, 다음 청크 첫 LF 하나를 CRLF 의 뒷짝으로 보고 건너뛴다. */
  let skipLeadingLf = false;
  let dataBuffer = '';
  let eventTypeBuffer = '';
  let lastEventId = '';
  let serverRetryMs: number | undefined;

  function processField(field: string, value: string): void {
    switch (field) {
      case 'event':
        eventTypeBuffer = value;
        break;
      case 'data':
        dataBuffer += value + LF;
        break;
      case 'id':
        // NUL 이 포함된 id 는 무시한다 (기존 값 유지)
        if (!value.includes(NUL)) lastEventId = value;
        break;
      case 'retry':
        // ASCII 숫자로만 이루어진 경우에만 유효
        if (/^\d+$/.test(value)) serverRetryMs = Number.parseInt(value, 10);
        break;
      default:
        // 알 수 없는 필드는 무시
        break;
    }
  }

  function dispatch(out: SseMessage[]): void {
    if (dataBuffer === '') {
      // 데이터가 없으면 이벤트를 발생시키지 않고 버퍼만 비운다
      eventTypeBuffer = '';
      return;
    }
    const data = dataBuffer.endsWith(LF) ? dataBuffer.slice(0, -1) : dataBuffer;
    out.push({
      type: eventTypeBuffer === '' ? 'message' : eventTypeBuffer,
      data,
      lastEventId,
    });
    dataBuffer = '';
    eventTypeBuffer = '';
  }

  function processLine(line: string, out: SseMessage[]): void {
    if (line === '') {
      dispatch(out);
      return;
    }
    if (line.startsWith(':')) return; // 주석
    const colon = line.indexOf(':');
    if (colon === -1) {
      processField(line, '');
      return;
    }
    const field = line.slice(0, colon);
    let value = line.slice(colon + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    processField(field, value);
  }

  return {
    push(chunk) {
      if (chunk === '') return [];
      let input = chunk;
      if (!bomChecked) {
        bomChecked = true;
        if (input.startsWith(BOM)) input = input.slice(1);
      }
      if (skipLeadingLf) {
        skipLeadingLf = false;
        if (input.startsWith(LF)) input = input.slice(1);
      }
      const scan = scanLines(pending + input);
      pending = scan.remainder;
      skipLeadingLf = scan.deferLf;

      const out: SseMessage[] = [];
      for (const line of scan.lines) processLine(line, out);
      return out;
    },

    end() {
      // 스펙: EOF 시 남은 데이터는 폐기한다 (dispatch 하지 않는다)
      pending = '';
      dataBuffer = '';
      eventTypeBuffer = '';
      bomChecked = false;
      skipLeadingLf = false;
    },

    get lastEventId() {
      return lastEventId;
    },

    get serverRetryMs() {
      return serverRetryMs;
    },
  };
}
