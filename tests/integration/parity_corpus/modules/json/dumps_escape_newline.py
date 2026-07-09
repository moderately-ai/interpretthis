# Newlines must serialize as the `\n` escape, not literal LF bytes.
import json
print(json.dumps("line1\nline2"))
