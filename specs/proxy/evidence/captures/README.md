# Captures of real harness traffic

Decision record for open item OPEN-029 (PROPOSED default, story 120).

- Only redacted captures are committed here. A raw capture never leaves the machine that recorded it.
- Redaction replaces every text value with a filler of the same byte length and every credential, cookie, session value and user name with a keyed token. The key of the run is discarded.
- A capture is committed only after the leak scan found no run of 16 or more characters of the raw text in the output. The scan writes the stamp `leak-scan-v1:16` into the file (`scan_stamp`); `scripts/capture-redact.sh` refuses any file here without it.
- The owner reviews a capture before it is committed.
- A file here is never rewritten. A new recording writes a new file named `<harness>-<UTC timestamp>.json`.
- A small hand-reviewed set of real captures is kept privately (RISK-032) and is not in this repository.
