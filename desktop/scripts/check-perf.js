import { stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MAX_BINARY_SIZE_BYTES = 5 * 1024 * 1024; // 5 MB threshold (currently ~2.58 MB)
const MAX_FRONTEND_SIZE_BYTES = 200 * 1024; // 200 KB threshold (currently ~10 KB)

export async function checkPerformance() {
  const binaryName = process.platform === 'win32' ? 'tether-desktop.exe' : 'tether-desktop';
  const binaryPath = path.join(desktopRoot, 'src-tauri', 'target', 'release', binaryName);

  // 1. Frontend assets budget
  const frontendFiles = ['index.html', 'main.js', 'api.js', 'styles.css'];
  let totalFrontendSize = 0;
  for (const file of frontendFiles) {
    const s = await stat(path.join(desktopRoot, 'src', file));
    totalFrontendSize += s.size;
  }

  if (totalFrontendSize > MAX_FRONTEND_SIZE_BYTES) {
    throw new Error(
      `Frontend assets exceeded budget: ${totalFrontendSize} bytes > ${MAX_FRONTEND_SIZE_BYTES} bytes`
    );
  }

  // 2. Release binary budget (if release binary exists)
  let binarySize = null;
  try {
    const s = await stat(binaryPath);
    binarySize = s.size;
    if (binarySize > MAX_BINARY_SIZE_BYTES) {
      throw new Error(
        `Desktop release binary exceeded budget: ${binarySize} bytes > ${MAX_BINARY_SIZE_BYTES} bytes`
      );
    }
  } catch (err) {
    if (err.code !== 'ENOENT') {
      throw err;
    }
  }

  return {
    frontendSize: totalFrontendSize,
    binarySize,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const res = await checkPerformance();
  const binaryMsg = res.binarySize
    ? `Release binary: ${(res.binarySize / (1024 * 1024)).toFixed(2)} MB (budget <= 5.0 MB).`
    : 'Release binary not yet built.';
  console.log(
    `Desktop performance check passed: Frontend assets: ${(res.frontendSize / 1024).toFixed(1)} KB. ${binaryMsg}`
  );
}
