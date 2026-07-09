# A single integer key still gets stringified to "1" with no extra punctuation.
import json
print(json.dumps({1: "one"}))
