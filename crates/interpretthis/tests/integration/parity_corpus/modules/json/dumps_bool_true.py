# json.dumps(True) emits the lowercase `true` literal per RFC 8259.
import json
print(json.dumps(True))
