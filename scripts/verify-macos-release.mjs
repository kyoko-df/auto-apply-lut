import { execFile } from 'node:child_process';
import { readFile, realpath, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { verifyRuntime } from './prepare-ffmpeg.mjs';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';

const execFileAsync = promisify(execFile);
const systemPath = '/usr/bin:/bin:/usr/sbin:/sbin';

export async function main(args = process.argv.slice(2)) {
  if (args.length !== 1 || !args[0].endsWith('.app')) {
    throw new Error('Usage: node scripts/verify-macos-release.mjs /path/to/auto-apply-lut.app');
  }
  if (process.platform !== 'darwin') throw new Error('The macOS release must be verified on macOS.');
  const architecture = { arm64: 'aarch64', x64: 'x86_64' }[process.arch];
  if (!architecture) throw new Error(`Unsupported macOS host architecture: ${process.arch}`);
  const application = await realpath(path.resolve(args[0]));
  if (!(await stat(application)).isDirectory()) throw new Error('The .app path must be a directory.');
  const executable = path.join(application, 'Contents', 'MacOS', 'auto-apply-lut');
  const engineDirectory = path.join(application, 'Contents', 'Resources', 'bin', 'macos', architecture);
  const binaries = [executable, ...['ffmpeg', 'ffprobe'].map((name) => path.join(engineDirectory, name))];

  console.log(`[verify-macos-release] Application: ${application}`);
  for (const binary of binaries) {
    const relative = path.relative(application, await realpath(binary));
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
      throw new Error(`Bundled executable points outside the application: ${binary}`);
    }
    if (!(await stat(binary)).isFile()) throw new Error(`Required executable is not a file: ${binary}`);
    const info = inspectBinary(await readFile(binary));
    if (info.platform !== 'darwin' || !info.slices.some((slice) => slice.arch === architecture)) {
      throw new Error(`Expected a Mach-O executable supporting ${architecture}: ${binary}`);
    }
    validateDependencies(info);
    const dependencies = new Set(info.slices.flatMap((slice) => slice.dependencies));
    console.log(`[verify-macos-release] ${path.relative(application, binary)}: ${info.slices.map((slice) => slice.arch).join(', ')}; ${dependencies.size} system dependencies; no external libraries.`);
  }

  // verifyRuntime reads process.env for each child. Temporarily narrow it here
  // so neither Homebrew/PATH nor user library search paths can hide a broken
  // application bundle. Restore the caller's environment even if a check fails.
  const previousEnvironment = { ...process.env };
  try {
    for (const key of Object.keys(process.env)) {
      if (/^(?:DYLD_|LD_|FFMPEG(?:_|$)|FFPROBE(?:_|$)|FFPLAY(?:_|$))/i.test(key)) delete process.env[key];
    }
    process.env.PATH = systemPath;
    console.log(`[verify-macos-release] Runtime PATH=${systemPath}; FFmpeg and library overrides removed.`);
    await verifyRuntime(engineDirectory, ['ffmpeg', 'ffprobe']);
    const { stdout, stderr } = await execFileAsync('/usr/bin/codesign', ['--verify', '--deep', '--strict', '--verbose=2', application], {
      env: { ...process.env }, timeout: 30_000, maxBuffer: 1024 * 1024,
    });
    for (const line of `${stdout}\n${stderr}`.trim().split(/\r?\n/).filter(Boolean)) {
      console.log(`[verify-macos-release] codesign: ${line}`);
    }
    console.log('[verify-macos-release] PASS: bundled engines encode H.264 / HEVC / ProRes with audio and create a JPEG preview using only system dependencies; application signature verification passed.');
  } finally {
    for (const key of Object.keys(process.env)) {
      if (!(key in previousEnvironment)) delete process.env[key];
    }
    Object.assign(process.env, previousEnvironment);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`[verify-macos-release] FAILED: ${error.message}`);
    process.exitCode = 1;
  });
}
