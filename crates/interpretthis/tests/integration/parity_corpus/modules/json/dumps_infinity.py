# CPython emits the non-RFC `Infinity` token for +inf when allow_nan is on (default).
import json
print(json.dumps(float('inf')))
