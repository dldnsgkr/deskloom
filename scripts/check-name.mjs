import { execSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const ALLOWED = [
  'src-tauri/tauri.conf.json',
  'scripts/check-name.test.ts',
  'docs/',
  'CLAUDE.md',
  'README.md',
];
const BINARY = /\.(png|ico|icns)$/i;

/** 허용 목록 밖의 파일 중 이름(대소문자 무시)이 들어 있는 경로를 돌려준다. */
export function findNameLeaks(files, name, allowed) {
  const needle = name.toLowerCase();
  return files
    .filter(({ path }) => !allowed.some((a) => (a.endsWith('/') ? path.startsWith(a) : path === a)))
    .filter(({ content }) => content.toLowerCase().includes(needle))
    .map(({ path }) => path);
}

function main() {
  const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
  const name = String(conf.productName).toLowerCase();
  const paths = execSync('git ls-files --cached --others --exclude-standard', { encoding: 'utf8' })
    .split('\n')
    .filter((p) => p && !BINARY.test(p) && existsSync(p));
  const files = paths.map((path) => ({ path, content: readFileSync(path, 'utf8') }));
  const leaks = findNameLeaks(files, name, ALLOWED);
  if (leaks.length > 0) {
    console.error(`"${name}" 이름이 허용 목록 밖의 파일에 있다:`);
    for (const p of leaks) console.error(`  ${p}`);
    process.exit(1);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
