# Land2Port

A powerful video processing tool that automatically detects objects like faces or heads in videos, then crops them to portrait format (9:16 aspect ratio), and adds AI-generated transcriptions. Perfect for converting landscape videos to portrait format for social media platforms like TikTok, Instagram Reels, and YouTube Shorts.

## Features

- **🎯 Object Detection**: Uses YOLO models to detect faces, heads, footballs, sports balls, frisbees, persons, cars, trucks, or boats in video frames with high accuracy
- **📱 Portrait Cropping**: Automatically crops videos to 9:16 aspect ratio for mobile viewing
- **🎬 Smart Cropping Logic**: 
  - Single object: Centers crop on the detected object
  - Multiple objects: Intelligently positions crops to capture all subjects
  - Stacked crops when appropriate: Creates two 9:8 crops stacked vertically for 2 or 4-5 objects
  - **Advanced 3-head cropping**: Specialized logic for 3-person interviews with optimized aspect ratios (9:6 + 9:10)
- **🎙️ AI Transcription**: Generates SRT captions using OpenAI Whisper
- **🎨 Caption Styling**: Customizable caption appearance with fonts, colors, and positioning
- **⚡ Smooth Transitions**: Prevents jarring crop changes with intelligent smoothing
- **🔧 Flexible Configuration**: Extensive command-line options for customization
- **🎥 Cut Detection**: Intelligent scene cut detection to optimize processing
- **🖼️ Graphic Preservation**: Optional preservation of graphic elements using CLIP model classification

## Installation

### Prerequisites

- **Rust** (latest stable version, edition 2024)
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
  --object face \
  --source ./video/input.mp4 \
  --headless \
  --use-stack-crop \
  --smooth-percentage 5.0 \
  --smooth-duration 1.5 \
  --device cuda:0 \
  --ver 11.0 \
  --scale l \
  --object-prob-threshold 0.8 \
  --object-area-threshold 0.02 \
  --use-simple-smoothing
```

### Command Line Options

#### Input/Output
- `--source <FILE>`: Input video file (default: `./video/video1.mp4`)

#### Object Detection
- `--object <TYPE>`: Object type to detect - `face`, `head`, `ball`, `sports ball`, `frisbee`, `person`, `car`, `truck`, or `boat` (default: `face`)
- `--object-prob-threshold <FLOAT>`: Threshold where object gets included in crop logic (default: `0.7`)
- `--object-area-threshold <FLOAT>`: Minimum object area as percentage of frame (0.01 = 1%, ignored for ball objects) (default: `0.01`)

#### Model Configuration
- `--device <DEVICE>`: Processing device - `cpu:0`, `cuda:0`, `coreml` (default: `cpu:0`)
- `--scale <SCALE>`: Model scale - `n`, `s`, `m`, `l` (default: `m`)
- `--dtype <DTYPE>`: Model data type - `auto`, `f32`, `f16` (default: `auto`)
- `--ver <VERSION>`: YOLO version (default: `11.0`)

#### Cropping Options
- `--use-stack-crop`: Enable stacked crop mode for interviews with 2 people
- `--smooth-percentage <FLOAT>`: Smoothing threshold percentage (default: `10.0`)
- `--smooth-duration <FLOAT>`: Smoothing duration in seconds (default: `1.5`)
- `--use-simple-smoothing`: Use simple smoothing instead of history smoothing

#### Cut Detection Options
- `--cut-similarity <FLOAT>`: Cut similarity threshold (default: `0.3`)
- `--cut-start <FLOAT>`: Cut start threshold (default: `0.8`)

#### Graphic Processing Options
- `--keep-graphic`: Keep graphic elements in the video
- `--graphic-threshold <FLOAT>`: Graphic threshold for CLIP model classification (default: `0.3`)

#### Processing Options
- `--headless`: Run without GUI display

## How It Works

### 1. Object Detection
The tool uses selected YOLO models to detect objects in each video frame. It filters detections by confidence threshold to ensure accuracy.
Use the `--object` param to select which type of object to detect. Current options:
- **face**: Detects faces
- **head**: Detects heads
- **person**: Detects people
- **ball**: Detects footballs (soccer balls)
- **sports ball**: Detects sport balls
- **frisbee**: Detects frisbees
- **car**: Detects cars
- **truck**: Detects trucks
- **boat**: Detects boats

### 2. Crop Calculation
Based on the number of detected objects, the tool calculates optimal crop areas:

- **0 objects**: Centered crop with 3:4 aspect ratio
- **1 object**: Crop centered on the detected object
- **2 objects**: 
  - If objects are close: Single crop containing both
  - If objects are far apart: Two stacked crops (when `--use-stack-crop` is enabled)
- **3 objects**: 
  - **Special case**: When heads are similar in size and equally spaced, creates optimized stacked crops:
    - First crop: 9:6 aspect ratio for two heads (top portion of final 9:16)
    - Second crop: 9:10 aspect ratio for single head (bottom portion of final 9:16)
  - **Fallback**: Standard stacked crop logic for other 3-head scenarios
- **4-5 objects**: Similar logic to 2 objects
- **6+ objects**: Crop based on the largest detected object

### 3. Smoothing
To prevent jarring transitions, the tool implements intelligent smoothing:
- Compares crop similarity using percentage thresholds
- Maintains crop consistency for a configurable duration in seconds
- Smooths transitions between different crop types
- Supports both history-based smoothing and simple smoothing modes

### 4. Video Processing
- Crops each frame according to the calculated areas
- Maintains 9:16 aspect ratio for portrait output
- Processes frames at the original video's frame rate
- Detects scene cuts to optimize processing using similarity thresholds
- Optionally preserves graphic elements using CLIP model classification

### 5. Advanced 3-Head Cropping
The tool includes sophisticated logic for handling 3-head scenarios:
- **Smart Detection**: Automatically detects when 3 heads are similar in size and equally spaced
- **Optimized Ratios**: Creates two crops with specific aspect ratios designed to work together:
  - Top crop (9:6 ratio) captures two heads for the upper portion of the final 9:16 video
  - Bottom crop (9:10 ratio) captures the third head for the lower portion
- **Intelligent Positioning**: Automatically positions crops to ensure all heads are properly captured
- **Fallback Logic**: Falls back to standard stacked crop behavior when the special case criteria aren't met

### 6. Transcription
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

The tool automatically selects the appropriate model based on the `--object`, `--ver`, and `--scale` parameters. Available models in the `model/` directory include:

#### Face Detection Models
- `yolov6m-face.onnx` (v6 medium)
- `yolov6n-face.onnx` (v6 nano)
- `yolov8l-face.onnx` (v8 large)
- `yolov8m-face.onnx` (v8 medium)
- `yolov8n-face.onnx` (v8 nano)
- `yolov10n-face.onnx` (v10 nano)
- `yolov10s-face.onnx` (v10 small)
- `yolov10m-face.onnx` (v10 medium)
- `yolov10l-face.onnx` (v10 large)
- `yolov11n-face.onnx` (v11 nano)
- `yolov11s-face.onnx` (v11 small)
- `yolov11m-face.onnx` (v11 medium) - default
- `yolov11l-face.onnx` (v11 large)

#### Head Detection Models
- `v8-head-fp16.onnx` (v8 head detection)

#### Football Detection Models
- `yolov8n-football.onnx` (v8 nano)
- `yolov8m-football.onnx` (v8 medium)

#### Other Objects
For other objects like `person`, `car`, `truck`, `boat`, `sports ball`, `frisbee`, the tool downloads the standard COCO-80 yolo model with class filtering.

## Examples

### Convert a landscape interview to portrait
```bash
cargo run --release -- \
  --object face \
  --ver 11.0 \
  --scale s \
  --source interview.mp4 \
  --headless \
  --smooth-percentage 5.0 \
  --smooth-duration 2.0
```

### Process a two person interview with stacked crops
```bash
cargo run --release -- \
  --object face \
  --ver 11.0 \
  --scale s \
  --source group_shot.mp4 \
  --headless \
  --use-stack-crop \
  --smooth-percentage 8.0
```

### Process a three person interview with advanced cropping
```bash
cargo run --release -- \
  --object face \
  --ver 11.0 \
  --scale s \
  --source three_person.mp4 \
  --headless \
  --use-stack-crop \
  --smooth-percentage 8.0
```
**Note**: The tool automatically detects when 3 heads are similar in size and equally spaced, creating optimized crops with 9:6 and 9:10 aspect ratios that work together for the final 9:16 portrait video.

### High-quality processing with GPU acceleration
```bash
cargo run --release -- \
  --object face \
  --ver 11.0 \
  --scale l \
  --source high_quality.mp4 \
  --device cuda:0 \
  --headless
```

### Detect football/soccer balls
```bash
cargo run --release -- \
  --object ball \
  --ver 8.0 \
  --scale m \
  --source football_match.mp4 \
  --headless
```

### Detect heads instead of faces
```bash
cargo run --release -- \
  --object head \
  --source interview.mp4 \
  --headless
```

### Detect other objects (person, car, etc.)
```bash
cargo run --release -- \
  --object person \
  --source street_scene.mp4 \
  --headless
```

### Process with cut detection and graphic preservation
```bash
cargo run --release -- \
  --object face \
  --source presentation.mp4 \
  --headless \
  --cut-similarity 0.2 \
  --cut-start 0.6 \
  --keep-graphic \
  --graphic-threshold 0.4
```

### Use simple smoothing for faster processing
```bash
cargo run --release -- \
  --object face \
  --source video.mp4 \
  --headless \
  --use-simple-smoothing \
  --smooth-percentage 15.0
```

## Performance Tips

- **GPU Acceleration**: Use `--device cuda:0` or `--device coreml` for faster processing
- **Model Size**: Larger models (`--scale l`) provide better accuracy but slower processing
- **Headless Mode**: Use `--headless` for faster processing without GUI overhead
- **Smoothing Strategy**: Use `--use-simple-smoothing` for faster processing with basic smoothing
- **Cut Detection**: Adjust `--cut-similarity` and `--cut-start` thresholds for your video content

## Dependencies

This project uses the following key dependencies:
- **usls**: Computer vision library with video processing capabilities
- **openai-api-rs**: OpenAI API client for transcription
- **image-compare**: Image similarity comparison for cut detection
- **ndarray**: Numerical computing for image processing
- **tokio**: Async runtime for transcription processing

## Troubleshooting

### Common Issues

1. **ffmpeg not found**: Install ffmpeg and ensure it's in your PATH
2. **CUDA errors**: Ensure CUDA drivers and toolkit are properly installed
3. **Memory issues**: Try using a smaller model scale (`--scale n` or `--scale s`)
4. **Transcription fails**: Check your OpenAI API key and internet connection
5. **Cut detection issues**: Adjust similarity thresholds based on your video content

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
- [YOLO](https://github.com/ultralytics/ultralytics) - Object detection models
- [YOLO-Face](https://github.com/akanametov/yolo-face) - Yolo Face detection
- [YOLO-Football](https://github.com/noorkhokhar99/YOLOv8-football) - Yolo for football (soccer)

## Support

If you encounter any issues or have questions, please:
1. Check the [Issues](https://github.com/yourusername/land2port/issues) page
2. Create a new issue with detailed information about your problem
3. Include your system information and command used

---

**Made with ❤️ for content creators**