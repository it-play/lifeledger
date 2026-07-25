import type { EventStreamParser, SseMessage } from './types.js';

const LF = '\n';
const CR = '\r';
const BOM = '\uFEFF';
const NUL = '\u0000';

/**
 * Implementation of the WHATWG HTML "event stream" interpretation algorithm.
 * https://html.spec.whatwg.org/multipage/server-sent-events.html
 *
 * The easily-missed parts of the spec are honoured exactly:
 *  - line separators are CRLF, LF and CR alike
 *  - only one leading BOM is stripped from the stream
 *  - a line starting with `:` is a comment (used for keep-alive)
 *  - a line without a colon is a field name with an empty value
 *  - only one space after the colon is stripped; a second stays in the value
 *  - data lines join with LF, and dispatch removes only the final LF
 *  - empty data dispatches no event
 *  - an id containing NUL leaves the field untouched
 *  - lastEventId survives dispatch; only the event and data buffers reset
 *  - incomplete data at EOF is discarded
 */
interface LineBoundary {
  readonly line: string;
  readonly nextCursor: number;
  /** Buffer ended on CR, so the next chunk's leading LF is the other half of a CRLF. */
  readonly deferLf: boolean;
}

/**
 * Takes the next line from buffer[cursor..], or undefined when no separator is present.
 * All three separators (CRLF, LF, CR) are handled here and nowhere else.
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
    // The line is already complete, so yield it now. Holding it back would delay the
    // event by a chunk, or let the next LF read as a blank line.
    return { line, nextCursor: nextCr + 1, deferLf: true };
  }
  const skipLf = buffer[nextCr + 1] === LF ? 1 : 0;
  return { line, nextCursor: nextCr + 1 + skipLf, deferLf: false };
}

interface LineScan {
  readonly lines: readonly string[];
  readonly remainder: string;
  /** Whether the last line ended on CR, so the next chunk's leading LF is skipped. */
  readonly deferLf: boolean;
}

/** Takes every complete line, leaving the remainder. Stateless. */
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
  /** Text not yet terminated by a separator. */
  let pending = '';
  let bomChecked = false;
  /** Set when the previous chunk ended on CR, so one leading LF is skipped. */
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
        // An id containing NUL is ignored, keeping the previous value
        if (!value.includes(NUL)) lastEventId = value;
        break;
      case 'retry':
        // Valid only when made entirely of ASCII digits
        if (/^\d+$/.test(value)) serverRetryMs = Number.parseInt(value, 10);
        break;
      default:
        // Unknown fields are ignored
        break;
    }
  }

  function dispatch(out: SseMessage[]): void {
    if (dataBuffer === '') {
      // No data dispatches nothing; the buffers are simply cleared
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
    if (line.startsWith(':')) return; // comment
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
      // Per spec, data left at EOF is discarded rather than dispatched
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
