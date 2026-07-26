"""
Direct ONNX + Librosa mel spectrogram test for Whisper.
This bypasses our Rust code entirely to prove:
- If this works: our Rust mel computation is wrong
- If this also fails: the ONNX models themselves have an issue

Run: python test/integration/py_onnx_whisper_test.py
Requires: pip install onnxruntime librosa numpy
"""
import numpy as np
import time

AUDIO_FILE = r"C:\Users\Aryan\Downloads\Recording.m4a"
MODEL_DIR  = r"C:\Users\Aryan\.cluaiz\models\audio\whisper-large-v3-turbo-INT8"

ENCODER_PATH         = MODEL_DIR + r"\encoder_model_int8.onnx"
DECODER_PATH         = MODEL_DIR + r"\decoder_model_int8.onnx"
TOKENIZER_PATH       = MODEL_DIR + r"\tokenizer.json"

N_FFT        = 400
HOP_LENGTH   = 160
N_MELS       = 128
SAMPLE_RATE  = 16000
MAX_FRAMES   = 3000
MAX_SAMPLES  = 480000  # 30 seconds

# Special tokens for whisper-large-v3-turbo
SOT          = 50258  # <|startoftranscript|>
LANG_HI      = 50276  # <|hi|>
TRANSCRIBE   = 50360  # <|transcribe|>
NO_TIMESTAMPS = 50364  # <|notimestamps|>
EOT          = 50257  # <|endoftext|>

print("\n" + "="*60)
print("  Python ONNX Whisper Direct Test")
print("="*60)

# Step 1: Load audio
print("\n[1] Loading audio...")
try:
    import librosa
    audio, sr = librosa.load(AUDIO_FILE, sr=SAMPLE_RATE, mono=True)
    print(f"    Duration: {len(audio)/SAMPLE_RATE:.2f}s, SR: {sr}")
except ImportError:
    print("    librosa not found, using soundfile + resampy fallback")
    import soundfile as sf
    audio, sr = sf.read(AUDIO_FILE)
    if sr != SAMPLE_RATE:
        import resampy
        audio = resampy.resample(audio, sr, SAMPLE_RATE)
    print(f"    Duration: {len(audio)/SAMPLE_RATE:.2f}s")

# Pad or trim to 30 seconds
if len(audio) < MAX_SAMPLES:
    audio = np.pad(audio, (0, MAX_SAMPLES - len(audio)))
else:
    audio = audio[:MAX_SAMPLES]

# Step 2: Compute mel spectrogram using librosa (same as Whisper)
print("\n[2] Computing mel spectrogram (librosa)...")
t0 = time.time()
try:
    import librosa
    mel = librosa.feature.melspectrogram(
        y=audio.astype(np.float32),
        sr=SAMPLE_RATE,
        n_fft=N_FFT,
        hop_length=HOP_LENGTH,
        n_mels=N_MELS,
        fmin=0.0,
        fmax=SAMPLE_RATE // 2,
        window='hann',
        center=False,
        norm='slaney',
        htk=False,
        power=2.0
    )
    # Trim/pad to MAX_FRAMES
    if mel.shape[1] < MAX_FRAMES:
        mel = np.pad(mel, ((0,0),(0, MAX_FRAMES - mel.shape[1])))
    else:
        mel = mel[:, :MAX_FRAMES]
    
    log_mel = np.log10(np.maximum(mel, 1e-10))
    log_mel = np.maximum(log_mel, log_mel.max() - 8.0)
    log_mel = (log_mel + 4.0) / 4.0
    
    print(f"    Shape: {log_mel.shape}, Min: {log_mel.min():.3f}, Max: {log_mel.max():.3f}")
    print(f"    Time: {time.time()-t0:.2f}s")
    mel_input = log_mel[np.newaxis, :, :].astype(np.float32)  # [1, 128, 3000]
except Exception as e:
    print(f"    ERROR: {e}")
    exit(1)

# Step 3: Run encoder
print("\n[3] Running ONNX encoder...")
try:
    import onnxruntime as ort
    sess_opts = ort.SessionOptions()
    sess_opts.intra_op_num_threads = 4
    encoder = ort.InferenceSession(ENCODER_PATH, sess_options=sess_opts, providers=['CPUExecutionProvider'])
    
    t0 = time.time()
    enc_out = encoder.run(None, {"input_features": mel_input})
    hidden_states = enc_out[0]  # [1, 1500, 1280]
    print(f"    Hidden states shape: {hidden_states.shape}")
    print(f"    Min: {hidden_states.min():.4f}, Max: {hidden_states.max():.4f}, Mean: {hidden_states.mean():.4f}")
    print(f"    Time: {time.time()-t0:.2f}s")
    
    if np.all(hidden_states == 0):
        print("    WARNING: All-zero hidden states! Encoder produced nothing.")
    else:
        print("    Encoder produced valid hidden states.")
except Exception as e:
    print(f"    Encoder ERROR: {e}")
    exit(1)

# Step 4: Run decoder (greedy)
print("\n[4] Running ONNX decoder (greedy, language=hi)...")
try:
    decoder = ort.InferenceSession(DECODER_PATH, sess_options=sess_opts, providers=['CPUExecutionProvider'])
    
    decoder_ids = [SOT, LANG_HI, TRANSCRIBE, NO_TIMESTAMPS]
    speech_tokens = []
    max_tokens = 100
    
    t0 = time.time()
    for step in range(max_tokens):
        ids = np.array([decoder_ids], dtype=np.int64)  # [1, seq_len]
        
        # Build decoder inputs
        dec_inputs = {
            "encoder_hidden_states": hidden_states,
            "input_ids": ids,
        }
        # Check if decoder needs attention_mask
        input_names = [i.name for i in decoder.get_inputs()]
        if "attention_mask" in input_names:
            dec_inputs["attention_mask"] = np.ones_like(ids)
        
        dec_out = decoder.run(None, dec_inputs)
        logits = dec_out[0]  # [1, seq_len, vocab_size]
        
        # Get last token logits
        last_logits = logits[0, -1, :]  # [vocab_size]
        
        # Suppress special tokens except EOT
        last_logits[SOT] = -float('inf')
        last_logits[LANG_HI] = -float('inf')
        last_logits[TRANSCRIBE] = -float('inf')
        last_logits[NO_TIMESTAMPS] = -float('inf')
        
        next_tok = int(np.argmax(last_logits[:EOT+1]))
        
        if next_tok == EOT:
            break
        
        decoder_ids.append(next_tok)
        speech_tokens.append(next_tok)
        
        if step < 5:
            print(f"    Step {step}: token={next_tok}, score={last_logits[next_tok]:.3f}")
    
    print(f"    Decode time: {time.time()-t0:.2f}s, Tokens: {len(speech_tokens)}")
    
    # Decode tokens to text
    print("\n[5] Decoding tokens to text...")
    try:
        from tokenizers import Tokenizer
        tokenizer = Tokenizer.from_file(TOKENIZER_PATH)
        text = tokenizer.decode(speech_tokens, skip_special_tokens=True)
        print(f"    Result: '{text}'")
    except ImportError:
        print(f"    tokenizers not installed. Raw tokens: {speech_tokens[:20]}")
        
except Exception as e:
    import traceback
    print(f"    Decoder ERROR: {e}")
    traceback.print_exc()

print("\n" + "="*60)
