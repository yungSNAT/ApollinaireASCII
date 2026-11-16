// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT
// The Ink export, rendered by ink-testing-library on real timers: Ink paces its own output
// with timers, so fake ones would stop it.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as wait } from 'node:timers/promises';
import { createElement } from 'react';
import { render } from 'ink-testing-library';
import { frames } from './frames.mjs';

const once = (await import('./out/once-ink.js')).default;
const looping = (await import('./out/loop-ink.js')).default;

// The frame on show, with the trailing spaces Ink trims put back.
function shown(app) {
  return frames.indexOf((app.lastFrame() ?? '').split('\n').map((row) => row.padEnd(5)).join('\n'));
}

async function until(app, index) {
  for (let tries = 0; shown(app) !== index; tries++) {
    assert.ok(tries < 500, `frame ${index} never showed; showing ${shown(app)}`);
    await wait(2);
  }
}

// Check every 10 ms for 300 ms, longer than the 3 frames together, that `index` stays.
async function stays(app, index) {
  for (let ms = 0; ms < 300; ms += 10) {
    await wait(10);
    assert.equal(shown(app), index);
  }
}

// Render `component`, run `check`, and unmount even when it fails, so no timer keeps Node up.
async function play(component, check) {
  const app = render(createElement(component));
  try {
    await check(app);
  } finally {
    app.unmount();
  }
}

test('plays each frame in turn, then stays on the last', () => play(once, async (app) => {
  for (const index of [0, 1, 2]) await until(app, index);
  await stays(app, 2);
}));

test('loops, and holds its frame while paused', () => play(looping, async (app) => {
  for (const index of [0, 1, 2, 0]) await until(app, index);
  app.rerender(createElement(looping, { paused: true }));
  await stays(app, shown(app));
}));
