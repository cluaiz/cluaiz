# Video input refused because the native media pack is missing

Status: P2 availability gap on this Windows debug setup. Other release packages were not tested.

Repro: create `red-clip.mp4` with `ffmpeg -f lavfi -i color=c=red:s=64x64:d=1 -c:v libx264 -pix_fmt yuv420p`, put it in the isolated workspace, then run `video_probe.py` with the Qwen3.5 0.8B vision model and its projector. The request uses the documented `video_url` chat part and asks for the frame colour.

Expected: HTTP 200 with a red answer, or documentation that the native pack must be installed separately. Actual: HTTP 400 `media_preparation_failed`: `Video decoder unavailable: The OCR pack (PDFium and Tesseract) is not installed; the release packages do not include it yet`. See `../raw/video_check.jsonl` and the last video request/response in `../raw/requests.jsonl` and `../raw/responses.jsonl`.

`docs/api/README.md` lists `video_url` as an image/video route. `engine/media/src/request.rs:65` refuses video when `attachment_formats` fails; `engine/media/src/native_assets.rs:72` reports the missing pack. No pack installation or Engine source change was made.
