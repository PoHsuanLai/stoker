"""Drives a running or freshly spawned speech-host over its socket with WAV files (stdlib only).

For each WAV and thread count: spawn the host, Hello, Begin, stream the audio paced at real
time in --frame-ms frames (a held microphone), End, read events to Done. Then the same audio
unpaced, for the real-time factor. Prints one line per run. Never touches a device.
"""
import threading, argparse, base64, json, os, socket, struct, subprocess, sys, tempfile, time, wave


def send(sock, msg):
    body = json.dumps(msg).encode()
    sock.sendall(struct.pack(">I", len(body)) + body)


def recv(sock):
    head = b""
    while len(head) < 4:
        part = sock.recv(4 - len(head))
        if not part:
            raise EOFError("host closed")
        head += part
    (n,) = struct.unpack(">I", head)
    body = b""
    while len(body) < n:
        body += sock.recv(n - len(body))
    return json.loads(body)


def read_wav(path):
    with wave.open(path) as w:
        assert (w.getnchannels(), w.getsampwidth()) == (1, 2), path
        raw, rate = w.readframes(w.getnframes()), w.getframerate()
    if rate == 16000:
        return raw
    # Linear resample to 16 kHz for the test wavs recorded at other rates (a dev tool, not a product path).
    xs = struct.unpack(f"<{len(raw) // 2}h", raw)
    n = int(len(xs) * 16000 / rate)
    out = []
    for i in range(n):
        pos = i * rate / 16000
        j = int(pos)
        frac = pos - j
        nxt = xs[min(j + 1, len(xs) - 1)]
        out.append(int(xs[j] * (1 - frac) + nxt * frac))
    return struct.pack(f"<{n}h", *out)


def utterance(sock, pcm, frame_ms, paced, lang):
    """Returns (first partial after the first audio, release to Done, total, text, audio seconds)."""
    sock.settimeout(120)
    seen = []  # (time, message), filled by a reader thread so event times are not the sender's

    def reader():
        while True:
            m = recv(sock)
            seen.append((time.monotonic(), m))
            if m["kind"] in ("done", "failed"):
                return

    send(sock, {"kind": "begin", "v": {
        "model": "nemotron-3.5-asr-streaming",
        "mode": {"kind": "streaming", "v": {"chunk": 560}},
        "lang": lang,
        "format": {"rate": 16000, "pcm": "s16_le"}}})
    t = threading.Thread(target=reader)
    t.start()
    step = 16 * frame_ms * 2  # bytes per frame
    t0 = time.monotonic()
    for i, at in enumerate(range(0, len(pcm), step)):
        if paced:
            wait = t0 + i * frame_ms / 1000 - time.monotonic()
            if wait > 0:
                time.sleep(wait)
        send(sock, {"kind": "audio", "v": {
            "format": {"rate": 16000, "pcm": "s16_le"}, "at": at // 2,
            "pcm": base64.b64encode(pcm[at:at + step]).decode()}})
    released = time.monotonic()
    send(sock, {"kind": "end"})
    t.join()
    when, last = seen[-1]
    if last["kind"] == "failed":
        raise RuntimeError(last["v"])
    first = next((w - t0 for w, m in seen if m["kind"] == "event" and m["v"]["kind"] == "partial"), None)
    return first, when - released, when - t0, last["v"]["text"], len(pcm) / 32000


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", required=True, help="path of the speech-host binary")
    ap.add_argument("--model-dir", required=True)
    ap.add_argument("--threads", default="4,6,8")
    ap.add_argument("--frame-ms", type=int, default=80)
    ap.add_argument("--chunk-ms", type=int, default=560)
    ap.add_argument("--lang", default="auto")
    ap.add_argument("wavs", nargs="+")
    a = ap.parse_args()
    lang = {"kind": "auto"} if a.lang == "auto" else {"kind": "prefer", "v": [a.lang]}
    env = dict(os.environ)
    for threads in a.threads.split(","):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "host.sock")
            proc = subprocess.Popen([a.host, "--socket", path, "--model-dir", a.model_dir,
                                     "--threads", threads, "--chunk-ms", str(a.chunk_ms)], env=env)
            try:
                t_start = time.monotonic()
                while not os.path.exists(path):
                    if proc.poll() is not None:
                        sys.exit("host exited")
                    time.sleep(0.01)
                sock = socket.socket(socket.AF_UNIX)
                sock.connect(path)
                send(sock, {"kind": "hello", "v": {"vocab": 1}})
                assert recv(sock)["kind"] == "hello"
                print(f"threads={threads} socket up {time.monotonic() - t_start:.2f}s after spawn")
                for wav in a.wavs:
                    pcm = read_wav(wav)
                    name = os.path.basename(wav)
                    # one warm-up pass so model load and first-run costs are not in the numbers
                    utterance(sock, pcm, a.frame_ms, False, lang)
                    fp, r2f, total, text, secs = utterance(sock, pcm, a.frame_ms, True, lang)
                    _, _, fast, _, _ = utterance(sock, pcm, a.frame_ms, False, lang)
                    print(f"threads={threads} {name} audio={secs:.2f}s "
                          f"first_partial={fp if fp is None else round(fp, 3)}s "
                          f"release_to_final={r2f:.3f}s rtf={fast / secs:.3f} text={text!r}")
                sock.close()
            finally:
                proc.terminate()
                proc.wait()


main()
