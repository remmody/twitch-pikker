"""Offline Silero TTS helper for twitch-chat-reader.

Loads the model from a LOCAL file (argv[1]); never touches the network.
Protocol (stdout is reserved for it; everything else goes to stderr):
  -> "READY" once the model is loaded
  <- "<speaker>\t<text>"   (one request per line)
  -> "OK <path to wav>" | "ERR <message>"

Requires: pip install torch numpy
"""
import os
import sys
import tempfile

import torch

SAMPLE_RATE = 24000

model_path = sys.argv[1]
torch.set_num_threads(max(1, (os.cpu_count() or 2) // 2))
importer = torch.package.PackageImporter(model_path)
model = importer.load_pickle("tts_models", "model")
model.to(torch.device("cpu"))

print("READY", flush=True)

for line in sys.stdin:
    speaker, _, text = line.rstrip("\n").partition("\t")
    try:
        fd, path = tempfile.mkstemp(prefix="tcr-silero-", suffix=".wav")
        os.close(fd)
        model.save_wav(text=text, speaker=speaker, sample_rate=SAMPLE_RATE, audio_path=path)
        print(f"OK {path}", flush=True)
    except Exception as e:  # keep serving later messages
        print(f"ERR {str(e)[:200]}".replace("\n", " "), flush=True)
