// App builds only consume this verified package; they never compile LibRaw itself.
import { copyFile, mkdir, mkdtemp, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { archiveEntries, cacheDirectory, digest, download, execute, headers, manifest, parseArgs, targetSpec, validateStaticLibrary, verifyChecksum } from './libraw-dependency.mjs';

export function packageFiles(target) {
  return ['provenance.json', 'LICENSE.CDDL', 'COPYRIGHT', `lib/${targetSpec(target).library}`, ...headers.map(name => `include/libraw/${name}`)];
}
export async function verifyDirectory(directory, target, pinned) {
  const info = JSON.parse(await readFile(path.join(directory, 'provenance.json'), 'utf8'));
  const required = packageFiles(target).filter(name => name !== 'provenance.json');
  const official = pinned.archives[targetSpec(target).platform];
  const validOrigin = official
    ? info.distribution === 'official' && info.archiveSha256 === official.sha256
    : info.distribution === 'lutlab-prebuilt' && info.sourceSha256 === pinned.source.sha256 && JSON.stringify(info.build) === JSON.stringify(pinned.build);
  if (info.version !== pinned.version || info.target !== target || !validOrigin) throw new Error('LibRaw package version, target or build provenance does not match the manifest.');
  if (Object.keys(info.files || {}).sort().join('\n') !== required.sort().join('\n')) throw new Error('LibRaw package is missing required files or includes unexpected files.');
  for (const name of required) {
    const file = path.join(directory, name);
    if (!(await stat(file)).isFile()) throw new Error(`LibRaw package file is not regular: ${name}`);
    verifyChecksum(await readFile(file), info.files[name]);
  }
  const version = await readFile(path.join(directory, 'include/libraw/libraw_version.h'), 'utf8');
  if (!version.match(/LIBRAW_MAJOR_VERSION\s+0\b/) || !version.match(/LIBRAW_MINOR_VERSION\s+21\b/) || !version.match(/LIBRAW_PATCH_VERSION\s+4\b/)) throw new Error('LibRaw headers do not match version 0.21.4.');
  validateStaticLibrary(await readFile(path.join(directory, 'lib', targetSpec(target).library)), target);
  return { directory, link: targetSpec(target).link, libraries: official && targetSpec(target).platform === 'darwin' ? ['z'] : [] };
}
async function installOfficial(archive, target, pinned) {
  const spec = targetSpec(target), official = pinned.archives[spec.platform];
  verifyChecksum(await readFile(archive), official.sha256);
  const entries = await archiveEntries(archive);
  const prefix = `LibRaw-${pinned.version}/`;
  const selected = [official.library, 'LICENSE.CDDL', 'COPYRIGHT', ...headers.map(name => `libraw/${name}`)];
  if (selected.some(name => !entries.includes(prefix + name))) throw new Error('Official LibRaw archive is missing the matching library, headers or notices.');
  const stage = await mkdtemp(path.join(os.tmpdir(), 'lutlab-libraw-official-'));
  try {
    await execute('tar', ['-xf', path.resolve(archive), '-C', stage, ...selected.map(name => prefix + name)]);
    const extracted = path.join(stage, `LibRaw-${pinned.version}`), pack = path.join(stage, 'package');
    await mkdir(path.join(pack, 'lib'), { recursive: true }); await mkdir(path.join(pack, 'include/libraw'), { recursive: true });
    const library = path.join(pack, 'lib', spec.library);
    if (spec.platform === 'darwin') await execute('lipo', [path.join(extracted, official.library), '-thin', spec.arch === 'aarch64' ? 'arm64' : 'x86_64', '-output', library]);
    else await copyFile(path.join(extracted, official.library), library);
    for (const name of headers) await copyFile(path.join(extracted, 'libraw', name), path.join(pack, 'include/libraw', name));
    for (const name of ['LICENSE.CDDL', 'COPYRIGHT']) await copyFile(path.join(extracted, name), path.join(pack, name));
    const info = { version: pinned.version, target, distribution: 'official', archiveSha256: official.sha256, files: {} };
    for (const name of packageFiles(target).filter(name => name !== 'provenance.json')) info.files[name] = digest(await readFile(path.join(pack, name)));
    await writeFile(path.join(pack, 'provenance.json'), JSON.stringify(info, null, 2));
    const binary = path.join(stage, 'prebuilt.tar.gz');
    await execute('tar', ['-czf', binary, '-C', pack, ...packageFiles(target)]);
    return await installPackage(binary, digest(await readFile(binary)), target, pinned);
  } finally { await rm(stage, { recursive: true, force: true }); }
}
export async function installPackage(archive, sha256, target, pinned, destination = cacheDirectory(pinned.version, target)) {
  verifyChecksum(await readFile(archive), sha256);
  const entries = await archiveEntries(archive);
  const files = packageFiles(target);
  if (entries.some(name => !files.includes(name)) || files.some(name => !entries.includes(name))) throw new Error('LibRaw prebuilt package must contain only the library, matching headers, provenance and notices.');
  await mkdir(path.dirname(destination), { recursive: true });
  const stage = await mkdtemp(path.join(path.dirname(destination), '.staging-'));
  try {
    await execute('tar', ['-xzf', path.resolve(archive), '-C', stage, ...files]);
    await verifyDirectory(stage, target, pinned);
    await copyFile(archive, path.join(stage, 'package.tar.gz'));
    await writeFile(path.join(stage, 'receipt.json'), JSON.stringify({ sha256: sha256.toLowerCase() }));
    // Replace only after the complete new package has passed every check.
    const previous = `${destination}.previous-${process.pid}`;
    let moved = false;
    try { await rename(destination, previous); moved = true; } catch (e) { if (e.code !== 'ENOENT') throw e; }
    try { await rename(stage, destination); } catch (e) { if (moved) await rename(previous, destination); throw e; }
    await rm(previous, { recursive: true, force: true });
    return verifyDirectory(destination, target, pinned);
  } finally { await rm(stage, { recursive: true, force: true }); }
}
export async function prepare(options, env = process.env) {
  const pinned = await manifest();
  const target = options.target;
  const directory = cacheDirectory(pinned.version, target);
  const official = pinned.archives[targetSpec(target).platform];
  if (options.archive) {
    verifyChecksum(await readFile(options.archive), options.sha256);
    return official && options.sha256.toLowerCase() === official.sha256
      ? installOfficial(options.archive, target, pinned)
      : installPackage(options.archive, options.sha256, target, pinned);
  }
  try {
    const receipt = JSON.parse(await readFile(path.join(directory, 'receipt.json'), 'utf8'));
    verifyChecksum(await readFile(path.join(directory, 'package.tar.gz')), receipt.sha256);
    return await verifyDirectory(directory, target, pinned);
  } catch (error) {
    if (options.offline) throw new Error(`No valid cached LibRaw package for ${target}: ${error.message}. Run pnpm prepare:libraw with a verified archive, or pnpm build:libraw --target ${target} once.`);
  }
  const url = env.LIBRAW_VENDOR_URL || official?.url, sha256 = env.LIBRAW_VENDOR_SHA256 || official?.sha256;
  if (Boolean(env.LIBRAW_VENDOR_URL) !== Boolean(env.LIBRAW_VENDOR_SHA256)) throw new Error('LIBRAW_VENDOR_URL and LIBRAW_VENDOR_SHA256 must be supplied together.');
  if (!url || !sha256) throw new Error(`No prebuilt LibRaw package for ${target}. Set LIBRAW_VENDOR_URL / LIBRAW_VENDOR_SHA256, use --archive / --sha256, or run pnpm build:libraw --target ${target} once. App builds never compile the upstream library.`);
  const stage = await mkdtemp(path.join(os.tmpdir(), 'lutlab-libraw-download-'));
  try {
    const archive = path.join(stage, 'package.tar.gz');
    await download(url, archive, sha256);
    return official && !env.LIBRAW_VENDOR_URL
      ? await installOfficial(archive, target, pinned)
      : await installPackage(archive, sha256, target, pinned);
  } finally { await rm(stage, { recursive: true, force: true }); }
}
export async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  const targets = options.target === 'universal-apple-darwin' ? ['aarch64-apple-darwin', 'x86_64-apple-darwin'] : [options.target];
  if (options.json && targets.length !== 1) throw new Error('JSON link configuration needs one target.');
  for (const target of targets) {
    const result = await prepare({ ...options, target });
    console.log(options.json ? JSON.stringify(result) : `[prepare-libraw] Verified prebuilt LibRaw ${target}: ${result.directory}`);
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(error => { console.error(`[prepare-libraw] ${error.message}`); process.exitCode = 1; });
