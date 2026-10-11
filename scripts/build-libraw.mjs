// Maintainer/CI dependency build only. Normal Tauri/Cargo builds use prepare-libraw.
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { archiveEntries, cacheDirectory, digest, download, execute, headers, hostTarget, manifest, parseArgs, root, targetSpec, verifyChecksum } from './libraw-dependency.mjs';
import { installPackage, packageFiles, prepare } from './prepare-libraw.mjs';

export async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  if (options.archive || options.offline || options.json) throw new Error('build-libraw accepts --target only. Use prepare-libraw to import an existing prebuilt package.');
  if (options.target === 'universal-apple-darwin') {
    for (const target of ['aarch64-apple-darwin', 'x86_64-apple-darwin']) await main(['--target', target]);
    return;
  }
  try { const ready = await prepare({ ...options, offline: true }); console.log(`[build-libraw] Reusing ${ready.directory}`); return; } catch { /* Build the missing dependency once. */ }
  if (process.env.LIBRAW_VENDOR_URL || process.env.LIBRAW_VENDOR_SHA256) { await prepare(options); return; }
  const pinned = await manifest();
  const spec = targetSpec(options.target), host = targetSpec(hostTarget());
  if (pinned.archives[spec.platform]) { await prepare(options); return; }
  if (spec.platform !== 'linux' || spec.platform !== host.platform || spec.arch !== host.arch) throw new Error('Build the Linux dependency on its matching native architecture.');
  const archive = path.join(root, 'src-tauri/.cache/libraw', pinned.version, 'source.tar.gz');
  try { verifyChecksum(await readFile(archive), pinned.source.sha256); } catch { await download(pinned.source.url, archive, pinned.source.sha256); }
  await archiveEntries(archive);
  const work = await mkdtemp(path.join(os.tmpdir(), 'lutlab-libraw-build-'));
  try {
    console.log(`[build-libraw] Building pinned dependency once for ${options.target}; app builds will reuse the binary.`);
    await execute('tar', ['-xzf', archive, '-C', work]);
    const source = path.join(work, `LibRaw-${pinned.version}`);
    const makefile = await readFile(path.join(source, 'Makefile.dist'), 'utf8');
    const sources = [...new Set(makefile.match(/\bsrc\/[\w/.-]+\.cpp\b/g))].sort();
    if (sources.length < 60) throw new Error('Unexpected LibRaw source layout.');
    const cmake = `cmake_minimum_required(VERSION 3.15)\nproject(lutlab_raw_dependency LANGUAGES CXX)\nadd_library(raw STATIC\n${sources.map(file => `  "${source}/${file}"`).join('\n')}\n)\nset_target_properties(raw PROPERTIES CXX_STANDARD 11 CXX_STANDARD_REQUIRED YES POSITION_INDEPENDENT_CODE YES)\ntarget_include_directories(raw PRIVATE "${source}")\ntarget_compile_definitions(raw PRIVATE LIBRAW_NODLL)\n`;
    await writeFile(path.join(work, 'CMakeLists.txt'), cmake);
    const build = path.join(work, 'build');
    const args = ['-S', work, '-B', build, '-DCMAKE_BUILD_TYPE=Release'];
    await execute('cmake', args, { timeout: 60_000 });
    await execute('cmake', ['--build', build, '--config', 'Release', '--parallel', String(Math.min(4, os.availableParallelism()))], { timeout: 600_000, maxBuffer: 8 * 1024 * 1024 });
    const stage = path.join(work, 'package');
    await mkdir(path.join(stage, 'lib'), { recursive: true });
    await mkdir(path.join(stage, 'include/libraw'), { recursive: true });
    await copyFile(path.join(build, spec.library), path.join(stage, 'lib', spec.library));
    for (const header of headers) await copyFile(path.join(source, 'libraw', header), path.join(stage, 'include/libraw', header));
    for (const notice of ['LICENSE.CDDL', 'COPYRIGHT']) await copyFile(path.join(source, notice), path.join(stage, notice));
    const info = { version: pinned.version, target: options.target, distribution: 'lutlab-prebuilt', sourceSha256: pinned.source.sha256, build: pinned.build, files: {} };
    for (const file of packageFiles(options.target).filter(file => file !== 'provenance.json')) info.files[file] = digest(await readFile(path.join(stage, file)));
    await writeFile(path.join(stage, 'provenance.json'), JSON.stringify(info, null, 2));
    const binary = path.join(work, 'prebuilt.tar.gz');
    await execute('tar', ['-czf', binary, '-C', stage, ...packageFiles(options.target)]);
    const sha256 = digest(await readFile(binary));
    await installPackage(binary, sha256, options.target, pinned);
    console.log(`[build-libraw] Ready: ${path.join(cacheDirectory(pinned.version, options.target), 'package.tar.gz')}\n[build-libraw] SHA256: ${sha256}`);
  } finally { await rm(work, { recursive: true, force: true }); }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(error => { console.error(`[build-libraw] ${error.message}`); process.exitCode = 1; });
