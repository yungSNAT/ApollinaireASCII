// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT
// The OpenTUI export, drawn by OpenTUI's test renderer under Bun, on real timers.
import { expect, test } from 'bun:test';
import { createTestRenderer } from '@opentui/core/testing';
import { frames } from './frames.mjs';
import { addApollinaireArt as once } from './out/once-opentui.js';
import { addApollinaireArt as looping } from './out/loop-opentui.js';

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Draw, then return the index of the frame in the renderer's top-left 5 by 2 cells.
async function shown(setup) {
  await setup.renderOnce();
  const rows = setup.captureCharFrame().split('\n').slice(0, 2);
  return frames.indexOf(rows.map((row) => row.slice(0, 5)).join('\n'));
}

async function until(setup, index) {
  for (let tries = 0; (await shown(setup)) !== index; tries++) {
    expect(tries).toBeLessThan(500);
    await wait(2);
  }
}

// Check every 10 ms for 1000 ms, longer than the 3 frames together, that `index` stays.
async function stays(setup, index) {
  for (let ms = 0; ms < 1000; ms += 10) {
    await wait(10);
    expect(await shown(setup)).toBe(index);
  }
}

async function play(add, check) {
  const setup = await createTestRenderer({ width: 20, height: 4 });
  const art = add(setup.renderer);
  try {
    await check(setup, art);
  } finally {
    art.stop();
    setup.renderer.destroy();
  }
}

test('plays each frame in turn, then stays on the last', async () => {
  await play(once, async (setup) => {
    for (const index of [0, 1, 2]) await until(setup, index);
    await stays(setup, 2);
  });
});

test('loops back to the first frame, and stop holds the frame', async () => {
  await play(looping, async (setup, art) => {
    for (const index of [0, 1, 2, 0]) await until(setup, index);
    art.stop();
    await stays(setup, await shown(setup));
  });
});
