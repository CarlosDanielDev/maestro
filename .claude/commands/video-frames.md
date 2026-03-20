# Video Frame Extractor

Extract frames from a video using ffmpeg for visual analysis of UI issues like layout shifts, animation glitches, flickering, or crash moments.

## Usage

```
/video-frames <video_path> [options]
```

## Arguments

- `video_path`: Path to the video file (required)
- Options (parsed from natural language in `$ARGUMENTS`):
  - Second video path → enables side-by-side comparison
  - `from Xs to Ys` or `Xs-Ys` → time range
  - `Nfps` → custom frame rate
  - `save to /path` → custom output directory

## Examples

```bash
# Basic extraction
/video-frames /tmp/recording.mp4

# With time range
/video-frames /tmp/recording.mp4 from 2s to 5s

# Higher FPS for fast animations
/video-frames /tmp/recording.mp4 15fps

# Compare two videos side by side
/video-frames /tmp/before.mp4 /tmp/after.mp4

# Combined flags
/video-frames /tmp/recording.mp4 from 1s to 3s 10fps
```

## What This Command Does

1. Parses arguments from `$ARGUMENTS`
2. Invokes the `video-frame-extractor` skill
3. Extracts frames into a temp directory with timestamp-based names
4. Reports the output path and frame count so you can start reading the frames

## Workflow

### Step 1: Parse Arguments

Extract from `$ARGUMENTS`:
- First path found → primary video
- Second path found (if any) → comparison video (enables side-by-side mode)
- Pattern `from Xs to Ys` or `Xs-Ys` → start and end times
- Pattern `Nfps` → frames per second override
- Pattern `save to <path>` → custom output directory

### Step 2: Invoke Skill

Use the `video-frame-extractor` skill. Follow its instructions to:

1. **Validate** the video exists with `ls`
2. **Probe** the video with `ffprobe` to get duration and resolution
3. **Decide FPS** based on duration (or use user override)
4. **Extract frames** with `ffmpeg`
5. **Rename** frames with timestamps (`sec_0.00s.png`, `sec_0.20s.png`, etc.)
6. If comparison mode: extract both videos and create hstacked composites

### Step 3: Report Output

Print the output directory and frame count:
```
Frames extracted to: /tmp/tmp.XXXXXX/frames/
Total frames: 25
Duration: 5.0s at 5fps
```

Then read a few key frames (first, middle, last) to give the user an initial overview of the video content.

### Step 4: Wait for User

Ask the user what to look for, or if they already described the issue, start reading through the frames sequentially to identify it.

## Error Handling

- **ffmpeg not found**: Tell user to install with `brew install ffmpeg`
- **File not found**: Ask user to verify the path
- **Video too long (>60s) without time range**: Ask user to specify a time range
- **Corrupt video**: Report ffprobe error and ask for a different file
