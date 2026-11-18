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

// Ctrl+C can arrive during the first write, which clears the screen; a stream whose first write
// raises the interrupt stands in for it.
const firstWriteInterrupted = [
  ['Python', 'python3', '-c', `
import importlib.util, sys
spec = importlib.util.spec_from_file_location("art", "out/loop.py")
art = importlib.util.module_from_spec(spec)
spec.loader.exec_module(art)
class Stream:
    writes = 0
    def write(self, text):
        self.writes += 1
        if self.writes == 1:
            raise KeyboardInterrupt
        sys.stdout.write(text)
    def flush(self):
        sys.stdout.flush()
art.play(Stream())
`],
  ['Ruby', 'ruby', '-e', `
load "./out/loop.rb"
class Stream
  def initialize
    @writes = 0
  end
  def write(*texts)
    @writes += 1
    raise Interrupt if @writes == 1
    $stdout.write(*texts)
  end
  def flush
    $stdout.flush
  end
end
ApollinaireArt.play(Stream.new)
`],
];
for (const [name, command, flag, script] of firstWriteInterrupted) {
  test(`${name} ends its line when Ctrl+C arrives during the first write`, async () => {
    const result = run(command, [flag], script);
    assert.deepEqual(await result.exit, { code: 0, signal: null }, result.errors);
    assert.equal(result.output, '\n');
  });
}
