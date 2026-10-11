import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { digest, execute, headers, manifest, parseArgs, targetSpec, validateStaticLibrary, verifyChecksum } from './libraw-dependency.mjs';
import { installPackage, packageFiles, verifyDirectory } from './prepare-libraw.mjs';

function ar(object) {
  const header = Buffer.alloc(60, 32);
  header.write('test.o/', 0); header.write(String(object.length), 48); header.write('`\n', 58);
  return Buffer.concat([Buffer.from('!<arch>\n'), header, object, ...(object.length % 2 ? [Buffer.alloc(1)] : [])]);
}
function macho(minimum = 15, cpu = 0x0100000c) {
  const data = Buffer.alloc(56);
  for (const [offset, value] of [[0, 0xfeedfacf], [4, cpu], [12, 1], [16, 1], [20, 24], [32, 0x32], [36, 24], [40, 1], [44, minimum * 65536]]) data.writeUInt32LE(value, offset);
  return data;
}
test('LibRaw object validation rejects wrong CPU, future macOS, dynamic and truncated archives', () => {
  validateStaticLibrary(ar(macho()), 'aarch64-apple-darwin');
  assert.throws(() => validateStaticLibrary(ar(macho(26)), 'aarch64-apple-darwin'), /newer than 15/);
  assert.throws(() => validateStaticLibrary(ar(macho()), 'x86_64-apple-darwin'), /architecture/);
  assert.throws(() => validateStaticLibrary(ar(macho()).subarray(0, 72), 'aarch64-apple-darwin'), /bounds/);
  assert.throws(() => validateStaticLibrary(macho(), 'aarch64-apple-darwin'), /static archive/);
  const elf = Buffer.alloc(64); Buffer.from('7f454c460201', 'hex').copy(elf); elf.writeUInt16LE(1, 16); elf.writeUInt16LE(62, 18);
  validateStaticLibrary(ar(elf), 'x86_64-unknown-linux-gnu');
  assert.throws(() => validateStaticLibrary(ar(elf), 'aarch64-unknown-linux-gnu'), /architecture/);
  const coff = Buffer.alloc(20); coff.writeUInt16LE(0x8664);
  validateStaticLibrary(ar(coff), 'x86_64-pc-windows-msvc');
  assert.throws(() => validateStaticLibrary(ar(elf), 'x86_64-pc-windows-msvc'), /architecture/);
});
test('LibRaw target selection and checksums reject unsupported or changed inputs', () => {
  assert.equal(parseArgs(['--target', 'x86_64-pc-windows-msvc'], {}).target, 'x86_64-pc-windows-msvc');
  assert.equal(targetSpec('aarch64-apple-darwin').library, 'libraw.a');
  assert.throws(() => parseArgs(['--target', 'i686-pc-windows-msvc'], {}), /Unsupported/);
  assert.throws(() => parseArgs(['--archive', 'package.tar.gz'], {}), /together/);
  assert.throws(() => verifyChecksum(Buffer.from('changed'), digest(Buffer.from('original'))), /SHA256/);
});
test('LibRaw package installation verifies every file and preserves the previous valid dependency on failure', async () => {
  const work = await mkdtemp(path.join(os.tmpdir(), 'lutlab-libraw-test-'));
  const pinned = await manifest(), target = 'aarch64-apple-darwin';
  const source = path.join(work, 'source'), destination = path.join(work, 'installed');
  try {
    await mkdir(path.join(source, 'include/libraw'), { recursive: true }); await mkdir(path.join(source, 'lib'));
    await writeFile(path.join(source, 'lib/libraw.a'), ar(macho()));
    for (const name of headers) await writeFile(path.join(source, 'include/libraw', name), name === 'libraw_version.h' ? '#define LIBRAW_MAJOR_VERSION 0\n#define LIBRAW_MINOR_VERSION 21\n#define LIBRAW_PATCH_VERSION 4\n' : '// synthetic header');
    await writeFile(path.join(source, 'LICENSE.CDDL'), 'synthetic license'); await writeFile(path.join(source, 'COPYRIGHT'), 'synthetic notice');
    const info = { version: pinned.version, target, distribution: 'official', archiveSha256: pinned.archives.darwin.sha256, files: {} };
    for (const file of packageFiles(target).filter(file => file !== 'provenance.json')) info.files[file] = digest(await readFile(path.join(source, file)));
    await writeFile(path.join(source, 'provenance.json'), JSON.stringify(info));
    const archive = path.join(work, 'binary.tar.gz');
    await execute('tar', ['-czf', archive, '-C', source, ...packageFiles(target)]);
    await installPackage(archive, digest(await readFile(archive)), target, pinned, destination);
    const original = await readFile(path.join(destination, 'lib/libraw.a'));
    await writeFile(path.join(source, 'lib/libraw.a'), ar(macho(26)));
    // Update its own digest: deployment-target validation must still reject it.
    info.files['lib/libraw.a'] = digest(await readFile(path.join(source, 'lib/libraw.a')));
    await writeFile(path.join(source, 'provenance.json'), JSON.stringify(info));
    await execute('tar', ['-czf', archive, '-C', source, ...packageFiles(target)]);
    await assert.rejects(installPackage(archive, digest(await readFile(archive)), target, pinned, destination), /newer than 15/);
    assert.deepEqual(await readFile(path.join(destination, 'lib/libraw.a')), original);
    await writeFile(path.join(destination, 'include/libraw/libraw.h'), 'tampered');
    await assert.rejects(verifyDirectory(destination, target, pinned), /SHA256/);
    await rm(path.join(source, 'lib/libraw.a')); await writeFile(path.join(work, 'outside'), 'not a library');
    await execute('tar', ['-czf', archive, '-C', work, 'outside']);
    // Metadata/file allowlisting is checked before extraction.
    await assert.rejects(installPackage(archive, digest(await readFile(archive)), target, pinned, destination), /must contain only/);
  } finally { await rm(work, { recursive: true, force: true }); }
});
