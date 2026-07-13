# json.dumps(False) emits the lowercase `false` literal per RFC 8259.
import json
print(json.dumps(False))
