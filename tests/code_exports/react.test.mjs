// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT
// The React export, rendered into a DOM with fake timers.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { JSDOM } from 'jsdom';
import { frames } from './frames.mjs';

const { window } = new JSDOM('<!doctype html><body></body>');
globalThis.window = window;
globalThis.document = window.document;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const { act, createElement } = await import('react');
const { createRoot } = await import('react-dom/client');
const once = (await import('./out/once-react.js')).default;
const looping = (await import('./out/loop-react.js')).default;

// Render `component` with fake timers; `wait` advances them.
async function mount(t, component, props = {}) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const container = document.createElement('div');
  const root = createRoot(container);
  await act(async () => root.render(createElement(component, props)));
  const shown = () => frames.indexOf(container.querySelector('pre').textContent);
  const wait = async (ms) => act(async () => t.mock.timers.tick(ms));
  return { container, root, shown, wait };
}

test('plays each frame for its duration, then stays on the last', async (t) => {
  const { container, root, shown, wait } = await mount(t, once, { className: 'art' });
  const pre = container.querySelector('pre');
  assert.equal(pre.getAttribute('role'), 'img');
  assert.equal(pre.getAttribute('aria-label'), 'ASCII artwork');
  assert.equal(pre.className, 'art');
  assert.equal(shown(), 0);
  await wait(199);
  assert.equal(shown(), 0);
  await wait(1);
  assert.equal(shown(), 1);
  await wait(300);
  assert.equal(shown(), 2);
  await wait(1000);
  assert.equal(shown(), 2);
  await act(async () => root.unmount());
});

test('loops back to the first frame', async (t) => {
  const { container, root, shown, wait } = await mount(t, looping, { label: 'Ghost' });
  assert.equal(container.querySelector('pre').getAttribute('aria-label'), 'Ghost');
  // Each frame's timer starts once React has shown it, so advance one frame at a time.
  for (const [ms, index] of [[200, 1], [300, 2], [400, 0], [200, 1]]) {
    await wait(ms);
    assert.equal(shown(), index);
  }
  await act(async () => root.unmount());
});

test('leaves no timer behind when paused, resumed and unmounted mid-frame', async (t) => {
  const { root, shown, wait } = await mount(t, looping);
  // Count the timers set and not yet cleared, from here on.
  const live = new Set();
  const [set, clear] = [globalThis.setTimeout, globalThis.clearTimeout];
  globalThis.setTimeout = (...args) => {
    const id = set(...args);
    live.add(id);
    return id;
  };
  globalThis.clearTimeout = (id) => {
    live.delete(id);
    clear(id);
  };
  t.after(() => Object.assign(globalThis, { setTimeout: set, clearTimeout: clear }));
  await wait(100);
  await act(async () => root.render(createElement(looping, { paused: true })));
  await wait(1000);
  assert.equal(shown(), 0);
  await act(async () => root.render(createElement(looping, { paused: false })));
  assert.equal(live.size, 1, 'one timer while playing');
  await wait(50);
  await act(async () => root.unmount());
  assert.equal(live.size, 0, 'no timer after unmounting');
});

test('holds its frame while paused', async (t) => {
  const { root, shown, wait } = await mount(t, looping, { paused: true });
  await wait(1000);
  assert.equal(shown(), 0);
  await act(async () => root.render(createElement(looping, { paused: false })));
  await wait(200);
  assert.equal(shown(), 1);
  await act(async () => root.unmount());
});

test('stays still when reduced motion is asked for between rendering and subscribing', async (t) => {
  // The first query, read while rendering, says no; every later one says yes, with no event.
  let queries = 0;
  window.matchMedia = () => {
    const query = new window.EventTarget();
    query.matches = queries++ > 0;
    return query;
  };
  t.after(() => delete window.matchMedia);
  const { root, shown, wait } = await mount(t, looping);
  await wait(200);
  assert.equal(shown(), 0);
  await act(async () => root.unmount());
});

test('starts still when reduced motion is asked for, and plays once it is not', async (t) => {
  const query = new window.EventTarget();
  query.matches = true;
  window.matchMedia = (text) => {
    assert.equal(text, '(prefers-reduced-motion: reduce)');
    return query;
  };
  t.after(() => delete window.matchMedia);
  const { root, shown, wait } = await mount(t, looping);
  await wait(1000);
  assert.equal(shown(), 0);
  query.matches = false;
  await act(async () => query.dispatchEvent(new window.Event('change')));
  await wait(200);
  assert.equal(shown(), 1);
  await act(async () => root.unmount());
});
