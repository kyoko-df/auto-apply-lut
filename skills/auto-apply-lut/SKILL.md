---
name: auto-apply-lut
description: Applies a LUT (Look-Up Table, .cube) to a video file for color grading, matching the core capability of the Auto Apply LUT project.
---

# Auto Apply LUT Skill

## When to use
- When the user asks you to apply a LUT to a video.
- When you need to perform color grading or correction on a video using a `.cube` file.
- When you need to batch process multiple videos with a specific LUT.
- When testing or demonstrating the core capability of the `auto-apply-lut` project without using its graphical user interface.

## Prerequisites
- **FFmpeg**: This skill relies on FFmpeg being installed on the system (specifically the `lut3d` filter).
- **Input Video**: A valid video file path.
- **LUT File**: A valid 3D LUT file path (usually `.cube` format).

## How to use

We have encapsulated the core LUT application logic (including intensity blending) into an executable Node.js script located at `skills/auto-apply-lut/apply-lut.js`.

### 1. Basic Usage (CLI Script)
You can directly invoke the provided Node.js script to process a video. The script replicates the exact FFmpeg filter logic used by the Rust backend.

```bash
node skills/auto-apply-lut/apply-lut.js <input_video> <lut_file> <output_video> [intensity]
```

- `input_video`: Path to the input video (e.g., `input.mp4`).
- `lut_file`: Path to the `.cube` LUT file (e.g., `SLog3_To_Rec709.cube`).
- `output_video`: Path where the graded video will be saved.
- `intensity`: (Optional) Float value between `0.0` and `1.0` representing the strength of the LUT effect. Default is `1.0`.

**Example:**
```bash
node skills/auto-apply-lut/apply-lut.js ./raw_footage.mp4 ./cinematic.cube ./graded_footage.mp4 0.8
```

### 2. Manual FFmpeg Command (Alternative)
If you prefer to run the FFmpeg command directly without the script, you can use the following syntax:

**For 100% Intensity (1.0):**
```bash
ffmpeg -i "$INPUT_VIDEO" -vf "lut3d=file='$LUT_FILE',format=yuv422p" -c:v libx264 -preset fast -crf 23 -c:a copy -y "$OUTPUT_VIDEO"
```

**For Partial Intensity (e.g., 0.8):**
*Note: We use the `split` and `mix` filters to blend the original video with the LUT-applied video based on the specified weights.*
```bash
ffmpeg -i "$INPUT_VIDEO" -vf "split[orig][lut];[lut]lut3d=file='$LUT_FILE',format=yuv422p[lutted];[orig]format=yuv422p[origfmt];[origfmt][lutted]mix=weights=0.2000 0.8000" -c:v libx264 -preset fast -crf 23 -c:a copy -y "$OUTPUT_VIDEO"
```

## Batch Processing
To batch process a directory of videos, write a small shell loop or script that calls `apply-lut.js` for each file.

```bash
for file in ./input_dir/*.mp4; do
  filename=$(basename "$file")
  node skills/auto-apply-lut/apply-lut.js "$file" "./my_lut.cube" "./output_dir/$filename"
done
```

## Troubleshooting
- **LUT file path issues**: FFmpeg's `lut3d` filter requires single quotes to be escaped. The `apply-lut.js` script handles this automatically. If running manually, replace `'` with `\'`.
- **Pixel format errors**: Ensure `format=yuv422p` is appended after the `lut3d` filter to avoid pixel format incompatibility with the encoder.
- **FFmpeg not found**: Install it via `brew install ffmpeg` (macOS), `apt install ffmpeg` (Linux), or `winget install ffmpeg` (Windows).
