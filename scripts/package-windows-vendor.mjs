// Build a deterministic Windows FFmpeg vendor archive for release builds.
// The CI job downloads FFMPEG_WINDOWS_VENDOR_URL and verifies it against
// FFMPEG_WINDOWS_VENDOR_SHA256, so the archive must be a stable, audited ZIP
// containing exactly windows/x86_64/ffmpeg.exe and windows/x86_64/ffprobe.exe.
// Upstream zips (gyan.dev, martin-riedl.de, ...) do not use that layout, so
// maintainers repackage the audited binaries through this script.
//
//   node scripts/package-windows-vendor.mjs --archive ffmpeg-9.0.2-essentials_build.zip --output vendor.zip
//   node scripts/package-windows-vendor.mjs --ffmpeg path/to/ffmpeg.exe --ffprobe path/to/ffprobe.exe --output vendor.zip
//
// Every binary is validated with the same rules the release build applies,
// and the ZIP uses fixed timestamps and ordering so a given input always
// produces the same SHA256.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';
import { selectArchiveEntry } from './fetch-ffmpeg.mjs';

const REQUIRED_FILES = ['ffmpeg.exe', 'ffprobe.exe'];
const ENTRY_PREFIX = 'windows/x86_64/';

const USAGE = 'Usage: node scripts/package-windows-vendor.mjs (--archive <zip> | --ffmpeg <exe> --ffprobe <exe>) [--output <zip>]';

export function parseArgs(argv = process.argv.slice(2)) {
  const options = { archive: null, ffmpeg: null, ffprobe: null, output: 'lutlab-ffmpeg-vendor-windows-x86_64.zip', help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    const value = argv[i + 1];
    if (key === '--help' || key === '-h') {
      options.help = true;
      continue;
    }
    if (!['--archive', '--ffmpeg', '--ffprobe', '--output'].includes(key) || !value || value.startsWith('--')) {
      throw new Error(`Unknown or incomplete option: ${key}`);
    }
    options[key.slice(2)] = value;
    i += 1;
  }
  if (options.help) return options;
  if (options.archive) {
    if (options.ffmpeg || options.ffprobe) throw new Error('--archive cannot be combined with --ffmpeg/--ffprobe.');
  } else if (!options.ffmpeg || !options.ffprobe) {
    throw new Error('Provide either --archive or both --ffmpeg and --ffprobe.');
  }
  return options;
}

function unzip(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('unzip', args, { stdio: ['ignore', 'pipe', 'inherit'] });
    const chunks = [];
    child.stdout.on('data', (chunk) => chunks.push(chunk));
    child.on('error', () => reject(new Error('The Windows vendor packager requires `unzip` on PATH.')));
    child.on('exit', (code) => (code === 0 ? resolve(Buffer.concat(chunks)) : reject(new Error(`unzip exited with code ${code}.`))));
  });
}

async function binariesFromArchive(archive) {
  const listing = (await unzip(['-Z1', archive])).toString('utf8');
  const binaries = new Map();
  for (const name of REQUIRED_FILES) {
    const entry = selectArchiveEntry(listing, name);
    binaries.set(name, await unzip(['-p', archive, entry]));
  }
  return binaries;
}

export function validateWindowsBinary(name, buffer) {
  const info = inspectBinary(buffer);
  if (info.platform !== 'win32' || info.slices.length !== 1 || info.slices[0].arch !== 'x86_64') {
    throw new Error(`${name} must be a Windows x86_64 executable.`);
  }
  validateDependencies(info);
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let value = i;
    for (let bit = 0; bit < 8; bit += 1) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    table[i] = value >>> 0;
  }
  return table;
})();

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

// Fixed DOS date (1980-01-01) keeps archives byte-for-byte reproducible.
const DOS_TIME = 0;
const DOS_DATE = 33;

export function createVendorZip(binaries) {
  const chunks = [];
  const directory = [];
  let offset = 0;
  for (const name of REQUIRED_FILES) {
    const data = binaries.get(name);
    const entryName = ENTRY_PREFIX + name;
    const compressed = zlib.deflateRawSync(data, { level: 9 });
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(8, 8);
    local.writeUInt16LE(DOS_TIME, 10);
    local.writeUInt16LE(DOS_DATE, 12);
    local.writeUInt32LE(crc32(data), 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(Buffer.byteLength(entryName), 26);
    chunks.push(local, Buffer.from(entryName, 'utf8'), compressed);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(8, 10);
    central.writeUInt16LE(DOS_TIME, 12);
    central.writeUInt16LE(DOS_DATE, 14);
    central.writeUInt32LE(crc32(data), 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(Buffer.byteLength(entryName), 28);
    central.writeUInt32LE(offset, 42);
    directory.push(central, Buffer.from(entryName, 'utf8'));
    offset += local.length + Buffer.byteLength(entryName) + compressed.length;
  }
  const directorySize = directory.reduce((sum, chunk) => sum + chunk.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(REQUIRED_FILES.length, 8);
  end.writeUInt16LE(REQUIRED_FILES.length, 10);
  end.writeUInt32LE(directorySize, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...chunks, ...directory, end]);
}

export async function main() {
  const options = parseArgs();
  if (options.help) {
    console.log(USAGE);
    return;
  }
  const binaries = options.archive
    ? await binariesFromArchive(options.archive)
    : new Map([[REQUIRED_FILES[0], await readFile(options.ffmpeg)], [REQUIRED_FILES[1], await readFile(options.ffprobe)]]);
  for (const [name, data] of binaries) {
    validateWindowsBinary(name, data);
    console.log(`Validated ${name}: Windows x86_64, system DLLs only (${(data.length / 1048576).toFixed(1)} MiB).`);
  }
  const zip = createVendorZip(binaries);
  const output = path.resolve(options.output);
  await writeFile(output, zip);
  const sha256 = createHash('sha256').update(zip).digest('hex');
  console.log(`Wrote ${output} (${(zip.length / 1048576).toFixed(1)} MiB).`);
  console.log('');
  console.log('Upload this file to a durable HTTPS location, then set these GitHub repository variables:');
  console.log(`  FFMPEG_WINDOWS_VENDOR_URL=<uploaded URL>`);
  console.log(`  FFMPEG_WINDOWS_VENDOR_SHA256=${sha256}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`Error: ${error.message}`);
    console.error(USAGE);
    process.exit(1);
  });
}
