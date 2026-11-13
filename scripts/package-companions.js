import fs from 'node:fs';
import path from 'node:path';
import { COMPANIONS } from '../gui/companions.js';

const root = path.resolve('.');
const companionsDir = path.join(root, 'assets', 'companions');
const distDir = path.join(root, 'dist', 'companions');

if (!fs.existsSync(distDir)) {
  fs.mkdirSync(distDir, { recursive: true });
}

console.log(`Preparing metadata for ${COMPANIONS.length} catalog companions...`);

for (const comp of COMPANIONS) {
  const targetDir = path.join(companionsDir, comp.id);
  if (!fs.existsSync(targetDir)) {
    console.warn(`Warning: Directory not found for ${comp.id}`);
    continue;
  }

  // 1. Desktop manifest.json
  const manifest = {
    id: comp.id,
    name: comp.name,
    description: comp.description,
    tags: comp.tags,
    isCustom: false,
    isInstalled: true,
    avatarType: 'clips',
    actions: ['idle', 'walk', 'wave', 'water', 'stretch', 'happy', 'sad', 'eyes'],
    clips: {
      idle: 'idle.webp',
      walk: 'walk.webp',
      wave: 'wave.webp',
      water: 'water.webp',
      stretch: 'stretch.webp',
      happy: 'happy.webp',
      sad: 'sad.webp',
      eyes: 'eyes.webp',
    },
  };
  fs.writeFileSync(
    path.join(targetDir, 'manifest.json'),
    JSON.stringify(manifest, null, 2)
  );

  // 2. Core CLI pet.json
  const petJson = {
    name: comp.name,
    author: 'AMG',
    version: 1,
    license: 'CC0',
    anchor: 'bottom',
    clips: {
      idle: 'idle.webp',
      walk: 'walk.webp',
      wave: 'wave.webp',
      water: 'water.webp',
      stretch: 'stretch.webp',
      happy: 'happy.webp',
      sad: 'sad.webp',
      eyes: 'eyes.webp',
    },
  };
  fs.writeFileSync(
    path.join(targetDir, 'pet.json'),
    JSON.stringify(petJson, null, 2)
  );
}

console.log('Companion metadata files generated successfully.');
