const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

/**
 * 封装 auto-apply-lut 的核心能力为 CLI 脚本
 * 让 Agent 可以直接调用来处理视频
 */

function printUsage() {
  console.log(`
Usage: node apply-lut.js <input_video> <lut_file> <output_video> [intensity]

Arguments:
  input_video   Path to the input video file
  lut_file      Path to the .cube LUT file
  output_video  Path to the output video file
  intensity     (Optional) LUT intensity from 0.0 to 1.0, default is 1.0

Example:
  node apply-lut.js input.mp4 test.cube output.mp4 0.8
  `);
}

async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 3) {
    printUsage();
    process.exit(1);
  }

  const inputVideo = path.resolve(args[0]);
  const lutFile = path.resolve(args[1]);
  const outputVideo = path.resolve(args[2]);
  const intensity = args[3] ? parseFloat(args[3]) : 1.0;

  if (!fs.existsSync(inputVideo)) {
    console.error(`Error: Input video not found at ${inputVideo}`);
    process.exit(1);
  }

  if (!fs.existsSync(lutFile)) {
    console.error(`Error: LUT file not found at ${lutFile}`);
    process.exit(1);
  }

  console.log(`[Auto Apply LUT] Starting processing...`);
  console.log(`Input:     ${inputVideo}`);
  console.log(`LUT:       ${lutFile}`);
  console.log(`Output:    ${outputVideo}`);
  console.log(`Intensity: ${intensity}`);

  // Escape single quotes for ffmpeg filter
  const escapedLutPath = lutFile.replace(/'/g, "\\'");
  
  let vfFilter = '';
  const clampedIntensity = Math.max(0.0, Math.min(1.0, intensity));
  
  if (clampedIntensity >= 1.0) {
    vfFilter = `lut3d=file='${escapedLutPath}',format=yuv422p`;
  } else {
    const weightOrig = (1.0 - clampedIntensity).toFixed(4);
    const weightLut = clampedIntensity.toFixed(4);
    vfFilter = `split[orig][lut];[lut]lut3d=file='${escapedLutPath}',format=yuv422p[lutted];[orig]format=yuv422p[origfmt];[origfmt][lutted]mix=weights=${weightOrig} ${weightLut}`;
  }

  const ffmpegArgs = [
    '-i', inputVideo,
    '-vf', vfFilter,
    '-c:v', 'libx264',
    '-preset', 'fast',
    '-crf', '23',
    '-c:a', 'copy',
    '-y',
    outputVideo
  ];

  console.log(`Running FFmpeg: ffmpeg ${ffmpegArgs.join(' ')}`);

  const ffmpeg = spawn('ffmpeg', ffmpegArgs, { stdio: 'inherit' });

  ffmpeg.on('close', (code) => {
    if (code === 0) {
      console.log(`\n[Success] Video processed successfully!`);
      console.log(`Saved to: ${outputVideo}`);
    } else {
      console.error(`\n[Error] FFmpeg exited with code ${code}`);
      process.exit(code);
    }
  });
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
