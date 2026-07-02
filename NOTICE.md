# Model Weights — Third-Party Licenses

The **source code** of land2port is licensed under the MIT License (see `LICENSE`).

The **model weights** are NOT part of this repository. They are downloaded and
cached at runtime (see `src/config.rs` / usls `Hub`). The weights are the work
of third parties and remain under their own licenses — **MIT does not apply to
them.** Anyone who downloads or redistributes these weights must comply with the
license listed below. In particular, the YOLO-derived weights carry copyleft
(AGPL-3.0 / GPL-3.0) obligations that can extend to services built around them.

| Object | Model(s) | Downloaded from | Upstream project | License |
|--------|----------|-----------------|------------------|---------|
| `face` | `yolov{6,8,10,11}{n,s,m,l}-face` | [`deepghs/yolo-face`](https://huggingface.co/deepghs/yolo-face) (ONNX exports) | [akanametov/yolo-face](https://github.com/akanametov/yolo-face) (YOLOv8/10/11 → Ultralytics; YOLOv6 → Meituan) | AGPL-3.0 (v8/10/11), GPL-3.0 (v6) |
| `head` | `v8-head-fp16` | [`jamjamjon/assets`](https://github.com/jamjamjon/assets/releases/tag/yolo) (usls default hub) | YOLOv8 (Ultralytics) | AGPL-3.0 |
| `ball` | `yolov8{n,m}-football` | GitHub release on this repo (`models-v1`) | [noorkhokhar99/YOLOv8-football](https://github.com/noorkhokhar99/YOLOv8-football) (YOLOv8 → Ultralytics) | AGPL-3.0 |

Notes:
- The `deepghs/yolo-face` repository additionally carries a "Model Distribution
  Disclaimer License"; the underlying Ultralytics/Meituan licenses still govern
  the weights themselves.
- **Commercial use** of the Ultralytics-derived weights without releasing your
  application under AGPL-3.0 requires an Ultralytics Enterprise License.
- The football weights are re-hosted here only because no upstream public
  download exists; they remain AGPL-3.0 works of their original authors.

If you are the rights holder for any weight referenced here and want it removed
or re-attributed, please open an issue.
