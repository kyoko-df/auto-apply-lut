import { createHash } from 'node:crypto';
import { spawn, execFile } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { chmod, copyFile, mkdir, mkdtemp, readFile, rename, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';

const exec = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function verifyChecksum(bytes, expected) {
  if (!/^[a-f0-9]{64}$/.test(expected)) throw new Error('Invalid pinned SHA256.');
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (actual !== expected) throw new Error(`SHA256 mismatch: expected ${expected}, received ${actual}`);
}

export function selectArchiveEntry(entries, name) {
  const matches = entries.split(/\r?\n/).filter((entry) => entry === name || entry.endsWith(`/${name}`));
  if (matches.length !== 1 || matches[0].split('/').some((part) => part === '..') || matches[0].startsWith('/')) {
    throw new Error(`Archive must contain exactly one safe ${name} entry.`);
  }
  return matches[0];
}

async function extract(archive, entry, destination) {
  // Extract only the selected executable to our own path, never archive paths.
  const child = spawn('unzip', ['-p', archive, entry], { stdio: ['ignore', 'pipe', 'pipe'] });
  let errorText = '';
  child.stderr.on('data', (data) => { errorText += data.toString(); });
  const completed = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code) => code === 0 ? resolve() : reject(new Error(`unzip failed: ${errorText}`)));
  });
  await Promise.all([pipeline(child.stdout, createWriteStream(destination, { mode: 0o755 })), completed]);
}

export async function main(args = process.argv.slice(2)) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!['--target', '--cache-dir'].includes(args[index]) || !args[index + 1] || args[index + 1].startsWith('--')) {
      throw new Error('Usage: node scripts/fetch-ffmpeg.mjs --target aarch64-apple-darwin [--cache-dir PATH]');
    }
    options[args[index].slice(2)] = args[index + 1];
  }
  const manifest = JSON.parse(await readFile(path.join(root, 'src-tauri/resources/ffmpeg-manifest.json'), 'utf8'));
  const target = options.target ?? 'aarch64-apple-darwin';
  const spec = manifest.targets[target];
  if (!spec) throw new Error(`No pinned download for ${target}. Supply audited binaries with FFMPEG_VENDOR_DIR and run prepare:ffmpeg.`);
  if (process.platform !== 'darwin') throw new Error('The pinned macOS downloader requires macOS curl and unzip.');
  const cache = path.resolve(options['cache-dir'] ?? path.join(os.tmpdir(), `lutlab-ffmpeg-${manifest.build}`));
  await mkdir(cache, { recursive: true });
  const staging = await mkdtemp(path.join(os.tmpdir(), 'lutlab-ffmpeg-staging-'));
  try {
    for (const file of spec.files) {
      const archive = path.join(cache, `${file.name}.zip`);
      let cached = false;
      try { verifyChecksum(await readFile(archive), file.sha256); cached = true; } catch { /* Download a verified replacement. */ }
      if (!cached) {
        console.log(`[fetch-ffmpeg] Downloading pinned ${file.name} ${manifest.version} (${target})`);
        const partial = `${archive}.partial`;
        await exec('curl', ['--fail', '--location', '--silent', '--show-error', '--retry', '3', '--connect-timeout', '30', '--max-time', '1200', '--output', partial, file.url], { timeout: 1_260_000 });
        verifyChecksum(await readFile(partial), file.sha256);
        await rename(partial, archive);
      }
      const { stdout } = await exec('unzip', ['-Z1', archive]);
      const executable = path.join(staging, file.name);
      await extract(archive, selectArchiveEntry(stdout, file.name), executable);
      const info = inspectBinary(await readFile(executable));
      if (info.platform !== 'darwin' || !info.slices.some((slice) => slice.arch === 'aarch64')) throw new Error('Unexpected downloaded executable architecture.');
      validateDependencies(info);
      console.log(`[fetch-ffmpeg] ${file.name}: pinned SHA256 and standalone dependencies verified`);
    }
    // Publish only after BOTH binaries have passed integrity and dependency checks.
    const destination = path.join(root, 'src-tauri/resources/bin', spec.directory);
    await mkdir(destination, { recursive: true });
    for (const file of spec.files) {
      const output = path.join(destination, file.name);
      await copyFile(path.join(staging, file.name), output);
      await chmod(output, 0o755);
    }
    console.log(`[fetch-ffmpeg] Ready: ${destination}`);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(`[fetch-ffmpeg] ${error.message}`); process.exitCode = 1; });
}
