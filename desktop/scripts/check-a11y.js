import axe from 'axe-core';
import { JSDOM } from 'jsdom';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pagePath = path.join(desktopRoot, 'src', 'index.html');
const stylesPath = path.join(desktopRoot, 'src', 'styles.css');
const contrastPairs = [
  { foreground: 'ink', background: 'paper' },
  { foreground: 'ink-soft', background: 'paper' },
  { foreground: 'ink-faint', background: 'paper' },
  { foreground: 'paper-raised', background: 'signal' },
  { foreground: 'ok', background: 'paper' },
  { foreground: 'signal', background: 'paper' },
];

function linearize(channel) {
  const value = channel / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

function luminance(color) {
  const channels = color.match(/[0-9a-f]{2}/gi).map((channel) => (
    linearize(Number.parseInt(channel, 16))
  ));
  return (0.2126 * channels[0]) + (0.7152 * channels[1]) + (0.0722 * channels[2]);
}

export function contrastRatio(firstColor, secondColor) {
  const firstLuminance = luminance(firstColor);
  const secondLuminance = luminance(secondColor);
  const lighter = Math.max(firstLuminance, secondLuminance);
  const darker = Math.min(firstLuminance, secondLuminance);
  return (lighter + 0.05) / (darker + 0.05);
}

export function findContrastFailures(styles, minimumRatio = 4.5) {
  const colors = new Map(
    [...styles.matchAll(/--([\w-]+)\s*:\s*(#[0-9a-f]{6})/gi)]
      .map(([, token, color]) => [token, color]),
  );
  const missingTokens = contrastPairs.flatMap(({ foreground, background }) => (
    [foreground, background].filter((token) => !colors.has(token))
  ));

  if (missingTokens.length > 0) {
    throw new Error(`Missing contrast tokens: ${[...new Set(missingTokens)].join(', ')}`);
  }

  return contrastPairs
    .map(({ foreground, background }) => ({
      foreground,
      background,
      ratio: contrastRatio(colors.get(foreground), colors.get(background)),
    }))
    .filter(({ ratio }) => ratio < minimumRatio);
}

export async function findSeriousAccessibilityViolations(html, styles = '') {
  const styledPage = html.replace(
    '<link rel="stylesheet" href="./styles.css">',
    `<style>${styles}</style>`,
  );
  const page = styledPage.replace(
    '<script type="module" src="./main.js"></script>',
    '',
  );
  const dom = new JSDOM(page, { runScripts: 'outside-only', pretendToBeVisual: true });

  try {
    dom.window.eval(axe.source);
    const results = await dom.window.axe.run(dom.window.document, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'],
      },
      rules: {
        'color-contrast': { enabled: false },
      },
    });
    return results.violations.filter(({ impact }) => (
      impact === 'critical' || impact === 'serious'
    ));
  } finally {
    dom.window.close();
  }
}

export async function checkAccessibility() {
  const [html, styles] = await Promise.all([
    readFile(pagePath, 'utf8'),
    readFile(stylesPath, 'utf8'),
  ]);
  const violations = await findSeriousAccessibilityViolations(html, styles);
  if (violations.length > 0) {
    const summary = violations.map(({ id, help }) => `${id}: ${help}`).join('; ');
    throw new Error(`Critical or serious accessibility findings: ${summary}`);
  }

  const contrastFailures = findContrastFailures(styles);
  if (contrastFailures.length > 0) {
    const summary = contrastFailures
      .map(({ foreground, background, ratio }) => `${foreground}/${background}: ${ratio.toFixed(2)}:1`)
      .join('; ');
    throw new Error(`Text color contrast is below 4.5:1: ${summary}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await checkAccessibility();
  console.log('Accessibility check passed: no critical/serious axe-core or text contrast findings.');
}
