import { execFile } from 'node:child_process';
import { access, chmod, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, '..');
const skip = process.env.SKIP_FFMPEG_BUNDLE_CHECK === '1' || process.env.SKIP_FFMPEG_BUNDLE_CHECK === 'true';

function toPosix(p) {
  return p.split(path.sep).join('/');
}

async function ensureFileExists(filePath) {
  try {
    await access(filePath);
    const s = await stat(filePath);
    if (!s.isFile()) throw new Error('not_a_file');
  } catch (e) {
    throw new Error(`Missing required file: ${toPosix(filePath)}`);
  }
}

async function ensureExecutableIfNeeded(filePath) {
  if (process.platform === 'win32') return;
  await chmod(filePath, 0o755);
}

async function versionLine(exePath) {
  const { stdout } = await execFileAsync(exePath, ['-version'], { timeout: 15_000 });
  return stdout.split(/\r?\n/)[0] ?? '';
}

function expectedFiles() {
  const base = path.join(root, 'src-tauri', 'resources', 'bin');

  if (process.platform === 'win32') {
    const dir = path.join(base, 'windows', 'x86_64');
    return {
      description: 'Windows x86_64',
      runnableDir: dir,
      allDirs: [dir],
      names: ['ffmpeg.exe', 'ffprobe.exe', 'ffplay.exe'],
    };
  }

  if (process.platform === 'darwin') {
    const armDir = path.join(base, 'macos', 'aarch64');
    const intelDir = path.join(base, 'macos', 'x86_64');
    const runnableDir = process.arch === 'arm64' ? armDir : intelDir;
    return {
      description: 'macOS universal (aarch64 + x86_64)',
      runnableDir,
      allDirs: [armDir, intelDir],
      names: ['ffmpeg', 'ffprobe', 'ffplay'],
    };
  }

  throw new Error(`Unsupported platform for Full bundle check: ${process.platform}`);
}

async function main() {
  if (skip) {
    console.log('[prepare-ffmpeg] SKIP_FFMPEG_BUNDLE_CHECK is set; skipping checks.');
    return;
  }

  const spec = expectedFiles();
  console.log(`[prepare-ffmpeg] Validating bundled FFmpeg binaries for ${spec.description}...`);

  // 1) Ensure files exist (for every expected dir)
  for (const dir of spec.allDirs) {
    for (const name of spec.names) {
      const p = path.join(dir, name);
      await ensureFileExists(p);
      await ensureExecutableIfNeeded(p);
    }
  }

  // 2) Run -version for the host-runnable slice (best-effort)
  for (const name of spec.names) {
    const exePath = path.join(spec.runnableDir, name);
    try {
      const line = await versionLine(exePath);
      console.log(`[prepare-ffmpeg] ${name}: ${line || '(no output)'}`);
    } catch (e) {
      // On macOS universal builds, the non-native slice may not be runnable without Rosetta.
      // Fail only if the native slice itself cannot run.
      throw new Error(`[prepare-ffmpeg] Failed to execute ${toPosix(exePath)} -version: ${e?.message ?? String(e)}`);
    }
  }

  console.log('[prepare-ffmpeg] OK');
}

main().catch((err) => {
  console.error(String(err?.message ?? err));
  console.error('');
  console.error('Fix: place the required binaries under:');
  console.error('  src-tauri/resources/bin/... (see src-tauri/resources/bin/README.md)');
  process.exit(1);
});
