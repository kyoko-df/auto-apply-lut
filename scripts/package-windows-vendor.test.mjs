import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import zlib from 'node:zlib';
import { createVendorZip, parseArgs, validateWindowsBinary } from './package-windows-vendor.mjs';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';

function pe(dependency = 'KERNEL32.dll') {
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
  return buffer;
}

function binaries() {
  return new Map([['ffmpeg.exe', pe('MSIMG32.dll')], ['ffprobe.exe', pe('AVICAP32.dll')]]);
}

test('parseArgs accepts an upstream archive or an explicit pair', () => {
  assert.equal(parseArgs(['--archive', 'in.zip']).archive, 'in.zip');
  const pair = parseArgs(['--ffmpeg', 'a.exe', '--ffprobe', 'b.exe', '--output', 'out.zip']);
  assert.equal(pair.ffmpeg, 'a.exe');
  assert.equal(pair.output, 'out.zip');
  assert.throws(() => parseArgs([]));
  assert.throws(() => parseArgs(['--ffmpeg', 'a.exe']));
  assert.throws(() => parseArgs(['--archive', 'in.zip', '--ffmpeg', 'a.exe']));
});

test('createVendorZip is deterministic and uses the windows/x86_64 layout', () => {
  const zip = createVendorZip(binaries());
  const again = createVendorZip(binaries());
  assert.equal(createHash('sha256').update(zip).digest('hex'), createHash('sha256').update(again).digest('hex'));
  const dir = mkdtempSync(path.join(tmpdir(), 'vendor-zip-'));
  try {
    const file = path.join(dir, 'vendor.zip');
    writeFileSync(file, zip);
    const entries = execFileSync('unzip', ['-Z1', file]).toString('utf8').trim().split('\n');
    assert.deepEqual(entries, ['windows/x86_64/ffmpeg.exe', 'windows/x86_64/ffprobe.exe']);
    for (const name of ['ffmpeg.exe', 'ffprobe.exe']) {
      const raw = execFileSync('unzip', ['-p', file, `windows/x86_64/${name}`]);
      assert.deepEqual(raw, binaries().get(name));
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('validateWindowsBinary accepts system-only PEs and rejects foreign DLLs', () => {
  validateWindowsBinary('ffmpeg.exe', pe('MSIMG32.dll'));
  validateWindowsBinary('ffprobe.exe', pe('AVICAP32.dll'));
  assert.throws(() => validateWindowsBinary('ffmpeg.exe', pe('libx264.dll')), /Non-system dependency/);
  assert.throws(() => validateWindowsBinary('ffmpeg.exe', Buffer.from('not a PE')), /MZ|Expected/);
});

test('validateDependencies treats the new allowlist entries as system DLLs', () => {
  const info = { platform: 'win32', slices: [{ arch: 'x86_64', dependencies: ['MSIMG32.dll', 'AVICAP32.dll'] }] };
  validateDependencies(info);
});
