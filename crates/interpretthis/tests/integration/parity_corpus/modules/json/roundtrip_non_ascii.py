# Round-trip restores the original code points after ensure_ascii escaping.
import json
print(json.loads(json.dumps("café")))
