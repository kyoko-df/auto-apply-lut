import { execFile } from 'node:child_process';
import { access, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const architecture = { arm64: 'aarch64', x64: 'x86_64' }[process.arch];
const suffix = process.platform === 'win32' ? '.exe' : '';

function parseArguments(args) {
  const result = { seconds: 2, output: null, engineDirectory: null };
  for (let index = 0; index < args.length; index += 1) {
    const flag = args[index];
    const value = args[++index];
    if (!value) throw new Error(`Missing value for ${flag}`);
    if (flag === '--seconds') result.seconds = Number(value);
    else if (flag === '--output') result.output = path.resolve(value);
    else if (flag === '--engine-dir') result.engineDirectory = path.resolve(value);
    else throw new Error(`Unknown argument ${flag}`);
  }
  if (!Number.isFinite(result.seconds) || result.seconds < 1 || result.seconds > 10) {
    throw new Error('--seconds must be between 1 and 10');
  }
  return result;
}

function engineDirectory(options) {
  if (options.engineDirectory) return options.engineDirectory;
  if (process.platform === 'darwin' && architecture) {
    return path.join(root, 'src-tauri', 'target', `${architecture}-apple-darwin`, 'release', 'bundle', 'macos', 'LUTlab.app', 'Contents', 'Resources', 'bin', 'macos', architecture);
  }
  if (process.platform === 'win32' && process.arch === 'x64') {
    return path.join(root, 'src-tauri', 'resources', 'bin', 'windows', 'x86_64');
  }
  throw new Error('Pass --engine-dir containing the actual bundled ffmpeg and ffprobe for this platform.');
}

async function run(file, args, extra = {}) {
  return exec(file, args, { cwd: root, timeout: 600_000, maxBuffer: 8 * 1024 * 1024, ...extra });
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const engineDir = engineDirectory(options);
  const ffmpeg = path.join(engineDir, `ffmpeg${suffix}`);
  const ffprobe = path.join(engineDir, `ffprobe${suffix}`);
  await Promise.all([access(ffmpeg), access(ffprobe)]).catch(() => {
    throw new Error(`Bundled engines missing at ${engineDir}. Build the Full application first, or pass --engine-dir. No system FFmpeg fallback is used.`);
  });
  const temporary = await mkdtemp(path.join(os.tmpdir(), 'lutlab-benchmark-'));
  const runtimeEnvironment = { ...process.env };
  for (const key of Object.keys(runtimeEnvironment)) {
    if (/^(?:DYLD_|LD_|FFMPEG(?:_|$)|FFPROBE(?:_|$))/i.test(key)) delete runtimeEnvironment[key];
  }
  if (process.platform === 'darwin') runtimeEnvironment.PATH = '/usr/bin:/bin:/usr/sbin:/sbin';
  try {
    const version = (await run(ffmpeg, ['-version'], { env: runtimeEnvironment })).stdout.split(/\r?\n/)[0];
    console.error('[benchmark-media] Compiling the production export harness (excluded from measurements).');
    await run('cargo', ['build', '--quiet', '--manifest-path', 'src-tauri/Cargo.toml', '--example', 'media_benchmark']);
    console.error(`[benchmark-media] Generating ${options.seconds}s of 3840×2160 / 30 fps video with audio (excluded from measurements).`);
    await run(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-f', 'lavfi', '-i', 'testsrc2=size=3840x2160:rate=30', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', String(options.seconds), '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '18', '-threads', '4', '-c:a', 'aac', '-pix_fmt', 'yuv420p', path.join(temporary, 'source-4k.mp4')], { env: runtimeEnvironment });
    const cube = ['TITLE "Benchmark Warm"', 'LUT_3D_SIZE 2'];
    for (let blue = 0; blue < 2; blue += 1) for (let green = 0; green < 2; green += 1) for (let red = 0; red < 2; red += 1) {
      cube.push(`${0.08 + red * 0.92} ${green * 0.96} ${blue * 0.84}`);
    }
    await writeFile(path.join(temporary, 'warm.cube'), `${cube.join('\n')}\n`);
    const reportPath = path.join(temporary, 'runs.json');
    console.error('[benchmark-media] Running original software, 50% LUT software, and 50% LUT automatic hardware exports through the app engine.');
    await run(path.join(root, 'src-tauri', 'target', 'debug', 'examples', `media_benchmark${suffix}`), [ffmpeg, temporary, reportPath], { env: runtimeEnvironment });
    const runs = JSON.parse(await readFile(reportPath, 'utf8'));
    for (const result of runs) {
      if (!result.success) continue;
      const probe = JSON.parse((await run(ffprobe, ['-v', 'error', '-count_frames', '-show_streams', '-show_format', '-of', 'json', result.output_path], { env: runtimeEnvironment })).stdout);
      const video = probe.streams.find((stream) => stream.codec_type === 'video');
      const audio = probe.streams.some((stream) => stream.codec_type === 'audio');
      const frames = Number(video?.nb_read_frames);
      result.validation = { width: video?.width, height: video?.height, audio_preserved: audio, duration_seconds: Number(probe.format.duration), frames };
      result.frames_per_second = frames / result.elapsed_seconds;
      result.media_seconds_per_second = Number(probe.format.duration) / result.elapsed_seconds;
      result.used_hardware = Boolean(result.actual_encoder && result.actual_encoder !== 'libx264');
      delete result.output_path;
      if (video?.width !== 3840 || video?.height !== 2160 || !audio || !Number.isFinite(frames) || Math.abs(frames - Math.ceil(options.seconds * 30)) > 1 || Math.abs(Number(probe.format.duration) - options.seconds) > 0.2) {
        result.success = false;
        result.error = 'Export output failed 4K/audio/frame-count verification.';
      }
    }
    const report = {
      schema_version: 1, timestamp: new Date().toISOString(),
      machine: { platform: process.platform, architecture: process.arch, os_release: os.release(), cpu: os.cpus()[0]?.model, logical_cores: os.cpus().length, memory_gib: Number((os.totalmem() / 1024 ** 3).toFixed(1)) },
      engine: { path: ffmpeg, version },
      fixture: { width: 3840, height: 2160, fps: 30, duration_seconds: options.seconds, audio: true, content: 'FFmpeg testsrc2' },
      measurement: 'Single-run wall time of the production LUT export pipeline. Fixture generation, harness compilation and output validation are excluded. Automatic hardware may fall back to software; inspect actual_encoder.',
      runs,
    };
    const json = `${JSON.stringify(report, null, 2)}\n`;
    if (options.output) await writeFile(options.output, json, { flag: 'wx' });
    process.stdout.write(json);
    if (runs.some((result) => !result.success)) process.exitCode = 1;
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`[benchmark-media] FAILED: ${error.message}`);
  process.exitCode = 1;
});
