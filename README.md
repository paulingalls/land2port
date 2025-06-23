# Land2Port

A powerful video processing tool that automatically detects heads in videos, crops them to portrait format (9:16 aspect ratio), and adds AI-generated transcriptions. Perfect for converting landscape videos to portrait format for social media platforms like TikTok, Instagram Reels, and YouTube Shorts.

## Features

- **🎯 Face or Head Detection**: Uses YOLO models to detect faces or heads in video frames with high accuracy
- **📱 Portrait Cropping**: Automatically crops videos to 9:16 aspect ratio for mobile viewing
- **🎬 Smart Cropping Logic**: 
  - Single head: Centers crop on the detected head
  - Multiple heads: Intelligently positions crops to capture all subjects
  - Stacked crops when appropriate: Creates two 9:8 crops stacked vertically for 2 or 3-5 heads
- **🎙️ AI Transcription**: Generates SRT captions using OpenAI Whisper
- **🎨 Caption Styling**: Customizable caption appearance with fonts, colors, and positioning
- **⚡ Smooth Transitions**: Prevents jarring crop changes with intelligent smoothing
- **🔧 Flexible Configuration**: Extensive command-line options for customization

## Installation

### Prerequisites

- **Rust** (latest stable version)
- **ffmpeg** (for video processing)
- **OpenAI API Key** (for transcription)

### Install ffmpeg

**macOS:**
```bash
brew install ffmpeg
```

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install ffmpeg
```

**Windows:**
Download from [ffmpeg.org](https://ffmpeg.org/download.html)

### Build from Source

```bash
git clone https://github.com/yourusername/land2port.git
cd land2port
cargo build --release
```

## Usage

### Basic Usage

```bash
# Process a video with default settings
cargo run --release -- --source ./video/input.mp4

# Process with headless mode (no GUI)
cargo run --release -- --source ./video/input.mp4 --headless
```

### Advanced Usage

```bash
# Process with custom settings
cargo run --release -- \
  --source ./video/input.mp4 \
  --headless \
  --use-stack-crop \
  --smooth-percentage 5.0 \
  --smooth-duration 30 \
  --device cuda:0 \
  --scale l
```

### Command Line Options

#### Input/Output
- `--source <FILE>`: Input video file (default: `./video/video1.mp4`)
- `--model <FILE>`: Custom YOLO model file (.onnx) See models in `./models`

#### Model Configuration
- `--task <TASK>`: Detection task - `det`, `seg`, `pose`, `classify`, `obb` (default: `det`)
- `--device <DEVICE>`: Processing device - `cpu:0`, `cuda:0`, `mps` (default: `cpu:0`)
- `--scale <SCALE>`: Model scale - `n`, `s`, `m`, `l`, `x` (default: `m`)
- `--dtype <DTYPE>`: Model data type - `auto`, `f32`, `f16` (default: `auto`)
- `--ver <VERSION>`: YOLO version (default: `8.0`)

#### Cropping Options
- `--use-stack-crop`: Enable stacked crop mode for wide scenes
- `--smooth-percentage <FLOAT>`: Smoothing threshold percentage (default: `10.0`)
- `--smooth-duration <INT>`: Smoothing duration in frames (default: `45`)

#### Processing Options
- `--headless`: Run without GUI display
- `--batch-size <INT>`: Batch size for processing (default: `1`)
- `--image-width <INT>`: Input image width (default: `640`)
- `--image-height <INT>`: Input image height (default: `640`)

#### Detection Options
- `--confs <FLOAT...>`: Confidence thresholds (default: `[0.2, 0.15]`)
- `--topk <INT>`: Top-k detections (default: `5`)

## How It Works

### 1. Head Detection
The tool uses YOLO models to detect heads in each video frame. It filters detections by confidence threshold to ensure accuracy.

### 2. Crop Calculation
Based on the number of detected heads, the tool calculates optimal crop areas:

- **0 heads**: Centered crop with 3:4 aspect ratio
- **1 head**: Crop centered on the detected head
- **2 heads**: 
  - If heads are close: Single crop containing both
  - If heads are far apart: Two stacked crops (when `--use-stack-crop` is enabled)
- **3-5 heads**: Similar logic to 2 heads
- **6+ heads**: Crop based on the largest detected head

### 3. Smoothing
To prevent jarring transitions, the tool implements intelligent smoothing:
- Compares crop similarity using percentage thresholds
- Maintains crop consistency for a configurable number of frames
- Smooths transitions between different crop types

### 4. Video Processing
- Crops each frame according to the calculated areas
- Maintains 9:16 aspect ratio for portrait output
- Processes frames at the original video's frame rate

### 5. Transcription
- Extracts audio from the video
- Compresses to MP3 format
- Uses OpenAI Whisper to generate SRT captions
- Burns captions into the final video

## Output Structure

The tool creates a timestamped output directory with the following files:

```
runs/20241201_143022/
├── extracted_audio.mp4      # Original audio track
├── compressed_audio.mp3     # Compressed audio for transcription
├── transcript.srt          # Generated captions
├── processed_video.mp4     # Cropped video without audio
├── captioned_video.mp4     # Video with burned-in captions
└── final_output.mp4        # Final video with audio
```

## Configuration

### Environment Variables

Set your OpenAI API key for transcription:
```bash
export OPENAI_API_KEY="your-api-key-here"
```

### Model Files

Place YOLO model files in the `model/` directory. The tool includes several pre-trained models for face detection:
- `yolov10n-face.onnx` (nano)
- `yolov10s-face.onnx` (small)
- `yolov10m-face.onnx` (medium)
- `yolov10l-face.onnx` (large)
- `yolov11n-face.onnx` (v11 nano)
- `yolov11s-face.onnx` (v11 small)
- `yolov11m-face.onnx` (v11 medium)
- `yolov11l-face.onnx` (v11 large)
- `v8-head-fp16.onnx` (v8 head detection)

## Examples

### Convert a landscape interview to portrait
```bash
cargo run --release -- \
  --model ./models/yolov11s-face.onnx \ 
  --version 11.0 \
  --source interview.mp4 \
  --headless \
  --smooth-percentage 5.0 \
  --smooth-duration 60
```

### Process a wide group shot with stacked crops
```bash
cargo run --release -- \
  --model ./models/yolov11s-face.onnx \ 
  --version 11.0 \
  --source group_shot.mp4 \
  --headless \
  --use-stack-crop \
  --smooth-percentage 8.0
```

### High-quality processing with GPU acceleration
```bash
cargo run --release -- \
  --model ./models/yolov11s-face.onnx \ 
  --version 11.0 \
  --source high_quality.mp4 \
  --device cuda:0 \
  --scale l \
  --image-width 1920 \
  --image-height 1080 \
  --headless
```

## Performance Tips

- **GPU Acceleration**: Use `--device cuda:0` for faster processing
- **Model Size**: Larger models (`--scale l` or `--scale x`) provide better accuracy but slower processing
- **Batch Size**: Increase `--batch-size` for faster processing if you have sufficient memory
- **Headless Mode**: Use `--headless` for faster processing without GUI overhead

## Troubleshooting

### Common Issues

1. **ffmpeg not found**: Install ffmpeg and ensure it's in your PATH
2. **CUDA errors**: Ensure CUDA drivers and toolkit are properly installed
3. **Memory issues**: Reduce batch size or image dimensions
4. **Transcription fails**: Check your OpenAI API key and internet connection

### Debug Mode

Run with verbose output to debug issues:
```bash
RUST_LOG=debug cargo run --release -- --source video.mp4
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development Setup

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and add tests
4. Run tests: `cargo test`
5. Commit your changes: `git commit -m 'Add amazing feature'`
6. Push to the branch: `git push origin feature/amazing-feature`
7. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [USLS](https://github.com/paulingalls/usls) - Computer vision library
- [OpenAI Whisper](https://openai.com/research/whisper) - Speech recognition
- [YOLO](https://github.com/ultralytics/yolov5) - Object detection models

## Support

If you encounter any issues or have questions, please:
1. Check the [Issues](https://github.com/yourusername/land2port/issues) page
2. Create a new issue with detailed information about your problem
3. Include your system information and command used

---

**Made with ❤️ for content creators** 