# Captures of real harness traffic

Decision record for open item OPEN-029 (PROPOSED default, story 120).

- Only redacted captures are committed here. A raw capture never leaves the machine that recorded it.
- Redaction replaces every text value with a filler of the same byte length and every credential, cookie, session value and user name with a keyed token. The key of the run is discarded.
- A capture is committed only after the leak scan found no run of 16 or more characters of the raw text in the output. The scan writes the stamp `leak-scan-v1:16` into the file (`scan_stamp`); `scripts/capture-redact.sh` refuses any file here without it.
- The owner reviews a capture before it is committed.
- A file here is never rewritten. A new recording writes a new file named `<harness>-<UTC timestamp>.json`.
- A small hand-reviewed set of real captures is kept privately (RISK-032) and is not in this repository.

## Engine replies (story 151)

The capture format holds requests. An engine reply is stored as the `body` of an entry whose path
names what it is (for example `/v1/chat/completions#terminal-chunk-second-identical-turn`), so the
same redaction and leak scan apply: strings become fillers, numbers and keys stay. Metrics text
holds numbers and metric names only and is stored as `.txt`.

- `llama-stream-timings-*.json`: a streamed turn on llama-server 0.5.0 (build 11146) with
  `stream_options.include_usage`, cold and then identical. Measured with Qwen3-1.7B Q4_K_M (the
  Ollama blob), not the hybrid Qwen3.5-2B the task names, which is not on this machine.
- `llama-props-*.json`: the `/props` body (`total_slots`, `default_generation_settings.n_ctx`).
- `llama-metrics-*.txt`: the `/metrics` text, run with `--metrics -np 2 -c 4096`.
