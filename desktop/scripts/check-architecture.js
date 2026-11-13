import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.join(desktopRoot, 'src');

export function findBridgeViolations(sources) {
  return sources
    .filter(({ relativePath, content }) => (
      relativePath !== 'api.js' && /\b__TAURI__\b|\.invoke\s*\(/.test(content)
    ))
    .map(({ relativePath }) => relativePath);
}

async function collectJavaScriptSources(directory = sourceRoot, parent = '') {
  const entries = await readdir(directory, { withFileTypes: true });
  const sources = [];

  for (const entry of entries) {
    const relativePath = path.join(parent, entry.name);
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      sources.push(...await collectJavaScriptSources(fullPath, relativePath));
    } else if (entry.isFile() && entry.name.endsWith('.js')) {
      sources.push({
        relativePath,
        content: await readFile(fullPath, 'utf8'),
      });
    }
  }

  return sources;
}

export async function checkArchitecture(sources) {
  const files = sources ?? await collectJavaScriptSources();
  const violations = findBridgeViolations(files);
  if (violations.length > 0) {
    throw new Error(`Tauri IPC must stay in src/api.js; found use in: ${violations.join(', ')}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await checkArchitecture();
  console.log('Desktop architecture check passed: Tauri IPC is confined to src/api.js.');
}
