// Shared integrity and object checks for independently built LibRaw packages.
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { readFile, mkdir, rename, rm } from 'node:fs/promises';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const execute = promisify(execFile);
export const headers = ['libraw.h', 'libraw_alloc.h', 'libraw_const.h', 'libraw_datastream.h', 'libraw_internal.h', 'libraw_types.h', 'libraw_version.h'];
export const digest = bytes => createHash('sha256').update(bytes).digest('hex');
export async function manifest() {
  return JSON.parse(await readFile(path.join(root, 'src-tauri/resources/libraw-manifest.json'), 'utf8'));
}
export function targetSpec(target) {
  const specs = {
    'aarch64-apple-darwin': { platform: 'darwin', arch: 'aarch64', library: 'libraw.a', link: 'raw' },
    'x86_64-apple-darwin': { platform: 'darwin', arch: 'x86_64', library: 'libraw.a', link: 'raw' },
    'x86_64-pc-windows-msvc': { platform: 'win32', arch: 'x86_64', library: 'raw.lib', link: 'raw' },
    'x86_64-unknown-linux-gnu': { platform: 'linux', arch: 'x86_64', library: 'libraw.a', link: 'raw' },
    'aarch64-unknown-linux-gnu': { platform: 'linux', arch: 'aarch64', library: 'libraw.a', link: 'raw' },
  };
  if (!specs[target]) throw new Error(`Unsupported LibRaw target: ${target}`);
  return specs[target];
}
export function hostTarget(platform = process.platform, arch = process.arch) {
  return `${arch === 'arm64' ? 'aarch64' : arch === 'x64' ? 'x86_64' : arch}-${{ darwin: 'apple-darwin', win32: 'pc-windows-msvc', linux: 'unknown-linux-gnu' }[platform]}`;
}
export function parseArgs(argv, env = process.env) {
  const options = { target: env.LIBRAW_BUNDLE_TARGET || env.TAURI_ENV_TARGET_TRIPLE || hostTarget(), offline: false, json: false };
  for (let i = 0; i < argv.length; i++) {
    if (['--offline', '--json'].includes(argv[i])) { options[argv[i].slice(2)] = true; continue; }
    if (!['--target', '--archive', '--sha256'].includes(argv[i]) || !argv[i + 1] || argv[i + 1].startsWith('--')) throw new Error(`Unknown/incomplete LibRaw option: ${argv[i]}`);
    options[argv[i].slice(2)] = argv[++i];
  }
  if (Boolean(options.archive) !== Boolean(options.sha256)) throw new Error('--archive and --sha256 must be supplied together.');
  if (options.target !== 'universal-apple-darwin') targetSpec(options.target);
  return options;
}
export const cacheDirectory = (version, target) => path.join(root, 'src-tauri/.cache/libraw', version, target);
export function verifyChecksum(bytes, expected) {
  if (!/^[a-f0-9]{64}$/i.test(expected || '') || digest(bytes) !== expected.toLowerCase()) throw new Error('LibRaw SHA256 mismatch or invalid checksum.');
}
export async function download(url, destination, sha256) {
  if (!/^https:\/\//.test(url)) throw new Error('LibRaw downloads require HTTPS.');
  await mkdir(path.dirname(destination), { recursive: true });
  const partial = `${destination}.${process.pid}.partial`;
  try {
    await execute(process.platform === 'win32' ? 'curl.exe' : 'curl', ['--fail', '--location', '--proto', '=https', '--proto-redir', '=https', '--silent', '--show-error', '--connect-timeout', '30', '--max-time', '300', '--output', partial, url], { timeout: 310_000 });
    verifyChecksum(await readFile(partial), sha256);
    await rename(partial, destination);
  } finally { await rm(partial, { force: true }); }
}
export async function archiveEntries(archive) {
  const { stdout } = await execute('tar', ['-tf', archive], { maxBuffer: 4 * 1024 * 1024 });
  const entries = stdout.trim().split(/\r?\n/);
  const seen = new Set();
  for (const name of entries) {
    if (!name || name.startsWith('/') || name.includes('\\') || /^[a-z]:/i.test(name) || name.split('/').includes('..') || seen.has(name)) throw new Error('Unsafe or duplicate LibRaw archive entry.');
    seen.add(name);
  }
  // Extraction never permits symlinks, hard links or device nodes.
  const listing = await execute('tar', ['-tvf', archive], { maxBuffer: 4 * 1024 * 1024 });
  if (listing.stdout.split(/\r?\n/).filter(Boolean).some(line => !['-', 'd'].includes(line[0]))) throw new Error('LibRaw archives may contain only regular files/directories.');
  return entries;
}
export function validateStaticLibrary(bytes, target) {
  const spec = targetSpec(target);
  if (bytes.toString('ascii', 0, 8) !== '!<arch>\n') throw new Error('LibRaw must be a thin static archive, not a dynamic library or universal archive.');
  let offset = 8, objects = 0;
  while (offset < bytes.length) {
    if (offset + 60 > bytes.length || bytes.toString('ascii', offset + 58, offset + 60) !== '`\n') throw new Error('Malformed LibRaw archive member.');
    let name = bytes.toString('ascii', offset, offset + 16).trim();
    const size = Number(bytes.toString('ascii', offset + 48, offset + 58).trim());
    if (!Number.isSafeInteger(size) || size < 0 || offset + 60 + size > bytes.length) throw new Error('LibRaw archive member is out of bounds.');
    let data = bytes.subarray(offset + 60, offset + 60 + size);
    if (name.startsWith('#1/')) {
      const length = Number(name.slice(3));
      if (!Number.isInteger(length) || length < 1 || length > data.length) throw new Error('Invalid LibRaw archive member name.');
      name = data.toString('utf8', 0, length).replace(/\0+$/, ''); data = data.subarray(length);
    }
    if (!['/', '//', '/SYM64/'].includes(name) && !name.startsWith('__.SYMDEF')) {
      if (data.length < 20) throw new Error('Truncated LibRaw object.');
      if (spec.platform === 'darwin') {
        const cpu = spec.arch === 'aarch64' ? 0x0100000c : 0x01000007;
        if (data.length < 32 || data.readUInt32LE(0) !== 0xfeedfacf || data.readUInt32LE(4) !== cpu || data.readUInt32LE(12) !== 1) throw new Error('Wrong LibRaw Mach-O architecture/object type.');
        const end = 32 + data.readUInt32LE(20); let p = 32, minimum;
        if (end > data.length) throw new Error('Malformed LibRaw Mach-O commands.');
        for (let n = 0; n < data.readUInt32LE(16); n++) {
          if (p + 8 > end) throw new Error('Malformed LibRaw Mach-O command.');
          const command = data.readUInt32LE(p), length = data.readUInt32LE(p + 4);
          if (length < 8 || p + length > end) throw new Error('Malformed LibRaw Mach-O command length.');
          if (command === 0x32) {
            if (length < 24 || data.readUInt32LE(p + 8) !== 1) throw new Error('Invalid LibRaw macOS deployment target.');
            minimum = data.readUInt32LE(p + 12);
          }
          if (command === 0x24) { if (length < 16) throw new Error('Invalid deployment target.'); minimum = data.readUInt32LE(p + 8); }
          p += length;
        }
        if (p !== end || minimum === undefined || minimum > 15 * 65536) throw new Error('LibRaw requires macOS newer than 15.0.');
      } else if (spec.platform === 'linux') {
        if (data.length < 64 || data.toString('hex', 0, 6) !== '7f454c460201' || data.readUInt16LE(16) !== 1 || data.readUInt16LE(18) !== (spec.arch === 'aarch64' ? 183 : 62)) throw new Error('Wrong LibRaw ELF architecture/object type.');
      } else if (data.readUInt16LE(0) !== 0x8664 && !(data.readUInt32LE(0) === 0xffff0000 && data.readUInt16LE(6) === 0x8664)) throw new Error('Wrong LibRaw COFF architecture/object type.');
      objects++;
    }
    offset += 60 + size + (size % 2);
  }
  if (offset !== bytes.length || !objects) throw new Error('LibRaw archive contains no complete objects.');
}
