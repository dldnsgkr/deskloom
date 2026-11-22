import { describe, it, expect } from 'vitest';
import { execFileSync } from 'node:child_process';

const awk = (file: string, vars: Record<string, string | number>, input = '/dev/null') =>
  execFileSync('awk', [...Object.entries(vars).flatMap(([k, v]) => ['-v', `${k}=${v}`]), '-f', file, input], { encoding: 'utf8' }).trim();

// awk is a macOS measurement tool; the Windows runner may not have it.
describe.skipIf(process.platform === 'win32')('parse-top.awk', () => {
  it('sums the given pids per sample, skips the first sample, ignores other pids', () => {
    // 표본 2: cpu 3.0, mem 100+40=140 / 표본 3: cpu 4.0, mem 1024+50=1074
    expect(awk('scripts/perf/parse-top.awk', { pids: '101,202' }, 'scripts/perf/fixtures/top-sample.txt'))
      .toBe('avg_cpu=3.50 max_mem_mb=1074.0');
  });
});

describe.skipIf(process.platform === 'win32')('verdict.awk', () => {
  const v = (state: string, cpu: number, mem: number) => awk('scripts/perf/verdict.awk', { state, cpu, mem });
  it('paused needs cpu strictly under 1%', () => { expect(v('paused', 0.99, 120)).toBe('PASS'); expect(v('paused', 1.0, 120)).toBe('FAIL'); });
  it('ac allows up to 8%', () => { expect(v('ac', 8.0, 120)).toBe('PASS'); expect(v('ac', 8.01, 120)).toBe('FAIL'); });
  it('battery allows up to 4%', () => { expect(v('battery', 4.0, 120)).toBe('PASS'); expect(v('battery', 4.01, 120)).toBe('FAIL'); });
  it('memory allows up to 200MB in every state', () => { expect(v('ac', 1, 200)).toBe('PASS'); expect(v('paused', 0.5, 200.1)).toBe('FAIL'); });
  it('an unknown state fails', () => { expect(v('turbo', 0, 0)).toBe('FAIL'); });
});
