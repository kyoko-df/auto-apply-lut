import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveOptions, targetSpec } from './prepare-ffmpeg.mjs';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';

function macho(arch = 0x0100000c, dependencies = ['/usr/lib/libSystem.B.dylib']) {
  const commands = dependencies.map((name) => {
    const command = Buffer.alloc(Math.ceil((25 + Buffer.byteLength(name)) / 8) * 8);
    command.writeUInt32LE(0xc, 0);
    command.writeUInt32LE(command.length, 4);
    command.writeUInt32LE(24, 8);
    command.write(name, 24);
    return command;
  });
  const header = Buffer.alloc(32);
  header.writeUInt32LE(0xfeedfacf, 0);
  header.writeUInt32LE(arch, 4);
  header.writeUInt32LE(2, 12);
  header.writeUInt32LE(commands.length, 16);
  header.writeUInt32LE(commands.reduce((sum, command) => sum + command.length, 0), 20);
  return Buffer.concat([header, ...commands]);
}

function fatMacho() {
  const arm = macho();
  const intel = macho(0x01000007);
  const header = Buffer.alloc(48);
  header.writeUInt32BE(0xcafebabe);
  header.writeUInt32BE(2, 4);
  header.writeUInt32BE(0x0100000c, 8);
  header.writeUInt32BE(48, 16);
  header.writeUInt32BE(arm.length, 20);
  header.writeUInt32BE(0x01000007, 28);
  header.writeUInt32BE(48 + arm.length, 36);
  header.writeUInt32BE(intel.length, 40);
  return Buffer.concat([header, arm, intel]);
}

function pe(dependency = 'KERNEL32.dll', delayDependency) {
  const buffer = Buffer.alloc(2048);
  buffer.write('MZ');
  buffer.writeUInt32LE(128, 0x3c);
  buffer.write('PE\0\0', 128);
  buffer.writeUInt16LE(0x8664, 132);
  buffer.writeUInt16LE(1, 134);
  buffer.writeUInt16LE(240, 148);
  buffer.writeUInt16LE(2, 150);
  const optional = 152;
  buffer.writeUInt16LE(0x20b, optional);
  buffer.writeUInt32LE(16, optional + 108);
  buffer.writeUInt32LE(0x1000, optional + 120);
  buffer.writeUInt32LE(40, optional + 124);
  const section = optional + 240;
  buffer.writeUInt32LE(0x1000, section + 12);
  buffer.writeUInt32LE(1024, section + 16);
  buffer.writeUInt32LE(512, section + 20);
  buffer.writeUInt32LE(0x1100, 512 + 12);
  buffer.write(dependency, 768);
  if (delayDependency) {
    buffer.writeUInt32LE(0x1200, optional + 112 + 13 * 8);
    buffer.writeUInt32LE(64, optional + 116 + 13 * 8);
    buffer.writeUInt32LE(1, 1024);
    buffer.writeUInt32LE(0x1300, 1028);
    buffer.write(delayDependency, 1280);
  }
  return buffer;
}

test('target selection defaults to native and honors explicit CLI over environment', () => {
  assert.equal(resolveOptions([], {}, 'darwin', 'arm64').target, 'aarch64-apple-darwin');
  assert.equal(resolveOptions([], {}, 'darwin', 'x64').target, 'x86_64-apple-darwin');
  assert.equal(resolveOptions([], {}, 'win32', 'x64').target, 'x86_64-pc-windows-msvc');
  const explicit = resolveOptions(['--target=x86_64-apple-darwin', '--vendor-dir', '/vendor'], { FFMPEG_BUNDLE_TARGET: 'universal-apple-darwin' });
  assert.equal(explicit.target, 'x86_64-apple-darwin');
  assert.equal(explicit.vendorDir, '/vendor');
  assert.equal(resolveOptions([], { TAURI_ENV_TARGET_TRIPLE: 'universal-apple-darwin' }).specs.length, 2);
});

test('full preparation cannot skip validation or silently accept an unsupported target', () => {
  assert.throws(() => resolveOptions([], { SKIP_FFMPEG_BUNDLE_CHECK: '1' }), /cannot skip/);
  assert.throws(() => targetSpec('aarch64-pc-windows-msvc'), /Unsupported/);
  assert.throws(() => resolveOptions(['--target']), /Missing value/);
  assert.throws(() => resolveOptions(['--unsafe']), /Unknown argument/);
});

test('required files exclude unused ffplay and universal requires both architecture folders', () => {
  const specs = targetSpec('universal-apple-darwin');
  assert.deepEqual(specs.map((spec) => spec.directory), ['macos/aarch64', 'macos/x86_64']);
  assert.deepEqual(specs[0].names, ['ffmpeg', 'ffprobe']);
  assert.deepEqual(targetSpec('x86_64-pc-windows-msvc')[0].names, ['ffmpeg.exe', 'ffprobe.exe']);
});

test('Mach-O metadata distinguishes arm64, Intel and universal executables', () => {
  assert.deepEqual(inspectBinary(macho()).slices.map((slice) => slice.arch), ['aarch64']);
  assert.deepEqual(inspectBinary(macho(0x01000007)).slices.map((slice) => slice.arch), ['x86_64']);
  assert.deepEqual(inspectBinary(fatMacho()).slices.map((slice) => slice.arch), ['aarch64', 'x86_64']);
  validateDependencies(inspectBinary(fatMacho()));
});

test('Mach-O standalone checks reject Homebrew, rpath and path traversal libraries', () => {
  for (const dependency of ['/opt/homebrew/opt/x265/lib/libx265.dylib', '@rpath/libavcodec.dylib', '/usr/lib/../../opt/local/libx264.dylib']) {
    assert.throws(() => validateDependencies(inspectBinary(macho(undefined, [dependency]))), /Non-system dependency/);
  }
  validateDependencies(inspectBinary(macho(undefined, ['/System/Library/Frameworks/VideoToolbox.framework/Versions/A/VideoToolbox'])));
});

test('Mach-O bounds and fat architecture agreement are checked before use', () => {
  const malformed = macho();
  malformed.writeUInt32LE(0xffff, 36);
  assert.throws(() => inspectBinary(malformed), /command size/);
  const mismatch = fatMacho();
  mismatch.writeUInt32BE(0x01000007, 8);
  assert.throws(() => inspectBinary(mismatch), /does not match/);
  assert.throws(() => inspectBinary(macho().subarray(0, 34)), /outside the file/);
});

test('PE dependencies include normal and delayed DLL imports', () => {
  const info = inspectBinary(pe('KERNEL32.dll', 'api-ms-win-core-file-l1-1-0.dll'));
  assert.equal(info.platform, 'win32');
  assert.equal(info.slices[0].arch, 'x86_64');
  assert.deepEqual(info.slices[0].dependencies, ['KERNEL32.dll', 'api-ms-win-core-file-l1-1-0.dll']);
  validateDependencies(info);
});

test('PE checks reject shared FFmpeg, MinGW and separately installed VC runtime DLLs', () => {
  for (const name of ['avcodec-61.dll', 'libgcc_s_seh-1.dll', 'VCRUNTIME140.dll']) {
    assert.throws(() => validateDependencies(inspectBinary(pe(name))), /Non-system dependency/);
    assert.throws(() => validateDependencies(inspectBinary(pe('kernel32.dll', name))), /Non-system dependency/);
  }
});

test('PE bounds, architecture and executable type are validated', () => {
  const invalidArch = pe();
  invalidArch.writeUInt16LE(0xaa64, 132);
  assert.throws(() => inspectBinary(invalidArch), /x86_64/);
  const dll = pe();
  dll.writeUInt16LE(0x2002, 150);
  assert.throws(() => inspectBinary(dll), /not a DLL/);
  const invalidRva = pe();
  invalidRva.writeUInt32LE(0xffffff, 524);
  assert.throws(() => inspectBinary(invalidRva), /import RVA/);
});

test('scripts, empty files and Linux executables cannot masquerade as a full release binary', () => {
  for (const source of [Buffer.from('#!/bin/sh\nffmpeg "$@"'), Buffer.from([0x7f, 69, 76, 70])]) {
    assert.throws(() => inspectBinary(source), /native Mach-O or PE/);
  }
  assert.throws(() => inspectBinary(Buffer.alloc(0)), /outside the file/);
});
