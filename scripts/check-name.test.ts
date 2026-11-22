import { describe, it, expect } from 'vitest';
import { findNameLeaks } from './check-name.mjs';

const allowed = ['src-tauri/tauri.conf.json', 'docs/', 'CLAUDE.md', 'README.md'];

describe('findNameLeaks', () => {
  it('flags a file outside the allowed list that contains the name, ignoring case', () => {
    expect(findNameLeaks([{ path: 'src/lab/main.ts', content: 'const t = "Deskloom"' }], 'deskloom', allowed))
      .toEqual(['src/lab/main.ts']);
  });
  it('allows exact files and folder prefixes', () => {
    expect(findNameLeaks([
      { path: 'src-tauri/tauri.conf.json', content: '"productName": "Deskloom"' },
      { path: 'docs/superpowers/specs/x.md', content: 'deskloom' },
    ], 'deskloom', allowed)).toEqual([]);
  });
  it('does not treat a file named like an allowed folder as allowed', () => {
    expect(findNameLeaks([{ path: 'docs.ts', content: 'deskloom' }], 'deskloom', allowed)).toEqual(['docs.ts']);
  });
});
