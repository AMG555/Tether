// Companion catalog for Tether.
//
// Each entry is a companion that ships with the app or is available as a free download.
// The lion king companion ships separately and users can supply their own custom companions.
//
// Fields:
//   id          URL slug; matches the asset path under assets/companions/<id>/
//   name        Display name shown in the picker and pet label
//   description One-line character description shown beneath the name
//   tags        Coarse categories used for filtering (array of strings)
//
// Asset convention:
//   Animated clip  → assets/companions/<id>/<state>.webp
//   Still preview  → assets/companions/<id>/<state>.still.webp
//   States: idle · walk · wave · water · stretch · eyes · happy · sad

/** @type {Array<{id:string, name:string, description:string, tags:string[]}>} */
export const COMPANIONS = [
  {
    id: 'grace',
    name: 'Grace',
    description: 'A warm, grounding presence — she will remind you to drink water and rest.',
    tags: ['human', 'adult', 'calm'],
  },
  {
    id: 'rory',
    name: 'Rory',
    description: 'The lion cub heir — small, boisterous, and already convinced he runs the place.',
    tags: ['animal', 'cub', 'playful'],
  },
  {
    id: 'finn',
    name: 'Finn',
    description: 'A football kid who takes every break as a chance to practice his step-overs.',
    tags: ['human', 'kid', 'sporty'],
  },
  {
    id: 'ollie',
    name: 'Ollie',
    description: 'The class bookworm — always mid-chapter, always has a fun fact ready.',
    tags: ['human', 'kid', 'studious'],
  },
  {
    id: 'blaze',
    name: 'Blaze',
    description: 'A pint-sized superhero who takes your water reminders very seriously.',
    tags: ['human', 'kid', 'hero'],
  },
  {
    id: 'axel',
    name: 'Axel',
    description: 'A skater boy who grinds deadlines the same way he grinds rails.',
    tags: ['human', 'kid', 'sporty'],
  },
  {
    id: 'spike',
    name: 'Spike',
    description: 'A jackfruit with opinions — slow to ripen, but worth the wait.',
    tags: ['food', 'whimsical', 'calm'],
  },
  {
    id: 'zippy',
    name: 'Zippy',
    description: 'A baby monkey who cannot sit still and will not let you sit still either.',
    tags: ['animal', 'baby', 'playful'],
  },
  {
    id: 'splash',
    name: 'Splash',
    description: 'A single water drop on a mission — your most single-minded hydration coach.',
    tags: ['nature', 'whimsical', 'water'],
  },
  {
    id: 'marmalade',
    name: 'Marmalade',
    description: 'An orange cat who treats every stretch reminder as a personal request.',
    tags: ['animal', 'cat', 'calm'],
  },
  {
    id: 'theo',
    name: 'Theo',
    description: 'A curious kid who asks "why?" about everything, including your posture.',
    tags: ['human', 'kid', 'curious'],
  },
  {
    id: 'bolt',
    name: 'Bolt',
    description: 'A mini robot that runs on hydration data and your good intentions.',
    tags: ['robot', 'tech', 'calm'],
  },
  {
    id: 'captain-dew',
    name: 'Captain Dew',
    description: 'A water superhero whose only power is showing up every time you forget to hydrate.',
    tags: ['human', 'hero', 'water'],
  },
  {
    id: 'shadow',
    name: 'Shadow',
    description: 'A ninja cat who appears silently, reminds you, and vanishes before you blink.',
    tags: ['animal', 'cat', 'playful'],
  },
  {
    id: 'nova',
    name: 'Nova',
    description: 'An astronaut on a solo mission to keep you hydrated across the solar system.',
    tags: ['human', 'hero', 'sci-fi'],
  },
  {
    id: 'ember',
    name: 'Ember',
    description: 'A small dragon who breathes not fire but gentle nudges to drink water.',
    tags: ['fantasy', 'dragon', 'calm'],
  },
  {
    id: 'bao',
    name: 'Bao',
    description: 'A panda chef who insists a good work session starts with a full glass.',
    tags: ['animal', 'panda', 'calm'],
  },
  {
    id: 'peanut',
    name: 'Peanut',
    description: 'A baby elephant with an excellent memory — especially for missed water breaks.',
    tags: ['animal', 'elephant', 'playful'],
  },
  {
    id: 'coco',
    name: 'Coco',
    description: 'A coconut who rolls in with refreshment and refuses to roll out until you hydrate.',
    tags: ['food', 'whimsical', 'water'],
  },
  {
    id: 'nimbus',
    name: 'Nimbus',
    description: 'A little rain cloud who thinks every dry throat is a personal affront.',
    tags: ['nature', 'whimsical', 'water'],
  },
  {
    id: 'pixel',
    name: 'Pixel',
    description: 'A night-owl coder who deep-focuses on staying hydrated through long builds.',
    tags: ['human', 'adult', 'tech'],
  },
  {
    id: 'zen',
    name: 'Zen',
    description: 'A yoga turtle who takes the long view — consistent breaks beat burnout every time.',
    tags: ['animal', 'turtle', 'calm'],
  },
  {
    id: 'doc',
    name: 'Doc',
    description: 'A bunny doctor who writes one prescription: eight glasses a day.',
    tags: ['animal', 'rabbit', 'calm'],
  },
  {
    id: 'waddle',
    name: 'Waddle',
    description: 'A baby penguin who slides in just when you are about to skip your water break.',
    tags: ['animal', 'penguin', 'playful'],
  },
  {
    id: 'penny',
    name: 'Penny',
    description: 'A schoolgirl who colour-codes her study breaks and absolutely yours too.',
    tags: ['human', 'kid', 'studious'],
  },
  {
    id: 'daisy',
    name: 'Daisy',
    description: 'A bright, bubbly girl who dances every time you close a tab.',
    tags: ['human', 'kid', 'playful'],
  },
  {
    id: 'iris',
    name: 'Iris',
    description: 'A coder girl who ships features and water reminders with equal discipline.',
    tags: ['human', 'kid', 'tech'],
  },
  {
    id: 'twirl',
    name: 'Twirl',
    description: 'A ballerina who turns every stretch reminder into a five-second performance.',
    tags: ['human', 'kid', 'sporty'],
  },
  {
    id: 'luna',
    name: 'Luna',
    description: 'A moon fairy who keeps your circadian rhythm honest when you work late.',
    tags: ['fantasy', 'fairy', 'calm'],
  },
  {
    id: 'sage',
    name: 'Sage',
    description: 'A yoga girl who demonstrates every posture reminder with worrying enthusiasm.',
    tags: ['human', 'kid', 'sporty'],
  },
  {
    id: 'chip',
    name: 'Chip',
    description: 'A cricket boy who calls every water break a mandatory drinks interval.',
    tags: ['human', 'kid', 'sporty'],
  },
  {
    id: 'quest',
    name: 'Quest',
    description: 'A gamer boy who treats the reminder bell like an achievement unlocked.',
    tags: ['human', 'kid', 'tech'],
  },
];

/** Base path for companion assets, relative to the app root. */
export const COMPANION_BASE_URL = 'assets/companions';

/**
 * Returns the animated WebP URL for a given companion state.
 * @param {string} id - Companion slug
 * @param {string} state - Animation state (idle|walk|wave|water|stretch|eyes|happy|sad)
 * @returns {string}
 */
export function clipUrl(id, state) {
  return `${COMPANION_BASE_URL}/${id}/${state}.webp`;
}

/**
 * Returns the still (first-frame) WebP URL for a given companion state.
 * @param {string} id - Companion slug
 * @param {string} state - Animation state
 * @returns {string}
 */
export function stillUrl(id, state) {
  return `${COMPANION_BASE_URL}/${id}/${state}.still.webp`;
}

/**
 * Returns the idle still for use as a preview thumbnail.
 * @param {string} id - Companion slug
 * @returns {string}
 */
export function thumbnailUrl(id) {
  return stillUrl(id, 'idle');
}

/** Animation states every companion ships with. */
export const COMPANION_STATES = ['idle', 'walk', 'wave', 'water', 'stretch', 'eyes', 'happy', 'sad'];
