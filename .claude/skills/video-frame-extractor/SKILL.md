---
name: video-frame-extractor
version: "1.0.0"
description: "Extract frames from video files using ffmpeg for visual analysis. Use this skill whenever the user provides a video file (screen recording, simulator capture, device recording) and wants to analyze visual issues like layout shifts, animation glitches, flickering, crash moments, or any UI behavior that is hard to describe in words. Also trigger when the user wants to compare two videos side by side. Trigger on mentions of: video analysis, screen recording analysis, frame extraction, layout shift detection, visual glitch, UI flicker, animation bug, video comparison, ffmpeg frames."
allowed-tools: Bash, Read, Glob, Write
---

# Video Frame Extractor

Extract frames from video recordings using ffmpeg so the agent can visually inspect what happened. UI glitches, layout shifts, and animation bugs happen in milliseconds — too fast for a human to screenshot but trivial for an LLM to spot once the frames are laid out.

## How It Works

1. User provides a video path (and optionally a second video for comparison)
2. You run ffmpeg to extract frames into a temp directory
3. Frames are named with timestamps so the agent can navigate the timeline
4. The agent reads the frames and identifies what's going on

You are a frame extraction tool. Extract frames, organize them, and let the main agent do the analysis. Do not interpret what you see in the frames — just deliver them.

## Quick Reference

### Basic extraction (whole video)
```bash
# Create output directory
OUTPUT_DIR=$(mktemp -d)/frames
mkdir -p "$OUTPUT_DIR"

# Extract at 5 fps (good balance between coverage and file count)
ffmpeg -i "<video_path>" -vf "fps=5" -frame_pts 1 "$OUTPUT_DIR/frame_%04d.png" -y 2>&1

# Rename frames with actual timestamps
# fps=5 means each frame is 0.2s apart
for f in "$OUTPUT_DIR"/frame_*.png; do
  num=$(echo "$f" | grep -o '[0-9]\{4\}' | tail -1)
  # Frame numbers start at 1, convert to seconds
  sec=$(echo "scale=2; ($num - 1) * 0.2" | bc)
  mv "$f" "$OUTPUT_DIR/sec_${sec}s.png"
done

echo "Frames extracted to: $OUTPUT_DIR"
```

### Custom time range
```bash
# Extract frames between specific timestamps
# -ss = start time, -to = end time
ffmpeg -i "<video_path>" -ss <start> -to <end> -vf "fps=5" -frame_pts 1 "$OUTPUT_DIR/frame_%04d.png" -y 2>&1
```

### Higher FPS for fast animations
```bash
# Use 10-15 fps when analyzing rapid transitions or animations
ffmpeg -i "<video_path>" -vf "fps=15" -frame_pts 1 "$OUTPUT_DIR/frame_%04d.png" -y 2>&1
```

### Two-video side-by-side comparison
```bash
# Extract frames from both videos
OUTPUT_A=$(mktemp -d)/video_a
OUTPUT_B=$(mktemp -d)/video_b
mkdir -p "$OUTPUT_A" "$OUTPUT_B"

ffmpeg -i "<video_a_path>" -vf "fps=5" -frame_pts 1 "$OUTPUT_A/frame_%04d.png" -y 2>&1
ffmpeg -i "<video_b_path>" -vf "fps=5" -frame_pts 1 "$OUTPUT_B/frame_%04d.png" -y 2>&1

# Create side-by-side composites
COMPARE_DIR=$(mktemp -d)/comparison
mkdir -p "$COMPARE_DIR"

# Get the lesser frame count to avoid mismatches
COUNT_A=$(ls "$OUTPUT_A"/frame_*.png 2>/dev/null | wc -l | tr -d ' ')
COUNT_B=$(ls "$OUTPUT_B"/frame_*.png 2>/dev/null | wc -l | tr -d ' ')
COUNT=$(( COUNT_A < COUNT_B ? COUNT_A : COUNT_B ))

for i in $(seq -f "%04g" 1 $COUNT); do
  sec=$(echo "scale=2; ($i - 1) * 0.2" | bc)
  ffmpeg -i "$OUTPUT_A/frame_${i}.png" -i "$OUTPUT_B/frame_${i}.png" \
    -filter_complex "[0]pad=iw+10:ih[left];[left][1]hstack" \
    "$COMPARE_DIR/compare_sec_${sec}s.png" -y 2>/dev/null
done

echo "Comparison frames: $COMPARE_DIR"
echo "Video A frames: $OUTPUT_A"
echo "Video B frames: $OUTPUT_B"
```

## Flags

The user can control extraction with these flags. Parse them from the user's message:

| Flag | Purpose | Default |
|------|---------|---------|
| Time range | `-ss 2 -to 5` or "from 2s to 5s" or "between 2-5 seconds" | Whole video |
| FPS | "at 10fps" or "15 frames per second" | 5 fps |
| Compare | Second video path provided, or "compare", "side by side" | Off |
| Output dir | "save to /path" | System temp dir |

## Execution Steps

1. **Validate input**: Check the video file exists. Run `ffprobe` to get duration and resolution:
   ```bash
   ffprobe -v quiet -print_format json -show_format -show_streams "<video_path>" 2>&1
   ```

2. **Decide FPS**: Default to 5. If user asks for more detail or video is short (<3s), bump to 10-15. If video is long (>30s) and no time range given, consider dropping to 2-3 fps to avoid excessive frames.

3. **Extract frames**: Use the appropriate command from above.

4. **Rename with timestamps**: So the agent reading the frames knows exactly when each frame occurred in the video.

5. **Report**: Print the output directory path and frame count. If comparison mode, print all three directories (video A, video B, comparison).

## Frame Count Guidelines

LLMs have limits on how many images they can process effectively. Keep total frame count reasonable:

| Video Duration | Recommended FPS | Approx Frames |
|---------------|----------------|---------------|
| < 3 seconds | 10-15 | 15-45 |
| 3-10 seconds | 5 | 15-50 |
| 10-30 seconds | 3-5 | 30-150 |
| 30-60 seconds | 2-3 | 60-180 |
| > 60 seconds | Require time range from user | — |

For videos over 60 seconds without a specified time range, ask the user to narrow down the section of interest. Processing an entire long video wastes tokens and dilutes focus.

## Output Structure

```
/tmp/tmp.XXXXXX/frames/
├── sec_0.00s.png
├── sec_0.20s.png
├── sec_0.40s.png
├── sec_0.60s.png
├── ...
└── sec_N.NNs.png
```

For comparison mode:
```
/tmp/tmp.XXXXXX/comparison/
├── compare_sec_0.00s.png    # Side-by-side A|B
├── compare_sec_0.20s.png
└── ...

/tmp/tmp.XXXXXX/video_a/
├── frame_0001.png
└── ...

/tmp/tmp.XXXXXX/video_b/
├── frame_0001.png
└── ...
```

## Important Notes

- Always use `-y` flag to overwrite without prompting
- Redirect ffmpeg stderr with `2>&1` so you can see errors
- Use `-v quiet` on ffprobe to get clean JSON output
- Frame numbering starts at 1, timestamps start at 0
- The `bc` calculator is used for timestamp math — it's available on macOS and Linux
- If ffmpeg is not installed, tell the user: `brew install ffmpeg` (macOS) or `apt install ffmpeg` (Linux)
