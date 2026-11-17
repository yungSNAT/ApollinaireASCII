// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT
// The Python, Ruby and C# exports, run as programs: once.json plays each frame for its duration
// and ends with a line feed; loop.json plays until Ctrl+C, then ends the same way.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';
import { frames } from './frames.mjs';

const draw = (indices) => indices.map((index) => `\x1b[H${frames[index]}`).join('');
const programs = [
  ['Python', 'python3', [], 'py'],
  ['Ruby', 'ruby', [], 'rb'],
  ['C#', 'dotnet', ['run'], 'cs'],
];

// Start `program` in its own process group, as a terminal would, and collect its output.
function run(command, args, file) {
  const child = spawn(command, [...args, file], { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const result = { output: '', errors: '' };
  child.stdout.setEncoding('utf8').on('data', (text) => { result.output += text; });
  child.stderr.setEncoding('utf8').on('data', (text) => { result.errors += text; });
  result.exit = new Promise((resolve) => child.on('close', (code, signal) => resolve({ code, signal })));
  result.child = child;
  return result;
}

const until = async (check) => {
  while (!check()) await new Promise((resolve) => setTimeout(resolve, 10));
};

for (const [name, command, args, extension] of programs) {
  test(`${name} plays every frame once for its duration`, async () => {
    const result = run(command, args, `out/once.${extension}`);
    // The first draw marks the start: C# compiles before it plays.
    await until(() => result.output.length > 0 || result.child.exitCode !== null);
    const started = performance.now();
    assert.deepEqual(await result.exit, { code: 0, signal: null }, result.errors);
    const took = performance.now() - started;
    assert.equal(result.output, `\x1b[2J${draw([0, 1, 2])}\n`);
    // 200, 300 and 400 ms.
    assert.ok(took >= 850, `${took} ms`);
  });

  test(`${name} loops until Ctrl+C, then ends its line`, async () => {
    const result = run(command, args, `out/loop.${extension}`);
    const looped = `\x1b[2J${draw([0, 1, 2, 0])}`;
    await until(() => result.output.startsWith(looped) || result.child.exitCode !== null);
    process.kill(-result.child.pid, 'SIGINT');
    assert.deepEqual(await result.exit, { code: 0, signal: null }, result.errors);
    assert.ok(result.output.startsWith(looped), JSON.stringify(result.output));
    assert.ok(result.output.endsWith('\n'));
  });
}
