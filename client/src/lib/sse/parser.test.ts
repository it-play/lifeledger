import { describe, expect, it } from '@jest/globals';
import { createEventStreamParser } from './parser.js';
import type { EventStreamParser, SseMessage } from './types.js';

/**
 * 대상: event stream 파서 (핵심 로직)
 * 구조: Data(스트림 조각) — Context(어떤 형태로 도착했는가) — Interaction(파서가 어떻게 해석하는가)
 */

interface ParseOutcome {
  readonly parser: EventStreamParser;
  readonly messages: readonly SseMessage[];
}

/** given: 서버가 보낸 청크들 / when: 파서에 순서대로 넣는다 */
function whenParsing(chunks: readonly string[]): ParseOutcome {
  const parser = createEventStreamParser();
  const messages = chunks.flatMap((chunk) => [...parser.push(chunk)]);
  return { parser, messages };
}

describe('event stream 파서', () => {
  describe('맥락: 온전한 이벤트 하나가 도착한 경우', () => {
    it('given data 한 줄과 빈 줄, when 파싱하면, then message 타입 이벤트가 나온다', () => {
      const { messages } = whenParsing(['data: hello\n\n']);

      expect(messages).toEqual([{ type: 'message', data: 'hello', lastEventId: '' }]);
    });

    it('given event 필드, when 파싱하면, then 그 값이 이벤트 타입이 된다', () => {
      const { messages } = whenParsing(['event: tick\ndata: 1\n\n']);

      expect(messages[0]?.type).toBe('tick');
    });
  });

  describe('맥락: 필드 값을 해석하는 규칙', () => {
    it('given data 여러 줄, when 파싱하면, then LF 로 이어지고 마지막 LF 만 제거된다', () => {
      const { messages } = whenParsing(['data: a\ndata: b\ndata:\n\n']);

      expect(messages[0]?.data).toBe('a\nb\n');
    });

    it('given 값 앞 스페이스 두 개, when 파싱하면, then 하나만 제거된다', () => {
      const { messages } = whenParsing(['data:  두칸\n\n']);

      expect(messages[0]?.data).toBe(' 두칸');
    });

    it('given 콜론 없는 줄, when 파싱하면, then 필드명만 있고 값은 빈 문자열로 처리된다', () => {
      const { messages } = whenParsing(['data\ndata: x\n\n']);

      expect(messages[0]?.data).toBe('\nx');
    });

    it('given 콜론으로 시작하는 주석(keep-alive), when 파싱하면, then 무시된다', () => {
      const { messages } = whenParsing([': ping\n\ndata: real\n\n']);

      expect(messages).toHaveLength(1);
      expect(messages[0]?.data).toBe('real');
    });

    it('given data 없는 이벤트, when 파싱하면, then 이벤트가 발생하지 않는다', () => {
      const { messages } = whenParsing(['event: tick\n\n']);

      expect(messages).toHaveLength(0);
    });
  });

  describe('맥락: 줄바꿈이 세 가지로 섞여 오는 경우', () => {
    it('given CRLF · CR · LF, when 파싱하면, then 모두 줄바꿈으로 처리된다', () => {
      expect(whenParsing(['data: a\r\n\r\n']).messages[0]?.data).toBe('a');
      expect(whenParsing(['data: b\r\r']).messages[0]?.data).toBe('b');
      expect(whenParsing(['data: c\n\n']).messages[0]?.data).toBe('c');
    });

    it('given CRLF 가 청크 경계에 걸린 경우, when 파싱하면, then 빈 줄이 새로 생기지 않는다', () => {
      const { messages } = whenParsing(['data: split\r', '\ndata: more\r\n\r\n']);

      expect(messages).toHaveLength(1);
      expect(messages[0]?.data).toBe('split\nmore');
    });

    it('given 임의 위치에서 쪼갠 청크들, when 파싱하면, then 결과가 온전한 경우와 같다', () => {
      const whole = 'event: tick\nid: 7\ndata: {"a":1}\n\n';

      for (let cut = 1; cut < whole.length; cut += 1) {
        const { messages } = whenParsing([whole.slice(0, cut), whole.slice(cut)]);

        expect(messages).toEqual([{ type: 'tick', data: '{"a":1}', lastEventId: '7' }]);
      }
    });

    it('given 맨 앞 BOM, when 파싱하면, then 하나만 제거된다', () => {
      const { messages } = whenParsing(['\uFEFFdata: x\n\n']);

      expect(messages[0]?.data).toBe('x');
    });
  });

  describe('맥락: 재연결에 필요한 상태를 유지하는 경우', () => {
    it('given id 를 받은 뒤 다음 이벤트, when 파싱하면, then lastEventId 가 유지된다', () => {
      const { parser, messages } = whenParsing(['id: 42\ndata: a\n\n', 'data: b\n\n']);

      expect(messages[1]?.lastEventId).toBe('42');
      expect(parser.lastEventId).toBe('42');
    });

    it('given NUL 이 든 id, when 파싱하면, then 그 필드는 무시된다', () => {
      const { parser } = whenParsing(['id: 1\ndata: a\n\n', 'id: bad\u0000id\ndata: b\n\n']);

      expect(parser.lastEventId).toBe('1');
    });

    it('given retry 값, when 숫자면 채택하고 아니면 무시한다', () => {
      expect(whenParsing(['retry: 5000\n\n']).parser.serverRetryMs).toBe(5000);
      expect(whenParsing(['retry: 3s\n\n']).parser.serverRetryMs).toBeUndefined();
    });
  });

  describe('맥락: 스트림이 중간에 끊긴 경우', () => {
    it('given 미완성 이벤트, when end() 를 호출하면, then 남은 데이터는 폐기된다', () => {
      const parser = createEventStreamParser();
      parser.push('data: incomplete\n');

      parser.end();

      expect(parser.push('\n')).toHaveLength(0);
    });
  });
});
