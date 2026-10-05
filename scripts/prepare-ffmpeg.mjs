import { execFile } from 'node:child_process';
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { inspectBinary, validateDependencies } from './validate-bundled-binary.mjs';

const execFileAsync = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function targetSpec(target) {
  const mac = (arch) => ({ platform: 'darwin', arch, directory: `macos/${arch}`, names: ['ffmpeg', 'ffprobe'] });
  switch (target) {
    case 'aarch64-apple-darwin': return [mac('aarch64')];
    case 'x86_64-apple-darwin': return [mac('x86_64')];
    case 'universal-apple-darwin': return [mac('aarch64'), mac('x86_64')];
    case 'x86_64-pc-windows-msvc': return [{ platform: 'win32', arch: 'x86_64', directory: 'windows/x86_64', names: ['ffmpeg.exe', 'ffprobe.exe'] }];
    default: throw new Error(`Unsupported standalone FFmpeg target: ${target}. Supported: macOS arm64 / Intel / universal and Windows x64.`);
  }
}

export function resolveOptions(args = process.argv.slice(2), env = process.env, platform = process.platform, arch = process.arch) {
  const options = {};
  for (let i = 0; i < args.length; i += 1) {
    const match = /^(--target|--vendor-dir)(?:=(.*))?$/.exec(args[i]);
    if (!match) throw new Error(`Unknown argument: ${args[i]}`);
    const value = match[2] ?? args[++i];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${match[1]}`);
    options[match[1].slice(2)] = value;
  }
  if (env.SKIP_FFMPEG_BUNDLE_CHECK === '1' || env.SKIP_FFMPEG_BUNDLE_CHECK === 'true') {
    throw new Error('Full releases cannot skip FFmpeg validation. Use the Lite build for a system FFmpeg dependency.');
  }
  const hostTarget = platform === 'darwin'
    ? `${arch === 'arm64' ? 'aarch64' : arch === 'x64' ? 'x86_64' : arch}-apple-darwin`
    : platform === 'win32' && arch === 'x64' ? 'x86_64-pc-windows-msvc' : `${arch}-${platform}`;
  const target = options.target || env.FFMPEG_BUNDLE_TARGET || env.TAURI_ENV_TARGET_TRIPLE || hostTarget;
  return { target, specs: targetSpec(target), vendorDir: options['vendor-dir'] || env.FFMPEG_VENDOR_DIR };
}

async function execute(executable, args, options = {}) {
  return execFileAsync(executable, args, {
    timeout: 30_000, maxBuffer: 8 * 1024 * 1024, windowsHide: true,
    // No user-installed library search paths may make an otherwise broken bundle pass.
    env: { ...process.env, DYLD_LIBRARY_PATH: '', DYLD_FALLBACK_LIBRARY_PATH: '', LD_LIBRARY_PATH: '' },
    ...options,
  });
}

function requireCapabilities(output, names, category) {
  for (const name of names) {
    if (!output.split(/\r?\n/).some((line) => line.trim().split(/\s+/)[1]?.split(',').includes(name))) {
      throw new Error(`Bundled FFmpeg is missing required ${category}: ${name}`);
    }
  }
}

export async function verifyRuntime(directory, names) {
  const [ffmpeg, ffprobe] = names.map((name) => path.join(directory, name));
  for (const executable of [ffmpeg, ffprobe]) {
    const { stdout } = await execute(executable, ['-version']);
    console.log(`[prepare-ffmpeg] ${stdout.split(/\r?\n/)[0]}`);
  }
  const [{ stdout: encoders }, { stdout: filters }, { stdout: muxers }] = await Promise.all([
    execute(ffmpeg, ['-hide_banner', '-encoders']), execute(ffmpeg, ['-hide_banner', '-filters']),
    execute(ffmpeg, ['-hide_banner', '-muxers']),
  ]);
  requireCapabilities(encoders, ['libx264', 'libx265', 'prores_ks', 'aac', 'mjpeg', 'ffv1'], 'encoder');
  requireCapabilities(filters, ['lut3d', 'lut1d', 'blend', 'split', 'format', 'scale', 'setsar', 'pad', 'zscale', 'tonemap'], 'filter');
  requireCapabilities(muxers, ['mp4', 'mov', 'matroska', 'image2', 'nut'], 'muxer');
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'lutlab-bundle-check-'));
  try {
    await writeFile(path.join(workspace, 'identity.cube'), 'LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n');
    for (const [codec, pixelFormat, extension] of [
      ['libx264', 'yuv420p', 'mp4'], ['libx265', 'yuv420p', 'mp4'], ['prores_ks', 'yuv422p10le', 'mov'],
    ]) {
      const output = path.join(workspace, `${codec}.${extension}`);
      await execute(ffmpeg, ['-hide_banner', '-v', 'error', '-f', 'lavfi', '-i', 'color=c=red:s=64x64:r=4',
        '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '0.5',
        '-filter_complex_threads', '1', '-filter_complex',
        '[0:v]format=gbrp16le,split[original][input];[input]lut3d=file=identity.cube:interp=tetrahedral[graded];[original][graded]blend=all_mode=normal:all_opacity=0.5,format=' + pixelFormat + '[video]',
        '-map', '[video]', '-map', '1:a', '-c:v', codec, '-threads', '1', '-c:a', 'aac', output], { cwd: workspace });
      const { stdout } = await execute(ffprobe, ['-v', 'error', '-show_streams', '-of', 'json', output]);
      const streams = JSON.parse(stdout).streams;
      if (!streams.some((stream) => stream.codec_type === 'video') || !streams.some((stream) => stream.codec_type === 'audio')) {
        throw new Error(`${codec}: real encode/probe validation did not produce video and audio.`);
      }
    }
    await execute(ffmpeg, ['-hide_banner', '-v', 'error', '-i', path.join(workspace, 'libx264.mp4'),
      '-frames:v', '1', '-c:v', 'mjpeg', '-threads', '1', path.join(workspace, 'preview.jpg')]);
    if ((await stat(path.join(workspace, 'preview.jpg'))).size === 0) throw new Error('Preview JPEG is empty.');
    // The preview cache stores a lossless RGB16 working frame, then grades that
    // frame without seeking/decoding the source video on each strength edit.
    const workingFrame = path.join(workspace, 'working.nut');
    await execute(ffmpeg, ['-hide_banner', '-v', 'error', '-i', path.join(workspace, 'libx264.mp4'),
      '-frames:v', '1', '-an', '-vf', 'format=gbrp16le', '-c:v', 'ffv1', '-level', '3',
      '-g', '1', '-threads', '1', workingFrame]);
    const { stdout: frameInfo } = await execute(ffprobe, ['-v', 'error', '-show_streams', '-of', 'json', workingFrame]);
    const frameStream = JSON.parse(frameInfo).streams.find((stream) => stream.codec_type === 'video');
    if (frameStream?.codec_name !== 'ffv1' || frameStream.pix_fmt !== 'gbrp16le') {
      throw new Error('Preview cache did not preserve a lossless 16-bit RGB working frame.');
    }
    await execute(ffmpeg, ['-hide_banner', '-v', 'error', '-i', workingFrame, '-vf', 'lut3d=file=identity.cube',
      '-frames:v', '1', '-c:v', 'mjpeg', '-threads', '1', path.join(workspace, 'cached-preview.jpg')], { cwd: workspace });
    if ((await stat(path.join(workspace, 'cached-preview.jpg'))).size === 0) throw new Error('Cached preview JPEG is empty.');
    console.log('[prepare-ffmpeg] Real H.264 / HEVC / ProRes + audio exports, JPEG preview and RGB16 FFV1 cache passed.');
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
}

export async function main() {
  const options = resolveOptions();
  const base = path.join(root, 'src-tauri', 'resources', 'bin');
  console.log(`[prepare-ffmpeg] Validating standalone bundle for ${options.target}`);
  // Validate every vendor file before copying any of it into the resource tree.
  const checked = [];
  for (const spec of options.specs) {
    for (const name of spec.names) {
      const source = path.join(options.vendorDir ? path.resolve(options.vendorDir) : base, spec.directory, name);
      if (!(await stat(source)).isFile()) throw new Error(`Required binary is not a file: ${source}`);
      const info = inspectBinary(await readFile(source));
      if (info.platform !== spec.platform || !info.slices.some((slice) => slice.arch === spec.arch)) {
        throw new Error(`Wrong binary architecture for ${source}: expected ${spec.platform}/${spec.arch}`);
      }
      validateDependencies(info);
      checked.push({ source, destination: path.join(base, spec.directory, name) });
      console.log(`[prepare-ffmpeg] ${spec.directory}/${name}: architecture and standalone dependencies OK`);
    }
  }
  for (const { source, destination } of checked) {
    await mkdir(path.dirname(destination), { recursive: true });
    if (path.resolve(source) !== path.resolve(destination)) await copyFile(source, destination);
    if (process.platform !== 'win32') await chmod(destination, 0o755);
  }
  for (const spec of options.specs) {
    const hostArch = process.arch === 'arm64' ? 'aarch64' : process.arch === 'x64' ? 'x86_64' : process.arch;
    if (spec.platform === process.platform && spec.arch === hostArch) {
      await verifyRuntime(path.join(base, spec.directory), spec.names);
    } else {
      console.log(`[prepare-ffmpeg] ${spec.directory}: static checks passed; runtime checks require a native ${spec.arch} ${spec.platform} runner.`);
    }
  }
  console.log('[prepare-ffmpeg] OK');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`[prepare-ffmpeg] ${error.message}`);
    console.error('Supply standalone ffmpeg + ffprobe at src-tauri/resources/bin/<platform>/<arch>, or set FFMPEG_VENDOR_DIR to a directory with the same layout.');
    process.exitCode = 1;
  });
}
