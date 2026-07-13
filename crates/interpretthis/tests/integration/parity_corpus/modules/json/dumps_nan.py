# CPython emits the non-RFC `NaN` token for float('nan') when allow_nan is on (default).
import json
print(json.dumps(float('nan')))
