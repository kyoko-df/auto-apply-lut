import test from 'node:test';
import assert from 'node:assert/strict';
import { verifyChecksum, selectArchiveEntry } from './fetch-ffmpeg.mjs';

test('pinned checksums reject a changed archive', () => {
  const checksum = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';
  verifyChecksum(Buffer.from('abc'), checksum);
  assert.throws(() => verifyChecksum(Buffer.from('abcd'), checksum), /SHA256 mismatch/);
  assert.throws(() => verifyChecksum(Buffer.from('abc'), 'latest'), /Invalid pinned/);
});

test('archive extraction rejects missing, ambiguous and traversal entries', () => {
  assert.equal(selectArchiveEntry('README\nffmpeg\n', 'ffmpeg'), 'ffmpeg');
  assert.equal(selectArchiveEntry('bin/ffmpeg\n', 'ffmpeg'), 'bin/ffmpeg');
  for (const listing of ['README\n', 'ffmpeg\nbin/ffmpeg\n', '../ffmpeg\n', '/bin/ffmpeg\n']) {
    assert.throws(() => selectArchiveEntry(listing, 'ffmpeg'), /exactly one safe/);
  }
});
