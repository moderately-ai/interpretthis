# Int keys are stringified first, then sorted lexicographically — so "10" precedes "2".
import json
print(json.dumps({10: "ten", 2: "two", 1: "one"}, sort_keys=True))
